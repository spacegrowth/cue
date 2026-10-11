//! Cards that wait on you: permission asks, prompts open in a terminal, finished turns; answering,
//! dismissing and reordering them, and what their notifications say.

use super::*;
use super::history::{first_line, push_history};

impl Hub {
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
            let mut st = self.st();
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
    pub(super) fn terminal_ask(&self, origin: Origin, message: String, notify: bool) {
        self.terminal_ask_with(origin, message, notify, None)
    }

    /// The same, with the dialog on its screen when Cue can read it (`dialog`, or looked up now): the card
    /// then shows its title and offers its choices as buttons, instead of only sending you to the terminal.
    pub(super) fn terminal_ask_with(&self, origin: Origin, message: String, notify: bool, dialog: Option<crate::dialog::Dialog>) {
        let sid = origin.session_id.clone();
        let path = origin.transcript_path.clone();
        let call = (!path.is_empty()).then(|| transcript::waiting_tool_use(&path)).flatten();
        let dialog = dialog.or_else(|| crate::focus::dialog_on(&origin));
        let message = match &dialog {
            Some(d) if !d.title.is_empty() => format!("Asking in its terminal: {}", d.title),
            _ => message,
        };
        let scan_from = if path.is_empty() { 0 } else { transcript::tail_offset(&path) };
        let (id, title) = {
            let mut st = self.st();
            // Cue already shows what it's asking.
            if st.items.iter().any(|i| i.kind != "waiting" && i.origin.session_id == sid) {
                return;
            }
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            let at_prompt = st.sessions.state(&sid).as_deref() == Some("waiting");
            st.sessions.mark(&origin, "deciding", None);
            let thread = st.sessions.thread(&sid);
            let mut it = Self::new_item(&mut st, "terminal", origin);
            it.thread = thread;
            it.at_prompt = at_prompt;
            it.message = message;
            it.scan_from = scan_from;
            if let Some((tool_use_id, name, input)) = call {
                it.tool_use_id = Some(tool_use_id);
                it.tool_name = name;
                it.tool_input = input;
            } else if let Some(d) = dialog {
                it.tool_input = json!({ "dialog": d });
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

    /// Pick choice `n` (1-based) of the dialog a terminal card shows: that number pressed in its pane, and
    /// Enter if the dialog is still there a moment later. Blocking: call off the UI thread.
    pub fn answer_terminal(&self, id: &str, n: usize) -> Result<String, String> {
        let it = self.get(id).filter(|i| i.status == "pending").ok_or("that card is gone")?;
        let d: crate::dialog::Dialog = serde_json::from_value(it.tool_input.get("dialog").cloned().unwrap_or(Value::Null)).map_err(|_| "that card has no choices")?;
        let label = d.options.get(n.wrapping_sub(1)).ok_or("that isn't one of its choices")?.clone();
        let o = &it.origin;
        // The screen now, not the card's copy of it: the dialog may have closed since, and a permission
        // prompt in its place has numbered choices too, so the same key there would answer that instead.
        if !crate::focus::dialog_on(o).is_some_and(|now| now.options == d.options) {
            return Err("that dialog isn't on its screen any more".into());
        }
        crate::focus::tmux_keys(o, &[n.to_string()])?;
        std::thread::sleep(std::time::Duration::from_millis(400));
        if crate::focus::dialog_on(o).is_some_and(|now| now.options == d.options) {
            crate::focus::tmux_keys(o, &["Enter".into()])?;
        }
        self.finish(id, "answered", &format!("chose {n}. {label}"));
        // A command's dialog ("/model …"): answered, it's back at its prompt, not at work.
        if it.tool_name.is_empty() && self.st().sessions.thread(&o.session_id).iter().rev().find(|e| e.role == "you").is_some_and(|e| e.text.starts_with('/')) {
            self.st().sessions.mark(o, "waiting", None);
            self.changed();
        }
        Ok(format!("chose {label}"))
    }

    pub(super) fn clear_terminal_asks(&self, session_id: &str) {
        let ids: Vec<String> = self.st().items.iter().filter(|i| i.kind == "terminal" && i.origin.session_id == session_id).map(|i| i.id.clone()).collect();
        for id in ids {
            self.finish(&id, "answered_elsewhere", "answered in the terminal");
        }
    }

    /// "Later": put a session off, or back (`on` false). Kept with the session, so anything showing
    /// Cue's state sees it.
    pub fn set_later(&self, session_id: &str, on: bool) {
        let changed = self.st().sessions.set_later(session_id, on);
        if changed {
            self.changed();
        }
    }

    /// Star a session (you're following it), or unstar it. Kept with the session, like Later; a parked
    /// one keeps it in its saved record, so it stays under Starred while parked and after it's resumed.
    pub fn set_starred(&self, session_id: &str, on: bool) {
        let changed = {
            let mut st = self.st();
            if st.sessions.set_starred(session_id, on) {
                true
            } else if let Some(p) = st.parked.iter_mut().find(|p| p.session.origin.session_id == session_id && (p.session.starred_ms > 0) != on) {
                p.session.starred_ms = if on { now_ms() } else { 0 };
                crate::db::park(p);
                true
            } else {
                false
            }
        };
        if changed {
            self.changed();
        }
    }

    /// Answer from the window (or cue-ctl). False if the item is no longer pending.
    pub fn respond(&self, id: &str, d: Decision) -> bool {
        let outcome = describe(&d);
        let tx = self.st().waiters.remove(id);
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
            let mut st = self.st();
            if let Some(tx) = st.waiters.remove(id) {
                let _ = tx.send(Reply::Cancel(outcome.to_string()));
            }
            let Some(pos) = st.items.iter().position(|i| i.id == id) else { return };
            let mut it = st.items.remove(pos);
            // An answered permission or question means the agent is back at work. A command's dialog
            // answered at its prompt starts no turn (no Stop would ever come): back to its prompt.
            if it.kind != "waiting" && status != "gone" {
                st.sessions.mark(&it.origin, if it.at_prompt { "waiting" } else { "working" }, None);
                st.sessions.note(&it.origin.session_id, "you", outcome, crate::config::context_keep());
            }
            it.status = status.into();
            it.outcome = outcome.into();
            it.resolved_ms = Some(now_ms());
            push_history(&mut st, it);
        }
        self.changed();
    }

    /// Drop a "waiting" card (or give up on a stuck request) from the window.
    pub fn dismiss(&self, id: &str) {
        let is_waiting = self.st().items.iter().any(|i| i.id == id && i.kind == "waiting");
        if is_waiting {
            self.remove_waiting(id);
        } else {
            self.finish(id, "gone", "dismissed in Cue (the agent is still asking in its terminal)");
        }
    }

    /// A waiting card goes to the end of the queue: from now on it sorts by back_ms, so every card
    /// still on its original order comes first. Its age stays when the turn finished.
    pub fn send_to_back(&self, id: &str) -> Result<(), String> {
        let mut st = self.st();
        let it = st.items.iter_mut().find(|i| i.id == id).ok_or("that card is gone")?;
        if it.kind != "waiting" || it.status != "pending" {
            return Err("that card isn't waiting".into());
        }
        it.back_ms = Some(now_ms());
        drop(st);
        self.changed();
        Ok(())
    }

    pub fn set_tool_use_id(&self, id: &str, tool_use_id: String) {
        let mut st = self.st();
        if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
            it.tool_use_id = Some(tool_use_id);
        }
    }

    pub fn remove_waiting(&self, id: &str) {
        let mut st = self.st();
        st.items.retain(|i| i.id != id);
        drop(st);
        self.changed();
    }

    /// A rebuilt session sits at its finished turn again: its waiting card comes back (the reboot
    /// lost it), built from the session's saved thread exactly as a fresh "stopped" would.
    pub fn waiting_card(&self, origin: &Origin) {
        let sid = origin.session_id.clone();
        let (thread, message) = {
            let st = self.st();
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
        let mut st = self.st();
        st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
        let mut it = Self::new_item(&mut st, "waiting", origin.clone());
        it.thread = thread;
        it.context = context;
        it.message = message;
        st.items.push(it);
        drop(st);
        self.changed();
    }

    /// Compact from a notification (it names the card; Compact goes to its session).
    pub fn compact_for_item(&self, id: &str) -> Result<String, String> {
        let sid = self.get(id).map(|i| i.origin.session_id).ok_or("that card is gone")?;
        self.session_command(&sid, "compact")
    }

    /// Cue just started: show what was already waiting (menu bar icon, Dock badge) now,
    /// not at the next change.
    pub fn show_waiting(&self) {
        self.changed();
    }
}

/// What a session waits for in its terminal, in Claude Code's words ("approve Bash", "dialog open").
pub(super) fn waiting_words(waiting_for: &str) -> String {
    if waiting_for.is_empty() { "It's asking you something in its terminal".into() } else { format!("Waiting in its terminal: {waiting_for}") }
}

pub(super) fn notify_title(it: &Item) -> (String, String) {
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
    fn answering_a_commands_dialog_at_its_prompt_leaves_it_at_its_prompt() {
        let dir = std::env::temp_dir().join(format!("cue-dlg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        let hub = Hub::new(None);
        // At its prompt, /model opens its picker: answered, no turn starts, so no Stop ever comes.
        hub.event(origin.clone(), "stopped", "Done.".into(), vec![], "");
        hub.event(origin.clone(), "terminal_ask", "Waiting in its terminal: dialog open".into(), vec![], "");
        let id = hub.pending().into_iter().find(|i| i.kind == "terminal").unwrap().id;
        hub.finish(&id, "answered", "chose 1");
        assert_eq!(hub.st().sessions.state("s1").as_deref(), Some("waiting"), "not Working: a message you send now goes straight in");
        // Mid-turn, a prompt answered means it's back at work.
        hub.event(origin.clone(), "active", "fix it".into(), vec![], "");
        hub.event(origin, "terminal_ask", "A sandboxed command needs network access".into(), vec![], "");
        let id = hub.pending().into_iter().find(|i| i.kind == "terminal").unwrap().id;
        hub.finish(&id, "answered", "allowed");
        assert_eq!(hub.st().sessions.state("s1").as_deref(), Some("working"));
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
}
