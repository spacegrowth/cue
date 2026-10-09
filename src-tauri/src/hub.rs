//! The in-memory queue. Everything that changes state goes through here, and every change
//! pushes a fresh snapshot to the window, updates the dock badge, and (for new items) notifies.

use crate::model::*;
use crate::transcript;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::oneshot;

/// What an open Ask connection is told.
pub enum Reply {
    Decision(Decision),
    /// Answered elsewhere / session gone: the client should exit without deciding.
    Cancel(String),
}

#[derive(Default)]
struct Store {
    /// Each Claude Code session's cost so far, from its status line (shown in the chat's tab).
    costs: HashMap<String, f64>,
    items: Vec<Item>,
    history: VecDeque<Item>,
    /// Every answer of the last day, newest first (History's today numbers; the list above is capped).
    answers: Vec<Answer>,
    waiters: HashMap<String, oneshot::Sender<Reply>>,
    next: u64,
    /// When each session last started a turn — how a typed reply is confirmed as submitted.
    active_at: HashMap<String, u64>,
    /// Every live session and its recent activity (the Working column and the 30-minute bars).
    sessions: crate::sessions::Sessions,
    /// Cards with a notification in Notification Center — taken back once the card is gone.
    notified: HashSet<String>,
    /// Sessions that take replies directly (Pi's extension): session id → (connection number, channel).
    subscribers: HashMap<String, (u64, tokio::sync::mpsc::UnboundedSender<Value>)>,
    next_sub: u64,
    /// The same sessions' origins and when they connected: a Pi session subscribes the moment it
    /// starts, so the Sessions view lists it before its first turn.
    subscribed: HashMap<String, (Origin, u64)>,
    /// The plan's usage from the status line (None until a status line forwards it).
    rates: Option<crate::usage::Rates>,
    /// Limits already announced (scope + reset), so each one notifies once going out and once coming back.
    told_out: HashSet<String>,
    told_back: HashSet<String>,
    /// Reviews typed into a lead: executor → (its lead, when). "in review" until it's no longer done
    /// (15 minutes at most). A lead takes one review at a time: Review waits until it's free again.
    review_sent: HashMap<String, (String, u64)>,
    /// When a review was last typed into each lead: it takes a moment to show as working.
    review_last: HashMap<String, u64>,
}

/// After typing a review into a lead, wait this long before judging it free again.
const REVIEW_SETTLE_MS: u64 = 8000;
/// "in review" stops showing after this long even if the executor still reads as done.
const REVIEW_SHOWN_MS: u64 = 15 * 60_000;

/// Context this full (percent) at a turn's end: the notification says so and offers Compact.
pub const CONTEXT_HIGH: u8 = 80;
/// No new output for this long while running a command, or while only thinking: stuck.
const STUCK_RUNNING_MS: u64 = 10 * 60_000;
const STUCK_THINKING_MS: u64 = 5 * 60_000;
/// A queued message taken from the queue this recently was taken as the turn before it ended: it's
/// the next turn's prompt, not the prompt of the turn now ending.
const TAKEN_SETTLE_MS: u64 = 3000;

pub struct Hub {
    store: Mutex<Store>,
    app: Option<AppHandle>,
}

impl Hub {
    pub fn new(app: Option<AppHandle>) -> Self {
        let mut st = Store::default();
        st.history = crate::db::history(crate::config::history_keep());
        st.answers = crate::db::answers_since(now_ms().saturating_sub(crate::db::DAY_MS));
        let (sessions, waiting) = crate::db::live();
        st.sessions.restore(sessions);
        st.items = waiting;
        // Ids keep counting from the clock so they never collide with ids in saved history.
        st.next = now_ms();
        Self { store: Mutex::new(st), app }
    }

    fn new_item(st: &mut Store, kind: &str, origin: Origin) -> Item {
        st.next += 1;
        Item {
            id: format!("n{}", st.next),
            kind: kind.into(),
            project: project_of(&origin.cwd),
            origin,
            tool_name: String::new(),
            tool_input: Value::Null,
            suggestions: Value::Null,
            context: vec![],
            message: String::new(),
            followup: String::new(), interrupted: false, back_ms: None,
            images: vec![],
            created_ms: now_ms(),
            status: "pending".into(),
            outcome: String::new(),
            resolved_ms: None,
            thread: vec![],
            tool_use_id: None,
            scan_from: 0,
        }
    }

    /// A blocking request arrived. Returns its id and the receiver the connection waits on.
    pub fn add_ask(
        &self,
        origin: Origin,
        kind: String,
        tool_name: String,
        tool_input: Value,
        suggestions: Value,
        mut context: Vec<Ctx>,
    ) -> (String, oneshot::Receiver<Reply>) {
        if context.is_empty() && !origin.transcript_path.is_empty() {
            context = transcript::recent_context(&origin.transcript_path, 4);
        }
        let scan_from = if origin.transcript_path.is_empty() { 0 } else { transcript::tail_offset(&origin.transcript_path) };
        let (tx, rx) = oneshot::channel();
        let (id, title) = {
            let mut st = self.store.lock().unwrap();
            // A session asking for a decision is no longer "just waiting for you".
            let sid = origin.session_id.clone();
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            st.sessions.mark(&origin, "deciding", None);
            let thread = st.sessions.thread(&sid);
            let mut it = Self::new_item(&mut st, &kind, origin);
            it.thread = thread;
            it.tool_name = tool_name;
            it.tool_input = tool_input;
            it.suggestions = suggestions;
            it.context = context;
            it.scan_from = scan_from;
            let asked = match it.kind.as_str() {
                "question" => format!("asked: {}", summary(&it)),
                _ => format!("{}: {}", it.tool_name, summary(&it)),
            };
            st.sessions.note(&sid, "agent", &asked, crate::config::context_keep());
            let id = it.id.clone();
            let title = notify_title(&it);
            st.items.push(it);
            st.waiters.insert(id.clone(), tx);
            (id, title)
        };
        self.changed();
        if crate::config::flag("/notify/decisions") {
            self.notify(&id, &title.0, &title.1, "");
        }
        (id, rx)
    }

    /// Claude Code says a permission prompt is waiting in a session's terminal, one Cue has no card for
    /// (it can't answer it: a sandboxed command asking for network access). A card that sends you to
    /// the terminal, naming the tool call it's about (the newest one still without a result). It goes
    /// once that call gets its result, or the session reports anything else.
    fn terminal_ask(&self, origin: Origin, message: String, notify: bool) {
        let sid = origin.session_id.clone();
        let path = origin.transcript_path.clone();
        let call = (!path.is_empty()).then(|| transcript::waiting_tool_use(&path)).flatten();
        let scan_from = if path.is_empty() { 0 } else { transcript::tail_offset(&path) };
        let (id, title) = {
            let mut st = self.store.lock().unwrap();
            // Cue already shows what it's asking.
            if st.items.iter().any(|i| i.kind != "waiting" && i.origin.session_id == sid) {
                return;
            }
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            st.sessions.mark(&origin, "deciding", None);
            let thread = st.sessions.thread(&sid);
            let mut it = Self::new_item(&mut st, "terminal", origin);
            it.thread = thread;
            it.message = message;
            it.scan_from = scan_from;
            if let Some((tool_use_id, name, input)) = call {
                it.tool_use_id = Some(tool_use_id);
                it.tool_name = name;
                it.tool_input = input;
            }
            let (head, what) = notify_title(&it);
            let body = if it.tool_name.is_empty() { what } else { format!("{}: {what}", it.message) };
            let id = it.id.clone();
            st.items.push(it);
            (id, (head, body))
        };
        self.changed();
        if notify && crate::config::flag("/notify/decisions") {
            self.notify(&id, &title.0, &title.1, "");
        }
    }

    fn clear_terminal_asks(&self, session_id: &str) {
        let ids: Vec<String> = self.store.lock().unwrap().items.iter().filter(|i| i.kind == "terminal" && i.origin.session_id == session_id).map(|i| i.id.clone()).collect();
        for id in ids {
            self.finish(&id, "answered_elsewhere", "answered in the terminal");
        }
    }

    /// "Later": put a session off, or back (`on` false). Kept with the session, so anything showing
    /// Cue's state sees it.
    pub fn set_later(&self, session_id: &str, on: bool) {
        let changed = self.store.lock().unwrap().sessions.set_later(session_id, on);
        if changed {
            self.changed();
        }
    }

    /// Star a session (you're following it), or unstar it. Kept with the session, like Later.
    pub fn set_starred(&self, session_id: &str, on: bool) {
        let changed = self.store.lock().unwrap().sessions.set_starred(session_id, on);
        if changed {
            self.changed();
        }
    }

    /// Answer from the window (or cue-ctl). False if the item is no longer pending.
    pub fn respond(&self, id: &str, d: Decision) -> bool {
        let outcome = describe(&d);
        let tx = self.store.lock().unwrap().waiters.remove(id);
        match tx {
            Some(tx) => {
                let delivered = tx.send(Reply::Decision(d)).is_ok();
                self.finish(id, if delivered { "answered" } else { "gone" }, &if delivered { outcome } else { "agent had already stopped waiting".into() });
                delivered
            }
            None => false,
        }
    }

    /// Move an item to history. Any still-open connection is told to stand down.
    pub fn finish(&self, id: &str, status: &str, outcome: &str) {
        {
            let mut st = self.store.lock().unwrap();
            if let Some(tx) = st.waiters.remove(id) {
                let _ = tx.send(Reply::Cancel(outcome.to_string()));
            }
            let Some(pos) = st.items.iter().position(|i| i.id == id) else { return };
            let mut it = st.items.remove(pos);
            // An answered permission or question means the agent is back at work.
            if it.kind != "waiting" && status != "gone" {
                st.sessions.mark(&it.origin, "working", None);
                st.sessions.note(&it.origin.session_id, "you", outcome, crate::config::context_keep());
            }
            it.status = status.into();
            it.outcome = outcome.into();
            it.resolved_ms = Some(now_ms());
            push_history(&mut st, it);
        }
        self.changed();
    }

    /// Your message queued during the turn that just ended, unless the turn read it (absorbed between
    /// steps) or it's this turn's own prompt (taken from the queue a while ago).
    fn unread_queued(&self, origin: &Origin) -> Option<crate::model::Exchange> {
        let q = self.store.lock().unwrap().sessions.queued(&origin.session_id)?;
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
        // Its turn is over: the message you kept for it (sent while it worked) goes now.
        let sid = origin.session_id.clone();
        let held = event == "stopped" && self.store.lock().unwrap().sessions.held(&sid).is_some();
        self.on_event(origin, event, message, turn, driven_by);
        if held {
            self.send_held_soon(&sid, 0);
        }
    }

    fn on_event(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str) {
        // Pi's extension says what it's doing as each tool starts ("Running: cargo test").
        // Claude Code is summarizing the conversation (PreCompact: /compact typed anywhere, or its context
        // filled up). Until it's done the session isn't waiting on you, whatever it said last.
        if event == "compacting" {
            let mut st = self.store.lock().unwrap();
            if st.sessions.compacting_since(&origin.session_id) == 0 {
                st.sessions.set_compacting(&origin.session_id, now_ms());
            }
            drop(st);
            self.changed();
            return;
        }
        // A prompt showing in its terminal that never reached Cue (a sandboxed command's network access).
        if event == "terminal_ask" {
            self.terminal_ask(origin, message, true);
            return;
        }
        // Anything else from the session: it got past such a prompt.
        self.clear_terminal_asks(&origin.session_id);
        if event == "doing" {
            let changed = self.store.lock().unwrap().sessions.set_doing(&origin.session_id, &message, 0);
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
            self.store.lock().unwrap().sessions.set_queued(&origin.session_id, None);
            if origin.harness == "claude" {
                self.limits_lifted(&origin.session_id);
            }
        }
        let (message, followup) = split_turn(message, &turn, &crate::config::turn_mode());
        // Another agent drives this session (a helper its lead started, labelled CUE_DRIVEN_BY): its finished turn is for that
        // agent, so it stays in Working as "waiting on its lead" instead of becoming your card.
        if event == "stopped" && !driven_by.is_empty() && !crate::config::flag("/agents/show_driven") {
            {
                let mut st = self.store.lock().unwrap();
                let sid = origin.session_id.clone();
                st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
                st.sessions.mark(&origin, "agent", None);
                st.sessions.set_driven_by(&sid, driven_by);
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
                    let mut st = self.store.lock().unwrap();
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
                self.changed();
                self.turn_changes(&sid);
                // Context high (80%+): the notification says so and offers Compact.
                let full = (!origin_path.is_empty()).then(|| crate::steps::context_pct(&origin_path)).flatten().filter(|p| *p >= CONTEXT_HIGH);
                // Put off for later: it waits under Need to decide, without a notification.
                let later = self.store.lock().unwrap().sessions.is_later(&sid);
                // A message you kept for it goes now: nothing to tell you, it's working again in a moment.
                let held = self.store.lock().unwrap().sessions.held(&sid).is_some();
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
                    let mut st = self.store.lock().unwrap();
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
                }
                if started {
                    self.turn_started(&sid, &origin.cwd);
                }
                self.changed();
            }
            "ended" => {
                let ids: Vec<String> = {
                    let st = self.store.lock().unwrap();
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
            let mut st = self.store.lock().unwrap();
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
        let mut st = self.store.lock().unwrap();
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
        self.store.lock().unwrap().costs.insert(session_id.to_string(), cost);
    }
    /// A Claude Code session's cost so far, for API-key and proxy users only. On a plan (Claude Code sends
    /// its limits only then) it's what the session would cost at API prices, not what you pay.
    pub fn session_cost(&self, session_id: &str) -> Option<f64> {
        let st = self.store.lock().unwrap();
        if st.rates.is_some() {
            return None;
        }
        st.costs.get(session_id).copied()
    }

    pub fn set_rates(&self, rl: &Value) {
        let now = now_ms();
        {
            let st = self.store.lock().unwrap();
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
            let mut st = self.store.lock().unwrap();
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

    /// Every ~10s: catch turns that died without a hook reaching Cue, and announce limits that lifted.
    pub fn usage_tick(&self) {
        let now = now_ms();
        let (stuck, limited) = {
            let st = self.store.lock().unwrap();
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
            } else if let Some((at, reply)) = transcript::turn_ended(&o.transcript_path).filter(|(at, _)| now.saturating_sub(*at) > 5_000) {
                // Its turn ended but the Stop never reached Cue (Cue was restarting): finish it now.
                // Only a turn that ended after it started working, and seconds ago (the hook is faster).
                let since = self.store.lock().unwrap().sessions.all().into_iter().find(|s| s.origin.session_id == o.session_id).map(|s| s.since_ms).unwrap_or(u64::MAX);
                if at >= since {
                    let driver = crate::leads::resolve_driver(&o.session_id, "");
                    self.event(o, "stopped", reply, vec![], &driver);
                }
            }
        }
        // A limit whose reset time passed: say so once, per scope.
        let mut back: Vec<(String, usize)> = vec![];
        {
            let mut st = self.store.lock().unwrap();
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

    /// Drop a "waiting" card (or give up on a stuck request) from the window.
    pub fn dismiss(&self, id: &str) {
        let is_waiting = self.store.lock().unwrap().items.iter().any(|i| i.id == id && i.kind == "waiting");
        if is_waiting {
            self.remove_waiting(id);
        } else {
            self.finish(id, "gone", "dismissed in Cue (the agent is still asking in its terminal)");
        }
    }

    /// A waiting card goes to the end of the queue: from now on it sorts by back_ms, so every card
    /// still on its original order comes first. Its age stays when the turn finished.
    pub fn send_to_back(&self, id: &str) -> Result<(), String> {
        let mut st = self.store.lock().unwrap();
        let it = st.items.iter_mut().find(|i| i.id == id).ok_or("that card is gone")?;
        if it.kind != "waiting" || it.status != "pending" {
            return Err("that card isn't waiting".into());
        }
        it.back_ms = Some(now_ms());
        drop(st);
        self.changed();
        Ok(())
    }

    /// Type your reply into a finished session's terminal and confirm it submitted.
    /// Blocking (typing takes ~1-2s): call off the UI thread.
    pub fn reply(&self, id: &str, text: &str, images: &[Upload]) -> Result<String, String> {
        let it = self.get(id).filter(|i| i.status == "pending").ok_or("that card is gone")?;
        let sid = it.origin.session_id.clone();
        self.set_later(&sid, false); // you replied: decided
        let t0 = now_ms();
        let saved = crate::uploads::save(images)?;
        // The card is stale (the session went on to another turn since): typed now, the reply would land
        // mid-turn and could be lost. Kept in Cue instead, as the box does, and sent when the turn ends.
        let busy = {
            let st = self.store.lock().unwrap();
            (st.sessions.state(&sid).as_deref() == Some("working") || st.sessions.compacting_since(&sid) > 0) && !st.subscribers.contains_key(&sid)
        };
        if busy {
            let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
            let mut st = self.store.lock().unwrap();
            st.items.retain(|i| i.id != id);
            st.sessions.hold(&sid, text, paths.clone());
            let mut h = it;
            h.images = paths;
            h.status = "answered".into();
            h.outcome = format!("replied: “{}” (kept in Cue until its turn ends)", first_line(text, 200));
            h.resolved_ms = Some(now_ms());
            push_history(&mut st, h);
            drop(st);
            self.changed();
            return Ok("kept in Cue: it goes when this turn ends".into());
        }
        let cmd_before = command_before(&it.origin, text);
        // Direct delivery when the session listens for it (Pi); otherwise type into its terminal.
        let direct = self.send_direct(&sid, text, &saved, false);
        let via = if direct { "the session directly".to_string() } else { crate::focus::type_into(&it.origin, &crate::uploads::with_paths(text, &saved))? };
        // The agent reports a new turn (Claude: UserPromptSubmit, Pi: agent_start) once it submits.
        let mut submitted = self.took(&sid, &it.origin, text, cmd_before, t0);
        if !submitted && !direct && !text.starts_with('/') {
            let _ = crate::focus::press_enter(&it.origin);
            submitted = self.wait_active(&sid, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        let pics = if saved.is_empty() { String::new() } else { format!(" + {} image{}", saved.len(), if saved.len() == 1 { "" } else { "s" }) };
        let outcome = format!("replied: “{}”{pics}{}", first_line(text, 200), if submitted { "" } else { " (typed; didn't see it submit)" });
        {
            let mut st = self.store.lock().unwrap();
            st.items.retain(|i| i.id != id);
            st.sessions.note_sent(&sid, text, &paths, t0, crate::config::context_keep());
            if !submitted {
                st.sessions.mark_unsent(&sid);
            }
            let mut h = it;
            h.images = paths;
            h.status = "answered".into();
            h.outcome = outcome;
            h.resolved_ms = Some(now_ms());
            push_history(&mut st, h);
        }
        self.changed();
        Ok(if submitted { format!("sent via {via}") } else { format!("typed into {via}, but it didn't seem to submit. Check the tab") })
    }

    /// Send a message to any live session — a working one included (the "say something" box).
    /// Direct when the session listens (Pi); otherwise typed into its terminal, where the agent
    /// queues it if it's mid-turn. Blocking: call off the UI thread.
    /// Stop a session mid-turn (Esc in its terminal; Pi aborts directly). It's left at its prompt.
    /// "Send now" on a queued message: the same Esc as Stop (the agent then reads the queued message
    /// straight away), but nobody stopped anything, so no "⏹ stopped it" line and no stopped state.
    pub fn send_queued_now(&self, session_id: &str) -> Result<String, String> {
        let (origin, direct) = {
            let st = self.store.lock().unwrap();
            let direct = st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "interrupt" })).is_ok()).unwrap_or(false);
            (st.sessions.origin(session_id), direct)
        };
        if !direct {
            let origin = origin.ok_or("that session is gone")?;
            // Claude Code may already have taken it into the running turn (between two steps): Esc
            // now would interrupt the turn that's working on it ("Interrupted · What should Claude
            // do instead?") and leave it idle. Esc only while it's still waiting in the queue.
            let queued = self.store.lock().unwrap().sessions.queued_text(session_id).unwrap_or_default();
            let state = if origin.harness == "claude" { transcript::queued_state(&origin.transcript_path, &queued) } else { None };
            if matches!(state, Some(transcript::Queued::Absorbed | transcript::Queued::Taken(_))) {
                self.store.lock().unwrap().sessions.set_queued(session_id, None);
                self.changed();
                return Ok("it already has your message: it took it in between steps".into());
            }
            crate::focus::press_escape(&origin)?;
        }
        self.store.lock().unwrap().sessions.set_queued(session_id, None); // it's reading it now
        self.changed();
        Ok("sent now".into())
    }

    pub fn interrupt(&self, session_id: &str) -> Result<String, String> {
        let (origin, direct) = {
            let st = self.store.lock().unwrap();
            let direct = st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "interrupt" })).is_ok()).unwrap_or(false);
            (st.sessions.origin(session_id), direct)
        };
        let via = if direct { "the session directly".to_string() } else { crate::focus::press_escape(&origin.clone().ok_or("that session is gone")?)? };
        {
            let mut st = self.store.lock().unwrap();
            if let Some(o) = origin {
                st.sessions.mark(&o, "stopped", None);
            }
            st.sessions.set_queued(session_id, None);
            st.sessions.note(session_id, "you", "⏹ stopped it", crate::config::context_keep());
        }
        self.changed();
        // A message you kept for it goes now, as Claude Code sends its queue once you stop it.
        if self.store.lock().unwrap().sessions.held(session_id).is_some() {
            self.send_held_soon(session_id, 700);
        }
        Ok(format!("stopped via {via}"))
    }

    /// `now`: if it's mid-turn, interrupt it first so this is handled right away (Ctrl+Enter).
    pub fn send_to_session(&self, session_id: &str, text: &str, images: &[Upload], now: bool) -> Result<String, String> {
        if text.trim().is_empty() && images.is_empty() {
            return Err("nothing to send".into());
        }
        let saved = crate::uploads::save(images)?;
        self.send_saved(session_id, text, saved, now)
    }

    /// Send `typed` (and images already saved) to a session. Mid-turn, to an agent Cue types into: Cue
    /// keeps it and sends it when the turn ends (see `held`), unless `now`.
    fn send_saved(&self, session_id: &str, typed: &str, saved: Vec<crate::uploads::Saved>, now: bool) -> Result<String, String> {
        // " /…" keeps its space in `typed`: a message, not a command (see with_paths).
        let text = typed.trim();
        self.set_later(session_id, false); // you wrote to it: decided
        let (origin, busy) = {
            let st = self.store.lock().unwrap();
            // Its terminal is showing a permission prompt or a question: typing now would answer it.
            if st.sessions.state(session_id).as_deref() == Some("deciding") {
                return Err("it's asking you something first: answer that, then send".into());
            }
            // Compacting counts as busy: Claude Code holds what you type until it's done (that can take a minute).
            let busy = st.sessions.state(session_id).as_deref() == Some("working") || st.sessions.compacting_since(session_id) > 0;
            (st.sessions.origin(session_id), busy)
        };
        // Force-send to a terminal agent: Esc first, give it a moment to stop, then type.
        let direct = self.store.lock().unwrap().subscribers.contains_key(session_id);
        // Working, and Cue would type it into its terminal: Cue keeps it instead and sends it when the turn
        // ends, so until then it's yours to take back (Esc, Edit) or send now. (In the terminal's own queue
        // only Claude Code could give it back.)
        if busy && !now && !direct && origin.is_some() {
            self.store.lock().unwrap().sessions.hold(session_id, typed, saved.iter().map(|s| s.path.clone()).collect());
            self.changed();
            return Ok("kept in Cue: it goes when this turn ends".into());
        }
        let busy = if now && busy && !direct {
            crate::focus::press_escape(origin.as_ref().ok_or("that session is gone")?)?;
            std::thread::sleep(std::time::Duration::from_millis(700));
            false
        } else {
            busy && !now
        };
        let origin = origin.ok_or("that session is gone")?;
        let cmd_before = command_before(&origin, typed);
        let t0 = now_ms();
        let direct = self.send_direct(session_id, text, &saved, now);
        let via = if direct { "the session directly".to_string() } else { crate::focus::type_into(&origin, &crate::uploads::with_paths(typed, &saved))? };
        // At its prompt it should take this (as a reply does); if not, Enter once more, then flag it.
        let mut submitted = busy || direct || self.took(session_id, &origin, typed, cmd_before, t0);
        if !submitted && !typed.starts_with('/') {
            let _ = crate::focus::press_enter(&origin);
            submitted = self.wait_active(session_id, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        {
            let mut st = self.store.lock().unwrap();
            st.sessions.note_sent(session_id, text, &paths, t0, crate::config::context_keep());
            if busy {
                // It's mid-turn: the agent reads this when it finishes its current step.
                // The same message as the thread's copy (same time), so the chat shows it once, as queued.
                let at = st.sessions.thread(session_id).iter().rev().find(|e| e.role == "you").map(|e| e.at_ms).unwrap_or_else(now_ms);
                let q = crate::model::Exchange { role: "you".into(), text: text.to_string(), at_ms: at, images: paths, from: String::new(), unsent: false };
                st.sessions.set_queued(session_id, Some(q));
            }
            if !submitted {
                st.sessions.mark_unsent(session_id);
            }
            // "/compact" from the box: the same "Compacting" line as Compact in the ⋯ menu.
            if submitted && !busy && is_command(typed, "compact") {
                st.sessions.set_compacting(session_id, now_ms());
            }
        }
        self.changed();
        if !submitted {
            return Ok(format!("typed into {via}, but it didn't go through as a message. Check the tab"));
        }
        Ok(if busy { format!("queued via {via}: it reads this when it finishes its current step") } else { format!("sent via {via}") })
    }

    /// Send the message kept for a session: when its turn ends, or now (`now`: stop the turn first). If it
    /// can't be sent it's kept again, so nothing you wrote is lost.
    pub fn send_held(&self, session_id: &str, now: bool) -> Result<String, String> {
        let h = self.store.lock().unwrap().sessions.take_held(session_id).ok_or("nothing kept for it")?;
        let saved = h.images.iter().map(|p| crate::uploads::Saved { path: p.clone(), mime: crate::uploads::mime_of(p).into() }).collect();
        self.send_saved(session_id, &h.text, saved, now).inspect_err(|_| {
            self.store.lock().unwrap().sessions.hold(session_id, &h.text, h.images.clone());
            self.changed();
        })
    }

    /// The kept message goes off this thread (it types into a terminal): after `wait_ms`, for a
    /// terminal that was just stopped to settle.
    fn send_held_soon(&self, session_id: &str, wait_ms: u64) {
        let Some(app) = &self.app else { return };
        let hub = app.state::<Arc<Hub>>().inner().clone();
        let sid = session_id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            if let Err(e) = hub.send_held(&sid, false) {
                eprintln!("cue: couldn't send the message kept for {sid}: {e}");
            }
        });
    }

    /// The kept message, back to you (into the box, to change it): Esc, or Edit.
    pub fn unhold(&self, session_id: &str) -> Option<Exchange> {
        let h = self.store.lock().unwrap().sessions.take_held(session_id);
        if h.is_some() {
            self.changed();
        }
        h
    }

    /// Hand a message (and saved images) to a session that listens directly (Pi). False if none does.
    fn send_direct(&self, session_id: &str, text: &str, saved: &[crate::uploads::Saved], now: bool) -> bool {
        let st = self.store.lock().unwrap();
        // now: Pi takes it as a "steer", read mid-turn, instead of a follow-up after the turn.
        let msg = json!({ "type": "reply", "text": text, "images": saved, "now": now });
        st.subscribers.get(session_id).map(|(_, tx)| tx.send(msg).is_ok()).unwrap_or(false)
    }

    /// A session starts listening for direct replies. Returns its connection number for unsubscribe.
    pub fn subscribe(&self, origin: &Origin) -> (u64, tokio::sync::mpsc::UnboundedReceiver<Value>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let n = {
            let mut st = self.store.lock().unwrap();
            st.next_sub += 1;
            let n = st.next_sub;
            st.subscribers.insert(origin.session_id.clone(), (n, tx));
            st.subscribed.insert(origin.session_id.clone(), (origin.clone(), now_ms()));
            n
        };
        self.redraw();
        (n, rx)
    }

    /// Drop a listener — only if it's still the same connection (a reconnect may have replaced it).
    pub fn unsubscribe(&self, session_id: &str, n: u64) {
        let mut st = self.store.lock().unwrap();
        if st.subscribers.get(session_id).is_some_and(|(m, _)| *m == n) {
            st.subscribers.remove(session_id);
            st.subscribed.remove(session_id);
            drop(st);
            self.redraw();
        }
    }

    /// Did what you typed go through? A message: the agent started a turn with it. "/name …" in Claude Code:
    /// that, or its transcript logged the command running (an unknown command like "/reomte" leaves no
    /// trace, so it's caught). A command in an agent Cue can't check: assume it ran.
    fn took(&self, session_id: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> bool {
        if !text.starts_with('/') {
            // A long paste takes a while to land and submit: Enter again too soon would send what's left of
            // it as a second message.
            return self.wait_active(session_id, t0, (5000 + text.len() as u64 / 2).min(15_000));
        }
        let Some(before) = cmd_before else { return true };
        let name = text[1..].split_whitespace().next().unwrap_or("");
        (0..16).any(|_| self.wait_active(session_id, t0, 250) || crate::transcript::command_count(&origin.transcript_path, name) > before)
    }

    fn wait_active(&self, session_id: &str, since: u64, timeout_ms: u64) -> bool {
        let end = now_ms() + timeout_ms;
        while now_ms() < end {
            if self.store.lock().unwrap().active_at.get(session_id).is_some_and(|t| *t >= since) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        false
    }

    pub fn get(&self, id: &str) -> Option<Item> {
        let st = self.store.lock().unwrap();
        st.items.iter().chain(st.history.iter()).find(|i| i.id == id).cloned()
    }

    pub fn pending(&self) -> Vec<Item> {
        self.store.lock().unwrap().items.clone()
    }

    pub fn set_tool_use_id(&self, id: &str, tool_use_id: String) {
        let mut st = self.store.lock().unwrap();
        if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
            it.tool_use_id = Some(tool_use_id);
        }
    }

    pub fn remove_waiting(&self, id: &str) {
        let mut st = self.store.lock().unwrap();
        st.items.retain(|i| i.id != id);
        drop(st);
        self.changed();
    }

    /// (session id, agent pid) for every live session — the watcher drops the ones whose agent exited.
    pub fn session_pids(&self) -> Vec<(String, i32)> {
        self.store.lock().unwrap().sessions.pids()
    }

    /// Where a live session's terminal is — for "Go to tab" on a working session.
    pub fn session_origin(&self, session_id: &str) -> Option<Origin> {
        self.store.lock().unwrap().sessions.origin(session_id)
    }

    /// Every live session as it was when Cue last ran (each with its name): what a reboot may
    /// have left without a tmux pane.
    pub fn saved_sessions(&self) -> Vec<crate::sessions::Session> {
        self.store.lock().unwrap().sessions.all()
    }

    /// A session's state set by hand (a moved one: "working" while it loads, "waiting" once its prompt is up).
    pub fn mark_state(&self, origin: &Origin, state: &str) {
        self.store.lock().unwrap().sessions.mark(origin, state, None);
        self.changed();
    }

    /// A rebuilt session sits at its finished turn again: its waiting card comes back (the reboot
    /// lost it), built from the session's saved thread exactly as a fresh "stopped" would.
    pub fn waiting_card(&self, origin: &Origin) {
        let sid = origin.session_id.clone();
        let (thread, message) = {
            let st = self.store.lock().unwrap();
            let thread = st.sessions.thread(&sid);
            let message = thread.iter().rev().find(|e| e.role == "agent").map(|e| e.text.clone()).unwrap_or_default();
            (thread, message)
        };
        let context = if !message.trim().is_empty() {
            vec![Ctx { role: "assistant".into(), text: message.clone() }]
        } else if !origin.transcript_path.is_empty() {
            transcript::recent_context(&origin.transcript_path, 2)
        } else {
            vec![]
        };
        let mut st = self.store.lock().unwrap();
        st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
        let mut it = Self::new_item(&mut st, "waiting", origin.clone());
        it.thread = thread;
        it.context = context;
        it.message = message;
        st.items.push(it);
        drop(st);
        self.changed();
    }

    /// When a session was starred; 0 if it isn't.
    pub fn session_star(&self, session_id: &str) -> u64 {
        self.store.lock().unwrap().sessions.all().iter().find(|s| s.origin.session_id == session_id).map(|s| s.starred_ms).unwrap_or(0)
    }

    pub fn restore_star(&self, session_id: &str, starred_ms: u64) {
        self.store.lock().unwrap().sessions.restore_star(session_id, starred_ms);
        self.changed();
    }

    /// The name a session shows under (yours, or the one it gave itself); "" if it has none.
    pub fn session_name(&self, session_id: &str) -> String {
        self.store.lock().unwrap().sessions.all().into_iter().find(|s| s.origin.session_id == session_id).map(|s| s.name).unwrap_or_default()
    }

    /// A session Cue just started ("+ New session"): known from the start, so it opens in Active and
    /// takes what you type before it has said anything. With a first message it's already working.
    pub fn started(&self, origin: Origin, message: &str, asks_trust: bool) {
        {
            let mut st = self.store.lock().unwrap();
            let sid = origin.session_id.clone();
            let msg = message.trim();
            st.sessions.mark(&origin, if msg.is_empty() { "waiting" } else { "working" }, (!msg.is_empty()).then_some(msg));
            if asks_trust {
                st.sessions.set_trust(&sid, now_ms());
            }
            if !msg.is_empty() {
                st.sessions.note(&sid, "you", msg, crate::config::context_keep());
            }
        }
        self.changed();
    }

    /// An older session Cue just resumed (or one it takes in): known at once, waiting for you (or working),
    /// with its name and the last few messages of its conversation so Active shows where it left off.
    pub fn resumed(&self, origin: Origin, name: &str, recent: &[Ctx], working: bool) {
        {
            let mut st = self.store.lock().unwrap();
            let sid = origin.session_id.clone();
            st.sessions.mark(&origin, if working { "working" } else { "waiting" }, None);
            if !name.is_empty() {
                st.sessions.set_name(&sid, name);
            }
            for c in recent {
                st.sessions.note(&sid, if c.role == "user" { "you" } else { "agent" }, &c.text, crate::config::context_keep());
            }
        }
        self.changed();
    }

    /// Open a live Claude session Cue hasn't heard from yet (one you resumed in a terminal after a restart,
    /// say) like any other: its terminal from its process, its last messages from its transcript. Its
    /// hooks take over from the next thing it does.
    pub fn adopt(&self, session_id: &str) -> Result<(), String> {
        if self.store.lock().unwrap().sessions.origin(session_id).is_some() {
            return Ok(());
        }
        let q = crate::live::claude().into_iter().find(|q| q.session_id == session_id).ok_or("that session is gone")?;
        let origin = crate::live::origin_of(&q);
        let path = origin.transcript_path.clone();
        let recent = if path.is_empty() { vec![] } else { crate::transcript::recent_context(&path, 6) };
        // Its name comes from Claude Code's registry, as for every session.
        self.resumed(origin.clone(), "", &recent, q.status == "busy");
        // In Waiting as if Cue had been there all along (no notification: it isn't new). A prompt open in its
        // terminal (one Cue missed): a card that sends you there.
        if q.status == "waiting" {
            self.terminal_ask(origin, waiting_words(&q.waiting_for), false);
        } else if q.status == "idle" && crate::leads::resolve_driver(session_id, "").is_empty() {
            // Its last turn ended with its reply and nothing from you since: your turn. (One a lead drives
            // is the lead's to pick up.)
            if let Some((at, reply)) = (!path.is_empty()).then(|| transcript::turn_ended(&path)).flatten() {
                self.your_turn_since(origin, reply, at);
            }
        }
        Ok(())
    }

    /// A finished turn Cue missed, as a "your turn" card dated when it ended.
    fn your_turn_since(&self, origin: Origin, reply: String, at: u64) {
        let sid = origin.session_id.clone();
        {
            let mut st = self.store.lock().unwrap();
            if st.items.iter().any(|i| i.origin.session_id == sid) {
                return;
            }
            st.sessions.mark(&origin, "waiting", None);
            let thread = st.sessions.thread(&sid);
            let mut it = Self::new_item(&mut st, "waiting", origin);
            it.thread = thread;
            it.context = vec![Ctx { role: "assistant".into(), text: reply.clone() }];
            it.message = reply;
            it.created_ms = at;
            st.items.push(it);
        }
        self.changed();
    }

    /// Every few seconds, from Claude Code's list of live sessions:
    /// - take in any session Cue doesn't know yet (resumed after a restart, started while Cue was closed),
    ///   so it shows on the Board with its chat from the start, not only once it has sent Cue something;
    /// - a prompt open in a session's terminal that Cue has no card for (an MCP server asking for input,
    ///   a teammate's permission prompt, a dialog: they don't come through Cue's permission hook) gets an
    ///   "asking in its terminal" card, once it has been open a few seconds;
    /// - that card goes once Claude Code says the session stopped waiting (you answered it there).
    pub fn sync_live(&self) {
        let live = crate::live::claude();
        let now = now_ms();
        let (fresh, answered, asking) = {
            let st = self.store.lock().unwrap();
            let fresh: Vec<String> = live.iter().map(|q| q.session_id.clone()).filter(|sid| st.sessions.origin(sid).is_none()).collect();
            let answered: Vec<String> = live.iter().filter(|q| q.status != "waiting")
                .filter(|q| st.items.iter().any(|i| i.kind == "terminal" && i.origin.session_id == q.session_id && q.since_ms > i.created_ms))
                .map(|q| q.session_id.clone()).collect();
            // Not one Cue has a card for, and not one you just answered in Cue (the list can lag a moment).
            let answered_here = |sid: &str| st.history.iter().any(|i| i.origin.session_id == sid && i.resolved_ms.is_some_and(|r| now.saturating_sub(r) < 6_000));
            let asking: Vec<(Origin, String)> = live.iter()
                .filter(|q| q.status == "waiting" && now.saturating_sub(q.since_ms) > WAITING_GRACE_MS)
                .filter(|q| !st.items.iter().any(|i| i.origin.session_id == q.session_id && i.kind != "waiting") && !answered_here(&q.session_id))
                .filter_map(|q| st.sessions.origin(&q.session_id).map(|o| (o, waiting_words(&q.waiting_for))))
                .collect();
            (fresh, answered, asking)
        };
        for sid in answered {
            self.clear_terminal_asks(&sid);
        }
        for (origin, what) in asking {
            self.terminal_ask(origin, what, true);
        }
        for sid in fresh {
            let _ = self.adopt(&sid);   // gone meanwhile: nothing to take in
        }
    }

    /// Whether a session can be closed from Cue without losing anything: not while it's working
    /// (its turn would be cut off) or asking you something (the question would go unanswered).
    pub fn closable(&self, session_id: &str) -> Result<(), String> {
        let st = self.store.lock().unwrap();
        match st.sessions.state(session_id).as_deref() {
            Some("working") => return Err("it's working: stop it first, or let it finish".into()),
            Some("deciding") => return Err("it's asking you something: answer that first".into()),
            _ => {}
        }
        if st.items.iter().any(|i| i.origin.session_id == session_id && i.kind != "waiting") {
            return Err("it's asking you something: answer that first".into());
        }
        drop(st);
        if crate::live::claude().iter().any(|q| q.session_id == session_id && q.status == "busy") {
            return Err("it's working: stop it first, or let it finish".into());
        }
        Ok(())
    }

    /// Where a live session runs, for "Go to tab" in the Sessions view: one Cue has heard from, a
    /// Pi session that only subscribed, or a quiet Claude session (its terminal, from its process).
    pub fn live_origin(&self, session_id: &str) -> Option<Origin> {
        let st = self.store.lock().unwrap();
        if let Some(o) = st.sessions.origin(session_id).or_else(|| st.subscribed.get(session_id).map(|(o, _)| o.clone())) {
            return Some(o);
        }
        drop(st);
        let q = crate::live::claude().into_iter().find(|q| q.session_id == session_id)?;
        Some(Origin { session_id: q.session_id, harness: q.harness, cwd: q.cwd, tty: crate::live::tty_of(q.pid), agent_pid: Some(q.pid), ..Default::default() })
    }

    pub fn drop_session(&self, session_id: &str) {
        let removed = self.store.lock().unwrap().sessions.remove(session_id);
        if removed {
            self.changed();
        }
    }

    pub fn snapshot(&self) -> Value {
        let mut st = self.store.lock().unwrap();
        let reviews = Self::reviews_view(&mut st);
        // The Sessions view: live sessions Cue hasn't heard from yet (quiet Claude ones from its
        // registry, Pi ones that only subscribed), and each folder's git branch.
        let known: HashSet<String> = st.sessions.all().iter().map(|s| s.origin.session_id.clone()).chain(st.items.iter().map(|i| i.origin.session_id.clone())).collect();
        let pi = st.subscribed.values().map(|(o, at)| crate::live::Quiet {
            session_id: o.session_id.clone(),
            harness: o.harness.clone(),
            name: String::new(),
            cwd: o.cwd.clone(),
            pid: o.agent_pid.unwrap_or(0),
            status: "idle".into(),
            waiting_for: String::new(),
            since_ms: *at,
        });
        let live: Vec<crate::live::Quiet> = crate::live::claude().into_iter().chain(pi).filter(|q| !known.contains(&q.session_id)).collect();
        // What each Claude session is about (AI title, your latest request), from its transcript: no model.
        let about: HashMap<String, Value> = st.sessions.all().iter().filter(|s| s.origin.harness == "claude").map(|s| (s.origin.session_id.clone(), s.origin.transcript_path.clone()))
            .chain(live.iter().filter(|q| q.harness == "claude").filter_map(|q| crate::live::claude_transcript(&q.cwd, &q.session_id).map(|p| (q.session_id.clone(), p))))
            .filter_map(|(sid, path)| { let (title, prompt) = crate::live::about(&path); (!title.is_empty() || !prompt.is_empty()).then(|| (sid, json!({ "title": title, "prompt": prompt }))) })
            .collect();
        let branches: HashMap<String, String> = st.sessions.all().iter().map(|s| s.origin.cwd.clone()).chain(live.iter().map(|q| q.cwd.clone())).filter(|c| !c.is_empty()).map(|c| { let b = crate::live::branch(&c); (c, b) }).collect();
        json!({
            "items": st.items,
            "history": st.history,
            // Every answer of the last day, in brief: today's numbers count these, not the History list.
            "answers": st.answers,
            "sessions": with_registry_names(st.sessions.snapshot()),
            "reviews": reviews,
            "crew": crate::leads::view(&known.iter().cloned().chain(live.iter().map(|q| q.session_id.clone())).collect()),
            "live": live,
            "branches": branches,
            "about": about,
            "window_ms": crate::sessions::WINDOW_MS,
            "now_ms": now_ms(),
            "settings": crate::config::effective(),
            "connections": crate::config::connections(),
            "usage": st.rates,
            "usage_file": crate::usage_file::read(),
        })
    }

    /// Review, tapped: type `text` into the lead now. Only one review at a time per lead (two typed into
    /// a busy lead can run together as one message), so while it's busy this refuses and says with what;
    /// you tap Review again once it's free. Cue never types a review you didn't just ask for.
    pub fn review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        {
            let mut st = self.store.lock().unwrap();
            if st.review_sent.contains_key(exec) {
                return Err("its lead is already reviewing it".into());
            }
            if let Some(why) = Self::lead_busy(&st, lead, now_ms()) {
                return Err(format!("its lead is {why}: tap Review again when it's done"));
            }
            st.review_sent.insert(exec.to_string(), (lead.to_string(), now_ms()));
            st.review_last.insert(lead.to_string(), now_ms());
        }
        self.changed(); // "in review" shows now; typing it in takes a moment
        match self.send_to_session(lead, text, &[], false) {
            Ok(_) => Ok(format!("Sent {text}")),
            Err(e) => {
                self.store.lock().unwrap().review_sent.remove(exec);
                self.changed();
                Err(e)
            }
        }
    }

    /// Why a lead can't take a review right now, or None when it's free: working (a review or anything
    /// else), compacting, asking you something, or a review just typed in (it takes a moment to show as working).
    fn lead_busy(st: &Store, lead: &str, now: u64) -> Option<String> {
        let reviewing = || st.review_sent.iter().filter(|(_, (l, _))| l == lead).max_by_key(|(_, (_, at))| *at).map(|(e, _)| crate::leads::name_of(e));
        let settling = now.saturating_sub(st.review_last.get(lead).copied().unwrap_or(0)) < REVIEW_SETTLE_MS;
        if settling || st.sessions.state(lead).as_deref() == Some("working") {
            return Some(reviewing().map_or_else(|| "busy".to_string(), |n| format!("reviewing {n}")));
        }
        if st.sessions.compacting_since(lead) > 0 {
            return Some("compacting".into());
        }
        if st.items.iter().any(|i| i.origin.session_id == lead && i.kind != "waiting") {
            return Some("asking you something".into());
        }
        None
    }

    /// For every screen: each executor whose review you sent, executor → "in review".
    fn reviews_view(st: &mut Store) -> Value {
        let now = now_ms();
        st.review_sent.retain(|exec, (_, at)| now.saturating_sub(*at) < REVIEW_SHOWN_MS && crate::leads::is_done(exec));
        Value::Object(st.review_sent.keys().map(|e| (e.clone(), json!("in review"))).collect())
    }

    /// Change one setting. History size applies right away (memory and file are trimmed).
    pub fn set_setting(&self, key: &str, value: Value) -> Result<(), String> {
        crate::config::set(key, value)?;
        if key == "quick.phrases" {
            crate::notify::set_phrases(&crate::config::quick_phrases());
        }
        if key == "history.keep" {
            let history = crate::db::history(crate::config::history_keep());
            self.store.lock().unwrap().history = history;
        }
        self.changed();
        Ok(())
    }

    /// Rename a session: Cue shows the name at once, and the agent is told in its own way. Claude Code:
    /// "/rename <name>" typed at its prompt (deferred to the end of the turn if it's busy). Pi: through
    /// Cue's extension, nothing typed. Codex: Cue only. Returns what happened, for a toast. Blocking.
    pub fn rename(&self, session_id: &str, name: &str) -> Result<String, String> {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
        let (harness, state, origin) = self.store.lock().unwrap().sessions.set_name(session_id, &name).ok_or("that session is gone")?;
        self.changed();
        if name.is_empty() {
            return Ok("Name cleared in Cue".into());
        }
        match harness.as_str() {
            "pi" => {
                let sent = self.store.lock().unwrap().subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "rename", "name": name })).is_ok()).unwrap_or(false);
                Ok(if sent { "Renamed in Cue and Pi".into() } else { "Renamed in Cue (Pi isn't connected to Cue right now)".into() })
            }
            "claude" if state == "working" => {
                self.store.lock().unwrap().sessions.defer_rename(session_id);
                Ok("Renamed in Cue · Claude Code gets it when this turn ends".into())
            }
            "claude" => crate::focus::type_into(&origin, &format!("/rename {name}")).map(|_| "Renamed in Cue and Claude Code".into()),
            _ => Ok("Renamed in Cue".into()),
        }
    }

    /// A Claude Code turn just ended: type the rename it missed while busy (after its Stop hooks settle).
    /// Type one of the agent's own commands into its session (Compact: `/compact`; a relay lead's Hand off:
    /// `/relay:handoff`). Not mid-turn: typed then, it would land in the middle of its work.
    pub fn session_command(&self, session_id: &str, action: &str) -> Result<String, String> {
        let (origin, state) = {
            let st = self.store.lock().unwrap();
            let s = st.sessions.all().into_iter().find(|s| s.origin.session_id == session_id).ok_or("that session is gone")?;
            (s.origin.clone(), s.state.clone())
        };
        if state == "working" {
            return Err("It's working: do this when its turn ends".into());
        }
        let line = match action {
            "compact" => "/compact",
            "handoff" => {
                let lead = crate::leads::view(&std::iter::once(session_id.to_string()).collect()).get(session_id).is_some_and(|m| m["role"] == "lead" && m["plugin"] == "relay");
                if !lead {
                    return Err("only a relay lead can hand off".into());
                }
                "/relay:handoff"
            }
            _ => return Err(format!("Cue doesn't know \"{action}\"")),
        };
        crate::focus::type_into(&origin, line)?;
        if action == "compact" {
            self.store.lock().unwrap().sessions.set_compacting(session_id, now_ms());
            self.changed();
        }
        Ok(match action {
            "compact" => "Compacting: it summarizes the conversation to free up context".into(),
            _ => "Handing off: it writes its notes, opens its successor, then steps down".into(),
        })
    }

    /// This hub as the app holds it, for work handed to another thread (none in tests: no app).
    fn shared(&self) -> Option<Arc<Hub>> {
        self.app.as_ref().map(|a| a.state::<Arc<Hub>>().inner().clone())
    }

    /// A turn began: note the folder's git state (in the background: git can take a moment) to tell
    /// later what this turn changed.
    fn turn_started(&self, session_id: &str, cwd: &str) {
        let (Some(me), sid, cwd) = (self.shared(), session_id.to_string(), cwd.to_string()) else { return };
        std::thread::spawn(move || {
            let base = crate::changes::base(&cwd);
            me.store.lock().unwrap().sessions.set_turn_base(&sid, base);
        });
    }

    /// A turn ended: what it changed, for the "4 files · +98 −9" pill and Commit.
    fn turn_changes(&self, session_id: &str) {
        let Some(base) = self.store.lock().unwrap().sessions.take_turn_base(session_id) else { return };
        let (Some(me), sid) = (self.shared(), session_id.to_string()) else { return };
        std::thread::spawn(move || {
            let changes = crate::changes::since(&base);
            if me.store.lock().unwrap().sessions.set_changes(&sid, changes) {
                me.changed();
            }
        });
    }

    /// Compact from a notification (it names the card; Compact goes to its session).
    pub fn compact_for_item(&self, id: &str) -> Result<String, String> {
        let sid = self.get(id).map(|i| i.origin.session_id).ok_or("that card is gone")?;
        self.session_command(&sid, "compact")
    }

    fn deliver_deferred_rename(&self, session_id: &str) {
        if let Some((name, origin)) = self.store.lock().unwrap().sessions.take_rename(session_id) {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = crate::focus::type_into(&origin, &format!("/rename {name}"));
            });
        }
    }

    /// Every second: what each working Claude Code session is doing now, read from the end of its
    /// transcript (no hook involved, so it costs the agent nothing).
    pub fn refresh_activity(&self) {
        let todo = self.store.lock().unwrap().sessions.working_transcripts();
        let found: Vec<(String, (String, u64))> =
            todo.into_iter().filter_map(|(sid, harness, path)| crate::transcript::activity_of(&harness, &path).map(|a| (sid, a))).collect();
        // A message you queued that Claude Code or Codex has since taken (into the running turn, or as
        // the next prompt): it's no longer queued, so no "Queued" bubble pinned below the answer, and no Send now.
        let queued: Vec<(String, String, String, crate::model::Exchange)> = {
            let st = self.store.lock().unwrap();
            st.sessions.working_transcripts().into_iter().filter_map(|(sid, h, path)| st.sessions.queued(&sid).map(|q| (sid, h, path, q))).collect()
        };
        // Interrupted (Esc, on purpose or not) a few seconds ago with nothing since: no Stop comes for
        // that, so it lands in Waiting as "interrupted", with Continue. A few seconds: ⌘Enter's Esc
        // is followed at once by your message.
        let now = now_ms();
        let stuck: Vec<Origin> = {
            let st = self.store.lock().unwrap();
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness == "claude" && !s.origin.transcript_path.is_empty())
                .filter(|s| transcript::interrupted_at(&s.origin.transcript_path).is_some_and(|at| at + 1500 >= s.since_ms && now.saturating_sub(at) > 3000))
                .map(|s| s.origin).collect()
        };
        for o in stuck {
            let sid = o.session_id.clone();
            let driver = crate::leads::resolve_driver(&sid, "");
            self.event(o, "stopped", "Interrupted: it stopped mid-turn and is waiting for you. Say “continue” to pick up where it left off.".into(), vec![], &driver);
            if let Some(it) = self.store.lock().unwrap().items.iter_mut().find(|i| i.kind == "waiting" && i.origin.session_id == sid) {
                it.interrupted = true;
            }
            self.changed();
        }
        // Stuck: working, but nothing new in its transcript for 10 minutes while it runs a command (a long
        // build is fine; one waiting for input in its terminal isn't), or 5 while it's only thinking. Not
        // while a helper agent works (that writes to its own log). Said once per quiet spell.
        let quiet: Vec<(String, u64, String, u64, String)> = {
            let st = self.store.lock().unwrap();
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness != "pi" && !s.origin.transcript_path.is_empty())
                .filter_map(|s| {
                    let at = std::fs::metadata(&s.origin.transcript_path).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
                    let name = if s.name.is_empty() { s.project.clone() } else { s.name.clone() };
                    Some((s.origin.session_id.clone(), at.max(s.since_ms), s.doing.clone(), s.stuck_ms, name))
                }).collect()
        };
        for (sid, at, doing, was, name) in quiet {
            let limit = if doing.starts_with("Delegating") { u64::MAX } else if doing.starts_with("Running") || doing.starts_with("Using") { STUCK_RUNNING_MS } else { STUCK_THINKING_MS };
            let stuck = now.saturating_sub(at) >= limit;
            if stuck && was != at {
                self.store.lock().unwrap().sessions.set_stuck(&sid, at);
                self.changed();
                if crate::config::flag("/notify/decisions") {
                    let what = if doing.is_empty() { "Thinking".to_string() } else { doing.trim_end_matches('…').to_string() };
                    self.notify(&format!("stuck-{sid}"), &format!("{name} · no new output for {}m", now.saturating_sub(at) / 60_000), &format!("{what}. If it's waiting for input, it's in its terminal."), "");
                }
            } else if !stuck && was != 0 {
                self.store.lock().unwrap().sessions.set_stuck(&sid, 0);
                crate::notify::remove(&[format!("stuck-{sid}")]);
                self.changed();
            }
        }
        // Compacting (from Cue, its terminal, or on its own) until its transcript says it's done; given up after 15 minutes.
        let compacted: Vec<(String, bool)> = {
            let st = self.store.lock().unwrap();
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
            let mut st = self.store.lock().unwrap();
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
        // A session Cue started in a folder Claude Code didn't trust yet: asking in its terminal until
        // ~/.claude.json says it's trusted (answered there, or with Trust in Cue). Given up after 30 minutes.
        let done: Vec<String> = {
            let asking = self.store.lock().unwrap().sessions.asking_trust();
            asking.into_iter().filter(|(_, cwd, at)| now.saturating_sub(*at) > 30 * 60_000 || crate::focus::claude_trusts(cwd)).map(|(sid, _, _)| sid).collect()
        };
        if !done.is_empty() {
            let mut st = self.store.lock().unwrap();
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
            let mut st = self.store.lock().unwrap();
            for sid in &taken {
                st.sessions.set_queued(sid, None);
            }
            found.iter().fold(!taken.is_empty(), |any, (sid, (a, at))| st.sessions.set_doing(sid, a, *at) || any)
        };
        if changed {
            self.changed();
        }
    }

    /// A prompt you sent the session (not a background agent's notification): it shows as yours
    /// in the chat and labels what the session is working on.
    pub fn prompt_from_you(&self, session_id: &str, message: &str) {
        if message.trim().is_empty() {
            return;
        }
        {
            let mut st = self.store.lock().unwrap();
            let message = crate::sessions::unwrap_pasted(message);
            st.sessions.set_prompt(session_id, &message);
            st.sessions.note(session_id, "you", &message, crate::config::context_keep());
        }
        self.changed();
    }

    /// A message another agent session sent this one: shown in its chat as from that session.
    pub fn prompt_from_peer(&self, session_id: &str, name: &str, pid: Option<i32>, body: &str) {
        if body.trim().is_empty() {
            return;
        }
        {
            let mut st = self.store.lock().unwrap();
            // The project Cue shows for the sender, else Claude's own name for it.
            let who = pid.and_then(|p| st.sessions.project_by_pid(p)).unwrap_or_else(|| if name.is_empty() { "another agent".into() } else { name.to_string() });
            st.sessions.note_peer(session_id, &who, body, crate::config::context_keep());
        }
        self.changed();
    }

    /// Something outside the store changed what the window shows (who leads whom): redraw only.
    pub fn redraw(&self) {
        if let Some(app) = &self.app {
            let _ = app.emit("state", self.snapshot());
        }
    }

    /// Cue just started: show what was already waiting (menu bar icon, Dock badge) now,
    /// not at the next change.
    pub fn show_waiting(&self) {
        self.changed();
    }

    fn changed(&self) {
        let Some(app) = &self.app else { return };
        {
            let st = self.store.lock().unwrap();
            let waiting: Vec<&Item> = st.items.iter().filter(|i| i.kind == "waiting").collect();
            crate::db::sync_live(&st.sessions.all(), &waiting);
        }
        let _ = app.emit("state", self.snapshot());
        let (decisions, items, answered) = {
            let mut st = self.store.lock().unwrap();
            let live: HashSet<String> = st.items.iter().map(|i| i.id.clone()).collect();
            // Answered anywhere (Cue, menu bar, the terminal): take the notification back.
            let answered: Vec<String> = st.notified.difference(&live).cloned().collect();
            for id in &answered {
                st.notified.remove(id);
            }
            (st.items.iter().filter(|i| i.kind != "waiting").count(), st.items.clone(), answered)
        };
        crate::notify::remove(&answered);
        if std::env::var_os("CUE_QUIET").is_none() {
            crate::tray::sync(app, &items);
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_badge_count(if decisions > 0 { Some(decisions as i64) } else { None });
        }
    }

    /// `category`: "turn" puts Reply and the quick phrases on it; "turn-full" adds Compact.
    fn notify(&self, id: &str, title: &str, body: &str, category: &str) {
        let Some(app) = &self.app else { return };
        // Test instances run with CUE_QUIET=1 so they never ping you.
        if std::env::var_os("CUE_QUIET").is_some() {
            return;
        }
        // Cue is the window in front of you: the new item is already on screen in Waiting.
        if app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false)) {
            return;
        }
        if crate::notify::post(id, title, body, category) {
            self.store.lock().unwrap().notified.insert(id.to_string());
        } else {
            // Unbundled dev binary: a plain notification Cue can't take back.
            let _ = app.notification().builder().title(title).body(body).show();
        }
    }
}

// ---------- history: saved to cue.db the moment it's answered ----------

fn push_history(st: &mut Store, it: Item) {
    crate::db::save_answered(&it);
    let day_ago = now_ms().saturating_sub(crate::db::DAY_MS);
    st.answers.retain(|a| a.resolved_ms >= day_ago);
    st.answers.insert(0, Answer::of(&it));
    st.history.push_front(it);
    st.history.truncate(crate::config::history_keep());
}

/// Pick what a finished card shows. "answer" (default): the agent's final message from before any
/// Stop hook sent it back to work (not its mid-turn narration), with the hook's follow-up kept
/// separately. "last": just the very last message.
/// A Claude Code session's name as Claude Code has it now: `/rename` in its terminal updates Claude
/// Code's own list of running sessions (Cue's rename sends `/rename` too), and Cue's record may not know.
fn with_registry_names(sessions: Vec<crate::sessions::Session>) -> Value {
    let named: HashMap<String, String> = crate::live::claude().into_iter().filter(|q| !q.name.trim().is_empty()).map(|q| (q.session_id, q.name)).collect();
    let mut v = serde_json::to_value(sessions).unwrap_or_else(|_| json!([]));
    for s in v.as_array_mut().into_iter().flatten() {
        if let Some(name) = s.get("session_id").and_then(Value::as_str).and_then(|id| named.get(id)).cloned() {
            s["name"] = json!(name);
        }
    }
    v
}

pub fn split_turn(last: String, turn: &[TurnPart], mode: &str) -> (String, String) {
    if mode == "last" || turn.is_empty() {
        return (last, String::new());
    }
    let answer = turn.iter().rev().find(|p| !p.hooked && !p.text.trim().is_empty()).map(|p| p.text.trim().to_string());
    let after = turn.iter().filter(|p| p.hooked).map(|p| p.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    match answer {
        Some(a) => (a, after),
        None => (if after.is_empty() { last } else { after }, String::new()),
    }
}

fn first_line(s: &str, max: usize) -> String {
    let l = s.lines().next().unwrap_or("").trim();
    if l.chars().count() > max {
        format!("{}…", l.chars().take(max).collect::<String>())
    } else {
        l.to_string()
    }
}

/// One-line summary of what's being asked, for notifications.
pub fn summary(it: &Item) -> String {
    let i = &it.tool_input;
    let s = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match it.kind.as_str() {
        "question" => i.pointer("/questions/0/question").and_then(Value::as_str).map(String::from).unwrap_or_else(|| s("question")),
        "waiting" => it.context.last().map(|c| c.text.clone()).unwrap_or_else(|| "Finished, waiting for you".into()),
        // Asking in its terminal about a tool call Cue couldn't see: just what Claude Code said.
        "terminal" if it.tool_name.is_empty() => it.message.clone(),
        _ => match it.tool_name.to_lowercase().as_str() {
            "bash" => s("command"),
            "edit" | "write" | "multiedit" | "notebookedit" => format!("{} {}", it.tool_name, s("file_path")),
            "webfetch" => format!("Fetch {}", s("url")),
            _ => it.tool_name.clone(),
        },
    }
}

/// A prompt open this long in a terminal before Cue makes a card for it: its own permission hook (which
/// asks Cue at once) gets there first.
const WAITING_GRACE_MS: u64 = 4_000;

/// What a session waits for in its terminal, in Claude Code's words ("approve Bash", "dialog open").
fn waiting_words(waiting_for: &str) -> String {
    if waiting_for.is_empty() { "It's asking you something in its terminal".into() } else { format!("Waiting in its terminal: {waiting_for}") }
}

fn notify_title(it: &Item) -> (String, String) {
    let mut who = format!("{} · {}", it.origin.harness, if it.project.is_empty() { "?" } else { &it.project });
    if !it.origin.machine.is_empty() {
        who = format!("{who} on {}", crate::machines::display_name(&it.origin.machine));
    }
    let what = match it.kind.as_str() {
        "question" => "has a question",
        "waiting" => "finished, waiting for you",
        "terminal" => "is asking in its terminal",
        _ => "wants permission",
    };
    (format!("{who} {what}"), first_line(&summary(it), 140))
}

fn describe(d: &Decision) -> String {
    let base = match d.behavior.as_str() {
        "allow_always" => "allowed (always)".to_string(),
        "allow" if d.answers.is_some() => "answered".to_string(),
        "allow" => "allowed".to_string(),
        "deny" => "denied".to_string(),
        other => other.to_string(),
    };
    let mut out = base;
    if let Some(Value::Object(a)) = &d.answers {
        let parts: Vec<String> = a.values().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect();
        out = format!("{out}: {}", parts.join("; "));
    }
    if let Some(m) = &d.message {
        if !m.trim().is_empty() {
            out = format!("{out}: “{}”", first_line(m, 200));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lead_just_handed_a_review_is_busy_until_it_settles() {
        let mut st = Store::default();
        let t = 1_000_000;
        assert_eq!(Hub::lead_busy(&st, "L", t), None);
        st.review_sent.insert("E1".into(), ("L".into(), t));
        st.review_last.insert("L".into(), t);
        assert_eq!(Hub::lead_busy(&st, "L", t + 1000), Some("reviewing E1".into()));
        assert_eq!(Hub::lead_busy(&st, "L", t + REVIEW_SETTLE_MS + 1), None);
        assert_eq!(Hub::lead_busy(&st, "OTHER", t + 1000), None);
    }

    #[test]
    fn split_turn_shows_the_answer_and_keeps_the_hook_follow_up_aside() {
        let part = |t: &str, h: bool| TurnPart { text: t.into(), hooked: h };
        let turn = vec![part("Here's the fix.", false), part("All done, tests pass.", false), part("Checked: all tests pass.", true)];
        let (msg, follow) = split_turn("Checked: all tests pass.".into(), &turn, "answer");
        assert_eq!(msg, "All done, tests pass.", "the final answer, not the mid-turn narration");
        assert_eq!(follow, "Checked: all tests pass.");
        assert_eq!(split_turn("Checked: all tests pass.".into(), &turn, "last"), ("Checked: all tests pass.".into(), String::new()));
        // No hook in the turn: the final message is the answer.
        let plain = vec![part("a", false), part("b", false)];
        assert_eq!(split_turn("b".into(), &plain, "answer"), ("b".into(), String::new()));
        // Nothing before the hook: fall back to what there is.
        assert_eq!(split_turn("x".into(), &[part("x", true)], "answer"), ("x".into(), String::new()));
        // Old clients send no turn at all.
        assert_eq!(split_turn("last".into(), &[], "answer"), ("last".into(), String::new()));
    }

    #[test]
    fn a_rebuilt_session_gets_its_waiting_card_back_from_its_saved_thread() {
        let dir = std::env::temp_dir().join(format!("cue-wcard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin.clone(), "stopped", "Here's the fix.".into(), vec![], "");
        assert_eq!(hub.pending().len(), 1, "the finished turn's card is up");
        // The reboot: the pane came back fresh, and the card comes back from the saved thread.
        let fresh = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), tmux_pane: "%9".into(), ..Default::default() };
        hub.mark_state(&fresh, "waiting");
        hub.waiting_card(&fresh);
        let items = hub.pending();
        assert_eq!(items.len(), 1, "still one card: it follows the session into its new pane");
        assert_eq!((items[0].kind.as_str(), items[0].message.as_str()), ("waiting", "Here's the fix."));
        assert_eq!(items[0].origin.tmux_pane, "%9");
        assert!(!items[0].thread.is_empty(), "the card keeps the saved thread");
    }

    #[test]
    fn sending_a_waiting_card_to_the_back_reorders_it_without_reaging_it() {
        let dir = std::env::temp_dir().join(format!("cue-back-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin, "stopped", "Here's the fix.".into(), vec![], "");
        let it = hub.pending().remove(0);
        assert!(it.back_ms.is_none());
        let created = it.created_ms;
        hub.send_to_back(&it.id).unwrap();
        let it = hub.pending().remove(0);
        assert!(it.back_ms.is_some(), "the card now sorts by its back_ms");
        assert_eq!(it.created_ms, created, "created_ms untouched: the row's age stays honest");
        // A card that's not a waiting turn can't be sent to back, and neither can a missing one.
        hub.finish(&it.id, "answered", "replied");
        assert!(hub.send_to_back(&it.id).is_err(), "an answered card isn't waiting any more");
        assert!(hub.send_to_back("nope").is_err());
    }

    #[test]
    fn a_prompt_cue_cant_answer_is_a_card_until_the_session_moves_on() {
        let dir = std::env::temp_dir().join(format!("cue-term-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let path = dir.join("t.jsonl");
        std::fs::write(&path, r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"Bash","input":{"command":"curl -sI https://example.com"}}]}}"#.to_string() + "\n").unwrap();
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), transcript_path: path.to_string_lossy().into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin.clone(), "terminal_ask", "A sandboxed command needs network access".into(), vec![], "");
        let items = hub.pending();
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].kind.as_str(), summary(&items[0]).as_str()), ("terminal", "curl -sI https://example.com"));
        assert_eq!(items[0].tool_use_id.as_deref(), Some("tu1"), "the watcher clears it once that call gets its result");
        // Asked again while it's up: still one card.
        hub.event(origin.clone(), "terminal_ask", "A sandboxed command needs network access".into(), vec![], "");
        assert_eq!(hub.pending().len(), 1);
        // The turn ends: the prompt was answered; it's your turn now.
        hub.event(origin, "stopped", "Done.".into(), vec![], "");
        let kinds: Vec<String> = hub.pending().iter().map(|i| i.kind.clone()).collect();
        assert_eq!(kinds, ["waiting"]);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_prompt_open_in_its_terminal_gets_a_card_until_it_stops_waiting() {
        let dir = std::env::temp_dir().join(format!("cue-waiting-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s4".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin, "active", "use the docs server".into(), vec![], "");
        let q = |status: &str, since_ms: u64| crate::live::Quiet { session_id: "s4".into(), harness: "claude".into(), name: String::new(), cwd: String::new(), pid: std::process::id() as i32, status: status.into(), waiting_for: "input needed".into(), since_ms };
        // Just opened: its own permission hook may still be on the way. No card yet.
        crate::live::set_claude(vec![q("waiting", now_ms())]);
        hub.sync_live();
        assert!(hub.pending().is_empty());
        // Open a while, and Cue has nothing for it: a card that sends you to the terminal.
        crate::live::set_claude(vec![q("waiting", now_ms() - 10_000)]);
        hub.sync_live();
        let items = hub.pending();
        assert_eq!((items.len(), items[0].kind.as_str(), items[0].message.as_str()), (1, "terminal", "Waiting in its terminal: input needed"));
        hub.sync_live();
        assert_eq!(hub.pending().len(), 1, "one card, however often it's seen");
        // Answered in the terminal: Claude Code says it's busy again, after the card came.
        crate::live::set_claude(vec![q("busy", now_ms() + 1)]);
        hub.sync_live();
        assert!(hub.pending().iter().all(|i| i.kind != "terminal"));
        crate::live::set_claude(vec![]);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_message_sent_while_it_works_is_kept_in_cue_until_taken_back() {
        let dir = std::env::temp_dir().join(format!("cue-held-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        // No terminal to type into: anything typed would fail, so a pass means nothing was typed.
        let origin = Origin { session_id: "s5".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin, "active", "refactor the router".into(), vec![], "");
        assert!(hub.send_to_session("s5", "also keep the old session", &[], false).unwrap().starts_with("kept in Cue"));
        hub.send_to_session("s5", "and run the tests", &[], false).unwrap();
        let held = |h: &Hub| h.store.lock().unwrap().sessions.held("s5");
        assert_eq!(held(&hub).map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()), "one kept message, the second after the first");
        assert!(hub.store.lock().unwrap().sessions.thread("s5").iter().all(|e| e.role != "you" || e.text == "refactor the router"), "not in the chat as sent");
        // Esc / Edit: back to you, and nothing kept any more.
        assert_eq!(hub.unhold("s5").map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()));
        assert!(held(&hub).is_none() && hub.unhold("s5").is_none());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_turn_cue_missed_waits_from_when_it_ended_once() {
        let dir = std::env::temp_dir().join(format!("cue-missed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s3".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.your_turn_since(origin.clone(), "Done: header moved.".into(), 1_000);
        hub.your_turn_since(origin, "Done: header moved.".into(), 1_000);
        let items = hub.pending();
        assert_eq!(items.len(), 1, "one card, however often it's seen");
        assert_eq!((items[0].kind.as_str(), items[0].created_ms, items[0].message.as_str()), ("waiting", 1_000, "Done: header moved."));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn later_is_kept_with_the_session_and_its_finished_turns_still_wait() {
        let dir = std::env::temp_dir().join(format!("cue-later-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s2".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin.clone(), "stopped", "First.".into(), vec![], "");
        hub.set_later("s2", true);
        let later_ms = |h: &Hub| h.store.lock().unwrap().sessions.all().iter().find(|s| s.origin.session_id == "s2").map(|s| s.later_ms).unwrap();
        let at = later_ms(&hub);
        assert!(at > 0);
        hub.set_later("s2", true);
        assert_eq!(later_ms(&hub), at, "putting it off again keeps when it was first put off");
        // It finishes another turn: still put off, its card still there (the window shows it under Need to decide).
        hub.event(origin, "stopped", "Second.".into(), vec![], "");
        assert!(later_ms(&hub) > 0);
        assert_eq!(hub.pending().iter().filter(|i| i.kind == "waiting").count(), 1);
        hub.set_later("s2", false);
        assert_eq!(later_ms(&hub), 0);
        let starred_ms = |h: &Hub| h.store.lock().unwrap().sessions.all().iter().find(|s| s.origin.session_id == "s2").map(|s| s.starred_ms).unwrap();
        hub.set_starred("s2", true);
        let at = starred_ms(&hub);
        assert!(at > 0);
        hub.set_starred("s2", true);
        assert_eq!(starred_ms(&hub), at, "starring it again keeps when it was first starred");
        hub.set_starred("s2", false);
        assert_eq!(starred_ms(&hub), 0);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn todays_answers_are_all_kept_and_counted_however_short_history_is() {
        let dir = std::env::temp_dir().join(format!("cue-today-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        std::fs::write(dir.join("config.json"), r#"{"history":{"keep":10}}"#).unwrap();
        crate::db::reset();
        let now = now_ms();
        let item = |n: u64, at: u64| Item {
            id: format!("t{n}"), kind: "waiting".into(), origin: Origin::default(), project: "cue".into(),
            tool_name: String::new(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: String::new(),
            created_ms: at - 1000, status: "answered".into(), outcome: "replied".into(), resolved_ms: Some(at), thread: vec![], tool_use_id: None, scan_from: 0, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        let mut st = Store::default();
        push_history(&mut st, item(0, now - crate::db::DAY_MS - 60_000)); // yesterday, beyond the cap: goes
        for n in 1..=15 {
            push_history(&mut st, item(n, now - 60_000 + n));
        }
        assert_eq!(st.history.len(), 10, "the History list keeps its setting");
        assert_eq!(st.answers.len(), 15, "but every answer of the last day is counted");
        crate::db::reset();
        assert_eq!(crate::db::history(10).len(), 10);
        let kept = crate::db::answers_since(now - crate::db::DAY_MS);
        assert_eq!(kept.len(), 15, "the database keeps the last day's answers past the cap");
        assert_eq!(kept[0].resolved_ms, now - 60_000 + 15, "newest first");
        std::env::remove_var("CUE_HOME");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn history_keeps_the_configured_number_newest_first_and_trims_the_database() {
        let dir = std::env::temp_dir().join(format!("cue-hist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let item = |n: u64| Item {
            id: format!("h{n}"), kind: "permission".into(), origin: Origin::default(), project: String::new(),
            tool_name: "Bash".into(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: String::new(),
            created_ms: n, status: "answered".into(), outcome: "allowed".into(), resolved_ms: Some(n), thread: vec![], tool_use_id: None, scan_from: 0, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        let mut st = Store::default();
        for n in 1..=15 {
            push_history(&mut st, item(n));
        }
        assert_eq!(crate::config::history_keep(), crate::config::HISTORY_DEFAULT as usize);
        crate::config::set("history.keep", json!(10)).unwrap();
        let h = crate::db::history(crate::config::history_keep());
        assert_eq!(h.len(), 10);
        assert_eq!(h.front().unwrap().id, "h15", "newest first");
        assert_eq!(h.back().unwrap().id, "h6");
        crate::db::reset();
        assert_eq!(crate::db::history(100).len(), 10, "older rows deleted from the database too");
        std::fs::write(dir.join("config.json"), r#"{"pi":{"gate":"all"},"history":{"keep":12}}"#).unwrap();
        crate::config::set("history.keep", json!(20)).unwrap();
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        assert_eq!(cfg["pi"]["gate"], "all", "other settings untouched");
        assert_eq!(cfg["history"]["keep"], 20);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Is `text` the command `/name` (with or without arguments)?
fn is_command(text: &str, name: &str) -> bool {
    text.strip_prefix('/').and_then(|t| t.split_whitespace().next()) == Some(name)
}

/// For "/name …" typed into Claude Code: how many times it had already logged running that command,
/// so a new entry afterwards proves it ran. None for a plain message, or an agent Cue can't check.
fn command_before(origin: &Origin, text: &str) -> Option<usize> {
    let name = text.strip_prefix('/')?.split_whitespace().next()?;
    (origin.harness == "claude" && !origin.transcript_path.is_empty()).then(|| crate::transcript::command_count(&origin.transcript_path, name))
}
