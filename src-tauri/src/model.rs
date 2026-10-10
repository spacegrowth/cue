use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One line of conversation shown as context next to a request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Ctx {
    pub role: String,
    pub text: String,
}

/// One step of a session's back-and-forth: what the agent said or asked, what you said or decided.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Exchange {
    /// "agent" | "you" | "peer" (another agent session sent it)
    pub role: String,
    pub text: String,
    pub at_ms: u64,
    /// Images you sent with it (paths under uploads/ in Cue's data folder).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// For "peer": the session that sent it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from: String,
    /// Yours, typed into the terminal, but the agent never took it as a message (it may have run as a
    /// command, or still be sitting in its box). Cue says so on the bubble.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unsent: bool,
    /// Yours, typed into the terminal, and Cue hasn't yet seen the agent take it (its hook, its transcript,
    /// or a dialog it opened). Cue keeps looking for a while; the bubble shows a faint ring meanwhile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
}

/// An image attached in Cue (base64, optionally as a data: URL).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Upload {
    pub name: String,
    pub mime: String,
    pub data: String,
}

/// One piece of a finished turn; `hooked` = written after a Stop hook sent the agent back to work.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TurnPart {
    pub text: String,
    pub hooked: bool,
}

/// Where a request came from — enough to label it and to jump back to its terminal.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Origin {
    pub harness: String,
    pub session_id: String,
    pub cwd: String,
    pub transcript_path: String,
    pub tty: String,
    pub agent_pid: Option<i32>,
    pub term_program: String,
    pub iterm_session_id: String,
    pub tmux_pane: String,
    pub wezterm_pane: String,
    pub kitty_window_id: String,
    pub kitty_listen_on: String,
    /// The machine it runs on (+ New → Machine), reached over SSH; empty for this Mac.
    pub machine: String,
}

/// Every message a client sends on the socket. One JSON object per line.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// A blocking decision: permission or question. The connection stays open until answered.
    Ask {
        #[serde(flatten)]
        origin: Origin,
        kind: String,
        #[serde(default)]
        tool_name: String,
        #[serde(default)]
        tool_input: Value,
        #[serde(default)]
        suggestions: Value,
        #[serde(default)]
        context: Vec<Ctx>,
    },
    /// Fire-and-forget lifecycle event: stopped (waiting for you), active, ended.
    Event {
        #[serde(flatten)]
        origin: Origin,
        event: String,
        #[serde(default)]
        message: String,
        /// The whole final turn, when the harness can tell (see TurnPart).
        #[serde(default)]
        turn: Vec<TurnPart>,
        /// Set when another agent drives this session (e.g. "its relay lead (…)"): its finished
        /// turn is that agent's to handle, not yours.
        #[serde(default)]
        driven_by: String,
        /// Claude's id for the prompt behind an "active" event: lets Cue ask the transcript who sent it.
        #[serde(default)]
        prompt_id: String,
        /// For "failed" (a turn that ended on an API error): Claude Code's error type, e.g. "rate_limit".
        #[serde(default)]
        error_type: String,
    },
    /// From Claude Code's status line: the plan's usage (its `rate_limits` object, plan users only), and
    /// that session's cost so far (`cost.total_cost_usd`, everyone).
    Usage {
        #[serde(default)]
        rate_limits: Value,
        #[serde(default)]
        session_id: String,
        #[serde(default)]
        cost: Option<f64>,
        /// Its context window in tokens (`context_window.context_window_size`).
        #[serde(default)]
        window: Option<u64>,
    },
    /// Sent on an open Ask connection when the agent got its answer somewhere else.
    Resolved {
        #[serde(default)]
        by: String,
        #[serde(default)]
        behavior: String,
    },
    /// Control: snapshot of everything.
    List,
    /// Control: answer an item (what the UI does; also used by tests and cue-ctl).
    Respond { id: String, decision: Decision },
    /// Control: jump to an item's terminal.
    Focus { id: String },
    /// Control: type a reply into a finished session's terminal.
    Reply {
        id: String,
        text: String,
        #[serde(default)]
        images: Vec<Upload>,
    },
    /// Control: send a message to a live session (working or not).
    SendTo {
        session_id: String,
        text: String,
        #[serde(default)]
        images: Vec<Upload>,
        /// Interrupt it first if it's mid-turn, so this is handled right away.
        #[serde(default)]
        now: bool,
    },
    /// Control: stop a session mid-turn (Esc in its terminal; Pi aborts directly).
    Interrupt {
        session_id: String,
    },
    /// A harness that can take replies directly (Pi): keep this connection open, and Cue sends
    /// {"type":"reply","text":…} down it instead of typing into a terminal.
    Subscribe {
        #[serde(flatten)]
        origin: Origin,
    },
}

/// Your answer, harness-neutral. The hook / extension maps it to its harness's format.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Decision {
    /// "allow" | "deny" | "allow_always"
    pub behavior: String,
    /// Deny reason, or the "do this instead" instruction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Question answers: { "<question text>": "<answer>" }.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answers: Option<Value>,
    /// For allow_always: which of the harness's own suggestions to apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<Value>,
}

/// One answered item in brief, for today's numbers in History (which count every answer of the
/// last day, however short the History list is kept).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub session_id: String,
    pub project: String,
    pub status: String,
    pub created_ms: u64,
    pub resolved_ms: u64,
    pub outcome: String,
}

impl Answer {
    pub fn of(it: &Item) -> Self {
        Answer { session_id: it.origin.session_id.clone(), project: it.project.clone(), status: it.status.clone(), created_ms: it.created_ms, resolved_ms: it.resolved_ms.unwrap_or(0), outcome: it.outcome.clone() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// "permission" | "question" | "waiting"
    pub kind: String,
    #[serde(flatten)]
    pub origin: Origin,
    pub project: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub suggestions: Value,
    pub context: Vec<Ctx>,
    pub message: String,
    /// What the agent wrote after a Stop hook made it continue (shown collapsed under the answer).
    #[serde(default)]
    pub followup: String,
    /// A finished-turn card for a turn that was interrupted (Esc), not one that finished: it's waiting
    /// at "What should Claude do instead?" until you say something ("continue" picks it back up).
    #[serde(default)]
    pub interrupted: bool,
    /// Sent to the back of the queue: the card sorts by this instead of created_ms. Its row still
    /// shows created_ms (the age) — sending it back doesn't make the turn any younger.
    #[serde(default)]
    pub back_ms: Option<u64>,
    pub created_ms: u64,
    /// "pending" | "answered" | "answered_elsewhere" | "gone"
    pub status: String,
    pub outcome: String,
    pub resolved_ms: Option<u64>,
    /// Images you sent with your reply (paths under uploads/ in Cue's data folder).
    #[serde(default)]
    pub images: Vec<String>,
    /// The session's last few exchanges before this card — the "what is this about?" context.
    #[serde(default)]
    pub thread: Vec<Exchange>,
    #[serde(skip)]
    pub tool_use_id: Option<String>,
    #[serde(skip)]
    pub scan_from: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn project_of(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}
