//! Sessions Cue hosts (docs/dev/cue-hosted-sessions.md): Claude Code started by Cue with no terminal. What one
//! reports comes in through the doors a hook's message uses (an ask, a turn started, a turn ended), so cards,
//! turns, Insights and the window treat it as any session; what you send it, answer it and Stop go to it
//! directly through the `Hosted` port instead of being typed.
//!
//! A hosted session records a process only while one runs (`agent_pid`): Cue quitting, or Claude exiting,
//! leaves it paused, with its conversation, and your next message starts it again (`--resume`).

use super::*;
use crate::hosted::{Answer as Given, Ask, AskKind, HostedEvent, InputAction, Program, Report, Spec, Start};
use std::sync::Weak;

/// What a hosted session's Claude starts with beyond its conversation.
#[derive(Clone, Debug, Default)]
pub struct HostedStart {
    /// As + New session's Permissions names it; empty: Claude's own default.
    pub permissions: String,
    /// More arguments (an MCP selection, a model), each whole.
    pub args: Vec<String>,
}

impl Hub {
    pub(crate) fn hosted(&self) -> &dyn crate::hosted::Hosted {
        &*self.ports.hosted
    }

    /// Start (`Start::New`) or resume a hosted session's Claude: `origin` with the process it now runs as.
    pub(crate) fn run_hosted(&self, origin: &Origin, start: Start, with: &HostedStart) -> Result<Origin, String> {
        let spec = Spec { cwd: origin.cwd.clone().into(), start, permissions: with.permissions.clone(), model: None, args: with.args.clone(), program: Program::Claude };
        let base = self.session_cost(&origin.session_id).unwrap_or(0.0);
        let pid = self.hosted().start(&spec, self.hosted_report(origin.clone(), base))?;
        Ok(Origin { agent_pid: Some(pid), hosted: true, ..origin.clone() })
    }

    /// `origin`'s conversation resumed hosted: on the model its log says it last used, with its MCP selection.
    /// The origin it now runs as. One whose conversation was never written (Claude writes it with the first message
    /// it takes) starts new under the same id: Claude can't resume what isn't there ("No conversation found").
    pub(crate) fn resume_hosted_as(&self, origin: &Origin) -> Result<Origin, String> {
        let sid = origin.session_id.as_str();
        let log = if origin.transcript_path.is_empty() { crate::live::claude_transcript_path(&origin.cwd, sid) } else { origin.transcript_path.clone().into() };
        let start = if log.is_file() { Start::Resume(sid.to_string()) } else { Start::New(sid.to_string()) };
        let mut args = crate::terminal::focus::model_args(&origin.transcript_path, "");
        args.extend(crate::logs::mcp::selection_flags(sid));
        // SHORTCUT: resumed in Claude's own default permission mode, not the one + New picked (it isn't kept), so a
        // session started on Auto asks more after a resume, never less. Keep the mode with the session (its record
        // or Origin) and pass it here when the trial says it matters.
        self.run_hosted(origin, start, &HostedStart { permissions: String::new(), args })
    }

    /// Your message to a hosted session. Noted in its chat as on its way before it goes (Claude can take it within
    /// milliseconds), handed over (the session started again first if it's paused), then settled by Claude itself:
    /// taken (`Taken`), or not sent if it couldn't be handed over or Claude exits first. Never waits on Claude.
    /// `busy`: it's mid-turn, so it's queued until Claude reads it at its next step.
    pub(crate) fn send_to_hosted(&self, sid: &str, typed: &str, saved: &[crate::uploads::Saved], busy: bool) -> Result<String, String> {
        let text = typed.trim();
        let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
        self.write(|st| {
            st.sessions.note_sent(sid, text, &paths, now_ms(), crate::config::context_keep());
            st.sessions.set_pending(sid, text, true);
            if busy {
                super::send::queue_typed(st, sid, text, paths.clone());
            }
        });
        self.changed();
        let handed = self.hosted_origin(sid).ok_or_else(|| "that session is gone".to_string()).and_then(|o| {
            self.wake_hosted(&o)?;
            self.hosted().send(sid, &crate::uploads::with_paths(typed, saved))
        });
        self.write(|st| match &handed {
            Ok(()) => st.sessions.note_delivery(sid, text, "handed to it directly"),
            Err(e) => {
                st.sessions.mark_unsent(sid, text, e);
                st.sessions.set_queued(sid, None);
            }
        });
        self.changed();
        handed.map(|()| if busy { "queued: it reads this at its next step".into() } else { "sent".into() })
    }

    /// A paused hosted session started again on its conversation, with the model it last used and its MCP
    /// selection, before a message goes to it. Nothing to do for one that runs.
    pub(crate) fn wake_hosted(&self, origin: &Origin) -> Result<(), String> {
        let sid = origin.session_id.as_str();
        if self.hosted().running(sid) {
            return Ok(());
        }
        let woken = self.resume_hosted_as(origin)?;
        self.write(|st| st.sessions.set_pid(sid, woken.agent_pid));
        self.changed();
        Ok(())
    }

    /// The session's origin when Cue hosts it.
    pub(crate) fn hosted_origin(&self, session_id: &str) -> Option<Origin> {
        self.read(|st| st.sessions.origin(session_id)).filter(|o| o.hosted)
    }

    /// What a session reports, into the hub, for as long as the hub is there. `base`: what the session had
    /// cost before this process (a resumed one counts its cost from zero).
    fn hosted_report(&self, origin: Origin, base: f64) -> Report {
        let hub: Option<Weak<Hub>> = self.shared().map(|h| Arc::downgrade(&h));
        Box::new(move |e| {
            if let Some(h) = hub.as_ref().and_then(Weak::upgrade) {
                h.on_hosted(&origin, base, e);
            }
        })
    }

    /// One thing a hosted session said, handled. `o`: its origin as it was started (the store's is used once
    /// there is one: it has the process it runs as).
    pub(crate) fn on_hosted(&self, o: &Origin, base: f64, e: HostedEvent) {
        let origin = self.hosted_origin(&o.session_id).unwrap_or_else(|| o.clone());
        match e {
            HostedEvent::Taken { text } => self.hosted_taken(origin, text),
            HostedEvent::Said { .. } | HostedEvent::ToolCall { .. } | HostedEvent::ToolResult { .. } | HostedEvent::Task { .. } => self.hosted_at_work(origin),
            HostedEvent::Ask(a) => self.hosted_ask(origin, a),
            HostedEvent::TurnEnded { text, failed, cost_usd, .. } => {
                self.set_cost(&origin.session_id, base + cost_usd);
                let driver = crate::leads::resolve_driver(&origin.session_id, "");
                if failed {
                    self.failed(origin, "error", &text);
                } else {
                    self.event(origin, "stopped", text, vec![], &driver);
                }
            }
            HostedEvent::Exited { code, during_turn, last_words } => self.hosted_exited(origin, code, during_turn, &last_words),
            HostedEvent::Refused { subtype, .. } => eprintln!("cue: hosted session {} asked for a “{subtype}”, which Cue can't answer: refused", o.session_id),
            HostedEvent::Started { .. } => {}
        }
    }

    /// Claude took a message: at once when it was idle (a turn starts with it), at its next step when it was
    /// at work (the turn goes on with it). Only Cue writes to it, and Cue notes in the thread whatever it sends
    /// (as it's sent, which may be just after this): nothing to note here, only the message to settle.
    fn hosted_taken(&self, origin: Origin, text: String) {
        let sid = origin.session_id.clone();
        if self.read(|st| st.sessions.state(&sid).as_deref() == Some("working")) {
            self.write(|st| {
                st.sessions.got_message(&sid, &text, "it took it mid-turn");
                st.sessions.set_queued(&sid, None);
            });
            self.changed();
            return;
        }
        let driver = crate::leads::resolve_driver(&sid, "");
        self.event(origin, "active", text.clone(), vec![], &driver);
        self.write(|st| st.sessions.set_prompt(&sid, &crate::sessions::unwrap_pasted(&text)));
        self.changed();
    }

    /// It's doing something: at work, if nothing of yours started that (a background task that finished
    /// can start a turn of its own). Not while it waits on you: an ask's answer puts it back to work.
    fn hosted_at_work(&self, origin: Origin) {
        let idle = self.read(|st| matches!(st.sessions.state(&origin.session_id).as_deref(), Some("waiting" | "stopped")));
        if idle {
            let driver = crate::leads::resolve_driver(&origin.session_id, "");
            self.event(origin, "active", String::new(), vec![], &driver);
        }
    }

    /// A permission, question, network request or MCP form: a card, as a hook's ask is, whose answer goes
    /// back to the session once you give it (or that it's gone, if the card goes unanswered).
    fn hosted_ask(&self, origin: Origin, a: Ask) {
        let input = a.kind == AskKind::Input;
        let (kind, tool_name, tool_input) = match a.kind {
            AskKind::Permission | AskKind::Network => ("permission", a.tool_name, a.input),
            AskKind::Question => ("question", a.tool_name, a.input),
            AskKind::Input => ("input", a.server.clone(), json!({ "server": a.server, "message": a.message, "form": a.form })),
        };
        let sid = origin.session_id.clone();
        let (_, answer) = self.add_ask(origin, kind.into(), tool_name, tool_input, a.suggestions, vec![], a.agent_id);
        let hosted = self.ports.hosted.clone();
        let request_id = a.request_id;
        // Waiting for your answer is the card's to do, not the session's: its next lines keep coming meanwhile.
        std::thread::spawn(move || {
            let given = given(input, answer.blocking_recv());
            if let Err(e) = hosted.answer(&sid, &request_id, &given) {
                eprintln!("cue: couldn't answer hosted session {sid}: {e}");
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
        self.write(|st| {
            st.sessions.set_pid(&sid, None);
            st.sessions.unsend_pending(&sid, "Claude Code exited first");
            st.sessions.set_queued(&sid, None);
        });
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
