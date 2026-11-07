//! Typing into a session: replies, held and queued messages, interrupts, slash commands, and
//! confirming that what was typed was taken.

use super::*;
use super::history::{first_line, push_history};

/// How long a message waits for the one before it to finish being typed into the same session. Typing one
/// takes seconds (up to ~25 with its checks); longer means that one is stuck, and this fails instead of
/// spinning on "Sending…" for good.
const TURN_WAIT_MS: u64 = 45_000;

/// This session's typing turn (see Hub::typing), or an error once the one before has held it for TURN_WAIT.
fn wait_turn(gate: &Mutex<()>) -> Result<std::sync::MutexGuard<'_, ()>, String> {
    let until = now_ms() + TURN_WAIT_MS;
    loop {
        match gate.try_lock() {
            Ok(g) => return Ok(g),
            Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) if now_ms() < until => crate::clock::sleep_ms(100),
            Err(std::sync::TryLockError::WouldBlock) => return Err("the message before this one is still being typed into its terminal (stuck?): try again in a moment".into()),
        }
    }
}

impl Hub {
    /// Typed into its terminal, and the agent hasn't said it took it. A command may have opened a dialog
    /// (it ran: a card with its choices). The text may still sit in its box (the Enter didn't take: the
    /// agent was busy drawing): Enter once more. Then it's pending: a ring on the bubble while Cue keeps
    /// looking (`confirm`), with no verdict until there's something to go on. One look at the box is never
    /// a verdict here: it took one false "still there" to say a message didn't go when it had.
    /// Returns true when it's known to have gone through after all.
    fn settle_typed(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> bool {
        if text.starts_with('/') {
            if let Some(d) = self.keys().dialog_on(origin) {
                self.write(|st| st.sessions.got_message(sid, text, "it opened a dialog"));
                self.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d), false);
                return true;
            }
        }
        // Not into a dialog: a bare Enter there picks whatever is highlighted.
        if self.keys().text_in_box(origin, text) == Some(true) && self.keys().dialog_on(origin).is_none() {
            let _ = self.keys().press_enter(origin);
            self.write(|st| st.sessions.note_delivery(sid, text, "Enter again: it was still in its box"));
            if self.wait_active(sid, t0, 3000) {
                return true;
            }
        }
        self.write(|st| {
            st.sessions.set_pending(sid, text, true);
            st.sessions.note_delivery(sid, text, "no word from it yet: Cue keeps looking");
        });
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
        let tell = |v: Value| self.ports.window.tell("upload", v);
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
        self.look_later(Watch { sid, origin, text, cmd_before, t0, start: now_ms(), late: false, enters: 0, last_enter: 0, seen_in_box: 0 });
    }

    /// The watch's next look (`confirm`): every 2 s at first, every 10 s once it's late.
    fn look_later(&self, mut w: Watch) {
        w.late = now_ms().saturating_sub(w.start) >= PENDING_MS;
        let after = if w.late { LATE_EVERY_MS } else { 2000 };
        self.later(after, move |h| h.confirm(w));
    }

    /// One look at a typed message, until something says what became of it. "Taken" is a fact: any witness
    /// proves it, whenever it speaks (`witness`): its prompt hook, its log, a dialog it opened. "Not taken"
    /// is a verdict, and needs something seen: the text still in its box after the wait, twice in a row,
    /// or the agent gone from its pane. Meanwhile the text seen in its box gets Enter once more (the
    /// first one landed while the agent was busy), a couple of times at most.
    /// The screen is read in tmux, iTerm and Terminal.app (focus::screen_of). With nothing seen either
    /// way (another terminal, or no box on the screen), there is no verdict: after `PENDING_MS` the
    /// bubble says Cue hasn't seen it taken, and after `LATE_MS` Cue stops looking.
    /// Its log is read only where it is on this Mac (a machine's is mirrored here, or not read at all).
    /// Undecided, the next look is scheduled (`look_later`).
    fn confirm(&self, mut w: Watch) {
        let late = w.late;
        if !self.read(|st| st.sessions.is_pending(&w.sid, &w.text)) {
            return; // a witness settled it (its hook came), or it was sent again
        }
        if let Some(why) = self.witness(&w.sid, &w.origin, &w.text, w.cmd_before, w.t0) {
            self.write(|st| st.sessions.got_message(&w.sid, &w.text, why));
            return self.changed();
        }
        if let Some(why) = self.box_verdict(&mut w, late) {
            self.write(|st| st.sessions.mark_unsent(&w.sid, &w.text, &why));
            return self.changed();
        }
        if late && self.keys().agent_gone(&w.origin) == Some(true) {
            self.write(|st| st.sessions.mark_unsent(&w.sid, &w.text, "its pane shows a shell prompt, not the agent"));
            return self.changed();
        }
        if now_ms().saturating_sub(w.start) >= LATE_MS {
            self.write(|st| {
                st.sessions.set_pending(&w.sid, &w.text, false);
                st.sessions.note_delivery(&w.sid, &w.text, "no word from it: Cue stopped looking");
            });
            return self.changed();
        }
        self.look_later(w);
    }

    /// What the input box says this look: the text seen still there twice in a row once it's late is a
    /// verdict (why it wasn't taken); seen there before that, Enter once more (not into a dialog: a bare
    /// Enter there picks whatever is highlighted).
    fn box_verdict(&self, w: &mut Watch, late: bool) -> Option<String> {
        match self.keys().text_in_box(&w.origin, &w.text) {
            Some(true) => {
                w.seen_in_box += 1;
                if late && w.seen_in_box >= 2 {
                    return Some(format!("still in its box after {}s", now_ms().saturating_sub(w.start) / 1000));
                }
                if w.enters < ENTER_AGAIN && now_ms().saturating_sub(w.last_enter) >= 3000 && self.keys().dialog_on(&w.origin).is_none() {
                    let _ = self.keys().press_enter(&w.origin);
                    self.write(|st| st.sessions.note_delivery(&w.sid, &w.text, "Enter again: it was still in its box"));
                    w.enters += 1;
                    w.last_enter = now_ms();
                }
            }
            Some(false) => w.seen_in_box = 0,
            None => {}
        }
        None
    }

    /// Who, if anyone, says the agent took `text` since `t0`: its prompt hook (`active_at`), its command
    /// logged as run, its log with the text as your turn, its queue log with the text taken in, or a
    /// dialog the command opened. One look; None when nobody has said so yet.
    fn witness(&self, sid: &str, origin: &Origin, text: &str, cmd_before: Option<usize>, t0: u64) -> Option<&'static str> {
        let hooked = self.read(|st| st.active_at.get(sid).is_some_and(|t| *t >= t0));
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
                const SAID: &str = "its log has it as your turn";
                // Settled by the log before the turn is started for it: the "active" below settles what's
                // still open as its prompt hook's word, and the record would credit the hook.
                self.write(|st| st.sessions.got_message(sid, text, SAID));
                let driver = crate::leads::resolve_driver(&origin.session_id, "");
                self.event(origin.clone(), "active", text.to_string(), vec![], &driver);
                // A quick turn may be over already (it ended before Cue looked): finish it now, as the
                // Stop hook would have. A longer one is caught when it ends (usage_tick).
                if let Some((_, reply, parts)) = crate::transcript::turn_ended_parts(log).filter(|(at, _, _)| *at + 2_000 >= t0) {
                    self.event(origin.clone(), "stopped", reply, parts, &driver);
                }
                return Some(SAID);
            }
            // Sent while it worked and taken into the turn: its queue log says so, or the log has the
            // message handed over between two steps (a `queued_command` attachment, not a turn of yours).
            if matches!(crate::transcript::queued_state(log, text), Some(crate::transcript::Queued::Absorbed | crate::transcript::Queued::Taken(_))) {
                return Some("its queue log has it taken");
            }
            if crate::transcript::taken_mid_turn(log, text, t0) {
                return Some("its log has it taken mid-turn");
            }
        }
        if !name.is_empty() {
            if let Some(d) = self.keys().dialog_on(origin) {
                self.terminal_ask_with(origin.clone(), "Waiting in its terminal: dialog open".into(), false, Some(d), false);
                return Some("it opened a dialog");
            }
        }
        None
    }

    /// Launch: whatever Cue was still unsure about when it last ran is asked again. Its log may have the
    /// message by now (then it's taken, verdict or no verdict), and what's still open is watched again.
    pub fn recheck_unsettled(&self) {
        let open = self.read(|st| st.sessions.unsettled());
        for (origin, e) in open {
            let sid = origin.session_id.clone();
            let logged = log_here(&origin).is_some_and(|log| crate::transcript::prompt_since(log, &e.text, e.at_ms) || crate::transcript::taken_mid_turn(log, &e.text, e.at_ms));
            if logged {
                self.write(|st| st.sessions.got_message(&sid, &e.text, "its log, read when Cue started"));
            } else if e.pending {
                self.write(|st| st.sessions.note_delivery(&sid, &e.text, "Cue started: looking again"));
                self.confirm_later(sid, origin, e.text, None, e.at_ms);
            }
        }
        self.changed();
    }

    /// Type your reply into a finished session's terminal and confirm it submitted.
    /// Blocking (typing takes ~1-2s): call off the UI thread.
    pub fn reply(&self, id: &str, text: &str, images: &[Upload]) -> Result<String, String> {
        // Nothing to type is a bare Enter, which on a screen that changed meanwhile picks what's highlighted
        // (a notification's reply field sent empty comes here as well as the box).
        if text.trim().is_empty() && images.is_empty() {
            return Err("nothing to send".into());
        }
        let it = self.get(id).filter(|i| i.status == "pending").ok_or("that card is gone")?;
        let sid = it.origin.session_id.clone();
        // One message at a time into a session: another being typed into it goes first (see Hub::typing).
        let gate = self.typing_gate(&sid);
        let _typing = wait_turn(&gate)?;
        self.set_later(&sid, false); // you replied: decided
        let t0 = now_ms();
        let saved = crate::uploads::save(images)?;
        // The card is stale (the session went on to another turn since): typed now, the reply would land
        // mid-turn and could be lost. Kept in Cue instead, as the box does, and sent when the turn ends.
        let busy = self.read(|st| (st.sessions.state(&sid).as_deref() == Some("working") || st.sessions.compacting_since(&sid) > 0) && !st.subscribers.contains_key(&sid));
        if busy {
            let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
            let mut h = it;
            h.images = paths.clone();
            h.status = "answered".into();
            h.outcome = format!("replied: “{}” (kept in Cue until its turn ends)", first_line(text, 200));
            h.resolved_ms = Some(now_ms());
            let answered = self.write(|st| {
                st.items.retain(|i| i.id != id);
                st.sessions.hold(&sid, text, paths);
                push_history(st, h)
            });
            self.save_answered(answered);
            self.changed();
            return Ok("kept in Cue: it goes when this turn ends".into());
        }
        let cmd_before = command_before(&it.origin, text);
        // Direct delivery when the session listens for it (Pi); otherwise type into its terminal.
        let direct = self.send_direct(&sid, text, &saved, false);
        let via = if direct { "the session directly".to_string() } else { self.keys().type_into(&it.origin, &crate::uploads::with_paths(text, &self.images_there(&sid, &it.origin, &saved)?))? };
        // The agent reports a new turn (Claude: UserPromptSubmit, Pi: agent_start) once it submits.
        let mut submitted = self.took(&sid, &it.origin, text, cmd_before, t0);
        if !submitted && !direct && !text.starts_with('/') && self.retry_enter(&it.origin, text) {
            submitted = self.wait_active(&sid, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        let pics = if saved.is_empty() { String::new() } else { format!(" + {} image{}", saved.len(), if saved.len() == 1 { "" } else { "s" }) };
        let outcome = format!("replied: “{}”{pics}{}", first_line(text, 200), if submitted { "" } else { " (typed; didn't see it submit)" });
        self.write(|st| {
            st.items.retain(|i| i.id != id);
            st.sessions.note_sent(&sid, text, &paths, t0, crate::config::context_keep());
            st.sessions.note_delivery(&sid, text, &delivery_step(&via, submitted));
        });
        if !submitted && !direct {
            submitted = self.settle_typed(&sid, &it.origin, text, cmd_before, t0);
        }
        let mut h = it;
        h.images = paths;
        h.status = "answered".into();
        h.outcome = outcome;
        h.resolved_ms = Some(now_ms());
        let answered = self.write(|st| push_history(st, h));
        self.save_answered(answered);
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
        let (origin, direct) = self.interrupt_direct(session_id);
        if !direct {
            let origin = origin.ok_or("that session is gone")?;
            // Claude Code may already have taken it into the running turn (between two steps): Esc
            // now would interrupt the turn that's working on it ("Interrupted · What should Claude
            // do instead?") and leave it idle. Esc only while it's still waiting in the queue.
            // Codex steers a message into its turn the same way: its own log says so.
            let queued = self.read(|st| st.sessions.queued(session_id));
            let (text, at) = queued.map(|q| (q.text, q.at_ms)).unwrap_or_default();
            // (Codex's on this Mac only: a machine's own path is never read from here.)
            let log = if origin.harness == "claude" { log_here(&origin).map(str::to_string) } else { (origin.machine.is_empty() && !origin.transcript_path.is_empty()).then(|| origin.transcript_path.clone()) };
            let state = log.and_then(|log| transcript::read_state(&origin.harness, &log, &text, at));
            if matches!(state, Some(transcript::Queued::Absorbed | transcript::Queued::Taken(_))) {
                self.write(|st| st.sessions.set_queued(session_id, None));
                self.changed();
                return Ok("it already has your message: it took it in between steps".into());
            }
            self.keys().press_escape(&origin)?;
        }
        self.write(|st| st.sessions.set_queued(session_id, None)); // it's reading it now
        self.changed();
        Ok("sent now".into())
    }

    pub fn interrupt(&self, session_id: &str) -> Result<String, String> {
        let (origin, direct) = self.interrupt_direct(session_id);
        let via = if direct { "the session directly".to_string() } else { self.keys().press_escape(&origin.clone().ok_or("that session is gone")?)? };
        self.write(|st| {
            if let Some(o) = origin {
                st.sessions.mark(&o, "stopped", None);
            }
            if !direct {
                st.escaped.insert(session_id.to_string(), now_ms());
            }
            st.sessions.set_queued(session_id, None);
            st.sessions.note(session_id, "you", "⏹ stopped it", crate::config::context_keep());
        });
        self.changed();
        // A message you kept for it goes now, as Claude Code sends its queue once you stop it.
        if self.read(|st| st.sessions.held(session_id).is_some()) {
            self.send_held_soon(session_id, 700);
        }
        Ok(format!("stopped via {via}"))
    }

    /// Stop or send-now, told to a session that listens directly (Pi) if one does: whether it was, and where
    /// the session's terminal is otherwise.
    fn interrupt_direct(&self, session_id: &str) -> (Option<Origin>, bool) {
        self.read(|st| {
            let direct = st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "interrupt" })).is_ok()).unwrap_or(false);
            (st.sessions.origin(session_id), direct)
        })
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
        // One message at a time into a session: another being typed into it goes first (see Hub::typing).
        let gate = self.typing_gate(session_id);
        let _typing = wait_turn(&gate)?;
        self.set_later(session_id, false); // you wrote to it: decided
        let (origin, busy, direct) = self.read(|st| Self::send_state(st, session_id))?;
        // Working, and Cue would type it into its terminal: Cue keeps it instead and sends it when the turn
        // ends, so until then it's yours to take back (Esc, Edit) or send now. (In the terminal's own queue
        // only Claude Code could give it back.)
        if busy && !now && !direct && origin.is_some() {
            self.write(|st| st.sessions.hold(session_id, typed, saved.iter().map(|s| s.path.clone()).collect()));
            self.changed();
            return Ok("kept in Cue: it goes when this turn ends".into());
        }
        // ⌘↵ while it works: its turn stopped first, so this goes now rather than queued.
        let stopped = now && busy && !direct;
        if stopped {
            self.stop_to_send(session_id, origin.as_ref().ok_or("that session is gone")?)?;
        }
        let busy = busy && !now;
        let origin = origin.ok_or("that session is gone")?;
        // Stopped before it answered, Claude Code hands the message it was on back to its box, and this one
        // is typed after it: the two go as one. A blank line first keeps them apart (an empty box drops it).
        // Its Stop button, a moment ago, does the same: the kept message sent right after goes apart too.
        let escaped = self.read(|st| st.escaped.get(session_id).is_some_and(|t| now_ms().saturating_sub(*t) < 30_000));
        let apart = if (stopped || escaped) && origin.harness == "claude" { "\n\n" } else { "" };
        let cmd_before = command_before(&origin, typed);
        let t0 = now_ms();
        let direct = self.send_direct(session_id, text, &saved, now);
        let via = if direct { "the session directly".to_string() } else { self.keys().type_into(&origin, &format!("{apart}{}", crate::uploads::with_paths(typed, &self.images_there(session_id, &origin, &saved)?)))? };
        // At its prompt it should take this (as a reply does); if not, Enter once more, then flag it.
        let mut submitted = busy || direct || self.took(session_id, &origin, typed, cmd_before, t0);
        if !submitted && !typed.starts_with('/') && self.retry_enter(&origin, typed) {
            submitted = self.wait_active(session_id, t0, 4000);
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        self.write(|st| {
            st.sessions.note_sent(session_id, text, &paths, t0, crate::config::context_keep());
            st.sessions.note_delivery(session_id, text, &delivery_step(&via, submitted));
            if busy {
                queue_typed(st, session_id, text, paths);
            }
            // "/compact" from the box: the same "Compacting" line as Compact in the ⋯ menu.
            if submitted && !busy && is_command(typed, "compact") {
                st.sessions.set_compacting(session_id, now_ms());
            }
        });
        if !submitted && self.settle_typed(session_id, &origin, typed, cmd_before, t0) {
            submitted = true;
        }
        self.changed();
        if !submitted {
            return Ok(format!("via {via}; Cue is watching for it to take it"));
        }
        Ok(if busy { format!("queued via {via}: it reads this when it finishes its current step") } else { format!("sent via {via}") })
    }

    /// Stop a session's turn to send a message now: Esc, noted as Cue's own (not a turn you stopped), and a
    /// moment for its terminal to settle.
    fn stop_to_send(&self, session_id: &str, origin: &Origin) -> Result<(), String> {
        self.write(|st| st.stopped_to_send.insert(session_id.to_string(), now_ms()));
        self.keys().press_escape(origin)?;
        crate::clock::sleep_ms(700);
        Ok(())
    }

    /// Where a session is and whether it's busy (working or compacting: Claude Code holds what you type until
    /// it's done, and that can take a minute), and whether it listens directly; an error while it's asking you
    /// something (its terminal shows a permission prompt or a question: typing now would answer it).
    fn send_state(st: &Store, session_id: &str) -> Result<(Option<Origin>, bool, bool), String> {
        let state = st.sessions.state(session_id);
        if state.as_deref() == Some("deciding") {
            return Err("it's asking you something first: answer that, then send".to_string());
        }
        let busy = state.as_deref() == Some("working") || st.sessions.compacting_since(session_id) > 0;
        Ok((st.sessions.origin(session_id), busy, st.subscribers.contains_key(session_id)))
    }

    /// Send the message kept for a session: when its turn ends, or now (`now`: stop the turn first). If it
    /// can't be sent it's kept again, so nothing you wrote is lost.
    pub fn send_held(&self, session_id: &str, now: bool) -> Result<String, String> {
        let h = self.write(|st| st.sessions.take_held(session_id)).ok_or("nothing kept for it")?;
        let saved = h.images.iter().map(|p| crate::uploads::Saved { path: p.clone(), mime: crate::uploads::mime_of(p).into() }).collect();
        self.send_saved(session_id, &h.text, saved, now).inspect_err(|e| {
            self.write(|st| {
                st.sessions.hold_back(session_id, &h.text, h.images.clone());
                st.sessions.held_failed(session_id, e);
            });
            self.changed();
        })
    }

    /// Enter once more, for a typed message the agent hasn't reported taking. Not when Cue can see the
    /// box and the text is gone from it (it was taken; a session without Cue's hooks never reports), and
    /// not into a dialog: a bare Enter there picks whatever is highlighted, "Yes" on a permission prompt.
    /// True when it was pressed.
    fn retry_enter(&self, origin: &Origin, text: &str) -> bool {
        // Only when the text is seen still in its box: a blind Enter (a screen Cue can't read) could answer a
        // prompt that came up meanwhile.
        if self.keys().text_in_box(origin, text) != Some(true) || self.keys().dialog_on(origin).is_some() {
            return false;
        }
        self.keys().press_enter(origin).is_ok()
    }

    /// The kept message goes off this thread (it types into a terminal): after `wait_ms`, for a
    /// terminal that was just stopped to settle.
    pub(super) fn send_held_soon(&self, session_id: &str, wait_ms: u64) {
        let sid = session_id.to_string();
        self.later(wait_ms, move |hub| {
            if let Err(e) = hub.send_held(&sid, false) {
                eprintln!("cue: couldn't send the message kept for {sid}: {e}");
            }
        });
    }

    /// The kept message, back to you (into the box, to change it): Esc, or Edit.
    pub fn unhold(&self, session_id: &str) -> Option<Exchange> {
        let h = self.write(|st| st.sessions.take_held(session_id));
        if h.is_some() {
            self.changed();
        }
        h
    }

    /// Hand a message (and saved images) to a session that listens directly (Pi). False if none does.
    fn send_direct(&self, session_id: &str, text: &str, saved: &[crate::uploads::Saved], now: bool) -> bool {
        // now: Pi takes it as a "steer", read mid-turn, instead of a follow-up after the turn.
        let msg = json!({ "type": "reply", "text": text, "images": saved, "now": now });
        self.read(|st| st.subscribers.get(session_id).map(|(_, tx)| tx.send(msg).is_ok()).unwrap_or(false))
    }

    /// What a subscribed session said its commands are (see `session_commands`).
    pub fn set_agent_commands(&self, session_id: &str, list: Vec<crate::commands::Cmd>) {
        self.write(|st| st.agent_commands.insert(session_id.to_string(), list));
    }

    pub fn agent_commands(&self, session_id: &str) -> Option<Vec<crate::commands::Cmd>> {
        self.read(|st| st.agent_commands.get(session_id).cloned())
    }

    /// Refresh: forget what the session reported and ask it again down its connection. False when
    /// it isn't connected (then there's nothing to wait for).
    pub fn ask_agent_commands(&self, session_id: &str) -> bool {
        self.write(|st| {
            st.agent_commands.remove(session_id);
            st.subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "commands" })).is_ok()).unwrap_or(false)
        })
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
            if self.read(|st| st.active_at.get(session_id).is_some_and(|t| *t >= since)) {
                return true;
            }
            crate::clock::sleep_ms(150);
        }
        false
    }

    /// A Claude Code turn just ended: type the rename it missed while busy (after its Stop hooks settle).
    /// Type one of the agent's own commands into its session (Compact: `/compact`; a relay lead's Hand off:
    /// `/relay:handoff`). Not mid-turn: typed then, it would land in the middle of its work.
    pub fn session_command(&self, session_id: &str, action: &str) -> Result<String, String> {
        let (origin, state) = self.read(|st| st.sessions.all().into_iter().find(|s| s.origin.session_id == session_id).map(|s| (s.origin, s.state))).ok_or("that session is gone")?;
        if state == "working" {
            return Err("It's working: do this when its turn ends".into());
        }
        let lead = || crate::leads::view(&std::iter::once(session_id.to_string()).collect()).get(session_id).is_some_and(|m| m["role"] == "lead" && m["plugin"] == "relay");
        let line = match action {
            "compact" => "/compact",
            "handoff" if !lead() => return Err("only a relay lead can hand off".into()),
            "handoff" => "/relay:handoff",
            // Relay mode on or off (the status line's Start relay / Stop relay, ⋯ Start relay): relay's own skills do it.
            "relay-on" if lead() => return Err("It's a relay lead already".into()),
            "relay-on" => "/relay:mode",
            "relay-off" if !lead() => return Err("It isn't a relay lead".into()),
            "relay-off" => "/relay:stop",
            _ => return Err(format!("Cue doesn't know \"{action}\"")),
        };
        self.keys().type_into(&origin, line)?;
        // Still in its box after a moment (it was busy drawing, or held the text): one more Enter, and if
        // it's still there, say so rather than show "Compacting" for the 15 minutes until Cue gives up,
        // holding every message meanwhile.
        crate::clock::sleep_ms(800);
        if self.keys().text_in_box(&origin, line) == Some(true) {
            let _ = self.keys().press_enter(&origin);
            crate::clock::sleep_ms(600);
            if self.keys().text_in_box(&origin, line) == Some(true) {
                return Err(format!("{line} is typed but it didn't take it: press Enter in its terminal"));
            }
        }
        if action == "compact" {
            self.write(|st| st.sessions.set_compacting(session_id, now_ms()));
            self.changed();
        }
        Ok(match action {
            "compact" => "Compacting: it summarizes the conversation to free up context".into(),
            "relay-on" => "Relay mode: it becomes a relay lead and hands work to helpers".into(),
            "relay-off" => "Relay mode off: it steps down as lead".into(),
            _ => "Relay handoff: it writes its notes, relay opens the new lead, then this one steps down".into(),
        })
    }

    /// A prompt you sent the session (not a background agent's notification): it shows as yours
    /// in the chat and labels what the session is working on.
    pub fn prompt_from_you(&self, session_id: &str, message: &str) {
        if message.trim().is_empty() {
            return;
        }
        self.write(|st| {
            let message = crate::sessions::unwrap_pasted(message);
            st.sessions.set_prompt(session_id, &message);
            st.sessions.note(session_id, "you", &message, crate::config::context_keep());
            // The agent has this text: a message of yours Cue was still unsure about is settled by it.
            st.sessions.got_message(session_id, &message, "its prompt hook");
        });
        self.changed();
    }

    /// Your message `message` to it was said by voice (its prompt hook added the line asking for a short spoken
    /// answer): its bubble says so.
    pub fn said_by_voice(&self, session_id: &str, message: &str) {
        self.write(|st| st.sessions.mark_voice(session_id, &crate::sessions::unwrap_pasted(message)));
        self.changed();
    }

    /// A turn ended: the status lists a prompt hook asked for with your recent messages (read from its transcript)
    /// go on those bubbles, as notes.
    pub fn status_asks_from(&self, session_id: &str, transcript_path: &str) {
        let asks = crate::logs::transcript::status_asks(transcript_path);
        let changed = self.write(|st| asks.iter().fold(false, |c, (text, ask)| st.sessions.mark_status_ask(session_id, &crate::sessions::unwrap_pasted(text), ask) || c));
        if changed {
            self.changed();
        }
    }

    /// A message another agent session sent this one: shown in its chat as from that session.
    pub fn prompt_from_peer(&self, session_id: &str, name: &str, pid: Option<i32>, body: &str) {
        if body.trim().is_empty() {
            return;
        }
        self.write(|st| {
            // The project Cue shows for the sender, else Claude's own name for it.
            let who = pid.and_then(|p| st.sessions.project_by_pid(p)).unwrap_or_else(|| if name.is_empty() { "another agent".into() } else { name.to_string() });
            st.sessions.note_peer(session_id, &who, body, crate::config::context_keep());
        });
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

/// A typed message Cue is watching for the agent to take (`Hub::confirm`), and what its looks have seen.
struct Watch {
    sid: String,
    origin: Origin,
    text: String,
    cmd_before: Option<usize>,
    /// When it was typed.
    t0: u64,
    /// When the watch started.
    start: u64,
    /// Past PENDING_MS when this look was scheduled: from then on only a verdict or LATE_MS ends the watch.
    late: bool,
    enters: u32,
    last_enter: u64,
    /// Looks in a row that saw it still in its box.
    seen_in_box: u32,
}

/// A message typed into a session mid-turn, queued: the agent reads it when it finishes its current step. The
/// same message as the thread's copy (same time), so the chat shows it once, as queued.
fn queue_typed(st: &mut Store, session_id: &str, text: &str, paths: Vec<String>) {
    let at = st.sessions.thread(session_id).iter().rev().find(|e| e.role == "you").map(|e| e.at_ms).unwrap_or_else(now_ms);
    let q = crate::model::Exchange { role: "you".into(), text: text.to_string(), at_ms: at, images: paths, from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![], voice: false, decision: false, status_ask: None };
    st.sessions.set_queued(session_id, Some(q));
}

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
pub(super) fn log_here(origin: &Origin) -> Option<&str> {
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
    fn a_message_waits_its_turn_but_not_for_good() {
        let gate = std::sync::Arc::new(Mutex::new(()));
        assert!(wait_turn(&gate).is_ok(), "free: at once");
        // Held a moment by the one before: it gets its turn.
        let g2 = gate.clone();
        let held = std::thread::spawn(move || { let _g = g2.lock().unwrap(); std::thread::sleep(std::time::Duration::from_millis(300)); });
        std::thread::sleep(std::time::Duration::from_millis(50));
        let t = std::time::Instant::now();
        assert!(wait_turn(&gate).is_ok());
        assert!(t.elapsed() >= std::time::Duration::from_millis(200));
        held.join().unwrap();
    }

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
        let held = |h: &Hub| h.read(|st| st.sessions.held("s5"));
        assert_eq!(held(&hub).map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()), "one kept message, the second after the first");
        assert!(hub.read(|st| st.sessions.thread("s5")).iter().all(|e| e.role != "you" || e.text == "refactor the router"), "not in the chat as sent");
        // Esc / Edit: back to you, and nothing kept any more.
        assert_eq!(hub.unhold("s5").map(|e| e.text), Some("also keep the old session\n\nand run the tests".into()));
        assert!(held(&hub).is_none() && hub.unhold("s5").is_none());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
