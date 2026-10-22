//! Typing into a session: replies, held and queued messages, interrupts, slash commands, and
//! confirming that what was typed was taken.

use super::*;
use super::history::{first_line, push_history};

impl Hub {
    /// Typed into its terminal, and the agent hasn't said it took it. A command may have opened a dialog
    /// (it ran: a card with its choices). The text may still sit in its box (the Enter didn't take: the
    /// agent was busy drawing): Enter once more. Then it's pending: a ring on the bubble while Cue keeps
    /// looking (`confirm`), with no verdict until there's something to go on. One look at the box is never
    /// a verdict here: it took one false "still there" to say a message didn't go when it had.
    /// Returns true when it's known to have gone through after all.
    fn settle_typed(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> bool {
        if text.starts_with('/') {
            if let Some(d) = crate::focus::dialog_on(origin) {
                self.st().sessions.got_message(sid, text, "it opened a dialog");
                self.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d), false);
                return true;
            }
        }
        if crate::focus::text_in_box(origin, text) == Some(true) {
            let _ = crate::focus::press_enter(origin);
            self.st().sessions.note_delivery(sid, text, "Enter again: it was still in its box");
            if self.wait_active(sid, t0, 3000) {
                return true;
            }
        }
        {
            let mut st = self.st();
            st.sessions.set_pending(sid, text, true);
            st.sessions.note_delivery(sid, text, "no word from it yet: Cue keeps looking");
        }
        self.confirm_later(sid.to_string(), origin.clone(), text.to_string(), cmd_before, t0);
        false
    }

    /// The images where the session can open them: on a machine, copied there first, the window told how
    /// far along the copy is ("upload": session_id, sent, total; `done` once it's over).
    fn images_there(&self, sid: &str, origin: &Origin, saved: &[crate::uploads::Saved]) -> Result<Vec<crate::uploads::Saved>, String> {
        if origin.machine.is_empty() || saved.is_empty() {
            return Ok(saved.to_vec());
        }
        let last = std::cell::Cell::new(0u64);
        let tell = |v: Value| {
            if let Some(app) = &self.app {
                let _ = app.emit("upload", v);
            }
        };
        let r = crate::uploads::on_machine(&origin.machine, saved, &|sent, total| {
            // A few times a second is enough to see it move.
            let t = now_ms();
            if sent == 0 || sent == total || t.saturating_sub(last.get()) >= 200 {
                last.set(t);
                tell(json!({ "session_id": sid, "sent": sent, "total": total, "count": saved.len(), "machine": origin.machine }));
            }
        });
        tell(json!({ "session_id": sid, "done": true }));
        r
    }

    /// Keep looking, off this thread, for the pending message to be taken (`confirm`).
    pub(super) fn confirm_later(&self, sid: String, origin: Origin, text: String, cmd_before: Option<usize>, t0: u64) {
        let Some(me) = self.shared() else { return };
        std::thread::spawn(move || me.confirm(&sid, &origin, &text, cmd_before, t0));
    }

    /// Watch a typed message until something says what became of it. "Taken" is a fact: any witness
    /// proves it, whenever it speaks (`witness`): its prompt hook, its log, a dialog it opened. "Not taken"
    /// is a verdict, and needs something seen: the text still in its box after the wait, twice in a row,
    /// or the agent gone from its pane. Meanwhile the text seen in its box gets Enter once more (the
    /// first one landed while the agent was busy), a couple of times at most.
    /// The screen is read in tmux, iTerm and Terminal.app (focus::screen_of). With nothing seen either
    /// way (another terminal, or no box on the screen), there is no verdict: after `PENDING_MS` the
    /// bubble says Cue hasn't seen it taken, and after `LATE_MS` Cue stops looking.
    /// Its log is read only where it is on this Mac (a machine's is mirrored here, or not read at all).
    fn confirm(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) {
        let start = now_ms();
        let (mut enters, mut last_enter, mut seen_in_box) = (0u32, 0u64, 0u32);
        loop {
            let late = now_ms().saturating_sub(start) >= PENDING_MS;
            std::thread::sleep(std::time::Duration::from_millis(if late { LATE_EVERY_MS } else { 2000 }));
            let pending = self.st().sessions.is_pending(sid, text);
            if !pending {
                return; // a witness settled it (its hook came), or it was sent again
            }
            if let Some(w) = self.witness(sid, origin, text, cmd_before, t0) {
                let w = w.to_string();
                self.st().sessions.got_message(sid, text, &w);
                self.changed();
                return;
            }
            match crate::focus::text_in_box(origin, text) {
                Some(true) => {
                    seen_in_box += 1;
                    if late && seen_in_box >= 2 {
                        let why = format!("still in its box after {}s", now_ms().saturating_sub(start) / 1000);
                        self.st().sessions.mark_unsent(sid, text, &why);
                        self.changed();
                        return;
                    }
                    // Not into a dialog: a bare Enter there picks whatever is highlighted.
                    if enters < ENTER_AGAIN && now_ms().saturating_sub(last_enter) >= 3000 && crate::focus::dialog_on(origin).is_none() {
                        let _ = crate::focus::press_enter(origin);
                        self.st().sessions.note_delivery(sid, text, "Enter again: it was still in its box");
                        enters += 1;
                        last_enter = now_ms();
                    }
                }
                Some(false) => seen_in_box = 0,
                None => {}
            }
            let gone = late && crate::focus::agent_gone(origin) == Some(true);
            if gone {
                self.st().sessions.mark_unsent(sid, text, "its pane shows a shell prompt, not the agent");
                self.changed();
                return;
            }
            if now_ms().saturating_sub(start) >= LATE_MS {
                let mut st = self.st();
                st.sessions.set_pending(sid, text, false);
                st.sessions.note_delivery(sid, text, "no word from it: Cue stopped looking");
                drop(st);
                self.changed();
                return;
            }
        }
    }

    /// Who, if anyone, says the agent took `text` since `t0`: its prompt hook (`active_at`), its command
    /// logged as run, its log with the text as your turn, its queue log with the text taken in, or a
    /// dialog the command opened. One look; None when nobody has said so yet.
    fn witness(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> Option<&'static str> {
        let hooked = self.st().active_at.get(sid).is_some_and(|t| *t >= t0);
        if hooked {
            return Some("its prompt hook");
        }
        let name = text.strip_prefix('/').and_then(|t| t.split_whitespace().next()).unwrap_or("");
        let log = log_here(origin);
        if let (Some(before), Some(log)) = (cmd_before, log) {
            if crate::transcript::command_count(log, name) > before {
                return Some("its log has the command run");
            }
        }
        if let Some(log) = log {
            // In its log as your turn, though no hook said so (its hooks report to another Cue, or one
            // got lost on the way from a machine): taken. Cue starts the turn as its prompt hook would,
            // so the session works, and its reply comes in from the log when the turn ends.
            if crate::transcript::prompt_since(log, text, t0) {
                let driver = crate::leads::resolve_driver(&origin.session_id, "");
                self.event(origin.clone(), "active", text.to_string(), vec![], &driver);
                // A quick turn may be over already (it ended before Cue looked): finish it now, as the
                // Stop hook would have. A longer one is caught when it ends (usage_tick).
                if let Some((_, reply, parts)) = crate::transcript::turn_ended_parts(log).filter(|(at, _, _)| *at + 2_000 >= t0) {
                    self.event(origin.clone(), "stopped", reply, parts, &driver);
                }
                return Some("its log has it as your turn");
            }
            // Sent while it worked and taken into the turn, by its queue log. (A message handed over
            // mid-turn also leaves a `queued_command` attachment in the log: the place for that witness,
            // once the transcript reader knows it.)
            if matches!(crate::transcript::queued_state(log, text), Some(crate::transcript::Queued::Absorbed | crate::transcript::Queued::Taken(_))) {
                return Some("its queue log has it taken");
            }
        }
        if !name.is_empty() {
            if let Some(d) = crate::focus::dialog_on(origin) {
                self.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d), false);
                return Some("it opened a dialog");
            }
        }
        None
    }

    /// Launch: whatever Cue was still unsure about when it last ran is asked again. Its log may have the
    /// message by now (then it's taken, verdict or no verdict), and what's still open is watched again.
    pub fn recheck_unsettled(&self) {
        let open = self.st().sessions.unsettled();
        for (origin, e) in open {
            let sid = origin.session_id.clone();
            let logged = log_here(&origin).is_some_and(|log| crate::transcript::prompt_since(log, &e.text, e.at_ms));
            if logged {
                self.st().sessions.got_message(&sid, &e.text, "its log, read when Cue started");
            } else if e.pending {
                self.st().sessions.note_delivery(&sid, &e.text, "Cue started: looking again");
                self.confirm_later(sid, origin, e.text, None, e.at_ms);
            }
        }
        self.changed();
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
        let via = if direct { "the session directly".to_string() } else { crate::focus::type_into(&it.origin, &crate::uploads::with_paths(text, &self.images_there(&sid, &it.origin, &saved)?))? };
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
            st.sessions.note_delivery(&sid, text, &delivery_step(&via, submitted));
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
            let state = log_here(&origin).and_then(|log| transcript::queued_state(log, &queued));
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
        let stopped = now && busy && !direct;
        let busy = if stopped {
            self.st().stopped_to_send.insert(session_id.to_string(), now_ms());
            crate::focus::press_escape(origin.as_ref().ok_or("that session is gone")?)?;
            std::thread::sleep(std::time::Duration::from_millis(700));
            false
        } else {
            busy && !now
        };
        let origin = origin.ok_or("that session is gone")?;
        // Stopped before it answered, Claude Code hands the message it was on back to its box, and this one
        // is typed after it: the two go as one. A blank line first keeps them apart (an empty box drops it).
        let apart = if stopped && origin.harness == "claude" { "\n\n" } else { "" };
        let cmd_before = command_before(&origin, typed);
        let t0 = now_ms();
        let direct = self.send_direct(session_id, text, &saved, now);
        let via = if direct { "the session directly".to_string() } else { crate::focus::type_into(&origin, &format!("{apart}{}", crate::uploads::with_paths(typed, &self.images_there(session_id, &origin, &saved)?)))? };
        // At its prompt it should take this (as a reply does); if not, Enter once more, then flag it.
        let mut submitted = busy || direct || self.took(session_id, &origin, typed, cmd_before, t0);
        if !submitted && !typed.starts_with('/') && Self::retry_enter(&origin, typed) {
            submitted = self.wait_active(session_id, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        {
            let mut st = self.st();
            st.sessions.note_sent(session_id, text, &paths, t0, crate::config::context_keep());
            st.sessions.note_delivery(session_id, text, &delivery_step(&via, submitted));
            if busy {
                // It's mid-turn: the agent reads this when it finishes its current step.
                // The same message as the thread's copy (same time), so the chat shows it once, as queued.
                let at = st.sessions.thread(session_id).iter().rev().find(|e| e.role == "you").map(|e| e.at_ms).unwrap_or_else(now_ms);
                let q = crate::model::Exchange { role: "you".into(), text: text.to_string(), at_ms: at, images: paths, from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![] };
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
        let (Some(before), Some(log)) = (cmd_before, log_here(origin)) else { return true };
        let name = text[1..].split_whitespace().next().unwrap_or("");
        (0..16).any(|_| self.wait_active(session_id, t0, 250) || crate::transcript::command_count(log, name) > before)
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
            _ => "Relay handoff: it writes its notes, relay opens the new lead, then this one steps down".into(),
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
            // The agent has this text: a message of yours Cue was still unsure about is settled by it.
            st.sessions.got_message(session_id, &message, "its prompt hook");
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

/// How long Cue looks for a typed message to be taken before the bubble says it hasn't seen it taken
/// (a verdict that it wasn't needs the text seen still in its box from here on).
const PENDING_MS: u64 = 30_000;
/// How often it looks after that, and for how long in all before it stops looking with no verdict.
/// SHORTCUT: in WezTerm, kitty and an editor's terminal Cue has no screen read (focus::screen_of), so
/// there a message nobody vouches for ends with no verdict at all; the upgrade is a screen read for
/// each (`wezterm cli get-text`, kitten's `get-text`) so the box check works there too.
const LATE_EVERY_MS: u64 = 10_000;
const LATE_MS: u64 = 10 * 60 * 1000;
/// How many times at most Enter is pressed again for a message seen still in its box.
const ENTER_AGAIN: u32 = 2;

/// Is `text` the command `/name` (with or without arguments)?
fn is_command(text: &str, name: &str) -> bool {
    text.strip_prefix('/').and_then(|t| t.split_whitespace().next()) == Some(name)
}

/// The first line of a message's delivery record: where it was typed, and whether the agent said it
/// took it before the send returned.
fn delivery_step(via: &str, submitted: bool) -> String {
    if submitted { format!("typed via {via}; taken: it started a turn") } else { format!("typed via {via}") }
}

/// The session's Claude Code log, where Cue can read it: on this Mac, or a machine's copied here
/// (machines::logs). A machine's own path is never read from here (a stat on it can stall).
fn log_here(origin: &Origin) -> Option<&str> {
    let path = origin.transcript_path.as_str();
    if origin.harness != "claude" || path.is_empty() {
        return None;
    }
    let mirrored = origin.machine.is_empty() || std::path::Path::new(path).starts_with(crate::server::cue_dir().join("machines"));
    mirrored.then_some(path)
}

/// For "/name …" typed into Claude Code: how many times it had already logged running that command,
/// so a new entry afterwards proves it ran. None for a plain message, or an agent Cue can't check.
fn command_before(origin: &Origin, text: &str) -> Option<usize> {
    let name = text.strip_prefix('/')?.split_whitespace().next()?;
    log_here(origin).map(|log| crate::transcript::command_count(log, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_is_read_only_where_it_is_on_this_mac() {
        let here = Origin { harness: "claude".into(), transcript_path: "/Users/me/.claude/projects/x/a.jsonl".into(), ..Default::default() };
        assert_eq!(log_here(&here), Some("/Users/me/.claude/projects/x/a.jsonl"));
        let mirrored = crate::server::cue_dir().join("machines").join("box").join("a.jsonl").to_string_lossy().into_owned();
        let on_machine = Origin { machine: "box".into(), transcript_path: mirrored.clone(), ..here.clone() };
        assert_eq!(log_here(&on_machine), Some(mirrored.as_str()), "a machine's log, copied here");
        let remote = Origin { machine: "box".into(), transcript_path: "/home/me/.claude/projects/x/a.jsonl".into(), ..here.clone() };
        assert_eq!(log_here(&remote), None, "the machine's own path: never touched from here");
        assert_eq!(log_here(&Origin { harness: "pi".into(), ..here.clone() }), None);
        assert_eq!(log_here(&Origin { transcript_path: String::new(), ..here }), None);
        assert_eq!(command_before(&remote, "/compact"), None);
    }

    #[test]
    fn the_delivery_record_starts_with_where_it_was_typed() {
        assert_eq!(delivery_step("tmux pane %3", true), "typed via tmux pane %3; taken: it started a turn");
        assert_eq!(delivery_step("iTerm session", false), "typed via iTerm session");
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
        assert!(hub.st().sessions.thread("s5").iter().all(|e| e.role != "you" || e.text == "refactor the router"), "not in the chat as sent");
        // Esc / Edit: back to you, and nothing kept any more.
        assert_eq!(hub.unhold("s5").map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()));
        assert!(held(&hub).is_none() && hub.unhold("s5").is_none());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
