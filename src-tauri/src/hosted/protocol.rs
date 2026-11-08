//! What a hosted agent's lines mean, one agent at a time (Claude Code's stream-json, Pi's RPC). The session
//! (session.rs) owns the process and moves lines; a `Protocol` turns its output into `HostedEvent`s and spells
//! what Cue writes to it. Pure: no process, no time, so each one is tested over lines its real agent wrote.

use super::{Answer, HostedEvent};

pub(super) trait Protocol: Send {
    /// What it's sent before anything of yours (a handshake, if it has one).
    fn opening(&mut self) -> Vec<String>;
    /// One line of its output: what it means for Cue, and any lines to write back at once (a request it
    /// can't answer, refused).
    fn read(&mut self, line: &str) -> (Vec<HostedEvent>, Vec<String>);
    /// Your message: taken at once when it's idle, at its next step when it's at work. Nothing yet when its
    /// conversation isn't open: it goes once it is.
    fn message(&mut self, text: &str) -> Vec<String>;
    /// Your answer to its request `request_id` (an `Ask`'s). Err when nothing waits for one under that id.
    fn answer(&mut self, request_id: &str, a: &Answer) -> Result<Vec<String>, String>;
    /// Stop what it's doing.
    fn interrupt(&mut self) -> Vec<String>;
    /// Whether a turn is under way.
    fn in_turn(&self) -> bool;
    /// Its process is gone: nothing waits for an answer any more.
    fn gone(&mut self);
    /// A line Cue wrote to it, read back from the host's log (Cue started again and reads what it missed): what
    /// writing it changed here, as `message`, `answer`, `interrupt` and `opening` change it when they make it.
    /// Replaying the log (its lines through `read`, Cue's through this) leaves the protocol as it was.
    fn wrote(&mut self, line: &str);
    /// Whether request `request_id` (an `Ask`'s) still waits for an answer.
    fn waits_on(&self, request_id: &str) -> bool;
    /// The conversation's id, once the agent has said it, for one that picks its own (Codex); None: the one Cue
    /// started it with.
    fn session_id(&self) -> Option<String> {
        None
    }
    /// Whether it's ready for your messages: its conversation is open (or it has none to open).
    fn ready(&self) -> bool {
        true
    }
    /// Where the agent keeps the conversation, once it has said so (Codex's rollout file).
    fn transcript(&self) -> Option<String> {
        None
    }
}

/// Claude Code (stream-json): translate.rs reads it, wire.rs writes it. A request's tool input is kept until
/// it's answered (an allow carries it back); a request of a kind Cue doesn't know is refused at once.
#[derive(Default)]
pub(super) struct Claude {
    translator: super::translate::Translator,
    pending: std::collections::HashMap<String, serde_json::Value>,
    next_request: u64,
}

impl Claude {
    fn request_id(&mut self) -> String {
        self.next_request += 1;
        format!("cue-{}", self.next_request)
    }
}

impl Protocol for Claude {
    fn opening(&mut self) -> Vec<String> {
        let id = self.request_id();
        vec![super::wire::initialize(&id)]
    }

    fn read(&mut self, line: &str) -> (Vec<HostedEvent>, Vec<String>) {
        let events = self.translator.step(&super::wire::parse(line));
        let mut replies = vec![];
        for e in &events {
            match e {
                HostedEvent::Ask(a) => {
                    self.pending.insert(a.request_id.clone(), a.input.clone());
                }
                HostedEvent::Refused { request_id, subtype } => replies.push(super::wire::refuse(request_id, &format!("Cue can't answer a “{subtype}” request"))),
                _ => {}
            }
        }
        (events, replies)
    }

    fn message(&mut self, text: &str) -> Vec<String> {
        vec![super::wire::user_message(text)]
    }

    fn answer(&mut self, request_id: &str, a: &Answer) -> Result<Vec<String>, String> {
        let input = self.pending.remove(request_id).ok_or("that request isn't waiting for an answer (answered already?)")?;
        Ok(vec![super::wire::answer(request_id, a, &input)])
    }

    fn interrupt(&mut self) -> Vec<String> {
        let id = self.request_id();
        vec![super::wire::interrupt(&id)]
    }

    fn in_turn(&self) -> bool {
        self.translator.in_turn()
    }

    fn gone(&mut self) {
        self.pending.clear();
    }

    fn wrote(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return };
        match v.get("type").and_then(serde_json::Value::as_str) {
            Some("control_request") => self.next_request = self.next_request.max(numbered(v["request_id"].as_str())),
            Some("control_response") => {
                if let Some(id) = v.pointer("/response/request_id").and_then(serde_json::Value::as_str) {
                    self.pending.remove(id);
                }
            }
            _ => {}
        }
    }

    fn waits_on(&self, request_id: &str) -> bool {
        self.pending.contains_key(request_id)
    }
}

/// N, from a request id of Cue's ("cue-N"); 0 for any other.
pub(super) fn numbered(id: Option<&str>) -> u64 {
    id.and_then(|i| i.strip_prefix("cue-")).and_then(|n| n.parse().ok()).unwrap_or(0)
}
