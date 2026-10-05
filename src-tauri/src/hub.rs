//! The in-memory queue. Everything that changes state goes through here, and every change
//! pushes a fresh snapshot to the window, updates the dock badge, and (for new items) notifies.

use crate::model::*;
use crate::transcript;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
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
    items: Vec<Item>,
    history: VecDeque<Item>,
    /// Every answer of the last day, newest first (History's today numbers; the list above is capped).
    answers: Vec<Answer>,
    waiters: HashMap<String, oneshot::Sender<Reply>>,
    next: u64,
    /// You closed the mini panel: keep it hidden until something new arrives.
    mini_snoozed: bool,
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
}

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
            followup: String::new(), interrupted: false,
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
            st.mini_snoozed = false;
            (id, title)
        };
        self.changed();
        if crate::config::flag("/notify/decisions") {
            self.notify(&id, &title.0, &title.1);
        }
        (id, rx)
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

    pub fn event(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str) {
        // Pi's extension says what it's doing as each tool starts ("Running: cargo test").
        if event == "doing" {
            let changed = self.store.lock().unwrap().sessions.set_doing(&origin.session_id, &message, 0);
            if changed {
                self.changed();
            }
            return;
        }
        if event == "stopped" {
            self.deliver_deferred_rename(&origin.session_id);
            // The turn is over, so whatever you queued during it has been read.
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
            }
            self.changed();
            return;
        }
        let sid = origin.session_id.clone();
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
                    let mut it = Self::new_item(&mut st, "waiting", origin);
                    it.thread = thread;
                    it.context = context;
                    it.message = message;
                    it.followup = followup;
                    let t = notify_title(&it);
                    let id = it.id.clone();
                    st.items.push(it);
                    st.mini_snoozed = false;
                    (id, t)
                };
                self.changed();
                if crate::config::flag("/notify/finished") {
                    self.notify(&id, &title.0, &title.1);
                }
            }
            "active" => {
                {
                    let mut st = self.store.lock().unwrap();
                    st.active_at.insert(sid.clone(), now_ms());
                    // Claude Code reports a message you queue while it's busy the moment it lands in its
                    // queue, not when it reads it (that's at its next step). So that's no new turn, and the
                    // message stays queued, with Send now, until the turn ends.
                    let echo = st.sessions.state(&sid).as_deref() == Some("working")
                        && st.sessions.queued_text(&sid).is_some_and(|q| !q.trim().is_empty() && message.trim().starts_with(q.trim()));
                    if !echo {
                        st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
                        // Working again, whoever started it. Whether the prompt was yours (and gets a bubble)
                        // is settled separately: see prompt_from_you.
                        st.sessions.mark(&origin, "working", None);
                        st.sessions.set_queued(&sid, None);
                    }
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
            self.notify("usage", &title, &body);
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
            self.notify("usage", &format!("{who} is back"), &paused);
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

    /// Type your reply into a finished session's terminal and confirm it submitted.
    /// Blocking (typing takes ~1-2s): call off the UI thread.
    pub fn reply(&self, id: &str, text: &str, images: &[Upload]) -> Result<String, String> {
        let it = self.get(id).filter(|i| i.status == "pending").ok_or("that card is gone")?;
        let sid = it.origin.session_id.clone();
        let t0 = now_ms();
        let saved = crate::uploads::save(images)?;
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
            st.sessions.note_with(&sid, "you", text, &paths, crate::config::context_keep());
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
            if matches!(state, Some(transcript::Queued::Absorbed | transcript::Queued::Taken)) {
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
        Ok(format!("stopped via {via}"))
    }

    /// `now`: if it's mid-turn, interrupt it first so this is handled right away (Ctrl+Enter).
    pub fn send_to_session(&self, session_id: &str, text: &str, images: &[Upload], now: bool) -> Result<String, String> {
        let typed = text; // " /…" keeps its space: a message, not a command (see with_paths)
        let text = text.trim();
        if text.is_empty() && images.is_empty() {
            return Err("nothing to send".into());
        }
        let (origin, busy) = {
            let st = self.store.lock().unwrap();
            // Its terminal is showing a permission prompt or a question: typing now would answer it.
            if st.sessions.state(session_id).as_deref() == Some("deciding") {
                return Err("it's asking you something first: answer that, then send".into());
            }
            (st.sessions.origin(session_id), st.sessions.state(session_id).as_deref() == Some("working"))
        };
        let saved = crate::uploads::save(images)?;
        // Force-send to a terminal agent: Esc first, give it a moment to stop, then type.
        let direct = self.store.lock().unwrap().subscribers.contains_key(session_id);
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
            st.sessions.note_with(session_id, "you", text, &paths, crate::config::context_keep());
            if busy {
                // It's mid-turn: the agent reads this when it finishes its current step.
                let q = crate::model::Exchange { role: "you".into(), text: text.to_string(), at_ms: now_ms(), images: paths, from: String::new(), unsent: false };
                st.sessions.set_queued(session_id, Some(q));
            }
            if !submitted {
                st.sessions.mark_unsent(session_id);
            }
        }
        self.changed();
        if !submitted {
            return Ok(format!("typed into {via}, but it didn't go through as a message. Check the tab"));
        }
        Ok(if busy { format!("queued via {via}: it reads this when it finishes its current step") } else { format!("sent via {via}") })
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
            return self.wait_active(session_id, t0, 5000);
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

    /// A session Cue just started ("+ New session"): known from the start, so it opens in Active and
    /// takes what you type before it has said anything. With a first message it's already working.
    pub fn started(&self, origin: Origin, message: &str) {
        {
            let mut st = self.store.lock().unwrap();
            let sid = origin.session_id.clone();
            let msg = message.trim();
            st.sessions.mark(&origin, if msg.is_empty() { "waiting" } else { "working" }, (!msg.is_empty()).then_some(msg));
            if !msg.is_empty() {
                st.sessions.note(&sid, "you", msg, crate::config::context_keep());
            }
        }
        self.changed();
    }

    /// An older session Cue just resumed: known at once, waiting for you, with its name and the last
    /// few messages of its conversation so Active shows where it left off.
    pub fn resumed(&self, origin: Origin, name: &str, recent: &[Ctx]) {
        {
            let mut st = self.store.lock().unwrap();
            let sid = origin.session_id.clone();
            st.sessions.mark(&origin, "waiting", None);
            if !name.is_empty() {
                st.sessions.set_name(&sid, name);
            }
            for c in recent {
                st.sessions.note(&sid, if c.role == "user" { "you" } else { "agent" }, &c.text, crate::config::context_keep());
            }
        }
        self.changed();
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
        let st = self.store.lock().unwrap();
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

    /// Change one setting. History size applies right away (memory and file are trimmed).
    pub fn set_setting(&self, key: &str, value: Value) -> Result<(), String> {
        crate::config::set(key, value)?;
        if key == "history.keep" {
            let history = crate::db::history(crate::config::history_keep());
            self.store.lock().unwrap().history = history;
        }
        self.changed();
        Ok(())
    }

    pub fn snooze_mini(&self) {
        self.store.lock().unwrap().mini_snoozed = true;
        self.changed();
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
        // A message you queued that Claude Code has since taken (into the running turn, or as the
        // next prompt): it's no longer queued, so no "Queued" bubble and no Send now.
        let queued: Vec<(String, String, String)> = {
            let st = self.store.lock().unwrap();
            st.sessions.working_transcripts().into_iter().filter(|(_, h, _)| h == "claude").filter_map(|(sid, _, path)| st.sessions.queued_text(&sid).map(|q| (sid, path, q))).collect()
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
        let taken: Vec<String> = queued.into_iter().filter(|(_, path, q)| matches!(transcript::queued_state(path, q), Some(transcript::Queued::Absorbed | transcript::Queued::Taken))).map(|(sid, _, _)| sid).collect();
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

    /// Cue just started: show what was already waiting (side panel, menu bar icon, Dock badge) now,
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
        let (decisions, total, snoozed, items, answered) = {
            let mut st = self.store.lock().unwrap();
            let live: HashSet<String> = st.items.iter().map(|i| i.id.clone()).collect();
            // Answered anywhere (Cue, side panel, menu bar, the terminal): take the notification back.
            let answered: Vec<String> = st.notified.difference(&live).cloned().collect();
            for id in &answered {
                st.notified.remove(id);
            }
            (st.items.iter().filter(|i| i.kind != "waiting").count(), st.items.len(), st.mini_snoozed, st.items.clone(), answered)
        };
        crate::notify::remove(&answered);
        if std::env::var_os("CUE_QUIET").is_none() {
            crate::tray::sync(app, &items);
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_badge_count(if decisions > 0 { Some(decisions as i64) } else { None });
        }
        if std::env::var_os("CUE_QUIET").is_none() {
            crate::mini::sync(app, total > 0 && !snoozed && crate::config::panel_enabled());
        }
    }

    fn notify(&self, id: &str, title: &str, body: &str) {
        let Some(app) = &self.app else { return };
        // Test instances run with CUE_QUIET=1 so they never ping you.
        if std::env::var_os("CUE_QUIET").is_some() {
            return;
        }
        // Cue is the window in front of you: the new item is already on screen in Waiting.
        if app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false)) {
            return;
        }
        if crate::notify::post(id, title, body) {
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
        _ => match it.tool_name.to_lowercase().as_str() {
            "bash" => s("command"),
            "edit" | "write" | "multiedit" | "notebookedit" => format!("{} {}", it.tool_name, s("file_path")),
            "webfetch" => format!("Fetch {}", s("url")),
            _ => it.tool_name.clone(),
        },
    }
}

fn notify_title(it: &Item) -> (String, String) {
    let who = format!("{} · {}", it.origin.harness, if it.project.is_empty() { "?" } else { &it.project });
    let what = match it.kind.as_str() {
        "question" => "has a question",
        "waiting" => "finished, waiting for you",
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
            created_ms: at - 1000, status: "answered".into(), outcome: "replied".into(), resolved_ms: Some(at), thread: vec![], tool_use_id: None, scan_from: 0, followup: String::new(), interrupted: false, images: vec![],
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
            created_ms: n, status: "answered".into(), outcome: "allowed".into(), resolved_ms: Some(n), thread: vec![], tool_use_id: None, scan_from: 0, followup: String::new(), interrupted: false, images: vec![],
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

/// For "/name …" typed into Claude Code: how many times it had already logged running that command,
/// so a new entry afterwards proves it ran. None for a plain message, or an agent Cue can't check.
fn command_before(origin: &Origin, text: &str) -> Option<usize> {
    let name = text.strip_prefix('/')?.split_whitespace().next()?;
    (origin.harness == "claude" && !origin.transcript_path.is_empty()).then(|| crate::transcript::command_count(&origin.transcript_path, name))
}
