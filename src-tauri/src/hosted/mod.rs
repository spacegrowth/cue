//! Sessions Cue hosts: Claude Code started by Cue itself, with no terminal, talking to Cue directly
//! (docs/dev/cue-hosted-sessions.md). Claude runs in its headless mode, JSON lines in and out: Cue writes your
//! messages, answers and interrupts to its input, and reads everything it does from its output. Every
//! permission, question, sandbox network request and MCP server's request for input arrives here as a
//! request, so none of them can end up as a prompt in a terminal Cue has to type into.
//!
//! - `wire`: the lines themselves, in and out. All the protocol's spelling lives there.
//! - `translate`: those lines as the few things Cue cares about (`HostedEvent`).
//! - `launch`: how a session is started or resumed.
//! - `session`: one running Claude: its process, its input, the requests waiting for an answer.
//! - `running`: the sessions running now, by id, behind the `Hosted` port the hub uses (hub/ports.rs), as Cue's
//!   own children.
//! - `driver`: a session's lines made sense of, wherever it runs.
//! - `host_link`, `hosts`: the sessions run by cue-host (host/), which keeps them running across Cue restarts:
//!   what the hub uses, with `running` for when the host can't be reached.
//!
//! None of this knows about the hub, the store or the window: a session reports through the function it was
//! started with, and that's all whoever starts it needs.

mod agent_check;
mod codex;
mod conn;
mod driver;
mod host_link;
mod hosts;
mod launch;
mod machine_host;
mod old_hosts;
mod on_machines;
mod remote;
mod pi;
mod protocol;
mod running;
mod session;
mod tails;
mod translate;
mod wire;

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod compat_tests;
#[cfg(test)]
mod mcp_compat_tests;
#[cfg(test)]
mod hosts_tests;
#[cfg(test)]
mod machine_tests;
#[cfg(test)]
mod pi_tests;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod translate_tests;

/// Tests: what Claude's recorded lines (a fixture's) say to Cue, line by line, as one process read them.
#[cfg(test)]
pub(crate) fn fixture_events(lines: &str) -> Vec<Vec<HostedEvent>> {
    let mut t = translate::Translator::default();
    lines.lines().map(|l| t.step(&wire::parse(l))).collect()
}

pub use launch::{Agent, Program, Spec, Start};
pub use hosts::Hosts;
pub use running::{end_leftovers, Hosted, Report, Running, Started};
pub use tails::{tail_running, tail_start, tail_stop};
pub use session::Session;

use serde_json::Value;

/// Set in a hosted session's environment, so Cue's own hook (hook.rs), run by that session, stays out of the
/// way: Cue already hears everything from the session itself.
pub const HOSTED_ENV: &str = "CUE_HOSTED";

/// What a hosted session tells Cue.
#[derive(Clone, Debug, PartialEq)]
pub enum HostedEvent {
    /// Claude is up, with what it runs with. Once per session, however many turns repeat it.
    Started { session_id: String, model: String, slash_commands: Vec<String>, terminal_only: Vec<String> },
    /// A turn began: Claude read a message while idle (a message of yours, or a background task's that
    /// finished). It says so at once; it repeats your message back (`Taken`) only with its first words.
    TurnStarted,
    /// A message of yours Claude has now taken: at once when it's idle, at its next step when it's busy.
    Taken { text: String },
    /// A message of yours it turned down, and why (Pi with no key for its model, say): it won't read it.
    NotTaken { text: String, why: String },
    /// Its words. `agent`: the tool call that started the agent saying them; None for the session itself.
    Said { text: String, agent: Option<String> },
    /// A tool it called (`id` pairs it with its result).
    ToolCall { id: String, name: String, input: Value, agent: Option<String> },
    /// A tool call's result.
    ToolResult { id: String, output: Value, failed: bool, agent: Option<String> },
    /// A background agent or task started, moved on or finished: Claude's own event, kept whole.
    Task { what: String, detail: Value },
    /// The plan's usage, as the status line reports it (`five_hour` / `seven_day`, each with
    /// `used_percentage` and `resets_at` in seconds): a hosted session has no status line to say it.
    Rates(Value),
    /// What you'd likely type next (Claude's prompt suggestion), after a turn with an obvious next step.
    Suggested { text: String },
    /// It needs an answer from you before it goes on (`Session::answer`).
    Ask(Ask),
    /// A turn is over. `unprompted`: nothing of yours started it (a background task that finished did).
    TurnEnded { text: String, failed: bool, cost_usd: f64, unprompted: bool },
    /// A request of a kind this code doesn't know, refused at once so the session isn't left waiting on it.
    Refused { request_id: String, subtype: String },
    /// Claude is gone. `during_turn`: it was in the middle of one. `last_words`: the end of what it said on
    /// its error output, for saying why.
    Exited { code: Option<i32>, during_turn: bool, last_words: String },
}

/// One request waiting for your answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    /// What the answer goes back with.
    pub request_id: String,
    pub kind: AskKind,
    /// The tool it wants to use; for `Input`, empty.
    pub tool_name: String,
    /// The tool call's input: a question's questions, a network request's `host`.
    pub input: Value,
    /// Claude's own "always allow" choices, as the hook gets them.
    pub suggestions: Value,
    /// The agent inside the session that asks; empty for the session itself.
    pub agent_id: String,
    pub tool_use_id: String,
    /// For `Input`: the MCP server asking, what it says, and the form it wants filled in (a JSON schema).
    pub server: String,
    pub message: String,
    pub form: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskKind {
    /// A tool it wants to use.
    Permission,
    /// AskUserQuestion: questions with choices.
    Question,
    /// A sandboxed command reaching a host on the network.
    Network,
    /// An MCP server asking you to fill in a form.
    Input,
}

/// Your answer to an `Ask`.
#[derive(Clone, Debug)]
pub enum Answer {
    /// To a permission, a question or a network request: Cue's decision, as the hook answers it.
    Decision(crate::model::Decision),
    /// To an MCP server's form: what you filled in, or that you declined or dismissed it.
    Input { action: InputAction, content: Value },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAction {
    Accept,
    Decline,
    Cancel,
}
