//! Sessions Cue hosts (docs/dev/cue-hosted-sessions.md): Claude Code started by Cue with no terminal. What one
//! reports comes in through the doors a hook's message uses (an ask, a turn started, a turn ended), so cards,
//! turns, Insights and the window treat it as any session; what you send it, answer it and Stop go to it
//! directly through the `Hosted` port instead of being typed.
//!
//! A hosted session records a process only while one runs (`agent_pid`): Cue quitting, or Claude exiting,
//! leaves it paused, with its conversation, and your next message starts it again (`--resume`).

use super::*;
use crate::hosted::{Agent, Answer as Given, Ask, AskKind, HostedEvent, InputAction, Program, Report, Spec, Start};
use std::sync::Weak;

/// What a hosted session's Claude starts with beyond its conversation.
#[derive(Clone, Debug, Default)]
pub struct HostedStart {
    /// As + New session's Permissions names it; empty: the agent's own default.
    pub permissions: String,
    /// More arguments (an MCP selection, a model), each whole.
    pub args: Vec<String>,
    /// For an agent that takes its model in what Cue sends it rather than as an argument (Codex). None: its own.
    pub model: Option<String>,
}

/// How long a paused hosted session Cue started again for a message has to take it (#857). Past this it didn't wake:
/// a resume can stall (a very large chat to read back), and the message would sit on "Sending…" for good. Under a
/// minute, so you hear within one.
pub(crate) const WAKE_LIMIT_MS: u64 = 50_000;

/// Claude Code's flag that starts it in fast mode.
pub(crate) fn fast_mode() -> [String; 2] {
    ["--settings".to_string(), r#"{"fastMode":true}"#.to_string()]
}

impl Hub {
    /// The hosted port, once the sessions an earlier Cue left running are picked up (picking_up.rs).
    pub(crate) fn hosted(&self) -> &dyn crate::hosted::Hosted {
        self.picking_up.wait();
        &*self.ports.hosted
    }

    /// Whether `e` is part of the turn a hosted Claude reports for Cue's `/rename` (its start, its one line, its end),
    /// which isn't shown: the rename is no work of its own. Anything else (a step, a question) means it isn't.
    fn renaming_turn(&self, sid: &str, e: &HostedEvent) -> bool {
        let Some(until) = self.read(|st| st.hosted_renaming.get(sid).copied()) else { return false };
        let ours = now_ms() < until;
        match e {
            HostedEvent::TurnStarted | HostedEvent::Said { agent: None, .. } if ours => true,
            HostedEvent::TurnEnded { .. } if ours => {
                self.write(|st| st.hosted_renaming.remove(sid));
                true
            }
            HostedEvent::TurnStarted | HostedEvent::Said { agent: None, .. } | HostedEvent::TurnEnded { .. } => {
                self.write(|st| st.hosted_renaming.remove(sid));
                false
            }
            _ => false,
        }
    }

    /// Start (`Start::New`) or resume a hosted session's Claude: `origin` with the process it now runs as.
    pub(crate) fn run_hosted(&self, origin: &Origin, start: Start, with: &HostedStart) -> Result<Origin, String> {
        let agent = Agent::named(&origin.harness).ok_or("Cue can only host Claude Code, Pi and Codex")?;
        // A message it was sending when its last process went (its call never came back): it didn't go.
        self.peer_sends_stopped(&origin.session_id);
        // A Claude lead starts with the team skill (team_out.rs), new or resumed: merged into a prompt you append; with
        // what its project counts one-way (Ask me before, desk/one_way.rs), as the setting is at this start.
        // A project's Desk lead (desk/hub/lead/run.rs) gets the Desk's instructions instead: it only hands work out, so no crew.
        let kinds = crate::desk::one_way::kinds(&super::crew_log::project_shown_for(origin));
        let skill = match self.desk_skill(&origin.session_id) {
            Some(desk) => [desk.to_string(), crate::desk::one_way::ask_first(&kinds)].join("\n").trim_end().to_string(),
            None => super::team::out::skill_for(&kinds),
        };
        // Your name (owner.rs), so what it writes about you uses it.
        let told = crate::owner::told_line();
        let skill = if told.is_empty() { skill } else { format!("{skill}\n{}", told.trim_end()) };
        let mut args = super::team::out::with_skill(agent, &with.args, &skill);
        // Its name in Claude Code's own list (what other sessions message it by, /resume): the one Cue shows,
        // unless it's given one here already (a fork's). Without it, Claude makes one up ("cue-5b").
        if agent == Agent::Claude && !args.iter().any(|a| a == "-n" || a == "--name") {
            let name = self.session_name(&origin.session_id);
            if !name.trim().is_empty() {
                args.extend(["-n".to_string(), name.trim().to_string()]);
            }
        }
        // A Desk lead runs in fast mode: turned on as it starts or resumes (hosted, Claude Code turns it on only when
        // its settings say so), and left to it after that. Other sessions start as they are.
        if agent == Agent::Claude && self.desk_skill(&origin.session_id).is_some() {
            args.extend(fast_mode());
        }
        let model = crate::config::load()["codex_models"][&origin.session_id].as_str()
            .filter(|_| agent == Agent::Codex).map(str::to_string).or_else(|| with.model.clone());
        let spec = Spec { agent, cwd: origin.cwd.clone().into(), start, permissions: with.permissions.clone(), model, args, program: Program::Agent, machine: origin.machine.clone() };
        let base = self.session_cost(&origin.session_id).unwrap_or(0.0);
        let started: crate::hosted::Started = self.hosted().start(&spec, self.hosted_report(origin.clone(), base))?;
        let transcript_path = match started.transcript {
            // On a machine, the agent's own file is there: Cue reads its copy here (machines/log_mirror.rs).
            Some(there) if !origin.machine.is_empty() => crate::machines::get(&origin.machine).map(|m| crate::machines::log_mirror::local_copy(&m, &there)).filter(|c| !c.is_empty()).unwrap_or_else(|| origin.transcript_path.clone()),
            Some(here) => here,
            None => origin.transcript_path.clone(),
        };
        Ok(Origin { agent_pid: Some(started.pid), hosted: true, session_id: started.session_id, transcript_path, ..origin.clone() })
    }

    /// `origin`'s conversation resumed hosted: on the model its log says it last used, with its MCP selection.
    /// The origin it now runs as. One whose conversation was never written (Claude writes it with the first message
    /// it takes) starts new under the same id: Claude can't resume what isn't there ("No conversation found").
    pub(crate) fn resume_hosted_as(&self, origin: &Origin) -> Result<Origin, String> {
        let sid = origin.session_id.as_str();
        // Pi and Codex pick their conversation up by its id (Pi starts it under that id if it was never written),
        // each on the model it last used: nothing to choose.
        if origin.harness != "claude" {
            return self.run_hosted(origin, Start::Resume(sid.to_string()), &HostedStart::default());
        }
        let log = if origin.transcript_path.is_empty() { crate::live::claude_transcript_path(&origin.cwd, sid) } else { origin.transcript_path.clone().into() };
        let written = if origin.machine.is_empty() { log.is_file() } else { written_on_machine(origin) };
        let start = if written { Start::Resume(sid.to_string()) } else { Start::New(sid.to_string()) };
        let mut args = crate::terminal::focus::model_args(&origin.transcript_path, "");
        args.extend(crate::logs::mcp::selection_flags(sid));
        // A fork whose own conversation isn't written yet starts as that fork again, named again, so it isn't empty.
        // On a machine that can't be asked, it counts as not written: forked again, its entry kept.
        let fork = fork_entry(sid);
        let fork_written = fork.is_some() && (origin.machine.is_empty() && written || !origin.machine.is_empty() && written_on_machine_known(origin) == Some(true));
        let start = if fork.is_some() && !fork_written { Start::New(sid.to_string()) } else { start };
        match fork {
            Some(f) if !fork_written => args.extend(["--resume".to_string(), fork_source(origin, f.from), "--fork-session".to_string(), "-n".to_string(), f.name]),
            Some(_) => forget_fork(sid),
            None => {}
        }
        // SHORTCUT: resumed in Claude's own default permission mode, not the one + New picked (it isn't kept), so a
        // session started on Auto asks more after a resume, never less. Keep the mode with the session (its record
        // or Origin) and pass it here when the trial says it matters.
        self.run_hosted(origin, start, &HostedStart { args, ..HostedStart::default() })
    }

    /// Your message to a hosted session. Noted in its chat as on its way before it goes (Claude can take it within
    /// milliseconds), handed over (the session started again first if it's paused), then settled by Claude itself:
    /// taken (`Taken`), or not sent if it couldn't be handed over or Claude exits first. Never waits on Claude.
    /// `busy`: it's mid-turn, so it's queued until Claude reads it at its next step.
    /// Under the session's typing gate, as `send_saved`: a message sent to it meanwhile (a reply, the box) waits
    /// until this one is handed over. Never called with the gate held (the gate's callers go by
    /// `send_to_hosted_held`).
    pub(crate) fn send_to_hosted(&self, sid: &str, typed: &str, saved: &[crate::uploads::Saved], busy: bool) -> Result<String, String> {
        // One message at a time into a session: another being sent to it goes first (see Hub::typing).
        let gate = self.typing_gate(sid);
        let _typing = super::send::wait_turn(&gate)?;
        self.send_to_hosted_held(sid, typed, saved, busy, None)
    }

    /// Cue's own words of kind `kind` to a hosted session, handed over as `send_to_hosted` does (queued to the agent
    /// when `busy`, never kept in Cue): known as Cue's first, so its chat never shows them as yours (#837).
    pub(crate) fn tell_hosted(&self, sid: &str, text: &str, busy: bool, kind: crate::sessions::Kind) -> Result<String, String> {
        self.cue_says(text, kind);
        self.send_to_hosted(sid, text, &[], busy)
    }

    /// The same, for the kept message `held` (send_held): its copy takes its place in the same write (#422).
    pub(crate) fn send_to_hosted_held(&self, sid: &str, typed: &str, saved: &[crate::uploads::Saved], busy: bool, held: Option<&crate::model::Exchange>) -> Result<String, String> {
        let text = typed.trim();
        // One of its agent's own commands (/config, /login): a terminal opens it, or it's refused; the session never
        // gets it (terminal_only.rs).
        if let Some(done) = self.run_own_command(sid, text) {
            return done;
        }
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        self.write(|st| {
            let t0 = now_ms();
            st.sessions.note_yours(sid, text, &paths, crate::config::context_keep());
            if let Some(h) = held {
                st.sessions.kept_went(sid, t0, h);
                // A Desk lead's changes in it: heard by the turn it starts (desk/hub/lead/feed.rs).
                crate::desk::hub::lead::feed::kept_went(st, sid, &h.text);
            }
            st.sessions.set_pending(sid, text, true);
            if busy {
                super::send::queue_typed(st, sid, text, paths.clone());
            }
        });
        self.changed();
        // A wake that failed (its agent wouldn't start again) is the session not waking, not the message refused.
        let mut wake_failed = false;
        let mut woke = None;
        let handed = self.hosted_origin(sid).ok_or_else(|| "that session is gone".to_string()).and_then(|o| {
            woke = self.wake_hosted(&o).inspect_err(|_| wake_failed = true)?;
            // On a machine, its images copied there first, so the paths in the message open there.
            let there = crate::uploads::on_machine(&o.machine, saved, &|_, _| {})?;
            let message = crate::uploads::with_paths(typed, &there);
            let config = crate::config::load();
            match config["codex_models"][sid].as_str().filter(|_| o.harness == "codex") {
                Some(model) => self.hosted().send_with_model(sid, &message, model),
                None => self.hosted().send(sid, &message),
            }
        });
        self.handed_over(sid, text, &handed, woke, wake_failed);
        handed.map(|()| if busy { "queued: it reads this at its next step".into() } else { "sent".into() })
    }

    /// Another session's message to hosted session `sid`, which was resting when that session's SendMessage looked
    /// for it (peer_sends.rs): started again and handed `body` the way Claude Code hands one over, from `from_name`
    /// (the name the sender's Claude goes by, so a SendMessage back reaches it). Its chat shows it from `sender`, never
    /// as yours. Under its typing gate, as any message to it: the process it was started again as, if it was. Err says
    /// why it didn't go, and whether it was the wake.
    pub(crate) fn hand_peer_to_hosted(&self, sid: &str, sender: &crate::sessions::peers::Sender, from_name: &str, body: &str) -> Result<Option<i32>, (bool, String)> {
        let gate = self.typing_gate(sid);
        let _typing = super::send::wait_turn(&gate).map_err(|e| (false, e))?;
        let o = self.hosted_origin(sid).ok_or((false, "that session is gone".to_string()))?;
        let woke = self.wake_hosted(&o).map_err(|e| (true, e))?;
        let message = peer_words(from_name, body);
        let handed = self.hosted().send(sid, &message);
        if handed.is_ok() {
            let keep = crate::config::context_keep();
            self.write(|st| st.sessions.note_peer_from(sid, sender, body, keep));
            self.changed();
        }
        // Started again for this: until it takes it, it isn't put back to rest (rest.rs). Its caller looks again at
        // WAKE_LIMIT (peer_sends.rs peer_wake_check).
        if let (Ok(()), Some(pid)) = (&handed, woke) {
            self.write(|st| st.hosted_waking.insert(sid.to_string(), pid));
        }
        handed.map(|()| woke).map_err(|e| (false, e))
    }

    /// How the hand-over of `text` to session `sid` went, in its chat: handed (`woke`: started again for it, as that
    /// process, which then has WAKE_LIMIT to take it, `wake_check`), not woken for (`wake_failed`: its agent wouldn't
    /// start again, #857), or not sent.
    fn handed_over(&self, sid: &str, text: &str, handed: &Result<(), String>, woke: Option<i32>, wake_failed: bool) {
        self.write(|st| match handed {
            Ok(()) => st.sessions.note_delivery(sid, text, if woke.is_some() { "handed to it directly, once started again" } else { "handed to it directly" }),
            Err(e) if wake_failed => {
                st.sessions.set_pending(sid, text, true);
                st.sessions.no_wake_pending(sid, &format!("it didn't wake: {e}"));
                st.sessions.set_queued(sid, None);
            }
            Err(e) => {
                st.sessions.mark_unsent(sid, text, e);
                st.sessions.set_queued(sid, None);
            }
        });
        self.changed();
        // Started again for this: it has WAKE_LIMIT to take it (`wake_check`).
        if let (Ok(()), Some(pid)) = (handed, woke) {
            self.write(|st| st.hosted_waking.insert(sid.to_string(), pid));
            let (sid, text) = (sid.to_string(), text.to_string());
            self.later(WAKE_LIMIT_MS, move |h| h.wake_check(&sid, &text, pid));
        }
        // Its Desk item's lead hears it (feed.rs holder_gone), off this send's typing gate.
        if wake_failed {
            self.later(0, |h| h.desk_feed_watch());
        }
    }

    /// A paused hosted session started again on its conversation, with the model it last used and its MCP
    /// selection, before a message goes to it: the process it now runs as. Nothing to do for one that runs (None).
    pub(crate) fn wake_hosted(&self, origin: &Origin) -> Result<Option<i32>, String> {
        let sid = origin.session_id.as_str();
        if self.hosted().running(sid) {
            return Ok(None);
        }
        let woken = self.resume_hosted_as(origin)?;
        // A new process: teammates of the one before ended with it, however that one went.
        self.write(|st| {
            st.sessions.set_pid(sid, woken.agent_pid);
            st.sessions.team_process_ended(sid, crate::clock::now_ms());
        });
        self.changed();
        Ok(woken.agent_pid)
    }

    /// WAKE_LIMIT after Cue started session `sid` again (as process `pid`) for `text`: still not taken, and no turn
    /// under way, it didn't wake (#857). Its process is closed (a late take would answer a message you've been told
    /// didn't go), what waited for it is marked so, with Try again and Send to a fresh session, and the lead of a
    /// Desk item it holds hears it. Taken meanwhile, ended (its exit said so), or started again since: nothing to do.
    pub(crate) fn wake_check(&self, sid: &str, text: &str, pid: i32) {
        let stalled = self.write(|st| {
            if st.hosted_waking.get(sid) != Some(&pid) {
                return false;
            }
            let working = st.sessions.state(sid).as_deref() == Some("working");
            if working || !st.sessions.is_pending(sid, text) {
                st.hosted_waking.remove(sid);
                return false;
            }
            st.hosted_waking.remove(sid);
            true
        });
        if !stalled {
            return;
        }
        self.hosted().close(sid);
        let secs = WAKE_LIMIT_MS / 1000;
        self.write(|st| {
            st.sessions.set_pid(sid, None);
            st.sessions.team_process_ended(sid, crate::clock::now_ms());
            st.sessions.no_wake_pending(sid, &format!("it didn't wake: started again, it took nothing in {secs}s"));
            st.sessions.set_queued(sid, None);
        });
        self.changed();
        self.desk_feed_watch();
    }

    /// Hosted sessions an earlier Cue left running under cue-host, picked up (at launch): each gets its process
    /// back and, mid-turn, shows as working again; what it said while Cue was away comes in as it would have, and
    /// an ask it still waits on is a card again.
    pub(crate) fn reattach_hosted(&self) {
        let hosted: Vec<Origin> = self.read(|st| st.sessions.all().into_iter().map(|s| s.origin).filter(|o| o.hosted).collect());
        for o in hosted {
            let Some(agent) = Agent::named(&o.harness) else { continue };
            let spec = Spec { agent, cwd: o.cwd.clone().into(), start: Start::Resume(o.session_id.clone()), permissions: String::new(), model: None, args: vec![], program: Program::Agent, machine: o.machine.clone() };
            // Costs aren't kept across restarts: this run's count from zero, as a resumed one's does.
            let Some(back) = self.ports.hosted.reattach(&spec, self.hosted_report(o.clone(), 0.0)) else { continue };
            // Its process noted first: what it reports next (an ask again, what it said meanwhile) finds it.
            if let Some(started) = &back.started {
                self.write(|st| st.sessions.set_pid(&o.session_id, Some(started.pid)));
                if started.in_turn {
                    self.mark_state(&Origin { agent_pid: Some(started.pid), ..o.clone() }, "working");
                }
            }
            (back.go)();
        }
        self.ports.hosted.settled();
        self.changed();
    }

    /// The session's origin when Cue hosts it.
    pub(crate) fn hosted_origin(&self, session_id: &str) -> Option<Origin> {
        self.read(|st| st.sessions.origin(session_id)).filter(|o| o.hosted)
    }

    /// What a session reports, into the hub, for as long as the hub is there. `base`: what the session had
    /// cost before this process (a resumed one counts its cost from zero).
    fn hosted_report(&self, origin: Origin, base: f64) -> Report {
        let hub: Option<Weak<Hub>> = self.shared().map(|h| Arc::downgrade(&h));
        Box::new(move |id, e| {
            if let Some(h) = hub.as_ref().and_then(Weak::upgrade) {
                h.on_hosted(&Origin { session_id: id.to_string(), ..origin.clone() }, base, e);
            }
        })
    }

    /// One thing a hosted session said, handled. `o`: its origin as it was started (the store's is used once
    /// there is one: it has the process it runs as).
    pub(crate) fn on_hosted(&self, o: &Origin, base: f64, e: HostedEvent) {
        let origin = self.hosted_origin(&o.session_id).unwrap_or_else(|| o.clone());
        if self.renaming_turn(&origin.session_id, &e) {
            return;
        }
        // Its team (team.rs): a teammate started, at work, finished.
        self.team_sees(&origin, &e);
        #[cfg(feature = "ext")]
        self.ext_sees(&origin, &e);
        // Its own messages to other sessions (peer_sends.rs): in its chat as they go out, and how each did.
        match &e {
            HostedEvent::ToolCall { id, name, input, agent: None } if name == "SendMessage" => self.peer_send_out(&origin, id, input),
            HostedEvent::ToolResult { id, output, failed, agent: None } => self.peer_send_back(&origin, id, output, *failed),
            HostedEvent::Exited { .. } => self.peer_sends_stopped(&origin.session_id),
            _ => {}
        }
        match e {
            HostedEvent::TurnStarted => self.hosted_turn_started(origin),
            HostedEvent::Taken { text } => self.hosted_taken(origin, text),
            HostedEvent::NotTaken { text, why } => self.hosted_not_taken(&origin, &text, &why),
            // Its own lines only: a teammate's (`agent`) and its tasks' news come while its turn has ended too.
            HostedEvent::Said { agent: None, .. } | HostedEvent::ToolCall { agent: None, .. } | HostedEvent::ToolResult { agent: None, .. } => self.hosted_at_work(origin),
            HostedEvent::Said { .. } | HostedEvent::ToolCall { .. } | HostedEvent::ToolResult { .. } | HostedEvent::Task { .. } => {}
            HostedEvent::Rates(rl) => self.set_rates(&rl),
            e @ (HostedEvent::Compacting { .. } | HostedEvent::CompactFailed) => self.hosted_compacted(&origin.session_id, &e),
            HostedEvent::Suggested { text } => self.hosted_suggested(&origin, &text),
            HostedEvent::Ask(a) => self.hosted_ask(origin, a),
            HostedEvent::ApiError { error, text } => {
                self.write(|st| st.hosted_api_error.insert(origin.session_id.clone(), (error, text)));
            }
            HostedEvent::TurnEnded { text, failed, cost_usd, unprompted } => self.hosted_turn_ended(origin, base + cost_usd, text, failed, unprompted),
            HostedEvent::Exited { code, during_turn, last_words } => {
                #[cfg(feature = "ext")]
                let sid = origin.session_id.clone();
                self.hosted_exited(origin, code, during_turn, &last_words);
                #[cfg(feature = "ext")]
                if during_turn {
                    self.ext_turn_ended(&sid, super::ext::TurnEnd::Exited(last_words));
                }
            }
            HostedEvent::Refused { subtype, .. } => eprintln!("cue: hosted session {} asked for a “{subtype}”, which Cue can't answer: refused", o.session_id),
            HostedEvent::Started { terminal_only, .. } => self.note_terminal_only(&origin.session_id, terminal_only),
        }
    }

    /// A hosted turn is over (`cost`: the session's cost so far): its reply, the stop Cue made, or an API error (a
    /// usage limit leaves it limited, with Cue's carry-on, as a terminal session's StopFailure does).
    fn hosted_turn_ended(&self, origin: Origin, cost: f64, text: String, failed: bool, unprompted: bool) {
        // A message to another session still going out as its turn ends: its call's result never came.
        self.peer_sends_stopped(&origin.session_id);
        self.set_cost(&origin.session_id, cost);
        let (stopped, api_error) = self.write(|st| (st.hosted_stopped.remove(&origin.session_id), st.hosted_api_error.remove(&origin.session_id)));
        if let Some((error, said)) = api_error {
            // Its turn ended on an API error (a usage limit, say): handled as a terminal session's StopFailure.
            let said = if said.trim().is_empty() { text } else { said };
            #[cfg(feature = "ext")]
            let sid = origin.session_id.clone();
            self.failed(origin, &error, &said);
            #[cfg(feature = "ext")]
            self.ext_turn_ended(&sid, super::ext::TurnEnd::Failed(said));
            return;
        }
        if failed && stopped.is_some() {
            // The turn Cue stopped, ending as Claude ends one ("[ede_diagnostic] …"): not an error. Stopped
            // by you, it stays stopped (a step it was on may have brought it back to work meanwhile).
            if stopped == Some(true) {
                self.write(|st| st.sessions.mark(&origin, "stopped", None));
                self.changed();
                #[cfg(feature = "ext")]
                self.ext_turn_ended(&origin.session_id, super::ext::TurnEnd::Stopped);
            }
            return;
        }
        #[cfg(feature = "ext")]
        let (sid, ended) = (origin.session_id.clone(), if failed { super::ext::TurnEnd::Failed(text.clone()) } else { super::ext::TurnEnd::Done(text.clone()) });
        if failed {
            // A lead's Away sweep that failed won't be read: the window stops waiting on it.
            if self.desk_away_turn_over(&origin.session_id, true) {
                self.changed();
            }
            self.failed(origin, "error", &text);
        } else {
            let turn = turn_parts(&origin.transcript_path, &text);
            let lead_turn = super::team::review::LeadTurn::read(&origin.transcript_path, &turn, &text, unprompted);
            self.team_reply(&origin.session_id, &lead_turn);
            let (lead, all, parts) = (origin.session_id.clone(), lead_turn.all, turn.clone());
            self.event(origin, "stopped", text, turn, "");
            // Its judgment calls (DECIDED: lines) once its reply is in its chat, at the reply's time (crew_log.rs).
            self.crew_calls_noted(&lead, &all);
            // Its Desk lines (desk.rs): a lead's, any session's ITEM lines, a holder's.
            self.desk_turn_ended(&lead, &all, &parts);
        }
        #[cfg(feature = "ext")]
        self.ext_turn_ended(&sid, ended);
    }

    /// It started compacting, or finished, or couldn't (CompactFailed: the Desk then hands it off, cache_tick.rs).
    fn hosted_compacted(&self, sid: &str, e: &HostedEvent) {
        let (done, failed) = match e {
            HostedEvent::Compacting { done } => (*done, false),
            _ => (true, true),
        };
        if failed {
            self.write(|st| st.desk.run.compact_failed.insert(sid.to_string(), now_ms()));
        } else if done {
            self.write(|st| st.desk.run.compact_failed.remove(sid));
        }
        self.hosted_compacting(sid, done);
    }

    /// Claude didn't take a message: it shows as unsent, and nothing waits queued for it.
    fn hosted_not_taken(&self, origin: &Origin, text: &str, why: &str) {
        self.write(|st| {
            st.sessions.mark_unsent(&origin.session_id, text, why);
            st.sessions.set_queued(&origin.session_id, None);
        });
        self.changed();
    }

    /// Claude's guess at what you'd type next. Only for a session back at its prompt: a turn already under way has
    /// moved past it.
    /// Compacting: not waiting on you meanwhile, and what you send is kept for when it's done. Done: noted, and
    /// a message kept for it goes now (unless a turn has started).
    fn hosted_compacting(&self, sid: &str, done: bool) {
        if !done {
            return self.on_compacting(sid);
        }
        let send_kept = self.write(|st| {
            if st.sessions.compacting_since(sid) == 0 {
                return false;
            }
            st.sessions.set_compacted(sid, now_ms());
            st.sessions.held(sid).is_some() && st.sessions.state(sid).as_deref() != Some("working")
        });
        self.changed();
        if send_kept {
            self.send_held_soon(sid, 0);
        }
    }

    fn hosted_suggested(&self, origin: &Origin, text: &str) {
        if self.write(|st| st.sessions.set_suggestion(&origin.session_id, text)) {
            self.changed();
        }
    }

    /// Claude took a message: at once when it was idle (a turn starts with it), at its next step when it was
    /// at work (the turn goes on with it). Only Cue writes to it, and Cue notes in the thread whatever it sends
    /// (as it's sent, which may be just after this): nothing to note here, only the message to settle.
    fn hosted_taken(&self, origin: Origin, text: String) {
        let sid = origin.session_id.clone();
        // A fork's first message taken: its own conversation is written now, so it resumes as itself from here on.
        if origin.machine.is_empty() && fork_entry(&sid).is_some() && written_as(&origin, &sid) == Some(true) {
            forget_fork(&sid);
        }
        self.write(|st| st.hosted_waking.remove(&sid));
        if self.read(|st| st.sessions.state(&sid).as_deref() == Some("working")) {
            // Nothing settled: the message that started this turn, already taken when the turn started.
            if self.write(|st| st.sessions.got_message(&sid, &text, "it took it mid-turn")) {
                self.write(|st| st.sessions.set_queued(&sid, None));
                self.changed();
            }
            return;
        }
        self.event(origin, "active", text.clone(), vec![], "");
        // Another session's message (one Cue handed it while it rested) is that session's, not the prompt of a turn
        // of yours.
        if crate::logs::transcript::handed_over_peer(&text) {
            return self.changed();
        }
        self.write(|st| st.sessions.set_prompt(&sid, &crate::sessions::unwrap_pasted(&text)));
        self.changed();
    }

    /// A turn began: the oldest message of yours on its way is the one it read (Cue sends
    /// it nothing else), taken now rather than when it repeats it back with its first words, a second or more
    /// later. A turn a background task started finds none, and goes to work with its first step.
    /// Still working to Cue, it's the turn after one Cue stopped for Send now: the message it reads now is the
    /// one Cue sent, and the turn is about it.
    fn hosted_turn_started(&self, origin: Origin) {
        let sid = origin.session_id.clone();
        // An API error from a turn whose end never came is that turn's, not this one's.
        self.write(|st| st.hosted_api_error.remove(&sid));
        let (first, working) = self.read(|st| (st.sessions.first_pending(&sid), st.sessions.state(&sid).as_deref() == Some("working")));
        let Some(text) = first else { return };
        if !working {
            return self.hosted_taken(origin, text);
        }
        self.write(|st| {
            st.sessions.got_message(&sid, &text, "it started a turn with it");
            st.sessions.set_queued(&sid, None);
            st.sessions.set_prompt(&sid, &crate::sessions::unwrap_pasted(&text));
        });
        self.changed();
    }

    /// It's doing something: at work, if nothing of yours started that (a background task that finished
    /// can start a turn of its own). Not while it waits on you: an ask's answer puts it back to work. Never for its
    /// background agents' work (#473): they go on after its turn ends, and a session taken for mid-turn by them
    /// keeps your message for a "Send now", whose stop ends them too.
    pub(super) fn hosted_at_work(&self, origin: Origin) {
        let idle = self.read(|st| matches!(st.sessions.state(&origin.session_id).as_deref(), Some("waiting" | "stopped")));
        if idle {
            self.event(origin, "active", String::new(), vec![], "");
        }
    }

    /// A permission, question, network request or MCP form: a card, as a hook's ask is, whose answer goes
    /// back to the session once you give it (or that it's gone, if the card goes unanswered).
    fn hosted_ask(&self, origin: Origin, a: Ask) {
        let input = a.kind == AskKind::Input;
        let sid = origin.session_id.clone();
        let (kind, tool_name, tool_input) = match a.kind {
            AskKind::Permission | AskKind::Network => ("permission", a.tool_name, a.input),
            AskKind::Question => ("question", a.tool_name, a.input),
            AskKind::Input => ("input", a.server.clone(), json!({ "server": a.server, "message": a.message, "form": a.form })),
            // Ask me before (guard.rs): a card when its Desk asks first about it, else "pass" at once.
            AskKind::Guard => {
                let card = self.hosted_guard(origin, &a.request_id, a.tool_name, a.input, &a.message);
                let Some(card) = card else {
                    let pass = Given::Decision(Decision { behavior: "pass".into(), ..Default::default() });
                    if let Err(e) = self.ports.hosted.answer(&sid, &a.request_id, &pass) {
                        eprintln!("cue: couldn't answer hosted session {sid}: {e}");
                    }
                    return;
                };
                let Some((card, answer)) = card else { return };
                return self.answer_hosted_when_given(sid, a.request_id, card, false, answer);
            }
        };
        let ask = NewAsk { kind: kind.into(), tool_name, tool_input, suggestions: a.suggestions, context: vec![], agent_id: a.agent_id };
        // Its card is already open: the one waiting answers it.
        let Some((card, answer)) = self.add_keyed_ask(origin, &a.request_id, ask) else { return };
        self.answer_hosted_when_given(sid, a.request_id, card, input, answer);
    }

    /// Hosted session `sid`'s request answered once you answer its card.
    fn answer_hosted_when_given(&self, sid: String, request_id: String, card: String, input: bool, answer: oneshot::Receiver<Reply>) {
        crate::approval_trace::record(&sid, &request_id, &card, "requested");
        let hosted = self.ports.hosted.clone();
        // Waiting for your answer is the card's to do, not the session's: its next lines keep coming meanwhile.
        std::thread::spawn(move || {
            let given = given(input, answer.blocking_recv());
            if let Err(e) = hosted.answer(&sid, &request_id, &given) {
                crate::approval_trace::record(&sid, &request_id, &card, "write_failed");
                eprintln!("cue: couldn't answer hosted session {sid}: {e}");
            } else {
                crate::approval_trace::record(&sid, &request_id, &card, "handed_to_transport");
            }
        });
    }

    /// Claude exited by itself (Cue closing it says nothing). The session stays, paused: its cards can't be
    /// answered any more, and your next message starts it again. Mid-turn, that turn is over, and its card
    /// says why.
    fn hosted_exited(&self, origin: Origin, code: Option<i32>, during_turn: bool, last_words: &str) {
        let sid = origin.session_id.clone();
        let asks: Vec<String> = self.read(|st| st.items.iter().filter(|i| i.origin.session_id == sid && i.kind != "waiting").map(|i| i.id.clone()).collect());
        for id in asks {
            self.finish(&id, "gone", "Claude Code exited");
        }
        let paused = Origin { agent_pid: None, ..origin };
        // Started again for a message and gone before taking it: it didn't wake (#857).
        let waking = self.write(|st| {
            let waking = st.hosted_waking.remove(&sid).is_some() && st.sessions.first_pending(&sid).is_some();
            st.sessions.set_pid(&sid, None);
            st.sessions.team_process_ended(&sid, crate::clock::now_ms());
            if waking {
                let code = code.map_or("a signal".to_string(), |c| format!("code {c}"));
                st.sessions.no_wake_pending(&sid, &format!("it didn't wake: Claude Code exited ({code}) before taking it"));
            } else {
                st.sessions.unsend_pending(&sid, "Claude Code exited first");
            }
            st.sessions.set_queued(&sid, None);
            waking
        });
        if waking {
            self.desk_feed_watch();
        }
        if during_turn {
            let why = last_words.lines().rev().find(|l| !l.trim().is_empty()).map(str::trim).unwrap_or("");
            let code = code.map_or("a signal".to_string(), |c| format!("code {c}"));
            let what = if why.is_empty() { format!("Claude Code exited ({code}); send a message to start it again") } else { format!("Claude Code exited ({code}): {why}. Send a message to start it again") };
            self.failed(paused, "exited", &what);
        } else {
            self.changed();
        }
    }
}

/// Another session's message as Claude Code hands one over, from `from_name`: the receiving Claude knows who sent it
/// and that a SendMessage to that name answers it, and Cue reads it as a peer's, never yours (transcript::handed_over_peer).
pub(crate) fn peer_words(from_name: &str, body: &str) -> String {
    let name = from_name.replace('"', "'");
    format!("Another Claude session sent a message:\n<cross-session-message from-name=\"{name}\">\n{}\n</cross-session-message>", body.trim())
}

/// What it wrote this turn, from its transcript, as a terminal session's Stop hook reports it: so the answer
/// shows, and what a Stop hook of yours sent it back to write (a checklist) folds away under it.
fn turn_parts(transcript: &str, last: &str) -> Vec<crate::model::TurnPart> {
    if transcript.is_empty() {
        return vec![];
    }
    let p = json!({ "transcript_path": transcript, "last_assistant_message": last });
    crate::hook::final_turn(&p).into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

/// Your answer to a card, as the session takes it. An MCP form: accepted with what you filled in (allow),
/// declined (deny), or dismissed (anything else, or the card gone). Anything else: your decision; a card that
/// went without one is a denial saying why, so Claude never waits on it.
fn given(input: bool, reply: Result<Reply, oneshot::error::RecvError>) -> Given {
    match (input, reply) {
        (true, Ok(Reply::Decision(d))) => {
            let action = match d.behavior.as_str() {
                "allow" | "allow_always" => InputAction::Accept,
                "deny" => InputAction::Decline,
                _ => InputAction::Cancel,
            };
            Given::Input { action, content: d.answers.unwrap_or_else(|| json!({})) }
        }
        (true, _) => Given::Input { action: InputAction::Cancel, content: Value::Null },
        (false, Ok(Reply::Decision(d))) => Given::Decision(d),
        (false, Ok(Reply::Cancel(why))) => Given::Decision(Decision { behavior: "deny".into(), message: Some(format!("Not answered in Cue: {why}")), ..Default::default() }),
        (false, Err(_)) => Given::Decision(Decision { behavior: "deny".into(), message: Some("Cue stopped waiting for an answer".into()), ..Default::default() }),
    }
}

/// Whether a session on a machine has a conversation there to resume. Its log here is a copy: anything in the copy
/// says so; an empty or missing one is asked of the machine (the copy can lag behind). One the machine can't be asked
/// about is taken to be there (it ran: a terminal session moved in, say), so a resume, not a second conversation.
fn written_on_machine(origin: &Origin) -> bool {
    written_on_machine_known(origin).unwrap_or(true)
}

/// As `written_on_machine`, but `None` when the machine couldn't be asked. A fork's entry is only dropped on a yes:
/// dropped on a guess, a fork not yet written would come back empty.
fn written_on_machine_known(origin: &Origin) -> Option<bool> {
    if std::fs::metadata(&origin.transcript_path).is_ok_and(|m| m.len() > 0) {
        return Some(true);
    }
    let m = crate::machines::get(&origin.machine)?;
    if origin.transcript_path.is_empty() {
        return Some(true);
    }
    crate::machines::log_mirror::written_there(&m, &origin.transcript_path)
}

/// A fork not yet written (`hosted.forked_from` in Settings): the session it forks, and its name. A fork is a copy of
/// that session only once Claude has written the fork's own conversation, which it does with the first message it
/// takes; until then a fork started again is forked again (`resume_hosted_as`).
pub(crate) struct Fork {
    pub from: String,
    pub name: String,
}

/// Fork `sid` of `from`, named `name`, kept until its conversation is written. Forks noted more than `CLOSED_DAYS`
/// ago go: Closed no longer offers them.
pub(crate) fn note_fork(sid: &str, from: &str, name: &str) {
    let now = now_ms();
    let _ = crate::config::update_with("hosted.forked_from", |c| {
        let mut all = c.pointer("/hosted/forked_from").and_then(Value::as_object).cloned().unwrap_or_default();
        all.retain(|_, v| v["at"].as_u64().is_some_and(|at| now.saturating_sub(at) < super::CLOSED_DAYS * 86_400_000));
        all.insert(sid.to_string(), json!({ "from": from, "name": name, "at": now }));
        Ok((Some(Value::Object(all)), ()))
    });
}

pub(crate) fn fork_entry(sid: &str) -> Option<Fork> {
    let v = crate::config::load().pointer("/hosted/forked_from").and_then(|m| m.get(sid)).cloned()?;
    Some(Fork { from: v["from"].as_str()?.to_string(), name: v["name"].as_str().unwrap_or_default().to_string() })
}

/// Fork `sid`'s entry gone: its conversation is written, or it was deleted.
pub(crate) fn forget_fork(sid: &str) {
    if fork_entry(sid).is_none() {
        return;
    }
    let _ = crate::config::update_with("hosted.forked_from", |c| {
        let mut all = c.pointer("/hosted/forked_from").and_then(Value::as_object).cloned().unwrap_or_default();
        Ok((all.remove(sid).map(|_| Value::Object(all)), ()))
    });
}

/// The conversation a fork of `origin`'s session is copied from: `from`, or, when `from` is itself a fork not yet
/// written, the one it forks, and so on to the first written one.
pub(crate) fn fork_source(origin: &Origin, from: String) -> String {
    let mut from = from;
    for _ in 0..20 {
        match fork_entry(&from) {
            Some(f) if written_as(origin, &from) != Some(true) => from = f.from,
            Some(_) => {
                forget_fork(&from);
                break;
            }
            None => break,
        }
    }
    from
}

/// Whether Claude has written conversation `id` beside `origin`'s (a fork's is in the same folder, here or on its
/// machine).
fn written_as(origin: &Origin, id: &str) -> Option<bool> {
    let mine = if origin.transcript_path.is_empty() { crate::live::claude_transcript_path(&origin.cwd, &origin.session_id) } else { origin.transcript_path.clone().into() };
    let path = mine.with_file_name(format!("{id}.jsonl"));
    if origin.machine.is_empty() {
        return Some(path.is_file());
    }
    written_on_machine_known(&Origin { transcript_path: path.to_string_lossy().into_owned(), ..origin.clone() })
}
