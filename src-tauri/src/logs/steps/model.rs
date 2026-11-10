//! What the feed is made of: a turn and its items (the agent's words and its steps), a turn's own work
//! and tokens by kind, and what the chat's tab shows about the session (its meta).

use super::step_line::without_cd;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Item {
    /// The agent's own words between steps.
    Say { text: String, at_ms: u64 },
    /// One tool call. `verb` + `subject` read as a line ("Ran" + "npm test"); `result` is how it went
    /// ("84 lines", "+6 −2", "exit 1"); `bad` when it failed; `done` once its result is in.
    Step {
        id: String,
        kind: &'static str,
        verb: String,
        subject: String,
        mono: bool,
        result: String,
        bad: bool,
        done: bool,
        at_ms: u64,
        #[serde(skip)]
        tool: String,
        #[serde(skip)]
        past: &'static str,
        /// Bash: the command in full and the one-line description the session gave it (a background script's name).
        #[serde(skip)]
        command: String,
        #[serde(skip)]
        desc: String,
    },
}

/// From one prompt (yours, or a hand-over from Claude Code) to the next.
#[derive(Clone, Serialize)]
pub struct Turn {
    pub(super) at_ms: u64,
    /// The message that started it (yours; "" for Claude Code's own hand-overs), shortened.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) prompt: String,
    pub(super) items: Vec<Item>,
    /// Earlier steps of this turn left out (MAX_ITEMS).
    pub(super) more: usize,
    /// When a Stop hook sent it back to work (0: none did). What it did and wrote from then on is the
    /// hook's follow-up, not its answer: the chat folds it under "After a stop hook" (transcript::is_stop_hook).
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) hooked_ms: u64,
    /// Tokens the session's own replies in this turn went through (input, cache reads and writes,
    /// output), summed per API call: what the turn cost, in tokens. Insights draw it as a bar.
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) tokens: u64,
    /// How much context its last reply carried (input, cache reads and writes): the session's context
    /// as of this turn. Insights draw it as the growth line.
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) context: u64,
    /// Its tokens by kind, for Insights' prices (the chat doesn't show them).
    #[serde(skip)]
    pub(super) usage: Usage,
    /// What it did itself that it could have handed to a helper, for Insights (the chat doesn't show it).
    #[serde(skip)]
    pub(super) work: Work,
}

impl Turn {
    /// A turn starting at `at_ms` on `prompt` ("" when it isn't yours), nothing in it yet.
    pub(super) fn starting(at_ms: u64, prompt: String) -> Turn {
        Turn { at_ms, prompt, items: Vec::new(), more: 0, hooked_ms: 0, tokens: 0, context: 0, usage: Usage::default(), work: Work::default() }
    }
}

/// A turn's own looking around and delegating, for Insights' "could have delegated": reads and searches
/// it ran itself, the background agents it started, and the files it edited.
#[derive(Clone, Default)]
pub struct Work {
    pub looks: u64,
    pub agents: u64,
    /// Each file edited, once (at most MAX_EDITED).
    pub edited: Vec<String>,
}

const MAX_EDITED: usize = 200;

impl Work {
    /// One tool call. A shell command counts as a look when it only reads (grep, cat, sed -n, git log …).
    pub(super) fn count(&mut self, block: &Value) {
        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
        let input = block.get("input").unwrap_or(&Value::Null);
        let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
        match name {
            "Read" | "Grep" | "Glob" | "WebFetch" | "WebSearch" => self.looks += 1,
            "Bash" if is_look(arg("command")) => self.looks += 1,
            "Agent" | "Task" => self.agents += 1,
            "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
                let f = if arg("file_path").is_empty() { arg("notebook_path") } else { arg("file_path") };
                if !f.is_empty() && self.edited.len() < MAX_EDITED && !self.edited.iter().any(|e| e == f) {
                    self.edited.push(f.to_string());
                }
            }
            _ => {}
        }
    }
}

/// Whether a shell command only reads: its first word (after any `cd … &&`) is a reader.
/// SHORTCUT: the first word only; `grep … | xargs sed -i` counts as a look. Upgrade path: every stage of the pipe.
fn is_look(cmd: &str) -> bool {
    let c = without_cd(cmd);
    const READERS: [&str; 14] = ["grep ", "rg ", "cat ", "sed -n", "head ", "tail ", "find ", "ls", "wc ", "git log", "git show", "git grep", "git diff", "git blame"];
    READERS.iter().any(|r| c.starts_with(r))
}

/// Where in a project an edited file is, so a session's edits can be told apart by area: its first folder
/// under the session's folder (a file at the top is "."), else, outside it, its own folder. Cue's and Claude
/// Code's own files and temporary ones aren't the project's: None.
pub fn area_of(cwd: &str, path: &str) -> Option<String> {
    if path.contains("/.claude/") || ["/tmp/", "/private/", "/var/folders/"].iter().any(|t| path.starts_with(t)) {
        return None;
    }
    let base = cwd.trim_end_matches('/');
    match path.strip_prefix(base).and_then(|r| r.strip_prefix('/')).filter(|_| !base.is_empty()) {
        Some(rest) => Some(rest.split_once('/').map_or(".", |(top, _)| top).to_string()),
        None => path.rsplit_once('/').map(|(dir, _)| dir.to_string()).filter(|d| !d.is_empty()),
    }
}

/// A turn's tokens by kind, summed over its replies (each reply once): fresh input, output, cache reads,
/// cache writes kept 5 minutes and kept an hour; what its first reply wrote and read (a reply after a long
/// wait writes the whole context again); and when its last reply came.
#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub fresh: u64,
    pub output: u64,
    pub read: u64,
    pub write_5m: u64,
    pub write_1h: u64,
    pub first_write: u64,
    pub first_read: u64,
    pub last_ms: u64,
}

impl Usage {
    pub(super) fn add(&mut self, fresh: u64, read: u64, write_5m: u64, write_1h: u64, output: u64, at_ms: u64) {
        if self.last_ms == 0 {
            self.first_write = write_5m + write_1h;
            self.first_read = read;
        }
        self.fresh += fresh;
        self.read += read;
        self.write_5m += write_5m;
        self.write_1h += write_1h;
        self.output += output;
        self.last_ms = at_ms.max(self.last_ms).max(1);
    }
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// What the chat's tab shows about a session, from its log: the model, how full its context is (the
/// latest reply's tokens of the model's window), and the cost so far
/// where the agent logs it (Pi; Claude Code's comes from its status line instead).
#[derive(Clone, Default, Serialize, PartialEq)]
pub struct Meta {
    pub(super) model: String,
    pub(super) context: u64,
    pub(super) window: u64,
    pub(super) cost: Option<f64>,
    /// When its latest reply came (from the log's timestamps), and how long the provider keeps that
    /// reply's context cached (Claude Code says, per reply: an hour or 5 minutes; 0 when the log doesn't
    /// say, or nothing was cached, as behind a proxy that strips it). Past that, the next reply pays to
    /// write the whole context again.
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) last_call_ms: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) cache_ttl_s: u64,
    /// The context its first reply carried, from the top of its log (a feed opened mid-file reads the
    /// first part once for it): what the session started at, so insights can say how far it has grown.
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) start_context: u64,
    /// When that first reply came.
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) start_ms: u64,
    /// How many warm replies its next one costs once its cache is gone (prices::cold_x; 0: not known).
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) cold_x: u64,
    /// What that cold reply's re-write costs at list prices, in cents (prices::cold_cents; 0: not known).
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) cold_cents: u64,
    /// The least context Claude Code has lately compacted sessions on its model at on its own
    /// (auto_compact_at; 0: not seen enough to say).
    #[serde(skip_serializing_if = "is_zero")]
    pub(super) auto_at: u64,
}
