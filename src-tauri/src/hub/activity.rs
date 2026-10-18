//! What a session is doing: its hook events, failures, the working/stuck/waiting read of each
//! session, usage limits and cost.

use super::*;
use super::asks::notify_title;

/// Context this full (percent) at a turn's end: the notification says so and offers Compact.
pub const CONTEXT_HIGH: u8 = 80;

/// The last background agent reported: Claude Code starts the session's next turn within a moment.
/// Past this with no turn, its last reply is taken as your turn.
const HELPERS_SETTLE_SECS: u64 = 15;

/// No new output for this long while running a command, or while only thinking: stuck.
const STUCK_RUNNING_MS: u64 = 10 * 60_000;

const STUCK_THINKING_MS: u64 = 5 * 60_000;
/// After Cue stops a turn to send your message now, this long before the stop counts as yours: typing the
/// message into a machine's terminal takes a few round trips, and a failed send says so itself.
const STOPPED_TO_SEND_MS: u64 = 30_000;
/// How far a machine's log copy may lag behind its hook: a turn begun this recently isn't read as stopped.
const LOG_LAG_MS: u64 = 10_000;

/// A queued message taken from the queue this recently was taken as the turn before it ended: it's
/// the next turn's prompt, not the prompt of the turn now ending.
const TAKEN_SETTLE_MS: u64 = 3000;

/// How long before a waiting session's cache runs out Cue gives you a heads-up: an hour's cache in its last
/// 10 minutes, a 5-minute one in its last 2; and the least context worth one. insights.js cacheWarn counts
/// the card down by the same rule (CACHE_WARN_MS, CACHE_WARN_SHORT_MS, CACHE_MIN_TOKENS).
const COLD_WARN_MS: u64 = 10 * 60_000;
const COLD_WARN_SHORT_MS: u64 = 2 * 60_000;
const COLD_MIN_TOKENS: u64 = 50_000;

impl Hub {
    /// Your message queued during the turn that just ended, unless the turn read it (absorbed between
    /// steps) or it's this turn's own prompt (taken from the queue a while ago).
    fn unread_queued(&self, origin: &Origin) -> Option<crate::model::Exchange> {
        let q = self.st().sessions.queued(&origin.session_id)?;
        if origin.transcript_path.is_empty() {
            return Some(q);
        }
        match transcript::read_state(&origin.harness, &origin.transcript_path, &q.text, q.at_ms) {
            Some(transcript::Queued::Absorbed) => None,
            Some(transcript::Queued::Taken(Some(at))) if now_ms().saturating_sub(at) > TAKEN_SETTLE_MS => None,
            _ => Some(q),
        }
    }

    pub fn event(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str) {
        self.event_in(origin, event, message, turn, driven_by, false)
    }

    /// An "active" event whose prompt is known to be yours (nothing to look up: Pi, Codex, a session on a
    /// machine): the prompt goes into the session's thread under the same lock as its state, so nothing
    /// sent to it the moment it shows as working can land in the thread first.
    pub fn active_from_you(&self, origin: Origin, message: String, turn: Vec<TurnPart>, driven_by: &str) {
        self.event_in(origin, "active", message, turn, driven_by, true)
    }

    fn event_in(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str, from_you: bool) {
        // Its turn is over: the message you kept for it (sent while it worked) goes now.
        let sid = origin.session_id.clone();
        let held = event == "stopped" && self.st().sessions.held(&sid).is_some();
        self.on_event(origin, event, message, turn, driven_by, from_you);
        if held {
            self.send_held_soon(&sid, 0);
        }
    }

    fn on_event(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str, from_you: bool) {
        // Pi's extension says what it's doing as each tool starts ("Running: cargo test").
        // Claude Code is summarizing the conversation (PreCompact: /compact typed anywhere, or its context
        // filled up). Until it's done the session isn't waiting on you, whatever it said last.
        if event == "compacting" {
            let mut st = self.st();
            if st.sessions.compacting_since(&origin.session_id) == 0 {
                st.sessions.set_compacting(&origin.session_id, now_ms());
            }
            drop(st);
            self.changed();
            return;
        }
        // A prompt showing in its terminal that never reached Cue (a sandboxed command's network access).
        // ("answer_lost": Cue's own answer never reached it, so it's asking there for sure.)
        if event == "terminal_ask" || event == "answer_lost" {
            self.terminal_ask_with(origin, message, true, None, event == "answer_lost");
            return;
        }
        // Anything else from the session: it got past such a prompt.
        self.clear_terminal_asks(&origin.session_id);
        if event == "doing" {
            let changed = self.st().sessions.set_doing(&origin.session_id, &message, 0);
            if changed {
                self.changed();
            }
            return;
        }
        // What you sent during this turn that it hasn't read yet: the next turn's prompt, so it goes
        // below this turn's reply (it was noted when you sent it, above where the reply lands).
        let mut unread = None;
        if event == "stopped" {
            self.deliver_deferred_rename(&origin.session_id);
            unread = self.unread_queued(&origin);
            // The turn is over: whatever you queued during it has been read, or is read next.
            self.st().sessions.set_queued(&origin.session_id, None);
            if origin.harness == "claude" {
                self.limits_lifted(&origin.session_id);
            }
            // It wrote its hand-off note itself: its card offers "Continue in a new session".
            if crate::ipc::notice_handoff(self, &origin.session_id) {
                self.changed();
            }
        }
        let (message, followup) = split_turn(message, &turn, &crate::config::turn_mode());
        // Its turn ended with background agents still at work (Claude Code's Agent tool): it's at its
        // prompt, but its answer isn't in yet; Claude Code wakes it when they report. So it waits on
        // them, not on you, and the chat shows them at work. (A message you send goes straight in.)
        let (helpers, background) = if event == "stopped" && driven_by.is_empty() && origin.harness == "claude" && !origin.transcript_path.is_empty() {
            (crate::steps::running_helpers(&origin.transcript_path).len() as u64, crate::steps::running_background(&origin.transcript_path) as u64)
        } else {
            (0, 0)
        };
        let driven_by = if helpers > 0 { crate::sessions::helpers_words(helpers) } else { driven_by.to_string() };
        // Another agent drives this session (a helper its lead started, labelled CUE_DRIVEN_BY): its finished turn is for that
        // agent, so it stays in Working as "waiting on its lead" instead of becoming your card.
        if event == "stopped" && !driven_by.is_empty() && (helpers > 0 || !crate::config::flag("/agents/show_driven")) {
            {
                let mut st = self.st();
                let sid = origin.session_id.clone();
                st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
                st.sessions.mark(&origin, "agent", None);
                st.sessions.set_driven_by(&sid, &driven_by);
                st.sessions.set_helpers(&sid, helpers);
                st.sessions.set_background(&sid, background);
                st.sessions.note(&sid, "agent", &message, crate::config::context_keep());
                if let Some(q) = &unread {
                    st.sessions.below_reply(&sid, q);
                }
            }
            self.changed();
            return;
        }
        let sid = origin.session_id.clone();
        let origin_path = origin.transcript_path.clone();
        match event {
            "stopped" => {
                let context = if !message.trim().is_empty() {
                    vec![Ctx { role: "assistant".into(), text: message.clone() }]
                } else if !origin.transcript_path.is_empty() {
                    transcript::recent_context(&origin.transcript_path, 2)
                } else {
                    vec![]
                };
                let (id, title) = {
                    let mut st = self.st();
                    // A session with an open decision is already in the queue — don't double-list it.
                    if st.items.iter().any(|i| i.kind != "waiting" && i.origin.session_id == sid) {
                        return;
                    }
                    st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
                    st.sessions.mark(&origin, "waiting", None);
                    st.sessions.set_queued(&sid, None);
                    let thread = st.sessions.thread(&sid);
                    st.sessions.note(&sid, "agent", &message, crate::config::context_keep());
                    if let Some(q) = &unread {
                        st.sessions.below_reply(&sid, q);
                    }
                    let mut it = Self::new_item(&mut st, "waiting", origin);
                    it.thread = thread;
                    it.context = context;
                    it.message = message;
                    it.followup = followup;
                    let t = notify_title(&it);
                    let id = it.id.clone();
                    st.items.push(it);
                            (id, t)
                };
                // Its card lands with its cache and context numbers, not at the next tick.
                self.cache_tick();
                self.changed();
                self.turn_changes(&sid);
                // A lead's turn ended: its next queued review goes (after a moment, for its hooks to settle).
                if let Some(me) = self.shared().filter(|_| self.st().review_queue.iter().any(|q| q.lead == sid)) {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        me.drain_reviews();
                    });
                }
                // Context high (80%+): the notification says so and offers Compact.
                let full = (!origin_path.is_empty()).then(|| crate::steps::context_pct(&origin_path)).flatten().filter(|p| *p >= CONTEXT_HIGH);
                // Put off for later: it waits under Need to decide, without a notification.
                let later = self.st().sessions.is_later(&sid);
                // A message you kept for it goes now: nothing to tell you, it's working again in a moment.
                let held = self.st().sessions.held(&sid).is_some();
                if crate::config::flag("/notify/finished") && !later && !held {
                    match full {
                        Some(p) => self.notify(&id, &format!("{} · context {p}%", title.0), &title.1, "turn-full"),
                        None => self.notify(&id, &title.0, &title.1, "turn"),
                    }
                }
            }
            "active" => {
                let mut started = false;
                {
                    let mut st = self.st();
                    st.active_at.insert(sid.clone(), now_ms());
                    // Claude Code reports a message you queue while it's busy the moment it lands in its
                    // queue, not when it reads it (that's at its next step). So that's no new turn, and the
                    // message stays queued, with Send now, until the turn ends.
                    let echo = st.sessions.state(&sid).as_deref() == Some("working")
                        && st.sessions.queued_text(&sid).is_some_and(|q| transcript::is_message(&message, &q));
                    if !echo {
                        started = st.sessions.state(&sid).as_deref() != Some("working");
                        st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
                        // Working again, whoever started it. Whether the prompt was yours (and gets a bubble)
                        // is settled separately: see prompt_from_you.
                        st.sessions.mark(&origin, "working", None);
                        st.sessions.set_queued(&sid, None);
                    }
                    if from_you && !message.trim().is_empty() {
                        let m = crate::sessions::unwrap_pasted(&message);
                        st.sessions.set_prompt(&sid, &m);
                        st.sessions.note(&sid, "you", &m, crate::config::context_keep());
                    }
                }
                if started {
                    self.turn_started(&sid, &origin.cwd, &origin.machine);
                }
                self.changed();
            }
            "ended" => {
                let ids: Vec<String> = {
                    let st = self.st();
                    st.items.iter().filter(|i| i.origin.session_id == sid).map(|i| i.id.clone()).collect()
                };
                for id in ids {
                    self.finish(&id, "gone", "session ended");
                }
                self.drop_session(&sid);
            }
            _ => {}
        }
    }

    /// A turn ended on an API error. A usage limit leaves the session "limited" (no card: there's
    /// nothing to answer until it resets); any other error is a finished turn that needs you.
    pub fn failed(&self, origin: Origin, error_type: &str, details: &str) {
        let sid = origin.session_id.clone();
        if sid.is_empty() {
            return;
        }
        let now = now_ms();
        let Some(mut limit) = crate::usage::parse(error_type, details, now) else {
            let what = if details.trim().is_empty() { error_type.replace('_', " ") } else { details.trim().to_string() };
            let driver = crate::leads::resolve_driver(&sid, "");
            self.event(origin, "stopped", format!("Stopped on an error: {what}"), vec![], &driver);
            return;
        };
        let announce = {
            let mut st = self.st();
            // Already marked by the hook or the transcript check: once is enough.
            if st.sessions.all().iter().any(|s| s.origin.session_id == sid && s.state == "limited") {
                return;
            }
            if limit.resets_ms.is_none() {
                limit.resets_ms = st.rates.as_ref().and_then(|r| r.models.iter().find(|m| m.name == limit.scope)).and_then(|m| m.resets_ms);
            }
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            st.sessions.mark(&origin, "limited", None);
            st.sessions.set_queued(&sid, None);
            let key = format!("{}|{}", limit.scope, limit.resets_ms.unwrap_or(0) / 60_000);
            st.sessions.set_limit(&sid, Some(limit.clone()));
            st.told_out.insert(key)
        };
        self.changed();
        if announce {
            let when = limit.resets_ms.map(|r| format!(" until {}", crate::usage::when(r, now))).unwrap_or_default();
            let (title, body) = if limit.scope == "all" {
                (format!("Claude Code is out of usage{when}"), "No Claude Code session can run until then.".to_string())
            } else {
                (format!("{} is used up{when}", limit.scope), "Other models still work: switch with /model.".to_string())
            };
            self.notify("usage", &title, &body, "");
        }
    }

    /// A Claude Code turn finished normally, so the plan's limit is behind us: every session that
    /// was waiting on it can run again. (A model's own limit only lifts for the session that hit it.)
    fn limits_lifted(&self, sid: &str) {
        let now = now_ms();
        let mut st = self.st();
        let mut lifted = false;
        for s in st.sessions.all() {
            let Some(mut l) = s.limit.clone() else { continue };
            if s.state != "limited" || l.scope != "all" || l.resets_ms.is_some_and(|r| r <= now) || s.origin.session_id == sid {
                continue;
            }
            l.resets_ms = Some(now);
            st.sessions.set_limit(&s.origin.session_id, Some(l));
            lifted = true;
        }
        drop(st);
        if lifted {
            self.changed();
        }
    }

    /// The status line's usage. Only redraws when the numbers move.
    pub fn set_cost(&self, session_id: &str, cost: f64) {
        self.st().costs.insert(session_id.to_string(), cost);
    }

    /// A Claude Code session's cost so far, for API-key and proxy users only. On a plan (Claude Code sends
    /// its limits only then) it's what the session would cost at API prices, not what you pay.
    pub fn session_cost(&self, session_id: &str) -> Option<f64> {
        let st = self.st();
        if st.rates.is_some() {
            return None;
        }
        st.costs.get(session_id).copied()
    }

    pub fn set_rates(&self, rl: &Value) {
        let now = now_ms();
        {
            let st = self.st();
            if let Some(old) = &st.rates {
                let same = |a: &Option<crate::usage::Window>, k: &str| {
                    a.as_ref().map(|w| w.pct) == rl.pointer(&format!("/{k}/used_percentage")).and_then(Value::as_f64)
                };
                // No numbers in this one at all: keep what we have, nothing to redraw.
                if rl.pointer("/five_hour/used_percentage").is_none() && rl.pointer("/seven_day/used_percentage").is_none() {
                    return;
                }
                // Re-reading ~/.claude.json for the per-model rows is the slow part: at most once a minute.
                if same(&old.five_hour, "five_hour") && same(&old.seven_day, "seven_day") && now.saturating_sub(old.at_ms) < 60_000 {
                    return;
                }
            }
        }
        let mut rates = crate::usage::rates_from_statusline(rl, now);
        let changed = {
            let mut st = self.st();
            // A status line without a window (another agent's, or before Claude Code has numbers) keeps the
            // last one instead of wiping it: the meter shouldn't vanish between updates.
            if let Some(old) = &st.rates {
                let fresh = rates.five_hour.is_some() || rates.seven_day.is_some();
                if rates.five_hour.is_none() { rates.five_hour = old.five_hour.clone(); }
                if rates.seven_day.is_none() { rates.seven_day = old.seven_day.clone(); }
                if !fresh { rates.at_ms = old.at_ms; }   // "as of" stays when the numbers it carries are old
            }
            let changed = st.rates.as_ref().map(|r| (&r.five_hour, &r.seven_day, &r.models)) != Some((&rates.five_hour, &rates.seven_day, &rates.models));
            st.rates = Some(rates);
            changed
        };
        if changed {
            self.changed();
        }
    }

    /// The board's cards' insight numbers (their cache's expiry and their context, from the end of their
    /// logs: steps::insight_state), kept for the state the window gets, and the heads-up before a waiting
    /// session's cache runs out. Off the main thread: every ~10 s and as a turn ends. Cards on this Mac
    /// only: another machine's log is never looked up from here (the window asks for those itself).
    fn cache_tick(&self) {
        let now = now_ms();
        let waiting: Vec<(Item, bool)> = {
            let st = self.st();
            st.items
                .iter()
                .filter(|i| i.status == "pending" && i.origin.machine.is_empty() && !i.origin.transcript_path.is_empty() && matches!(i.origin.harness.as_str(), "claude" | "codex"))
                .map(|i| (i.clone(), st.sessions.is_later(&i.origin.session_id)))
                .collect()
        };
        let mut heads_up = vec![];
        let mut insight = HashMap::new();
        for (it, later) in waiting {
            let sid = &it.origin.session_id;
            let Some((until, tokens, start, last)) = crate::steps::insight_state(&it.origin.transcript_path) else { continue };
            insight.insert(sid.clone(), serde_json::json!({ "until_ms": until, "tokens": tokens, "start": start, "last_ms": last }));
            // Codex logs don't say how long they cache: no heads-up.
            if until == 0 {
                continue;
            }
            // A 5-minute cache is always inside 10 minutes: told in its last 2, not as its turn ends.
            let warn = if until.saturating_sub(last) > COLD_WARN_MS { COLD_WARN_MS } else { COLD_WARN_SHORT_MS };
            if until <= now || until - now > warn || tokens < COLD_MIN_TOKENS || later {
                continue;
            }
            heads_up.push((it, until, tokens));
        }
        let heads_up = {
            let mut st = self.st();
            st.insight = insight;
            st.cold_warned.retain(|_, until| *until > now);
            let mut told = vec![];
            for (it, until, _) in heads_up {
                if st.cold_warned.insert(it.origin.session_id.clone(), until) != Some(until) {
                    told.push((it, until));
                }
            }
            told
        };
        if crate::config::flag("/notify/cold") {
            for (it, until) in heads_up {
                // The card's own notification, replaced: Reply on it still goes to the session.
                let project = crate::config::moved().remove(&it.origin.session_id).unwrap_or_else(|| it.project.clone());
                let mins = (until - now).div_ceil(60_000);
                let (_, what) = notify_title(&it);
                self.notify(&it.id, &format!("{project} · cache runs out in {mins}m"), &what, if it.kind == "waiting" { "turn" } else { "" });
            }
        }
    }

    /// Every ~10s: catch turns that died without a hook reaching Cue, and announce limits that lifted.
    pub fn usage_tick(&self) {
        // A lead that came free without a Stop reaching Cue still gets its queued reviews.
        self.drain_reviews();
        let now = now_ms();
        let (stuck, limited) = {
            let st = self.st();
            let all = st.sessions.all();
            let stuck: Vec<Origin> = all
                .iter()
                .filter(|s| s.state == "working" && s.origin.harness == "claude" && !s.origin.transcript_path.is_empty() && now.saturating_sub(s.since_ms) > 20_000)
                .map(|s| s.origin.clone())
                .collect();
            let limited: Vec<crate::usage::Limit> = all.iter().filter(|s| s.state == "limited").filter_map(|s| s.limit.clone()).collect();
            (stuck, limited)
        };
        for o in stuck {
            if let Some((kind, text)) = crate::usage::last_api_error(&o.transcript_path) {
                self.failed(o, &kind, &text);
            } else if let Some((at, reply, parts)) = transcript::turn_ended_parts(&o.transcript_path).filter(|(at, _, _)| now.saturating_sub(*at) > 5_000) {
                // Its turn ended but the Stop never reached Cue (Cue was restarting): finish it now.
                // Only a turn that ended after it started working, and seconds ago (the hook is faster).
                let since = self.st().sessions.all().into_iter().find(|s| s.origin.session_id == o.session_id).map(|s| s.since_ms).unwrap_or(u64::MAX);
                if at >= since {
                    let driver = crate::leads::resolve_driver(&o.session_id, "");
                    self.event(o, "stopped", reply, parts, &driver);
                }
            }
        }
        self.cache_tick();
        // A limit whose reset time passed: say so once, per scope.
        let mut back: Vec<(String, usize)> = vec![];
        {
            let mut st = self.st();
            for l in &limited {
                let Some(r) = l.resets_ms.filter(|r| *r <= now && now - *r < 3600 * 1000) else { continue };
                let key = format!("{}|{}", l.scope, r / 60_000);
                if st.told_back.insert(key) {
                    let n = limited.iter().filter(|x| x.scope == l.scope).count();
                    back.push((l.scope.clone(), n));
                }
            }
        }
        if !back.is_empty() {
            self.changed();
        }
        for (scope, n) in back {
            let who = if scope == "all" { "Claude Code".to_string() } else { scope };
            let paused = if n == 1 { "1 session didn't run: resend it from Cue.".to_string() } else { format!("{n} sessions didn't run: resend them from Cue.") };
            self.notify("usage", &format!("{who} is back"), &paused, "");
        }
    }

    /// A turn began: note the folder's git state (in the background: git can take a moment) to tell
    /// later what this turn changed.
    fn turn_started(&self, session_id: &str, cwd: &str, machine: &str) {
        let (Some(me), sid, cwd, machine) = (self.shared(), session_id.to_string(), cwd.to_string(), machine.to_string()) else { return };
        std::thread::spawn(move || {
            // A session on another machine: its folder is there, so git runs there.
            let base = if machine.is_empty() { crate::changes::base(&cwd) } else { crate::changes::base_on(&machine, &cwd) };
            me.st().sessions.set_turn_base(&sid, base);
        });
    }

    /// A turn ended: what it changed, for the "4 files · +98 −9" pill and Commit.
    fn turn_changes(&self, session_id: &str) {
        let Some(base) = self.st().sessions.take_turn_base(session_id) else { return };
        let (Some(me), sid) = (self.shared(), session_id.to_string()) else { return };
        std::thread::spawn(move || {
            let changes = crate::changes::since(&base);
            if me.st().sessions.set_changes(&sid, changes) {
                me.changed();
            }
        });
    }

    /// Every second: what each working Claude Code session is doing now, read from the end of its
    /// transcript (no hook involved, so it costs the agent nothing).
    pub fn refresh_activity(&self) {
        self.sync_agent_names();
        let todo = self.st().sessions.working_transcripts();
        let found: Vec<(String, (String, u64))> =
            todo.into_iter().filter_map(|(sid, harness, path)| crate::transcript::activity_of(&harness, &path).map(|a| (sid, a))).collect();
        // A message you queued that Claude Code or Codex has since taken (into the running turn, or as
        // the next prompt): it's no longer queued, so no "Queued" bubble pinned below the answer, and no Send now.
        let queued: Vec<(String, String, String, crate::model::Exchange)> = {
            let st = self.st();
            st.sessions.working_transcripts().into_iter().filter_map(|(sid, h, path)| st.sessions.queued(&sid).map(|q| (sid, h, path, q))).collect()
        };
        // Interrupted (Esc, on purpose or not) a few seconds ago with nothing since: no Stop comes for
        // that, so it lands in Waiting as "interrupted", with Continue. A few seconds: ⌘Enter's Esc
        // is followed at once by your message.
        // Not when the stop was Cue's own, to send your message now: typing it into a machine's terminal
        // can take longer than that. Nor when a message reached it since moments ago: a machine's log copy
        // can lag behind its hook, still ending on the interrupt while the agent works on what came next.
        let now = now_ms();
        let stuck: Vec<Origin> = {
            let st = self.st();
            let recent = |m: &HashMap<String, u64>, sid: &str, ms: u64| m.get(sid).is_some_and(|t| now.saturating_sub(*t) < ms);
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness == "claude" && !s.origin.transcript_path.is_empty())
                .filter(|s| !recent(&st.stopped_to_send, &s.origin.session_id, STOPPED_TO_SEND_MS) && !recent(&st.active_at, &s.origin.session_id, LOG_LAG_MS))
                .filter(|s| transcript::interrupted_at(&s.origin.transcript_path).is_some_and(|at| at + 1500 >= s.since_ms && now.saturating_sub(at) > 3000))
                .map(|s| s.origin).collect()
        };
        for o in stuck {
            let sid = o.session_id.clone();
            let driver = crate::leads::resolve_driver(&sid, "");
            self.event(o, "stopped", "Interrupted: it stopped mid-turn and is waiting for you. Say “continue” to pick up where it left off.".into(), vec![], &driver);
            if let Some(it) = self.st().items.iter_mut().find(|i| i.kind == "waiting" && i.origin.session_id == sid) {
                it.interrupted = true;
            }
            self.changed();
        }
        // Stuck: working, but nothing new in its transcript for 10 minutes while it runs a command (a long
        // build is fine; one waiting for input in its terminal isn't), or 5 while it's only thinking. Not
        // while a helper agent works (that writes to its own log). Said once per quiet spell.
        let quiet: Vec<(String, u64, String, u64, String)> = {
            let st = self.st();
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness != "pi" && !s.origin.transcript_path.is_empty())
                .filter_map(|s| {
                    let at = std::fs::metadata(&s.origin.transcript_path).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
                    let name = if s.name.is_empty() { crate::model::folder_of(&s.origin.cwd) } else { s.name.clone() };
                    Some((s.origin.session_id.clone(), at.max(s.since_ms), s.doing.clone(), s.stuck_ms, name))
                }).collect()
        };
        for (sid, at, doing, was, name) in quiet {
            let limit = if doing.starts_with("Delegating") { u64::MAX } else if doing.starts_with("Running") || doing.starts_with("Using") { STUCK_RUNNING_MS } else { STUCK_THINKING_MS };
            let stuck = now.saturating_sub(at) >= limit;
            if stuck && was != at {
                self.st().sessions.set_stuck(&sid, at);
                self.changed();
                if crate::config::flag("/notify/decisions") {
                    let what = if doing.is_empty() { "Thinking".to_string() } else { doing.trim_end_matches('…').to_string() };
                    self.notify(&format!("stuck-{sid}"), &format!("{name} · no new output for {}m", now.saturating_sub(at) / 60_000), &format!("{what}. If it's waiting for input, it's in its terminal."), "");
                }
            } else if !stuck && was != 0 {
                self.st().sessions.set_stuck(&sid, 0);
                crate::notify::remove(&[format!("stuck-{sid}")]);
                self.changed();
            }
        }
        // At its prompt while its background agents work: how many are left. Once none is, Claude Code
        // wakes it with their reports (a new turn); if that doesn't come, its last reply is your turn.
        // Collected first: a `for` keeps the lock alive for the whole loop, and the body locks again.
        let with_helpers = self.st().sessions.with_helpers();
        for o in with_helpers {
            let left = crate::steps::running_helpers(&o.transcript_path).len() as u64;
            let background = crate::steps::running_background(&o.transcript_path) as u64;
            let sid = o.session_id.clone();
            if left > 0 {
                let changed = {
                    let mut st = self.st();
                    st.sessions.set_helpers(&sid, left) | st.sessions.set_background(&sid, background)
                };
                if changed {
                    self.changed();
                }
                continue;
            }
            {
                let mut st = self.st();
                st.sessions.set_helpers(&sid, 0);
                st.sessions.set_background(&sid, background);
            }
            self.changed();
            let me = self.shared();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(HELPERS_SETTLE_SECS));
                let Some(me) = me else { return };
                let still = me.st().sessions.state(&sid).as_deref() == Some("agent");
                if still {
                    if let Some((at, reply, parts)) = transcript::turn_ended_parts(&o.transcript_path) {
                        me.your_turn_since(o, reply, parts, at);
                        me.changed();
                    }
                }
            });
        }
        // A background task that failed after running a while (noted by its session's steps feed): on the
        // session, so the window and the add-on can say so, and a notification like a stuck session's.
        let failures = crate::steps::take_task_failures();
        if !failures.is_empty() {
            let mut told: Vec<(String, String, String, String)> = vec![];
            {
                let mut st = self.st();
                for f in failures {
                    let Some(s) = st.sessions.all().into_iter().find(|s| s.origin.transcript_path == f.path) else { continue };
                    let sid = s.origin.session_id.clone();
                    if st.sessions.set_task_failed(&sid, f.at_ms, &f.desc, &f.line) {
                        told.push((sid, s.project.clone(), f.desc, f.line));
                    }
                }
            }
            if !told.is_empty() {
                self.changed();
                if crate::config::flag("/notify/decisions") {
                    for (sid, project, desc, line) in told {
                        self.notify(&format!("task-failed-{sid}"), &format!("{project}: background task failed"), &(if line.is_empty() { desc } else { line }), "");
                    }
                }
            }
        }
        // Compacting (from Cue, its terminal, or on its own) until its transcript says it's done; given up after 15 minutes.
        let compacted: Vec<(String, bool)> = {
            let st = self.st();
            st.sessions
                .compacting()
                .into_iter()
                .filter_map(|(sid, path, at)| {
                    if transcript::compact_finished(&path, at) {
                        Some((sid, true))
                    } else {
                        (now.saturating_sub(at) > 15 * 60_000).then_some((sid, false))
                    }
                })
                .collect()
        };
        if !compacted.is_empty() {
            let mut st = self.st();
            for (sid, done) in &compacted {
                if *done {
                    st.sessions.set_compacted(sid, now);
                } else {
                    st.sessions.set_compacting(sid, 0);
                }
            }
            drop(st);
            self.changed();
        }
        // Before Claude Code starts (no hook from it yet), its terminal can sit on a screen of its own: the
        // trust question, or one waiting for Enter. Read off its pane: this Mac's now, a machine's from
        // its last listing (leads::refresh_remote, no SSH here). Shown as such, not as at work.
        let before = self.st().sessions.before_start();
        let mut seen: Vec<(String, Option<crate::dialog::Setup>)> = vec![];
        for o in before {
            let setup = if o.machine.is_empty() {
                match crate::focus::tmux_screen(&o.tmux_pane) {
                    Ok(screen) => crate::dialog::setup_on(&screen),
                    Err(_) => continue,
                }
            } else if crate::leads::remote_screens_known(&o.machine) {
                crate::leads::remote_setup(&o.machine, &o.tmux_pane)
            } else {
                continue;
            };
            seen.push((o.session_id, setup));
        }
        if !seen.is_empty() {
            let mut st = self.st();
            let changed = seen.iter().fold(false, |any, (sid, setup)| st.sessions.set_setup(sid, *setup, now) || any);
            drop(st);
            if changed {
                self.changed();
            }
        }
        // A session Cue started in a folder Claude Code didn't trust yet, in a terminal Cue can't read:
        // asking until ~/.claude.json says it's trusted (answered there). Given up after 30 minutes. Only
        // on this Mac: a machine's folder is never looked up here.
        let done: Vec<String> = {
            let st = self.st();
            st.sessions.asking_trust().into_iter()
                .filter(|(sid, _, _)| st.sessions.origin(sid).is_some_and(|o| o.machine.is_empty() && o.tmux_pane.is_empty()))
                .filter(|(_, cwd, at)| now.saturating_sub(*at) > 30 * 60_000 || crate::focus::claude_trusts(cwd)).map(|(sid, _, _)| sid).collect()
        };
        if !done.is_empty() {
            let mut st = self.st();
            for sid in &done {
                st.sessions.set_trust(sid, 0);
            }
            drop(st);
            self.changed();
        }
        // Taken as the next turn's prompt only counts once the turn before it has had its say: the
        // queue is read the moment a turn ends, a hair before that turn's Stop, which moves it below the reply.
        let taken: Vec<String> = queued
            .into_iter()
            .filter(|(_, h, path, q)| match transcript::read_state(h, path, &q.text, q.at_ms) {
                Some(transcript::Queued::Absorbed) => true,
                Some(transcript::Queued::Taken(at)) => at.map_or(true, |at| now.saturating_sub(at) > TAKEN_SETTLE_MS),
                _ => false,
            })
            .map(|(sid, _, _, _)| sid)
            .collect();
        let changed = {
            let mut st = self.st();
            for sid in &taken {
                st.sessions.set_queued(sid, None);
            }
            found.iter().fold(!taken.is_empty(), |any, (sid, (a, at))| st.sessions.set_doing(sid, a, *at) || any)
        };
        if changed {
            self.changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transcript timestamp for `ms` ("2026-10-12T10:00:00.123Z").
    fn iso(ms: u64) -> String {
        let secs = (ms / 1000) as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::gmtime_r(&secs, &mut tm) };
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, ms % 1000)
    }

    #[test]
    fn a_turn_cue_stopped_to_send_now_isnt_called_interrupted_and_one_you_stopped_is() {
        let dir = std::env::temp_dir().join(format!("cue-int-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let interrupted = |sid: &str| -> Origin {
            let path = dir.join(format!("{sid}.jsonl"));
            let o = Origin { session_id: sid.into(), harness: "claude".into(), cwd: "/x/proj".into(), transcript_path: path.to_string_lossy().into(), ..Default::default() };
            hub.mark_state(&o, "working");
            // Stopped a second after it began: its log ends on the interrupt, nothing after it.
            let at = now_ms() - 1000;
            std::fs::write(&path, format!("{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":\"on the pill\"}}}}\n{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"[Request interrupted by user]\"}}]}}}}\n", iso(at - 500), iso(at))).unwrap();
            o
        };
        let cue_stopped = interrupted("now1");
        hub.st().stopped_to_send.insert("now1".into(), now_ms());   // ⌘↵: Cue's own Esc, your message on its way
        let lagging = interrupted("lag1");
        let you_stopped = interrupted("esc1");
        std::thread::sleep(std::time::Duration::from_millis(2300));
        hub.st().active_at.insert("lag1".into(), now_ms());   // its next message reached Cue; the log copy hasn't caught up
        hub.refresh_activity();
        let state = |o: &Origin| hub.st().sessions.state(&o.session_id);
        assert_eq!(state(&cue_stopped).as_deref(), Some("working"), "Cue stopped it to send yours: still at work, no Interrupted card");
        assert_eq!(state(&lagging).as_deref(), Some("working"), "a message just reached it: the log copy is behind, not stopped");
        assert_eq!(state(&you_stopped).as_deref(), Some("waiting"), "you stopped it: Interrupted, waiting for you");
        assert!(hub.pending().iter().any(|i| i.origin.session_id == "esc1" && i.interrupted));
        assert!(!hub.pending().iter().any(|i| i.origin.session_id != "esc1"));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_turn_that_ends_with_background_agents_at_work_waits_on_them_not_you() {
        let dir = std::env::temp_dir().join(format!("cue-helpers-{}", std::process::id()));
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore"}"#).unwrap();
        std::fs::write(sub.join("agent-a1.jsonl"), concat!(r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"h1","name":"Read","input":{"file_path":"/a/hub.rs"}}]}}"#, "\n")).unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"map the names"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Backend: names"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"launched"}]},"toolUseResult":{"status":"async_launched","agentId":"a1","description":"Backend: names"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"text","text":"An agent is on it."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let o = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x".into(), transcript_path: path.to_string_lossy().into_owned(), ..Default::default() };
        hub.event(o.clone(), "stopped", "An agent is on it.".into(), vec![], "");
        let s = hub.saved_sessions().into_iter().find(|s| s.origin.session_id == "s1").unwrap();
        assert_eq!((s.state.as_str(), s.driven_by.as_str(), s.helpers), ("agent", "1 background agent", 1), "at its prompt, waiting on its agent");
        assert!(hub.pending().is_empty(), "no card: its answer isn't in yet");
        assert_eq!(s.thread.last().map(|e| e.text.as_str()), Some("An agent is on it."), "what it said so far is in the chat");
        // You write to it while they work: it's at work again, no longer waiting on them (no stale count).
        hub.active_from_you(o.clone(), "and the ids".into(), vec![], "");
        let s = hub.saved_sessions().into_iter().find(|s| s.origin.session_id == "s1").unwrap();
        assert_eq!((s.state.as_str(), s.driven_by.as_str(), s.helpers), ("working", "", 0), "at work, not waiting on its agents");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
