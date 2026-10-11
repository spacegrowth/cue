//! Typing into a session: replies, held and queued messages, interrupts, slash commands, and
//! confirming that what was typed was taken.

use super::*;
use super::history::{first_line, push_history};

impl Hub {
    /// Typed into its terminal, and the agent hasn't said it took it. Looked at once more before deciding:
    /// a command may have opened a dialog (it ran: a card with its choices), or the text may still sit in
    /// its box (it didn't: the bubble says so). Otherwise it's pending: a ring on the bubble while Cue
    /// keeps looking (`confirm_later`), with no verdict until there's something to go on.
    /// Returns true when it's known to have gone through after all.
    fn settle_typed(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> bool {
        if text.starts_with('/') {
            if let Some(d) = crate::focus::dialog_on(origin) {
                self.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d));
                return true;
            }
        }
        // Still in its box: the Enter didn't take (the agent was busy drawing). One more, then a look.
        if crate::focus::text_in_box(origin, text) == Some(true) {
            let _ = crate::focus::press_enter(origin);
            std::thread::sleep(std::time::Duration::from_millis(700));
            if crate::focus::text_in_box(origin, text) == Some(true) {
                self.st().sessions.mark_unsent(sid);
                return false;
            }
            if self.wait_active(sid, t0, 3000) {
                return true;
            }
        }
        self.st().sessions.set_pending(sid, true);
        self.confirm_later(sid.to_string(), origin.clone(), text.to_string(), cmd_before, t0);
        false
    }

    /// Keep looking, off this thread, for the pending message to be taken: its hook (clears the ring
    /// itself), its command in the transcript, a dialog it opened, or the text still in its box (then it
    /// didn't go). After `PENDING_MS` with nothing to go on, the bubble says it may not have gone.
    pub(super) fn confirm_later(&self, sid: String, origin: Origin, text: String, cmd_before: Option<usize>, t0: u64) {
        let Some(me) = self.shared() else { return };
        std::thread::spawn(move || {
            let end = now_ms() + PENDING_MS;
            let name = text.strip_prefix('/').and_then(|t| t.split_whitespace().next()).unwrap_or("").to_string();
            while now_ms() < end {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                {
                    let st = me.st();
                    if st.sessions.pending_text(&sid).as_deref() != Some(text.as_str()) {
                        return; // its hook came (or it was sent again)
                    }
                }
                let taken = me.st().active_at.get(&sid).is_some_and(|t| *t >= t0)
                    || cmd_before.is_some_and(|before| crate::transcript::command_count(&origin.transcript_path, &name) > before);
                if taken {
                    me.st().sessions.set_pending(&sid, false);
                    me.changed();
                    return;
                }
                if !name.is_empty() {
                    if let Some(d) = crate::focus::dialog_on(&origin) {
                        me.st().sessions.set_pending(&sid, false);
                        me.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d));
                        return;
                    }
                }
                if crate::focus::text_in_box(&origin, &text) == Some(true) {
                    break;
                }
            }
            me.st().sessions.mark_unsent(&sid);
            me.changed();
        });
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
            let st = self.st();
            (st.sessions.state(&sid).as_deref() == Some("working") || st.sessions.compacting_since(&sid) > 0) && !st.subscribers.contains_key(&sid)
        };
        if busy {
            let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
            let mut st = self.st();
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
        if !submitted && !direct && !text.starts_with('/') && Self::retry_enter(&it.origin, text) {
            submitted = self.wait_active(&sid, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        let pics = if saved.is_empty() { String::new() } else { format!(" + {} image{}", saved.len(), if saved.len() == 1 { "" } else { "s" }) };
        let outcome = format!("replied: “{}”{pics}{}", first_line(text, 200), if submitted { "" } else { " (typed; didn't see it submit)" });
        {
            let mut st = self.st();
            st.items.retain(|i| i.id != id);
            st.sessions.note_sent(&sid, text, &paths, t0, crate::config::context_keep());
        }
        if !submitted && !direct {
            submitted = self.settle_typed(&sid, &it.origin, text, cmd_before, t0);
        }
        {
            let mut st = self.st();
            let mut h = it;
            h.images = paths;
            h.status = "answered".into();
            h.outcome = outcome;
            h.resolved_ms = Some(now_ms());
            push_history(&mut st, h);
        }
        self.changed();
        Ok(if submitted { format!("sent via {via}") } else { format!("via {via}; Cue is watching for it to take it") })
    }

    /// Send a message to any live session — a working one included (the "say something" box).
    /// Direct when the session listens (Pi); otherwise typed into its terminal, where the agent
    /// queues it if it's mid-turn. Blocking: call off the UI thread.
    /// Stop a session mid-turn (Esc in its terminal; Pi aborts directly). It's left at its prompt.
    /// "Send now" on a queued message: the same Esc as Stop (the agent then reads the queued message
    /// straight away), but nobody stopped anything, so no "⏹ stopped it" line and no stopped state.
    pub fn send_queued_now(&self, session_id: &str) -> Result<String, String> {
        let (origin, direct) = {
            let st = self.st();
            let direct = st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "interrupt" })).is_ok()).unwrap_or(false);
            (st.sessions.origin(session_id), direct)
        };
        if !direct {
            let origin = origin.ok_or("that session is gone")?;
            // Claude Code may already have taken it into the running turn (between two steps): Esc
            // now would interrupt the turn that's working on it ("Interrupted · What should Claude
            // do instead?") and leave it idle. Esc only while it's still waiting in the queue.
            let queued = self.st().sessions.queued_text(session_id).unwrap_or_default();
            let state = if origin.harness == "claude" { transcript::queued_state(&origin.transcript_path, &queued) } else { None };
            if matches!(state, Some(transcript::Queued::Absorbed | transcript::Queued::Taken(_))) {
                self.st().sessions.set_queued(session_id, None);
                self.changed();
                return Ok("it already has your message: it took it in between steps".into());
            }
            crate::focus::press_escape(&origin)?;
        }
        self.st().sessions.set_queued(session_id, None); // it's reading it now
        self.changed();
        Ok("sent now".into())
    }

    pub fn interrupt(&self, session_id: &str) -> Result<String, String> {
        let (origin, direct) = {
            let st = self.st();
            let direct = st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "interrupt" })).is_ok()).unwrap_or(false);
            (st.sessions.origin(session_id), direct)
        };
        let via = if direct { "the session directly".to_string() } else { crate::focus::press_escape(&origin.clone().ok_or("that session is gone")?)? };
        {
            let mut st = self.st();
            if let Some(o) = origin {
                st.sessions.mark(&o, "stopped", None);
            }
            st.sessions.set_queued(session_id, None);
            st.sessions.note(session_id, "you", "⏹ stopped it", crate::config::context_keep());
        }
        self.changed();
        // A message you kept for it goes now, as Claude Code sends its queue once you stop it.
        if self.st().sessions.held(session_id).is_some() {
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
            let st = self.st();
            // Its terminal is showing a permission prompt or a question: typing now would answer it.
            if st.sessions.state(session_id).as_deref() == Some("deciding") {
                return Err("it's asking you something first: answer that, then send".into());
            }
            // Compacting counts as busy: Claude Code holds what you type until it's done (that can take a minute).
            let busy = st.sessions.state(session_id).as_deref() == Some("working") || st.sessions.compacting_since(session_id) > 0;
            (st.sessions.origin(session_id), busy)
        };
        // Force-send to a terminal agent: Esc first, give it a moment to stop, then type.
        let direct = self.st().subscribers.contains_key(session_id);
        // Working, and Cue would type it into its terminal: Cue keeps it instead and sends it when the turn
        // ends, so until then it's yours to take back (Esc, Edit) or send now. (In the terminal's own queue
        // only Claude Code could give it back.)
        if busy && !now && !direct && origin.is_some() {
            self.st().sessions.hold(session_id, typed, saved.iter().map(|s| s.path.clone()).collect());
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
        if !submitted && !typed.starts_with('/') && Self::retry_enter(&origin, typed) {
            submitted = self.wait_active(session_id, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        {
            let mut st = self.st();
            st.sessions.note_sent(session_id, text, &paths, t0, crate::config::context_keep());
            if busy {
                // It's mid-turn: the agent reads this when it finishes its current step.
                // The same message as the thread's copy (same time), so the chat shows it once, as queued.
                let at = st.sessions.thread(session_id).iter().rev().find(|e| e.role == "you").map(|e| e.at_ms).unwrap_or_else(now_ms);
                let q = crate::model::Exchange { role: "you".into(), text: text.to_string(), at_ms: at, images: paths, from: String::new(), unsent: false, pending: false };
                st.sessions.set_queued(session_id, Some(q));
            }
            // "/compact" from the box: the same "Compacting" line as Compact in the ⋯ menu.
            if submitted && !busy && is_command(typed, "compact") {
                st.sessions.set_compacting(session_id, now_ms());
            }
        }
        if !submitted && self.settle_typed(session_id, &origin, typed, cmd_before, t0) {
            submitted = true;
        }
        self.changed();
        if !submitted {
            return Ok(format!("via {via}; Cue is watching for it to take it"));
        }
        Ok(if busy { format!("queued via {via}: it reads this when it finishes its current step") } else { format!("sent via {via}") })
    }

    /// Send the message kept for a session: when its turn ends, or now (`now`: stop the turn first). If it
    /// can't be sent it's kept again, so nothing you wrote is lost.
    pub fn send_held(&self, session_id: &str, now: bool) -> Result<String, String> {
        let h = self.st().sessions.take_held(session_id).ok_or("nothing kept for it")?;
        let saved = h.images.iter().map(|p| crate::uploads::Saved { path: p.clone(), mime: crate::uploads::mime_of(p).into() }).collect();
        self.send_saved(session_id, &h.text, saved, now).inspect_err(|_| {
            self.st().sessions.hold(session_id, &h.text, h.images.clone());
            self.changed();
        })
    }

    /// Enter once more, for a typed message the agent hasn't reported taking. Not when Cue can see the
    /// box and the text is gone from it (it was taken; a session without Cue's hooks never reports), and
    /// not into a dialog: a bare Enter there picks whatever is highlighted, "Yes" on a permission prompt.
    /// True when it was pressed.
    fn retry_enter(origin: &Origin, text: &str) -> bool {
        if crate::focus::text_in_box(origin, text) == Some(false) || crate::focus::dialog_on(origin).is_some() {
            return false;
        }
        crate::focus::press_enter(origin).is_ok()
    }

    /// The kept message goes off this thread (it types into a terminal): after `wait_ms`, for a
    /// terminal that was just stopped to settle.
    pub(super) fn send_held_soon(&self, session_id: &str, wait_ms: u64) {
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
        let h = self.st().sessions.take_held(session_id);
        if h.is_some() {
            self.changed();
        }
        h
    }

    /// Hand a message (and saved images) to a session that listens directly (Pi). False if none does.
    fn send_direct(&self, session_id: &str, text: &str, saved: &[crate::uploads::Saved], now: bool) -> bool {
        let st = self.st();
        // now: Pi takes it as a "steer", read mid-turn, instead of a follow-up after the turn.
        let msg = json!({ "type": "reply", "text": text, "images": saved, "now": now });
        st.subscribers.get(session_id).map(|(_, tx)| tx.send(msg).is_ok()).unwrap_or(false)
    }

    /// What a subscribed session said its commands are (see `session_commands`).
    pub fn set_agent_commands(&self, session_id: &str, list: Vec<crate::commands::Cmd>) {
        self.st().agent_commands.insert(session_id.to_string(), list);
    }

    pub fn agent_commands(&self, session_id: &str) -> Option<Vec<crate::commands::Cmd>> {
        self.st().agent_commands.get(session_id).cloned()
    }

    /// Refresh: forget what the session reported and ask it again down its connection. False when
    /// it isn't connected (then there's nothing to wait for).
    pub fn ask_agent_commands(&self, session_id: &str) -> bool {
        let mut st = self.st();
        st.agent_commands.remove(session_id);
        st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "commands" })).is_ok()).unwrap_or(false)
    }

    /// Did what you typed go through? A message: the agent started a turn with it. "/name …" in Claude Code:
    /// that, or its transcript logged the command running (an unknown command like "/reomte" leaves no
    /// trace, so it's caught). A command in an agent Cue can't check: assume it ran.
    pub(super) fn took(&self, session_id: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> bool {
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
            if self.st().active_at.get(session_id).is_some_and(|t| *t >= since) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        false
    }

    /// A Claude Code turn just ended: type the rename it missed while busy (after its Stop hooks settle).
    /// Type one of the agent's own commands into its session (Compact: `/compact`; a relay lead's Hand off:
    /// `/relay:handoff`). Not mid-turn: typed then, it would land in the middle of its work.
    pub fn session_command(&self, session_id: &str, action: &str) -> Result<String, String> {
        let (origin, state) = {
            let st = self.st();
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
        // Still in its box after a moment (it was busy drawing, or held the text): one more Enter, and if
        // it's still there, say so rather than show "Compacting" for the 15 minutes until Cue gives up,
        // holding every message meanwhile.
        std::thread::sleep(std::time::Duration::from_millis(800));
        if crate::focus::text_in_box(&origin, line) == Some(true) {
            let _ = crate::focus::press_enter(&origin);
            std::thread::sleep(std::time::Duration::from_millis(600));
            if crate::focus::text_in_box(&origin, line) == Some(true) {
                return Err(format!("{line} is typed but it didn't take it: press Enter in its terminal"));
            }
        }
        if action == "compact" {
            self.st().sessions.set_compacting(session_id, now_ms());
            self.changed();
        }
        Ok(match action {
            "compact" => "Compacting: it summarizes the conversation to free up context".into(),
            _ => "Handing off: it writes its notes, opens its successor, then steps down".into(),
        })
    }

    /// A prompt you sent the session (not a background agent's notification): it shows as yours
    /// in the chat and labels what the session is working on.
    pub fn prompt_from_you(&self, session_id: &str, message: &str) {
        if message.trim().is_empty() {
            return;
        }
        {
            let mut st = self.st();
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
            let mut st = self.st();
            // The project Cue shows for the sender, else Claude's own name for it.
            let who = pid.and_then(|p| st.sessions.project_by_pid(p)).unwrap_or_else(|| if name.is_empty() { "another agent".into() } else { name.to_string() });
            st.sessions.note_peer(session_id, &who, body, crate::config::context_keep());
        }
        self.changed();
    }
}

/// How long Cue keeps looking for a typed message to be taken before its bubble says it may not have gone.
/// SHORTCUT: outside tmux (iTerm, Terminal) Cue can't see the input box, so this timer is the only
/// verdict there; the upgrade is a per-terminal screen read (iTerm's "contents of session") so the
/// box check works everywhere and the timer goes.
const PENDING_MS: u64 = 30_000;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(hub.st().sessions.thread("s5").iter().all(|e| e.role != "you" || e.text == "refactor the router"), "not in the chat as sent");
        // Esc / Edit: back to you, and nothing kept any more.
        assert_eq!(hub.unhold("s5").map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()));
        assert!(held(&hub).is_none() && hub.unhold("s5").is_none());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
