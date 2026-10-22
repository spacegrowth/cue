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
    /// Yours, sent now (⌘↵) right after the one above, before it was answered: stopping its turn put that
    /// one back into Claude Code's box and this was typed after it, so the agent got both as one message.
    /// The text is what you typed; the bubble says it went with the one above (sessions.rs note_with).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub joined: bool,
    /// How this message of yours got to the agent, step by step: typed where, Enter pressed again, who
    /// said it was taken (its prompt hook, its log, a dialog it opened), or why Cue said it didn't go.
    /// The last few steps only; a line each, so the question "which path flagged it" has an answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delivery: Vec<Delivery>,
}

/// One step of a message's way to the agent (see `Exchange::delivery`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Delivery {
    pub at_ms: u64,
    pub what: String,
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
        /// The machine it runs on (empty: this Mac). A machine's window is that session's alone.
        #[serde(default)]
        machine: String,
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
        /// The commands the session offers (Pi: its extensions' commands, prompt templates, skills),
        /// for the "/" menu in its box. Pi's built-ins Cue knows itself.
        #[serde(default)]
        commands: Vec<AgentCmd>,
    },
    /// On a subscribe connection: the session's commands again (Cue asked with {"type":"commands"}:
    /// Refresh under the "/" menu).
    Commands {
        #[serde(default)]
        commands: Vec<AgentCmd>,
    },
}

/// A slash command as an agent reports it (Pi's SlashCommandInfo).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AgentCmd {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// "extension", "prompt" or "skill" (Pi).
    #[serde(default)]
    pub source: String,
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
    /// Opened while the session sat at its prompt (a command's dialog, like /model's picker): answering
    /// it starts no turn, so the session goes back to its prompt, not to Working.
    #[serde(skip)]
    pub at_prompt: bool,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A folder's own name ("fix-locks"): what a session there is called until you name it.
pub fn folder_of(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// A folder's own project, before anything you said: on this Mac its git repository, by its main
/// checkout's name, so every worktree of `cue` is `cue`; outside a repository, and on another machine
/// (its repository isn't read from here), the folder's own name.
pub fn own_project(cwd: &str, machine: &str) -> String {
    let repo = if machine.is_empty() { crate::logs::live::repo_root(cwd) } else { None };
    repo.map(|r| crate::logs::live::repo_name(&r)).unwrap_or_else(|| folder_of(cwd))
}

/// The project a folder belongs to, by three rules, the first that says one:
/// 1. you put the folder, or one it's in, in a project (+ New's Project, or Move to project; Settings'
///    project.folders). On this Mac a repository is put in as a whole, by its main checkout, so its
///    worktrees follow; a folder above a repository doesn't reach into it.
/// 2. its repository says so: `"project": "<name>"` in its .cue/actions.json (a docs or plugin repo that
///    goes with another).
/// 3. its own (`own_project`): the git repository, else the folder.
pub fn project_at(cwd: &str, machine: &str) -> String {
    project_said(cwd, machine, &crate::config::folders_put()).unwrap_or_else(|| own_project(cwd, machine))
}

/// Rules 1 and 2 of `project_at`, with your folders already read: None when neither says one.
pub fn project_said(cwd: &str, machine: &str, put: &std::collections::HashMap<String, String>) -> Option<String> {
    said_how(cwd, machine, put).map(|(p, _)| p)
}

/// A folder's project and which rule said it: "put" (you), "repo-says" (its .cue/actions.json), "repo"
/// (its git repository) or "folder" (its own name). For + New's Project line.
pub fn project_how(cwd: &str, machine: &str) -> (String, &'static str) {
    if let Some(said) = said_how(cwd, machine, &crate::config::folders_put()) {
        return said;
    }
    let repo = machine.is_empty() && crate::logs::live::repo_root(cwd).is_some();
    (own_project(cwd, machine), if repo { "repo" } else { "folder" })
}

fn said_how(cwd: &str, machine: &str, put: &std::collections::HashMap<String, String>) -> Option<(String, &'static str)> {
    let cwd = cwd.trim_end_matches('/');
    if cwd.is_empty() {
        return None;
    }
    if !machine.is_empty() {
        // Never a look at that path on this Mac: by the folders you put in alone.
        return std::path::Path::new(cwd).ancestors().find_map(|d| put.get(&format!("{machine}:{}", d.display())).map(|p| (p.clone(), "put")));
    }
    let top = crate::logs::live::checkout_top(cwd);
    let mut ancestors = std::path::Path::new(cwd).ancestors();
    let mut said = None;
    for d in ancestors.by_ref() {
        if let Some(p) = put.get(&d.to_string_lossy().to_string()) {
            said = Some(p.clone());
            break;
        }
        if top.as_deref() == Some(d) {
            break;
        }
    }
    let repo = top.as_ref().and_then(|_| crate::logs::live::repo_root(cwd));
    said.or_else(|| repo.as_ref().and_then(|r| put.get(&r.to_string_lossy().to_string()).cloned()))
        .map(|p| (p, "put"))
        .or_else(|| top.as_deref().and_then(declared).map(|p| (p, "repo-says")))
        .or_else(|| repo.as_deref().filter(|r| Some(*r) != top.as_deref()).and_then(declared).map(|p| (p, "repo-says")))
}

/// Where Move to project puts a folder: a repository's main checkout (all of it, worktrees too), else the
/// folder; "<machine>:<folder>" on another machine.
pub fn put_key(cwd: &str, machine: &str) -> String {
    let cwd = cwd.trim_end_matches('/');
    if !machine.is_empty() {
        return format!("{machine}:{cwd}");
    }
    crate::logs::live::repo_root(cwd).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| cwd.to_string())
}

static DECLARED: std::sync::Mutex<Option<std::collections::HashMap<std::path::PathBuf, (Option<std::time::SystemTime>, Option<String>)>>> = std::sync::Mutex::new(None);

/// The project a checkout's .cue/actions.json says it belongs to (`"project"`), read again when the file changes.
fn declared(top: &std::path::Path) -> Option<String> {
    let file = top.join(".cue").join("actions.json");
    let mtime = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    {
        let cache = crate::locking::lock(&DECLARED, "DECLARED");
        if let Some((at, p)) = cache.as_ref().and_then(|m| m.get(top)) {
            if *at == mtime {
                return p.clone();
            }
        }
    }
    let p = mtime.and_then(|_| std::fs::read_to_string(&file).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("project").and_then(serde_json::Value::as_str).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && s.len() <= 80);
    let mut cache = crate::locking::lock(&DECLARED, "DECLARED");
    cache.get_or_insert_with(std::collections::HashMap::new).insert(top.to_path_buf(), (mtime, p.clone()));
    p
}

/// The project a folder on this Mac belongs to (`project_at`).
pub fn project_of(cwd: &str) -> String {
    project_at(cwd, "")
}

/// The project a session's records keep: its folder's own (`own_project`). The window's state shows the
/// one you said in its place, where you said one (hub snapshot), so taking that back needs no rewrite.
pub fn project_of_origin(o: &Origin) -> String {
    own_project(&o.cwd, &o.machine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_from_a_machine_says_which_and_this_macs_says_none() {
        let from = |line: &str| match serde_json::from_str::<ClientMsg>(line).unwrap() {
            ClientMsg::Usage { session_id, window, machine, .. } => (session_id, window, machine),
            _ => panic!("not usage"),
        };
        assert_eq!(from(r#"{"type":"usage","session_id":"s","window":200000,"machine":"box"}"#), ("s".into(), Some(200_000), "box".into()));
        assert_eq!(from(r#"{"type":"usage","rate_limits":{},"session_id":"s","cost":1.5,"window":1000000}"#), ("s".into(), Some(1_000_000), String::new()), "this Mac's hook, as it always sent it");
    }
}
