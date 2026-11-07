//! One session as its agent's hooks report it: `world.session("a").claude().prompt("…")`. Every event goes
//! out as the hook sends it (the wire's JSON, through the World), so what's tested is what the socket gets.

use super::{Transcript, World};
use crate::hub::Reply;
use crate::model::Origin;
use crate::server::inbound::Outcome;
use serde_json::{json, Value};
use std::sync::Mutex;
use tokio::sync::oneshot;

pub struct Agent<'w> {
    world: &'w World,
    origin: Origin,
}

impl<'w> Agent<'w> {
    /// A Claude Code session in folder /work/<sid>, its transcript in the World's folder.
    pub(super) fn new(world: &'w World, sid: &str) -> Self {
        let transcript = world.home().join("claude").join(format!("{sid}.jsonl")).to_string_lossy().into_owned();
        let origin = Origin { harness: "claude".into(), session_id: sid.into(), cwd: format!("/work/{sid}"), transcript_path: transcript, ..Default::default() };
        Self { world, origin }
    }

    pub fn claude(mut self) -> Self {
        self.origin.harness = "claude".into();
        self
    }

    /// A Codex session: its rollout is its transcript.
    pub fn codex(mut self) -> Self {
        self.origin.harness = "codex".into();
        self
    }

    /// A Pi session: it takes messages directly once it subscribes, and has no transcript Cue reads.
    pub fn pi(mut self) -> Self {
        self.origin.harness = "pi".into();
        self.origin.transcript_path = String::new();
        self
    }

    /// In folder /work/<name>: the project its rows and cards go under.
    pub fn in_project(mut self, name: &str) -> Self {
        self.origin.cwd = format!("/work/{name}");
        self
    }

    /// In folder `path` on this Mac (one the test made: its repository and project are read from it).
    pub fn in_folder(mut self, path: &std::path::Path) -> Self {
        self.origin.cwd = path.to_string_lossy().into_owned();
        self
    }

    /// Running in tmux pane `pane` (where Cue types into it).
    pub fn in_tmux(mut self, pane: &str) -> Self {
        self.origin.tmux_pane = pane.into();
        self.origin.term_program = "tmux".into();
        self
    }



    /// Its transcript, to write what the agent logs.
    pub fn transcript(&self) -> Transcript {
        Transcript::at(&self.origin.transcript_path)
    }

    /// The hook's message: this session's origin, flattened, with `fields`.
    fn message(&self, fields: Value) -> Value {
        let mut msg = serde_json::to_value(&self.origin).unwrap();
        for (k, v) in fields.as_object().unwrap() {
            msg[k] = v.clone();
        }
        msg
    }

    fn event(&self, event: &str, fields: Value) -> &Self {
        let mut f = fields;
        f["type"] = json!("event");
        f["event"] = json!(event);
        self.world.drive(self.message(f));
        self
    }

    /// Cue started it (+ New), with no message yet: waiting for you.
    pub fn started(&self) -> &Self {
        self.world.hub().started(self.origin.clone(), "", &[], false);
        self.world.settle();
        self
    }

    /// Its prompt hook, for a prompt only you could have sent (no prompt id: nothing to look up).
    pub fn prompt(&self, text: &str) -> &Self {
        self.event("active", json!({"message": text}))
    }

    /// Its prompt hook with Claude's prompt id, the prompt logged in its transcript first as sent by
    /// `origin` (`{"kind": "human"}`, a peer, Claude Code's own plumbing): Cue asks the transcript who sent it.
    pub fn prompt_logged(&self, text: &str, prompt_id: &str, origin: Value) -> &Self {
        self.transcript().prompt_from(text, prompt_id, origin);
        self.event("active", json!({"message": text, "prompt_id": prompt_id}))
    }

    /// Its Stop hook: the turn ended with `reply`.
    pub fn stop(&self, reply: &str) -> &Self {
        self.event("stopped", json!({"message": reply}))
    }

    /// Its Stop hook, the session driven by another agent (`by`, CUE_DRIVEN_BY).
    pub fn stop_driven(&self, reply: &str, by: &str) -> &Self {
        self.event("stopped", json!({"message": reply, "driven_by": by}))
    }

    pub fn compacting(&self) -> &Self {
        self.event("compacting", json!({"message": "manual"}))
    }

    /// What it's doing now (Pi's extension says it as each tool starts).
    pub fn doing(&self, what: &str) -> &Self {
        self.event("doing", json!({"message": what}))
    }

    /// Its SessionEnd hook.
    pub fn ended(&self) -> &Self {
        self.event("ended", json!({}))
    }

    /// Its StopFailure hook: the turn ended on an API error.
    pub fn failed(&self, error_type: &str, details: &str) -> &Self {
        self.event("failed", json!({"error_type": error_type, "message": details}))
    }

    /// A permission prompt showing in its terminal that never reached Cue.
    pub fn terminal_ask(&self, message: &str) -> &Self {
        self.event("terminal_ask", json!({"message": message}))
    }

    /// Its status line: the session's cost so far.
    pub fn cost(&self, usd: f64) -> &Self {
        self.world.drive(json!({"type": "usage", "session_id": self.origin.session_id, "cost": usd}));
        self
    }

    /// Its permission hook for `tool` with `input`: the ask, open until it's answered.
    pub fn ask_permission(&self, tool: &str, input: Value) -> Ask {
        self.open_ask(json!({"type": "ask", "kind": "permission", "tool_name": tool, "tool_input": input}))
    }

    /// Its AskUserQuestion: `question` with its choices.
    pub fn ask_question(&self, question: &str, options: &[&str]) -> Ask {
        let options: Vec<Value> = options.iter().map(|o| json!({"label": o})).collect();
        let input = json!({"questions": [{"question": question, "options": options}]});
        self.open_ask(json!({"type": "ask", "kind": "question", "tool_name": "AskUserQuestion", "tool_input": input}))
    }

    fn open_ask(&self, fields: Value) -> Ask {
        match self.world.drive(self.message(fields)) {
            Outcome::Asked { id, answer } => Ask { id, answer: Mutex::new(answer) },
            _ => panic!("an ask's connection stays open for its answer"),
        }
    }

    /// Pi's extension subscribing: Cue's messages for it come on the returned channel.
    pub fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<Value> {
        match self.world.drive(self.message(json!({"type": "subscribe", "commands": []}))) {
            Outcome::Subscribed { messages, .. } => messages,
            _ => panic!("subscribe refused"),
        }
    }
}

/// An open ask: its id (the card's), and what its connection is told.
pub struct Ask {
    pub id: String,
    answer: Mutex<oneshot::Receiver<Reply>>,
}

impl Ask {
    /// What the hook got, if anything yet: the decision as it goes down the wire, or "cancel" with why.
    pub fn told(&self) -> Option<Value> {
        let r = crate::locking::lock(&self.answer, "an ask's answer").try_recv().ok()?;
        Some(match r {
            Reply::Decision(d) => serde_json::to_value(&d).unwrap(),
            Reply::Cancel(why) => json!({"cancel": why}),
        })
    }

    /// It got its answer in its own terminal (`behavior` by the `by`), and its hook said so on the connection.
    pub fn resolved_elsewhere(&self, world: &World, by: &str, behavior: &str) {
        let line = serde_json::from_value(json!({"type": "resolved", "by": by, "behavior": behavior})).unwrap();
        crate::server::inbound::ask_ended(world.hub(), &self.id, Some(line));
        world.settle();
    }

    /// The hook hung up without an answer (the agent stopped waiting).
    pub fn hang_up(&self, world: &World) {
        crate::server::inbound::ask_ended(world.hub(), &self.id, None);
        world.settle();
    }
}
