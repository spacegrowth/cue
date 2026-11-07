//! A session's hook events (its turn started, ended, it's compacting, doing something, gone): each sent to
//! its handler, which notes it in the store and then tells you and the window. A turn's end: turn_end.rs.

use super::*;

impl Hub {
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
        let held = event == "stopped" && self.read(|st| st.sessions.held(&sid).is_some());
        self.on_event(origin, event, message, turn, driven_by, from_you);
        if held {
            self.send_held_soon(&sid, 0);
        }
    }

    /// Each event to its handler.
    fn on_event(&self, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str, from_you: bool) {
        match event {
            "compacting" => return self.on_compacting(&origin.session_id),
            // A prompt showing in its terminal that never reached Cue (a sandboxed command's network access).
            // ("answer_lost": Cue's own answer never reached it, so it's asking there for sure.)
            "terminal_ask" | "answer_lost" => return self.terminal_ask_with(origin, message, true, None, event == "answer_lost"),
            _ => {}
        }
        // Anything else from the session: it got past such a prompt.
        self.clear_terminal_asks(&origin.session_id);
        match event {
            "doing" => self.on_doing(&origin.session_id, &message),
            "stopped" => self.on_stopped(origin, message, &turn, driven_by),
            "active" => {
                let (message, _) = split_turn(message, &turn, &crate::config::turn_mode());
                self.on_active(origin, &message, from_you);
            }
            "ended" => self.on_ended(&origin.session_id),
            _ => {}
        }
    }

    /// Claude Code is summarizing the conversation (PreCompact: /compact typed anywhere, or its context
    /// filled up). Until it's done the session isn't waiting on you, whatever it said last.
    fn on_compacting(&self, sid: &str) {
        self.write(|st| {
            if st.sessions.compacting_since(sid) == 0 {
                st.sessions.set_compacting(sid, now_ms());
            }
        });
        self.changed();
    }

    /// Pi's extension says what it's doing as each tool starts ("Running: cargo test").
    fn on_doing(&self, sid: &str, message: &str) {
        if self.write(|st| st.sessions.set_doing(sid, message, 0)) {
            self.changed();
        }
    }

    /// A turn began (its prompt hook): working again, whoever started it.
    fn on_active(&self, origin: Origin, message: &str, from_you: bool) {
        let started = self.write(|st| Self::working_on(st, &origin, message, from_you));
        if started {
            self.turn_started(&origin.session_id, &origin.cwd, &origin.machine);
        }
        self.changed();
    }

    /// The session is at work on `message`, noted in the store; true when that's a new turn.
    fn working_on(st: &mut Store, origin: &Origin, message: &str, from_you: bool) -> bool {
        let sid = origin.session_id.as_str();
        // A task's notice or a teammate's message starts a turn too, but it's no sign that a message
        // of yours went in: `active_at` (what vouches for one) is only for prompts that could be yours.
        let notice = transcript::harness_text(message) || transcript::peer_text(message);
        if !notice {
            st.active_at.insert(sid.to_string(), now_ms());
        }
        // Claude Code reports a message you queue while it's busy the moment it lands in its
        // queue, not when it reads it (that's at its next step). So that's no new turn, and the
        // message stays queued, with Send now, until the turn ends.
        // The prompt as you sent it (a long paste comes wrapped in `<pasted_content>` tags).
        let m = crate::sessions::unwrap_pasted(message);
        let echo = st.sessions.state(sid).as_deref() == Some("working") && st.sessions.queued_text(sid).is_some_and(|q| transcript::is_message(&m, &q));
        // Whoever's prompt this turns out to be, the agent has this text: a message of yours
        // Cue was still unsure about is settled by it, however late (a hook from a machine, a
        // paste that sat in its box past Cue's wait).
        if !m.trim().is_empty() {
            st.sessions.got_message(sid, &m, "its prompt hook");
        }
        let mut started = false;
        if !echo {
            started = st.sessions.state(sid).as_deref() != Some("working");
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            // Working again, whoever started it. Whether the prompt was yours (and gets a bubble)
            // is settled separately: see prompt_from_you.
            st.sessions.mark(origin, "working", None);
            st.sessions.set_queued(sid, None);
        }
        if from_you && !m.trim().is_empty() {
            st.sessions.set_prompt(sid, &m);
            st.sessions.note(sid, "you", &m, crate::config::context_keep());
        }
        started
    }

    /// The session ended: its cards finish as gone, its row goes.
    fn on_ended(&self, sid: &str) {
        let ids: Vec<String> = self.read(|st| st.items.iter().filter(|i| i.origin.session_id == sid).map(|i| i.id.clone()).collect());
        for id in ids {
            self.finish(&id, "gone", "session ended");
        }
        self.drop_session(sid);
    }

    /// A turn began: note the folder's git state (in the background: git can take a moment) to tell
    /// later what this turn changed.
    fn turn_started(&self, session_id: &str, cwd: &str, machine: &str) {
        let (sid, cwd, machine) = (session_id.to_string(), cwd.to_string(), machine.to_string());
        self.later(0, move |me: &Arc<Hub>| {
            // A session on another machine: its folder is there, so git runs there.
            let base = if machine.is_empty() { crate::changes::base(&cwd) } else { crate::changes::base_on(&machine, &cwd) };
            me.write(|st| st.sessions.set_turn_base(&sid, base));
        });
    }

    /// A turn ended: what it changed, for the "4 files · +98 −9" pill and Commit.
    pub(super) fn turn_changes(&self, session_id: &str) {
        let Some(base) = self.write(|st| st.sessions.take_turn_base(session_id)) else { return };
        let sid = session_id.to_string();
        self.later(0, move |me: &Arc<Hub>| {
            let changes = crate::changes::since(&base);
            if me.write(|st| st.sessions.set_changes(&sid, changes)) {
                me.changed();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::transcript::iso;

    /// The prompt hook's "active" is a witness whatever else it does: it settles a message Cue had said
    /// didn't go (the session marked working with no prompt, the text wrapped as a paste), it tells a
    /// queued message's echo from a new turn through that wrapper too, and on launch Cue asks the log
    /// about whatever it was still unsure of.
    #[test]
    fn a_tasks_notice_or_a_teammates_message_never_vouches_for_your_message() {
        let dir = std::env::temp_dir().join(format!("cue-notice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let origin = Origin { session_id: "n1".into(), harness: "claude".into(), ..Default::default() };
        hub.event(origin.clone(), "stopped", "Done.".into(), vec![], "");
        let vouched = |h: &Hub| h.read(|st| st.active_at.contains_key("n1"));
        hub.event(origin.clone(), "active", "<task-notification><task-id>x</task-id></task-notification>".into(), vec![], "");
        assert!(!vouched(&hub), "a background task reporting is no sign your message went in");
        hub.event(origin.clone(), "active", "Another Claude session sent a message:\n<teammate-message teammate_id=\"r\">hi</teammate-message>".into(), vec![], "");
        assert!(!vouched(&hub), "nor is a teammate's message");
        hub.event(origin.clone(), "active", "run the tests".into(), vec![], "");
        assert!(vouched(&hub), "a prompt that could be yours does");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Stop hook that blocks the turn's end makes the agent write once more and stop again: the second
    /// Stop reports the same answer. Noted at the first end, it isn't noted again, even with a message of
    /// yours (sent as the turn seemed over) between; a new turn's same words are a reply of their own.
    #[test]
    fn a_turns_second_end_after_a_stop_hook_doesnt_note_its_answer_again() {
        let dir = std::env::temp_dir().join(format!("cue-twice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let o = Origin { session_id: "t2".into(), harness: "claude".into(), ..Default::default() };
        let part = |text: &str, hooked: bool| TurnPart { text: text.into(), hooked };
        let replies = |h: &Hub| h.read(|st| st.sessions.thread("t2")).into_iter().filter(|e| e.role == "agent").map(|e| e.text).collect::<Vec<_>>();
        hub.event(o.clone(), "stopped", "Done. Charts are taller.".into(), vec![part("Done. Charts are taller.", false)], "");
        hub.write(|st| st.sessions.note("t2", "you", "Also call it wrong.", 50));
        hub.event(o.clone(), "stopped", "1. Shape: …".into(), vec![part("Done. Charts are taller.", false), part("1. Shape: …", true)], "");
        assert_eq!(replies(&hub), vec!["Done. Charts are taller."], "the second Stop's answer is the first's");
        hub.event(o.clone(), "active", "Also call it wrong.".into(), vec![], "");
        hub.event(o.clone(), "stopped", "Done. Charts are taller.".into(), vec![part("Done. Charts are taller.", false)], "");
        assert_eq!(replies(&hub).len(), 2, "a new turn's reply with the same words is its own");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_active_event_settles_what_cue_was_unsure_of_and_launch_asks_the_log() {
        let dir = std::env::temp_dir().join(format!("cue-unsure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let long = format!("you can make the change, but after that\n\ncheck why {}\n\nWant me to do that now?", "the message did not show ".repeat(20));
        let wrapped = format!("\n\n<pasted_content id=\"673a\">\n{long}\n</pasted_content id=\"673a\">\n");
        let path = dir.join("u1.jsonl");
        let o = Origin { session_id: "u1".into(), harness: "claude".into(), cwd: "/x/proj".into(), transcript_path: path.to_string_lossy().into(), ..Default::default() };
        hub.event(o.clone(), "stopped", "Done.".into(), vec![], "");
        hub.write(|st| {
            st.sessions.note("u1", "you", &long, 10);
            st.sessions.set_pending("u1", &long, true);
            st.sessions.mark_unsent("u1", &long, "still in its box after 30s");
        });
        let yours = |hub: &Hub| hub.read(|st| st.sessions.thread("u1")).into_iter().rev().find(|e| e.role == "you").unwrap();
        assert!(yours(&hub).unsent);
        hub.event(o.clone(), "active", wrapped.clone(), vec![], "");
        let e = yours(&hub);
        assert!(!e.unsent && !e.pending, "its prompt hook overturns the verdict: {e:?}");
        assert_eq!(hub.read(|st| st.sessions.state("u1")).as_deref(), Some("working"));

        // Queued while it worked: Claude Code reports the queued text the moment it lands, wrapped the
        // same way. That's its echo, not a new turn: the message stays queued, with Send now.
        let q = crate::model::Exchange { role: "you".into(), text: long.clone(), at_ms: now_ms(), images: vec![], from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![], voice: false, decision: false, status_ask: None };
        hub.write(|st| st.sessions.set_queued("u1", Some(q)));
        hub.event(o.clone(), "active", wrapped.clone(), vec![], "");
        assert!(hub.read(|st| st.sessions.queued_text("u1")).is_some(), "still queued: the wrapped report was its echo");

        // Cue restarted while unsure: the log has the message by now, so it's taken; a message the log
        // doesn't have stays as it was (a watch starts for a pending one only with the app around).
        hub.write(|st| {
            st.sessions.note("u1", "you", "and then commit", 10);
            st.sessions.set_pending("u1", "and then commit", true);
            st.sessions.mark_unsent("u1", &long, "still in its box after 30s");
        });
        let at = now_ms();
        std::fs::write(&path, format!("{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":{}}}}}\n", iso(at), serde_json::to_string(&wrapped).unwrap())).unwrap();
        hub.recheck_unsettled();
        let t = hub.read(|st| st.sessions.thread("u1"));
        let first = t.iter().find(|e| e.text == long).unwrap();
        assert!(!first.unsent && !first.pending, "{first:?}");
        assert_eq!(first.delivery.last().map(|d| d.what.as_str()), Some("taken: its log, read when Cue started"));
        let second = t.iter().find(|e| e.text == "and then commit").unwrap();
        assert!(second.pending && !second.unsent, "nothing says it went, nothing says it didn't");
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
