//! What a session did, step by step, read from its Claude Code transcript for the chat's step lines
//! (the window's Active pane, and anything else that asks): the agent's words between steps, and each
//! tool call as one line (what it did, how it went). Read only while someone has the session open:
//! each feed remembers how far into the file it got and reads just the new lines, and a feed nobody
//! asked for in a couple of minutes is dropped. Nothing is saved; the transcript is the only copy.
//!
//! A step's full output or diff isn't part of the feed: `detail` fetches it when you open the step.
//!
//! Pi's session log (~/.pi/agent/sessions/…) and Codex's (~/.codex/sessions/…/rollout-*.jsonl) are read
//! the same way: each of their entries is first put in the shape Claude Code's transcript uses
//! (`from_pi`, `from_codex`), so all three share everything below.
//!
//! SHORTCUT: viewers poll (every 0.5 s while the session works) rather than being
//! pushed new steps. Fine for a few viewers, since a poll with nothing new reads no file and answers
//! with just a version; push new steps instead if many viewers watch at once.
//! SHORTCUT: only the last 4 MB of a transcript is read, so very old turns of a huge session aren't
//! there. That's past anything the chat shows; read further back on demand if that changes.

use crate::logs::transcript::{content_blocks, cut, lines, stamp};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A first look reads this much from the end of the transcript (older turns are past what the chat shows).
const START_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TURNS: usize = 30;
/// Per turn: a very long turn keeps its newest steps and says how many came before.
const MAX_ITEMS: usize = 400;
const FORGET: Duration = Duration::from_secs(120);
/// A step's output, opened: this many lines at most (the start and, mostly, the end).
const OUT_HEAD: usize = 40;
const OUT_TAIL: usize = 160;
const LINE_CHARS: usize = 400;

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
    at_ms: u64,
    /// The message that started it (yours; "" for Claude Code's own hand-overs), shortened.
    #[serde(skip_serializing_if = "String::is_empty")]
    prompt: String,
    items: Vec<Item>,
    /// Earlier steps of this turn left out (MAX_ITEMS).
    more: usize,
    /// When a Stop hook sent it back to work (0: none did). What it did and wrote from then on is the
    /// hook's follow-up, not its answer: the chat folds it under "After a stop hook" (transcript::is_stop_hook).
    #[serde(skip_serializing_if = "is_zero")]
    hooked_ms: u64,
    /// Tokens the session's own replies in this turn went through (input, cache reads and writes,
    /// output), summed per API call: what the turn cost, in tokens. Insights draw it as a bar.
    #[serde(skip_serializing_if = "is_zero")]
    tokens: u64,
    /// How much context its last reply carried (input, cache reads and writes): the session's context
    /// as of this turn. Insights draw it as the growth line.
    #[serde(skip_serializing_if = "is_zero")]
    context: u64,
    /// Its tokens by kind, for Insights' prices (the chat doesn't show them).
    #[serde(skip)]
    usage: Usage,
    /// What it did itself that it could have handed to a helper, for Insights (the chat doesn't show it).
    #[serde(skip)]
    work: Work,
}

/// A turn's own looking around and delegating, for Insights' "could have delegated": reads and searches
/// it ran itself, the background agents and relay executors it started, and the files it edited.
#[derive(Clone, Default)]
pub struct Work {
    pub looks: u64,
    pub agents: u64,
    pub relay: u64,
    /// Each file edited, once (at most MAX_EDITED).
    pub edited: Vec<String>,
}

const MAX_EDITED: usize = 200;

impl Work {
    /// One tool call. A shell command counts as a look when it only reads (grep, cat, sed -n, git log …).
    fn count(&mut self, block: &Value) {
        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
        let input = block.get("input").unwrap_or(&Value::Null);
        let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
        match name {
            "Read" | "Grep" | "Glob" | "WebFetch" | "WebSearch" => self.looks += 1,
            "Bash" if is_look(arg("command")) => self.looks += 1,
            "Bash" if arg("command").contains("relay spawn") => self.relay += 1,
            "Agent" | "Task" => self.agents += 1,
            "Skill" if arg("skill").starts_with("relay:spawn") => self.relay += 1,
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
    fn add(&mut self, fresh: u64, read: u64, write_5m: u64, write_1h: u64, output: u64, at_ms: u64) {
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
    model: String,
    context: u64,
    window: u64,
    cost: Option<f64>,
    /// When its latest reply came (from the log's timestamps), and how long the provider keeps that
    /// reply's context cached (Claude Code says, per reply: an hour or 5 minutes; 0 when the log doesn't
    /// say, or nothing was cached, as behind a proxy that strips it). Past that, the next reply pays to
    /// write the whole context again.
    #[serde(skip_serializing_if = "is_zero")]
    last_call_ms: u64,
    #[serde(skip_serializing_if = "is_zero")]
    cache_ttl_s: u64,
    /// The context its first reply carried, from the top of its log (a feed opened mid-file reads the
    /// first part once for it): what the session started at, so insights can say how far it has grown.
    #[serde(skip_serializing_if = "is_zero")]
    start_context: u64,
    /// When that first reply came.
    #[serde(skip_serializing_if = "is_zero")]
    start_ms: u64,
    /// How many warm replies its next one costs once its cache is gone (prices::cold_x; 0: not known).
    #[serde(skip_serializing_if = "is_zero")]
    cold_x: u64,
    /// What that cold reply's re-write costs at list prices, in cents (prices::cold_cents; 0: not known).
    #[serde(skip_serializing_if = "is_zero")]
    cold_cents: u64,
    /// The least context Claude Code has lately compacted sessions on its model at on its own
    /// (auto_compact_at; 0: not seen enough to say).
    #[serde(skip_serializing_if = "is_zero")]
    auto_at: u64,
}

struct Feed {
    path: String,
    meta: Meta,
    /// Whose log it is (its first line says): Claude Code's, Pi's or Codex's.
    kind: Kind,
    /// Codex: each apply_patch's patch, by call id (its result doesn't repeat it; the step's size and
    /// diff come from it).
    patches: HashMap<String, String>,
    offset: u64,
    /// Started mid-file: the first (partial) line still has to be skipped.
    align: bool,
    turns: Vec<Turn>,
    version: u64,
    used: Instant,
    /// A background agent's own log (every entry is a sidechain one, and read as this feed's).
    helper: bool,
    /// Helper: when its last answer ended (its report is written, no step open); 0 while it works.
    ended_ms: u64,
    /// The background agents this session started (Claude Code's Agent tool), each read from its own log.
    helpers: Vec<Helper>,
    /// Helper: its tokens so far, summed over its replies (input, output, cache reads and writes).
    tokens: u64,
    /// Background tasks of this session that failed after a while, until the hub takes them.
    failures: Vec<TaskFailure>,
    /// Claude Code: the reply whose usage was counted last. It writes one line per content block, each
    /// with the same usage, so a turn's tokens count each reply once.
    last_reply: String,
    /// Codex: its running total of tokens; a turn's tokens are what it grew by.
    codex_total: u64,
    /// Its writes of a hand-off note (`is_handoff_note`) still waiting for their result: path by call id.
    note_calls: HashMap<String, String>,
    /// The newest hand-off note it wrote since your last prompt, and when (its result's time, by the log).
    note: Option<(String, u64)>,
    /// Your latest prompt: when (by the log), and whether it was Cue's hand-off ask (HANDOFF_ASK_START).
    prompted: (u64, bool),
}

/// How Cue's own hand-off ask starts (ipc/insights.rs HANDOFF_ASK): a turn it began is the note's turn.
pub const HANDOFF_ASK_START: &str = "Hand off: write a short note";

/// A background agent a Claude Code session started: it runs inside the session's process, with no
/// terminal of its own, and writes its log to `<session folder>/subagents/agent-<id>.jsonl`. Nothing
/// can be sent to it from outside; it reports to its session. Here so the chat can show it at work
/// (its steps, live) under the step that started it, and the session can be marked as waiting on it.
/// Also a shell command run in the background (`script`): the same place and the same ending (Claude
/// Code's task notice), but its body is the tail of its output file, not steps.
#[derive(Serialize)]
pub struct Helper {
    /// Claude Code's agent id.
    id: String,
    /// The Delegating step that started it (its tool_use id).
    step: String,
    /// The one-line description the session gave it.
    desc: String,
    /// Its agent type ("Explore", "general-purpose"), from its meta file; "" until that's read.
    kind: String,
    model: String,
    started_ms: u64,
    /// When the session was told it finished (its task-notification; a script also ends on the exit line
    /// at the end of its output file); 0 until then.
    finished_ms: u64,
    /// When it last did something: an agent's newest step, a script's output file last written; its
    /// start until then. Running with nothing for STUCK_MS reads as stuck (`stuck`, added in `steps`).
    last_ms: u64,
    /// An agent's tokens so far (input, output, and cache reads and writes), summed over its log's replies. 0 for a script.
    tokens: u64,
    /// Its own steps, from its log (the last MAX_HELPER_ITEMS; `more` says how many came before).
    items: Vec<Item>,
    more: usize,
    #[serde(skip)]
    feed: Option<Box<Feed>>,
    /// A background shell command, not an agent: `command` ran, `output` is the tail of what it wrote
    /// (`lines` in all), `failed` once its notice said so.
    script: bool,
    command: String,
    output: Vec<String>,
    lines: usize,
    failed: bool,
    #[serde(skip)]
    out_path: Option<std::path::PathBuf>,
    #[serde(skip)]
    out_len: u64,
    /// Script: its end has been seen to (its copy stopped, a long failure noted).
    #[serde(skip)]
    closed: bool,
    /// When its session last sent it a message that put it back to work (SendMessage); 0 if never.
    #[serde(skip)]
    resumed_ms: u64,
    /// A teammate (an agent started with a name): it stays after its task, idle, until its session gives it
    /// more; `name` is what its session calls it by. From its launch, else its meta file once that's there.
    teammate: bool,
    name: String,
    /// Its meta file has been read for what its launch didn't say (once it was there).
    #[serde(skip)]
    meta_read: bool,
}

/// A background task that failed after running this long: worth a word (a quick failure is one the
/// session sees to itself).
const FAILURE_WORTH_MS: u64 = 60_000;
/// Running with nothing new for this long: stuck.
const STUCK_MS: u64 = 180_000;

/// A session's background task that failed after a while (FAILURE_WORTH_MS): the session's log,
/// when, the task's description and its output's last line. The hub takes these (`take_task_failures`).
#[derive(Clone, Debug, PartialEq)]
pub struct TaskFailure {
    pub path: String,
    pub at_ms: u64,
    pub desc: String,
    pub line: String,
}

/// A helper's steps shown in the chat: the newest of them.
const MAX_HELPER_ITEMS: usize = 80;
/// Helpers remembered per session (finished ones go first, oldest first).
const MAX_HELPERS: usize = 12;
/// A background script's output shown: its last lines, read from the end of its file.
const MAX_SCRIPT_LINES: usize = 200;
const SCRIPT_TAIL_BYTES: u64 = 256 * 1024;

impl Helper {
    /// Running: not reported finished, and its log doesn't end on an answer (one from before its session
    /// sent it a message doesn't count, nor one that waits on its own background work: a command or agent
    /// it started, which brings it back when it's done).
    pub fn running(&self) -> bool {
        self.finished_ms == 0 && self.feed.as_ref().is_none_or(|f| f.ended_ms == 0 || f.ended_ms < self.resumed_ms || f.helpers.iter().any(Helper::running))
    }

    /// Running, with nothing done for STUCK_MS as of `now`.
    fn stuck(&self, now: u64) -> bool {
        self.running() && now.saturating_sub(self.last_ms) > STUCK_MS
    }

    /// Read what its log added; true when its steps changed.
    fn catch_up(&mut self) -> bool {
        if self.script {
            let changed = self.read_output();
            self.last_ms = self.out_path.as_deref().and_then(mtime_ms).unwrap_or(self.started_ms);
            return changed;
        }
        let Some(f) = self.feed.as_mut() else { return false };
        let before = f.version;
        f.catch_up();
        if f.version == before {
            return false;
        }
        self.tokens = f.tokens;
        let all: Vec<&Item> = f.turns.iter().flat_map(|t| t.items.iter()).collect();
        let skip = all.len().saturating_sub(MAX_HELPER_ITEMS);
        self.more = skip + f.turns.iter().map(|t| t.more).sum::<usize>();
        self.items = all.into_iter().skip(skip).cloned().collect();
        self.last_ms = self.items.last().map(|i| match i { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms }).filter(|&at| at > 0).unwrap_or(self.started_ms).max(self.resumed_ms);
        if self.model.is_empty() {
            self.model = f.meta.model.clone();
        }
        // Its meta file can come after it started (copied from another machine).
        if self.kind.is_empty() {
            self.kind = helper_kind(std::path::Path::new(&f.path));
        }
        // Taken up on a message to it, its launch not seen: named by its meta file, which also says a teammate.
        if self.desc.is_empty() || !self.meta_read {
            let meta = helper_meta(std::path::Path::new(&f.path));
            self.meta_read = !meta.is_null();
            let m = |k: &str| meta.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            if self.desc.is_empty() {
                self.desc = m("description");
            }
            if m("taskKind") == "in_process_teammate" {
                self.teammate = true;
                self.name = m("name");
            }
        }
        true
    }
}

impl Helper {
    /// A script's output file grew (or shrank): its tail again. True when what's shown changed.
    fn read_output(&mut self) -> bool {
        let Some(path) = self.out_path.as_ref() else { return false };
        let Ok(mut file) = std::fs::File::open(path) else { return false };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len == self.out_len {
            return false;
        }
        self.out_len = len;
        let from = len.saturating_sub(SCRIPT_TAIL_BYTES);
        if file.seek(SeekFrom::Start(from)).is_err() {
            return false;
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return false;
        }
        let text = String::from_utf8_lossy(&buf);
        // Cut mid-file: the first partial line goes.
        let text = if from > 0 { text.split_once('\n').map(|(_, r)| r).unwrap_or("") } else { &text };
        let all: Vec<&str> = text.lines().collect();
        self.lines = all.len() + usize::from(from > 0);
        self.output = all.iter().rev().take(MAX_SCRIPT_LINES).rev().map(|l| cut(&strip_ansi(l), LINE_CHARS)).collect();
        // Claude Code ends the file on "[exited with code N]": over, even if its notice never shows up.
        if self.finished_ms == 0 {
            if let Some(code) = self.output.iter().rev().find(|l| !l.trim().is_empty()).and_then(|l| exit_code_line(l)) {
                self.finished_ms = mtime_ms(path).unwrap_or_else(crate::model::now_ms).max(1);
                self.failed = code != 0;
            }
        }
        true
    }

    /// A script's output, its last line that says something (not the exit line).
    fn last_line(&self) -> String {
        self.output.iter().rev().map(|l| l.trim()).find(|l| !l.is_empty() && exit_code_line(l).is_none()).unwrap_or("").to_string()
    }
}

/// "[exited with code N]" (Claude Code's last line in a background command's output file): N.
fn exit_code_line(line: &str) -> Option<i64> {
    line.trim().strip_prefix("[exited with code ")?.strip_suffix(']')?.trim().parse().ok()
}

/// When a file was last written, in ms since 1970.
fn mtime_ms(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// A background agent's meta file, beside its log (its type, the description it was given); Null when there's none (yet).
fn helper_meta(log: &std::path::Path) -> Value {
    std::fs::read(log.with_extension("meta.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null)
}

/// A background agent's type ("Explore"), from the meta file beside its log; "" when there's none (yet).
fn helper_kind(log: &std::path::Path) -> String {
    helper_meta(log).get("agentType").and_then(Value::as_str).unwrap_or("").to_string()
}

/// An Agent call's launch status that leaves it working on its own: a background agent ("async_launched"),
/// or a teammate, one given a name ("teammate_spawned"). A teammate writes its log where the others do and
/// ends its turn the same way; it reports by a message (an idle notice), not a task notice, so it reads as
/// done once its log ends on its answer, and at work again when its session messages it.
fn in_background(status: &str) -> bool {
    matches!(status, "async_launched" | "teammate_spawned")
}

/// The background agent a SendMessage result says it put back to work (`resumedAgentId`, or `revivedAgentId`).
fn resumed_agent(extra: Option<&Value>) -> Option<String> {
    let x = extra?;
    ["resumedAgentId", "revivedAgentId"].iter().find_map(|k| x.get(*k).and_then(Value::as_str).filter(|id| !id.is_empty())).map(str::to_string)
}

/// Every `<task-notification>…</task-notification>` in a text, wherever it sits (Claude Code wraps them
/// in a system notice, and can put two in one message).
fn notifications(text: &str) -> Vec<&str> {
    text.split("<task-notification>").skip(1).filter_map(|r| r.split("</task-notification>").next()).collect()
}

/// Where a session's background agent writes its log, and its meta file (`<session folder>/subagents/`).
#[cfg(test)]
pub fn helper_path_for_test(session_log: &str, agent_id: &str) -> Option<std::path::PathBuf> {
    helper_path(session_log, agent_id)
}
fn helper_path(session_log: &str, agent_id: &str) -> Option<std::path::PathBuf> {
    let folder = session_log.strip_suffix(".jsonl")?;
    Some(std::path::Path::new(folder).join("subagents").join(format!("agent-{agent_id}.jsonl")))
}

static FEEDS: Mutex<Option<HashMap<String, Feed>>> = Mutex::new(None);

/// A session's background agent is still finished (not put back to work since); also when its feed is gone.
fn helper_done(path: &str, id: &str) -> bool {
    let guard = crate::locking::lock(&FEEDS, "FEEDS");
    let h = guard.as_ref().and_then(|feeds| feeds.get(path)).and_then(|f| f.helpers.iter().find(|h| h.id == id));
    h.is_none_or(|h| h.finished_ms != 0)
}

/// The session's turns and steps, newest last. `known`: the version the caller already has; when
/// nothing changed, only the version comes back.
pub fn steps(path: &str, known: u64) -> Value {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    // A feed nobody asked for in a while is dropped: a fresh one starts mid-file and counts its version
    // from 1 again. Its count can land exactly on the version the caller already has, and the "nothing
    // changed" answer would leave the caller's model and steps stale forever. A fresh feed answers in
    // full, whatever its count, so the caller redraws from what's there now.
    let fresh = !feeds.contains_key(path);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    if feed.version == known && !fresh {
        return json!({ "version": feed.version });
    }
    let turns: Vec<&Turn> = feed.turns.iter().filter(|t| !t.items.is_empty()).collect();
    let meta = (!feed.meta.model.is_empty() || feed.meta.context > 0).then_some(&feed.meta);
    // Each helper with whether it's still at work, and stuck (read from its log and the clock, not fields).
    let now = crate::model::now_ms();
    let helpers: Vec<Value> = feed.helpers.iter().map(|h| { let mut v = serde_json::to_value(h).unwrap_or(Value::Null); v["running"] = json!(h.running()); v["stuck"] = json!(h.stuck(now)); v }).collect();
    json!({ "version": feed.version, "turns": turns, "meta": meta, "helpers": helpers })
}

/// The background agents a Claude Code session has running right now: their descriptions.
/// (The session is at its prompt while they run; Claude Code wakes it when they report.)
pub fn running_helpers(path: &str) -> Vec<String> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    feed.helpers.iter().filter(|h| h.running() && !h.script).map(|h| h.desc.clone()).collect()
}

/// How many background tasks a Claude Code session has running right now: its agents and its shell
/// commands in the background alike (the session's card counts them).
pub fn running_background(path: &str) -> usize {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    feed.helpers.iter().filter(|h| h.running()).count()
}

/// The background tasks that failed after a while since last asked, across every open feed (each is
/// given out once). Nothing is read here: a failure is noted when its session's feed reads the notice.
pub fn take_task_failures() -> Vec<TaskFailure> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.values_mut().flat_map(|f| f.failures.drain(..)).collect()
}

/// How full the session's context is (the latest reply's tokens, as a share of the model's window), 0–100.
/// The model a session last answered with ("claude-opus-5-5"), from its transcript.
pub fn model_of(path: &str) -> Option<String> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    Some(model_for_window(&feed.meta.model, feed.meta.window)).filter(|m| !m.is_empty())
}

/// A model to start Claude Code on with the same window: the transcript names a 1M session's model without
/// "[1m]", and `--model` without it starts one on the standard window.
fn model_for_window(model: &str, window: u64) -> String {
    if model.is_empty() || window < 1_000_000 || model.ends_with("[1m]") {
        model.to_string()
    } else {
        format!("{model}[1m]")
    }
}

/// What the session last said (its latest turn's last words), for the session that continues its hand-off.
pub fn last_say(path: &str) -> Option<String> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    let turn = feed.turns.last()?;
    turn.items.iter().rev().find_map(|i| match i { Item::Say { text, .. } if !text.trim().is_empty() => Some(text.trim().to_string()), _ => None })
}

/// A hand-off note: a file named HANDOFF.md, or any .md whose name says hand-off ("handoff", "hand-off" or
/// "hand_off", in any case), wherever it is. Only the name counts: another file in a handoff/ folder doesn't.
pub fn is_handoff_note(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    name.ends_with(".md") && ["handoff", "hand-off", "hand_off"].iter().any(|w| name.contains(w))
}

/// The hand-off note the session wrote itself, if its last work did: its newest Write of a hand-off note
/// (`is_handoff_note`) that went through, with no prompt of yours after it. Its path as the log names it
/// (on the session's machine: a string to pass on, never looked up here), and when, by the log's clock.
/// Your latest prompt in a session's log: when, by its clock (0: none read), and whether it was Cue's hand-off ask.
pub fn last_prompt(path: &str) -> (u64, bool) {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    feed.prompted
}

pub fn handoff_note(path: &str) -> Option<(String, u64)> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    feed.note.clone()
}

/// Each Claude session's context window as its status line last said (by session id), and each model's
/// as a session on it last said. Its transcript doesn't say, and the model's name doesn't always either
/// (a 1M window without "[1m]"). Kept on disk: a status line only runs when its session does something,
/// so after Cue restarts an idle session would otherwise read as on the 200k guess (196k → 98%).
#[derive(Default, Serialize, serde::Deserialize)]
struct Windows {
    #[serde(default)]
    sessions: HashMap<String, u64>,
    #[serde(default)]
    models: HashMap<String, u64>,
}
static WINDOWS: Mutex<Option<Windows>> = Mutex::new(None);

fn windows_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("windows.json")
}

/// The windows Cue knows (read from disk the first time; tests never touch the real one).
fn with_windows<R>(f: impl FnOnce(&mut Windows) -> R) -> R {
    let mut g = crate::locking::lock(&WINDOWS, "WINDOWS");
    let w = g.get_or_insert_with(|| if cfg!(test) { Windows::default() } else { std::fs::read(windows_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() });
    f(w)
}

/// A session's window: what its status line said, else what one on the same model said.
fn known_window(path: &str, model: &str) -> Option<u64> {
    let sid = std::path::Path::new(path).file_stem()?.to_str()?;
    with_windows(|w| w.sessions.get(sid).or_else(|| w.models.get(model)).copied())
}

/// What Claude Code's status line says a session's window is. A feed that has it open takes it at
/// once (and counts a new version, so the window redraws its context). `for_model`: it also stands for
/// its model's window until a session says otherwise: this Mac's sessions, not a machine's (one there
/// may run the same model with another window, through another provider).
pub fn set_window(session_id: &str, window: u64, for_model: bool) {
    if window == 0 {
        return;
    }
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    let mine = |f: &Feed| f.kind == Kind::Claude && std::path::Path::new(&f.path).file_stem().and_then(|s| s.to_str()) == Some(session_id);
    let model = feeds.values().find(|f| mine(f)).map(|f| f.meta.model.clone()).unwrap_or_default();
    with_windows(|w| {
        // Old sessions' entries go now and then; the models' stay.
        if w.sessions.len() > 2000 {
            w.sessions.clear();
        }
        let new_session = w.sessions.insert(session_id.to_string(), window) != Some(window);
        let new_model = for_model && !model.is_empty() && w.models.insert(model, window) != Some(window);
        if (new_session || new_model) && !cfg!(test) {
            let tmp = windows_file().with_extension("json.tmp");
            if serde_json::to_vec(&*w).ok().is_some_and(|b| std::fs::write(&tmp, b).is_ok()) {
                let _ = std::fs::rename(&tmp, windows_file());
            }
        }
    });
    for f in feeds.values_mut() {
        if mine(f) && f.meta.window != window {
            f.meta.window = window;
            f.version += 1;
        }
    }
}

/// What a waiting card's insights need from a session's log.
#[derive(Serialize)]
pub struct InsightState {
    /// When its cached context runs out (0 when its log doesn't say it caches).
    pub until_ms: u64,
    /// Its context now, and the context it started with.
    pub tokens: u64,
    pub start: u64,
    /// When its latest reply came (a compaction since then makes the context count stale until its next reply).
    pub last_ms: u64,
    /// How many warm replies a cold one costs (prices::cold_x).
    pub cold_x: u64,
    /// What that cold reply's re-write costs at list prices, in cents (prices::cold_cents).
    pub cold_cents: u64,
    /// Where Claude Code has lately compacted sessions on its model on its own (auto_compact_at).
    pub auto_at: u64,
}

/// How many of the latest auto-compactions on a model auto_compact_at looks at, and the fewest it says anything from.
const AUTO_SEEN: u32 = 10;
const AUTO_MIN: usize = 2;
/// Where Claude Code has lately compacted sessions on `model` on its own: the least context of its last
/// AUTO_SEEN auto-compactions (from the turns history), 0 below AUTO_MIN. Not a fixed point: its trigger
/// moves (one model's sessions went from about 967k to anywhere between 206k and 474k between versions),
/// so the least seen lately is the honest warning. Asked again every 10 minutes, as the history is read.
fn auto_compact_at(model: &str) -> u64 {
    static SEEN: Mutex<Option<HashMap<String, (Instant, u64)>>> = Mutex::new(None);
    if model.is_empty() || cfg!(test) {
        return 0;
    }
    let mut guard = crate::locking::lock(&SEEN, "AUTO_SEEN");
    let seen = guard.get_or_insert_with(HashMap::new);
    if let Some(&(_, v)) = seen.get(model).filter(|(at, _)| at.elapsed() < Duration::from_secs(600)) {
        return v;
    }
    let all = crate::db::auto_compacts(model, AUTO_SEEN);
    let v = if all.len() >= AUTO_MIN { all.into_iter().min().unwrap_or(0) } else { 0 };
    seen.insert(model.to_string(), (Instant::now(), v));
    v
}

pub fn insight_state(path: &str) -> Option<InsightState> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    let m = &feed.meta;
    let until = if m.cache_ttl_s > 0 && m.last_call_ms > 0 { m.last_call_ms + m.cache_ttl_s * 1000 } else { 0 };
    (m.context > 0).then_some(InsightState { until_ms: until, tokens: m.context, start: m.start_context, last_ms: m.last_call_ms, cold_x: m.cold_x, cold_cents: m.cold_cents, auto_at: m.auto_at })
}

pub fn context_pct(path: &str) -> Option<u8> {
    let mut guard = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    let m = &feed.meta;
    (m.window > 0).then(|| (m.context * 100 / m.window).min(100) as u8)
}

/// A time a reply's line really carried (2001 on): a line without one still marks the turn as replied to.
const SANE_MS: u64 = 1_000_000_000_000;

/// One turn of a session, as Insights keeps it (logs/turns.rs): what it cost and how it went, and the start
/// of its prompt (and of the next one, when that corrected it).
pub struct TurnRow {
    pub at_ms: u64,
    pub tokens: u64,
    pub context: u64,
    pub prompt_chars: u64,
    pub model: String,
    pub cwd: String,
    /// Steps that failed (a command's non-zero exit, an edit that didn't apply).
    pub errors: u64,
    /// You stopped it mid-turn.
    pub interrupted: bool,
    /// Your next prompt read as a correction ("no, …", "actually …", "still broken").
    pub corrected: bool,
    /// Its tokens by kind (for prices, and what a late answer re-wrote).
    pub usage: Usage,
    /// How long the session had waited on you since its reply before (0: not known, as for a log's first
    /// turn read here).
    pub waited_ms: u64,
    /// Its prompt and, when corrected, your next one: the start of each, for Insights' examples.
    pub prompt: String,
    pub correction: String,
    /// Reads and searches it ran itself, background agents and relay executors it started.
    pub looks: u64,
    pub agents: u64,
    pub relay: u64,
    /// The areas of the project it edited (area_of), each once, one per line.
    pub areas: String,
    /// The context Claude Code compacted it at on its own (its log's compact_boundary; 0: it didn't).
    pub auto_compact: u64,
}

/// Whether a prompt reads as correcting the turn before it. English openers only.
/// SHORTCUT: a fixed list of openers, blind to corrections phrased any other way; upgrade to a judgment
/// the person can tune when the turns history is used for more than counting.
fn reads_as_correction(prompt: &str) -> bool {
    let p = prompt.trim_start().to_lowercase();
    const OPENERS: [&str; 30] = ["no ", "no,", "no.", "nope", "wrong", "that's not", "thats not", "not that", "not what i", "actually", "wait", "stop", "undo", "revert", "i meant", "i said", "you missed", "you forgot", "you didn't", "you broke", "why did you", "why are you", "still not", "still broken", "still failing", "it's still", "doesn't work", "didn't work", "does not work", "did not work"];
    OPENERS.iter().any(|o| p.starts_with(o)) || p == "no"
}

/// A prompt's first 160 characters as you wrote them: a long block of instructions appended in brackets at
/// its end (a dictation's or a tool's) is left out, or it would fill the snippet.
fn snippet(prompt: &str) -> String {
    let p = prompt.trim_end();
    let bare = p.strip_suffix(')').and_then(|rest| rest.rfind('(').filter(|&i| p.len() - i > 40 && !rest[i + 1..].contains(['(', ')'])).map(|i| rest[..i].trim_end())).filter(|b| !b.is_empty());
    bare.unwrap_or(p).chars().take(160).collect()
}

/// The turns in `path` from byte `from` on, for Insights: every complete turn, and the open last one too.
/// Returns them with the byte where that open turn starts, so the next read begins there and finishes it.
/// Reads at most `max` bytes, whole lines only.
pub fn turn_rows(path: &str, from: u64, max: u64) -> (Vec<TurnRow>, u64) {
    let Ok(file) = std::fs::File::open(path) else { return (Vec::new(), from) };
    let mut file = file;
    if file.seek(SeekFrom::Start(from)).is_err() {
        return (Vec::new(), from);
    }
    let mut buf = Vec::new();
    if file.take(max).read_to_end(&mut buf).is_err() {
        return (Vec::new(), from);
    }
    let Some(end) = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1) else { return (Vec::new(), from) };
    let kind = kind_of(path);
    let mut feed = Feed::start(path, 0, 0);
    feed.kind = kind;
    // Per turn, beside the feed's: when it started in the file, its model and folder, whether you stopped it.
    let (mut starts, mut models, mut cwds, mut stopped): (Vec<u64>, Vec<String>, Vec<String>, Vec<bool>) = Default::default();
    let mut autos: Vec<u64> = Vec::new();
    let mut cwd = String::new();
    let mut pos = 0usize;
    for line in buf[..end].split(|&b| b == b'\n') {
        let at = from + pos as u64;
        pos += line.len() + 1;
        let Ok(e) = serde_json::from_slice::<Value>(line) else { continue };
        if let Some(c) = e.get("cwd").or_else(|| e.pointer("/payload/cwd")).and_then(Value::as_str).filter(|c| !c.is_empty()) {
            cwd = c.to_string();
        }
        feed.observe(&e);
        for c in translate(kind, &e, &mut feed.patches) {
            if c.get("type").and_then(Value::as_str) == Some("user") {
                let blocks = content_blocks(&c);
                let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
                if text.trim_start().starts_with("[Request interrupted") {
                    if let Some(s) = stopped.last_mut() {
                        *s = true;
                    }
                }
            }
            feed.take(&c);
        }
        while starts.len() < feed.turns.len() {
            starts.push(at);
            models.push(String::new());
            cwds.push(cwd.clone());
            stopped.push(false);
            autos.push(0);
        }
        // Claude Code compacted it on its own, at this much context: kept with the turn it happened in.
        if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") && e.pointer("/compactMetadata/trigger").and_then(Value::as_str) == Some("auto") {
            if let (Some(a), Some(pre)) = (autos.last_mut(), e.pointer("/compactMetadata/preTokens").and_then(Value::as_u64)) {
                *a = pre;
            }
        }
        if let Some(m) = models.last_mut() {
            if !feed.meta.model.is_empty() {
                m.clone_from(&feed.meta.model);
            }
        }
    }
    let n = feed.turns.len();
    // A turn Claude Code started itself (a background task reporting back) has no prompt: it wasn't you
    // answering late, and the turn a correction corrects is the one before your next prompt.
    let next_yours = |i: usize| feed.turns[i + 1..].iter().find(|t| !t.prompt.is_empty());
    let rows = feed.turns.iter().enumerate().map(|(i, t)| TurnRow {
        at_ms: t.at_ms,
        tokens: t.tokens,
        context: t.context,
        prompt_chars: t.prompt.chars().count() as u64,
        model: models[i].clone(),
        cwd: cwds[i].clone(),
        errors: t.items.iter().filter(|x| matches!(x, Item::Step { bad: true, .. })).count() as u64,
        interrupted: stopped[i],
        corrected: next_yours(i).is_some_and(|next| reads_as_correction(&next.prompt)),
        usage: t.usage,
        waited_ms: i.checked_sub(1).filter(|_| !t.prompt.is_empty()).map(|p| feed.turns[p].usage.last_ms).filter(|&l| l > SANE_MS && t.at_ms > l).map_or(0, |l| t.at_ms - l),
        prompt: snippet(&t.prompt),
        correction: next_yours(i).filter(|next| reads_as_correction(&next.prompt)).map(|next| snippet(&next.prompt)).unwrap_or_default(),
        looks: t.work.looks,
        agents: t.work.agents,
        relay: t.work.relay,
        areas: {
            let mut a: Vec<String> = t.work.edited.iter().filter_map(|f| area_of(&cwds[i], f)).collect();
            a.sort();
            a.dedup();
            a.join("\n")
        },
        auto_compact: autos[i],
    }).collect();
    // The open turn starts the next read; none read: carry on after what was read.
    let next = if n > 0 { starts[n - 1] } else { from + end as u64 };
    (rows, next)
}

/// How far into a log to look for its first reply's context (the start a feed opened mid-file misses).
const HEAD_BYTES: u64 = 1024 * 1024;

/// The context a session's first reply carried, and when, from the top of its log: what it started at.
fn start_of(path: &str, kind: Kind) -> (u64, u64) {
    let Ok(file) = std::fs::File::open(path) else { return (0, 0) };
    let mut buf = Vec::new();
    if file.take(HEAD_BYTES).read_to_end(&mut buf).is_err() {
        return (0, 0);
    }
    let end = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut head = Feed::start(path, 0, 0);
    head.kind = kind;
    for e in lines(&String::from_utf8_lossy(&buf[..end])) {
        head.observe(&e);
        for c in translate(kind, &e, &mut head.patches) {
            head.take(&c);
        }
        if head.meta.start_context > 0 {
            return (head.meta.start_context, head.meta.start_ms);
        }
    }
    (0, 0)
}

impl Feed {
    fn open(path: &str, version: u64) -> Feed {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let offset = len.saturating_sub(START_BYTES);
        let mut f = Feed::start(path, version, offset);
        if offset > 0 {
            (f.meta.start_context, f.meta.start_ms) = start_of(path, f.kind);
        }
        f
    }

    fn start(path: &str, version: u64, offset: u64) -> Feed {
        Feed { path: path.to_string(), meta: Meta::default(), kind: kind_of(path), patches: HashMap::new(), offset, align: offset > 0, turns: Vec::new(), version, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false) }
    }

    /// A background agent's log, read from its start (it's small: one task).
    fn open_helper(path: &str) -> Feed {
        Feed { path: path.to_string(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: true, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false) }
    }

    /// A background agent just started (its launch result came in): remember it and open its log.
    fn add_helper(&mut self, step: &str, extra: &Value, at_ms: u64) {
        let s = |k: &str| extra.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        // A teammate's launch has no description: its meta file's, else its name.
        let meta = || helper_path(&self.path, &s("agentId")).map(|p| helper_meta(&p)).and_then(|m| m.get("description").and_then(Value::as_str).map(str::to_string));
        let desc = Some(s("description")).filter(|d| !d.is_empty()).or_else(meta).filter(|d| !d.is_empty()).unwrap_or_else(|| s("name"));
        self.add_agent(step, s("agentId"), desc, s("resolvedModel"), at_ms);
        if s("status") == "teammate_spawned" {
            if let Some(h) = self.helpers.iter_mut().find(|h| h.id == s("agentId")) {
                h.teammate = true;
                h.name = s("name");
            }
        }
    }

    fn add_agent(&mut self, step: &str, id: String, desc: String, model: String, at_ms: u64) {
        if id.is_empty() || self.helpers.iter().any(|h| h.id == id) {
            return;
        }
        // A session on another machine: its agent's log is copied over (to where it's read from here).
        crate::machines::logs::copy_helper(&self.path, &id);
        let path = helper_path(&self.path, &id);
        let kind = path.as_ref().map(|p| helper_kind(p)).unwrap_or_default();
        let feed = path.and_then(|p| p.to_str().map(|p| Box::new(Feed::open_helper(p))));
        self.helpers.push(Helper { id, step: step.to_string(), desc, kind, model, started_ms: at_ms, finished_ms: 0, last_ms: at_ms, tokens: 0, items: Vec::new(), more: 0, feed, script: false, command: String::new(), output: Vec::new(), lines: 0, failed: false, out_path: None, out_len: 0, closed: false, resumed_ms: 0, teammate: false, name: String::new(), meta_read: false });
        self.trim_helpers();
    }

    /// The session sent one of its background agents a message (Claude Code's SendMessage), which put it back
    /// to work: it writes on to the same log, and its task notifies again when it's done. At work again, now
    /// under the step that sent it; one not known here (read from mid-file, or dropped) is taken up from here.
    fn resume_helper(&mut self, step: &str, id: &str, at_ms: u64) {
        if id.is_empty() {
            return;
        }
        let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) else {
            // Not one of its agents Cue knows: taken only with its log there (a message to anything else, or an
            // agent whose log can't be read, would read as at work for good).
            if !helper_path(&self.path, id).is_some_and(|p| p.exists()) {
                return;
            }
            let desc = helper_path(&self.path, id).map(|p| helper_meta(&p)).and_then(|m| m.get("description").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
            self.add_agent(step, id.to_string(), desc, String::new(), at_ms);
            if let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) {
                h.resumed_ms = at_ms;
            }
            return;
        };
        if h.script {
            return;
        }
        h.finished_ms = 0;
        h.last_ms = at_ms;
        h.resumed_ms = at_ms;
        h.step = step.to_string();
        // Its copy from another machine was stopped when it reported: on again, from where it was.
        crate::machines::logs::copy_helper(&self.path, id);
    }

    /// A shell command went to the background (Claude Code's Bash tool, run in the background or moved
    /// there on a timeout): remember it, with its output file from the launch text. Named by the
    /// description the session gave it, else the command.
    fn add_script(&mut self, step: &str, desc: &str, subject: &str, command: &str, launch: &str, extra: &Value, at_ms: u64) {
        let id = extra.get("backgroundTaskId").and_then(Value::as_str).unwrap_or("").to_string();
        if id.is_empty() || self.helpers.iter().any(|h| h.id == id) {
            return;
        }
        let out_path = launch.split("Output is being written to: ").nth(1).and_then(|r| r.split_whitespace().next()).map(|p| std::path::PathBuf::from(p.trim_end_matches('.')));
        // A session on another machine: its output file is there, and is copied over (to where it's read from here), like an agent's log.
        let out_path = out_path.and_then(|remote| match crate::machines::logs::copy_task(&self.path, &id, &remote.to_string_lossy()) {
            Some(local) => Some(local),
            None if crate::machines::logs::is_copy(&self.path) => None,
            None => Some(remote),
        });
        let desc = if desc.trim().is_empty() { subject.to_string() } else { desc.trim().to_string() };
        self.helpers.push(Helper { id, step: step.to_string(), desc, kind: "script".into(), model: String::new(), started_ms: at_ms, finished_ms: 0, last_ms: at_ms, tokens: 0, items: Vec::new(), more: 0, feed: None, script: true, command: command.trim().to_string(), output: Vec::new(), lines: 0, failed: false, out_path, out_len: 0, closed: false, resumed_ms: 0, teammate: false, name: String::new(), meta_read: false });
        self.trim_helpers();
    }

    /// The oldest finished ones go first; what's running is never dropped.
    fn trim_helpers(&mut self) {
        while self.helpers.len() > MAX_HELPERS {
            match self.helpers.iter().position(|h| !h.running()) {
                Some(i) => { self.helpers.remove(i); }
                None => break,
            }
        }
    }

    /// Claude Code told the session a background task finished: every `<task-notification>` in `text`
    /// (it comes as the prompt of a queued command, or wrapped in a system notice in a user message,
    /// two at a time sometimes). True when one of this session's tasks ended on it.
    fn helpers_finished(&mut self, text: &str, at_ms: u64) -> bool {
        notifications(text).into_iter().fold(false, |any, n| self.helper_finished(n, at_ms) || any)
    }

    /// One task notice: mark its task finished (once; a notice can be read twice).
    fn helper_finished(&mut self, notification: &str, at_ms: u64) -> bool {
        let tag = |t: &str| notification.split(&format!("<{t}>")).nth(1).and_then(|r| r.split(&format!("</{t}>")).next()).map(str::trim).unwrap_or("").to_string();
        let (id, status) = (tag("task-id"), tag("status"));
        let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) else { return false };
        if status == "running" || h.finished_ms != 0 {
            return false;
        }
        // An agent that stopped to wait on background work of its own: "completed", but not done. Its
        // task notifies again when it is.
        if tag("summary").contains("waiting on its own background work") || tag("note").contains("background work of its own still running") {
            return false;
        }
        h.finished_ms = at_ms.max(1);
        if h.script {
            h.failed = status != "completed";
            h.read_output();   // its last lines, written before the notice
            return true;
        }
        // Copied from another machine: its log is complete once it reports (a moment for its last lines),
        // unless its session sent it a message meanwhile.
        let (copy, id) = (self.path.clone(), h.id.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if helper_done(&copy, &id) {
                crate::machines::logs::stop_helper(&copy, &id);
            }
        });
        true
    }

    /// A script that ended (its notice came, or its output file ends on the exit line), seen to once:
    /// its copy from another machine stopped, and a failure after a while noted for the hub.
    fn close_scripts(&mut self) {
        for h in self.helpers.iter_mut().filter(|h| h.script && h.finished_ms > 0 && !h.closed) {
            h.closed = true;
            let (copy, id) = (self.path.clone(), h.id.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                crate::machines::logs::stop_task(&copy, &id);
            });
            if h.failed && h.finished_ms.saturating_sub(h.started_ms) > FAILURE_WORTH_MS {
                self.failures.push(TaskFailure { path: self.path.clone(), at_ms: h.finished_ms, desc: h.desc.clone(), line: h.last_line() });
            }
        }
    }

    /// Read what was added since last time (whole lines only; a half-written line waits for the next call).
    fn catch_up(&mut self) {
        let mut changed = self.read_new();
        // Its background agents' logs too: their steps are part of what this session is doing.
        for h in &mut self.helpers {
            changed |= h.catch_up();
        }
        self.close_scripts();
        if changed {
            self.meta.cold_x = super::prices::cold_x(&self.meta.model, self.meta.cache_ttl_s);
            self.meta.cold_cents = super::prices::cold_cents(&self.meta.model, self.meta.cache_ttl_s, self.meta.context);
            self.meta.auto_at = if self.kind == Kind::Claude { auto_compact_at(&self.meta.model) } else { 0 };
            if self.turns.len() > MAX_TURNS {
                self.turns.drain(..self.turns.len() - MAX_TURNS);
            }
            self.version += 1;
        }
    }

    /// Read this log's new lines; true when anything shown changed.
    fn read_new(&mut self) -> bool {
        let Ok(mut file) = std::fs::File::open(&self.path) else { return false };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut changed = false;
        if len < self.offset {
            // The file was replaced: start over (and count a version, so callers redraw), still a background
            // agent's log if it was one (else its sidechain lines would all be skipped from here on).
            let helper = self.helper;
            *self = Feed::open(&self.path, self.version);
            self.helper = helper;
            changed = true;
        }
        if len <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return changed;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return changed;
        }
        let mut from = 0;
        if self.align {
            let Some(nl) = buf.iter().position(|&b| b == b'\n') else { return changed };
            from = nl + 1;
            self.align = false;
        }
        let Some(end) = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1).filter(|&e| e > from) else {
            self.offset += from as u64;
            return changed;
        };
        let text = String::from_utf8_lossy(&buf[from..end]);
        for e in lines(&text) {
            changed |= self.observe(&e);
            for c in translate(self.kind, &e, &mut self.patches) {
                changed |= self.take(&c);
            }
        }
        self.offset += end as u64;
        changed
    }

    /// The model, context and cost from one raw log entry (each agent logs them its own way).
    /// Returns whether they changed.
    fn observe(&mut self, e: &Value) -> bool {
        let before = self.meta.clone();
        let n = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0);
        let m = &mut self.meta;
        match self.kind {
            Kind::Claude => {
                if e.get("type").and_then(Value::as_str) == Some("assistant") && (self.helper || e.get("isSidechain").and_then(Value::as_bool) != Some(true)) {
                    let msg = e.get("message").cloned().unwrap_or(Value::Null);
                    if let Some(u) = msg.get("usage") {
                        let (fresh, read, wrote) = (n(u.get("input_tokens")), n(u.get("cache_read_input_tokens")), n(u.get("cache_creation_input_tokens")));
                        let all = fresh + read + wrote;
                        if all > 0 {
                            m.context = all;
                        }
                        self.tokens += all + n(u.get("output_tokens"));
                        let reply = msg.get("id").and_then(Value::as_str).unwrap_or("");
                        if reply.is_empty() || reply != self.last_reply {
                            self.last_reply = reply.to_string();
                            if let Some(t) = self.turns.last_mut() {
                                t.tokens += all + n(u.get("output_tokens"));
                                // Claude Code splits what it wrote by how long it's kept; an older log doesn't.
                                let cc = u.get("cache_creation");
                                let hour = n(cc.and_then(|c| c.get("ephemeral_1h_input_tokens"))).min(wrote);
                                t.usage.add(fresh, read, wrote - hour, hour, n(u.get("output_tokens")), stamp(e));
                                if all > 0 {
                                    t.context = all;
                                    if m.start_context == 0 {
                                        m.start_context = all;
                                        m.start_ms = stamp(e);
                                    }
                                }
                            }
                            m.last_call_ms = stamp(e).max(m.last_call_ms);
                            let cc = u.get("cache_creation");
                            m.cache_ttl_s = if n(cc.and_then(|c| c.get("ephemeral_1h_input_tokens"))) > 0 {
                                3600
                            } else if n(cc.and_then(|c| c.get("ephemeral_5m_input_tokens"))) > 0 {
                                300
                            } else if read == 0 && wrote == 0 {
                                0
                            } else {
                                m.cache_ttl_s // only reads this time: the cache it read keeps its lifetime
                            };
                        }
                        let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
                        if !model.is_empty() && !model.starts_with('<') {
                            m.model = model.to_string();
                        }
                        // The transcript doesn't say the window. Claude Code's status line does (for this
                        // session, or another on its model); until then 1M once the model's name says so or
                        // it's past 200k, else unknown (0): the chat shows the tokens, not a guessed share.
                        m.window = match known_window(&self.path, &m.model) {
                            Some(w) => w,
                            None if m.model.contains("[1m]") || m.context > 200_000 || m.window == 1_000_000 => 1_000_000,
                            None => 0,
                        };
                    }
                }
                // Compacted (by you or on its own): its context is what the compact left, now, not at its next
                // reply. Till then the insights went on as if it were still as big (a hand-off offered at once).
                if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") && e.get("isSidechain").and_then(Value::as_bool) != Some(true) {
                    let post = n(e.pointer("/compactMetadata/postTokens"));
                    if post > 0 {
                        m.context = post;
                    }
                }
            }
            Kind::Codex => {
                let p = e.get("payload").cloned().unwrap_or(Value::Null);
                if e.get("type").and_then(Value::as_str) == Some("turn_context") {
                    if let Some(model) = p.get("model").and_then(Value::as_str) {
                        m.model = model.to_string();
                    }
                }
                if p.get("type").and_then(Value::as_str) == Some("token_count") {
                    if let Some(info) = p.get("info").filter(|i| i.is_object()) {
                        let last = info.get("last_token_usage");
                        let input = n(last.and_then(|l| l.get("input_tokens")));
                        if input > 0 {
                            m.context = input;
                            if let Some(t) = self.turns.last_mut() {
                                t.context = input;
                                if m.start_context == 0 {
                                    m.start_context = input;
                                    m.start_ms = stamp(e);
                                }
                            }
                        }
                        let w = n(info.get("model_context_window"));
                        if w > 0 {
                            m.window = w;
                        }
                        let total = n(info.get("total_token_usage").and_then(|t| t.get("total_tokens")));
                        // A new call (the session's running total grew; it repeats when nothing did). Its own
                        // usage is `last`: the first total read may be everything before, when the read
                        // started mid-file, so it isn't this turn's.
                        if total > self.codex_total {
                            if let Some(t) = self.turns.last_mut() {
                                t.tokens += if self.codex_total > 0 { total - self.codex_total } else { n(last.and_then(|l| l.get("total_tokens"))) };
                                let cached = n(last.and_then(|l| l.get("cached_input_tokens")));
                                t.usage.add(input.saturating_sub(cached), cached, 0, 0, n(last.and_then(|l| l.get("output_tokens"))), stamp(e));
                            }
                            self.codex_total = total;
                            m.last_call_ms = stamp(e).max(m.last_call_ms);
                        }
                    }
                }
            }
            Kind::Pi => {
                let msg = e.get("message").cloned().unwrap_or(Value::Null);
                if e.get("type").and_then(Value::as_str) == Some("model_change") {
                    if let Some(model) = e.get("modelId").and_then(Value::as_str) {
                        m.model = model.to_string();
                        m.window = pi_window(e.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
                    }
                }
                if msg.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(u) = msg.get("usage") {
                        let (fresh, read, wrote) = (n(u.get("input")), n(u.get("cacheRead")), n(u.get("cacheWrite")));
                        let all = fresh + read + wrote;
                        if all > 0 {
                            m.context = all;
                        }
                        if let Some(t) = self.turns.last_mut() {
                            t.tokens += all + n(u.get("output"));
                            t.usage.add(fresh, read, wrote, 0, n(u.get("output")), stamp(e));
                            if all > 0 {
                                t.context = all;
                                if m.start_context == 0 {
                                    m.start_context = all;
                                    m.start_ms = stamp(e);
                                }
                            }
                        }
                        m.last_call_ms = stamp(e).max(m.last_call_ms);
                        if let Some(c) = u.pointer("/cost/total").and_then(Value::as_f64) {
                            m.cost = Some(m.cost.unwrap_or(0.0) + c);
                        }
                    }
                    if let Some(model) = msg.get("model").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                        if m.model != model || m.window == 0 {
                            m.window = pi_window(msg.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
                        }
                        m.model = model.to_string();
                    }
                }
            }
        }
        self.meta != before
    }

    /// One transcript entry. Returns whether anything shown changed.
    fn take(&mut self, e: &Value) -> bool {
        // A helper agent's own steps (older transcripts keep them inline) aren't this session's.
        if !self.helper && e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let at = stamp(e);
        if self.helper {
            // Its answer ended: its report is written. A new message to it (its session can send one) puts it back to work.
            match e.get("type").and_then(Value::as_str) {
                Some("assistant") if e.pointer("/message/stop_reason").and_then(Value::as_str) == Some("end_turn") => self.ended_ms = at.max(1),
                Some("user") if !content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) => self.ended_ms = 0,
                _ => {}
            }
        }
        match e.get("type").and_then(Value::as_str) {
            // A queued command delivered mid-turn: a task notice comes this way too.
            Some("attachment") => {
                let a = e.get("attachment").cloned().unwrap_or(Value::Null);
                let prompt = a.get("prompt").and_then(Value::as_str).unwrap_or("");
                if a.get("commandMode").and_then(Value::as_str) == Some("task-notification") || prompt.contains("<task-notification>") {
                    return self.helpers_finished(prompt, at);
                }
                // Your message, after a Stop hook sent it back: new work, an answer again, as a task's report starts.
                if crate::logs::transcript::yours_mid_turn(e) && self.turns.last().is_some_and(|t| t.hooked_ms > 0) {
                    self.close_open("");
                    self.turns.push(Turn { at_ms: at, prompt: String::new(), items: Vec::new(), more: 0, hooked_ms: 0, tokens: 0, context: 0, usage: Usage::default(), work: Work::default() });
                    return true;
                }
                false
            }
            Some("user") => {
                let blocks = content_blocks(e);
                // A task notice wherever it sits in the text (wrapped in a system notice, beside a tool result).
                let ended = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).fold(false, |any, t| self.helpers_finished(t, at) || any);
                let results: Vec<&Value> = blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
                if !results.is_empty() {
                    let extra = e.get("toolUseResult");
                    return results.into_iter().fold(ended, |any, b| self.finish(b, extra, at) || any);
                }
                // A Stop hook sent it back to work (a meta entry, so before those are passed over): more of
                // the same turn, from here on the hook's follow-up.
                if crate::logs::transcript::is_stop_hook(e) {
                    // Read from mid-file, its turn's start further back: the turn starts here, already hooked.
                    if self.turns.is_empty() {
                        self.turns.push(Turn { at_ms: at, prompt: String::new(), items: Vec::new(), more: 0, hooked_ms: 0, tokens: 0, context: 0, usage: Usage::default(), work: Work::default() });
                    }
                    let Some(turn) = self.turns.last_mut().filter(|t| t.hooked_ms == 0) else { return false };
                    turn.hooked_ms = at.max(1);
                    return true;
                }
                if e.get("isMeta").and_then(Value::as_bool) == Some(true) || e.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
                    return ended;
                }
                let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
                // A message Cue pasted in comes back wrapped (and cut into chunks): Cue's hand-off ask too.
                let text = crate::sessions::unwrap_pasted(&text);
                let text = text.trim_start();
                if text.starts_with("[Request interrupted") {
                    return self.close_open("stopped");
                }
                // A local command (/rename, /model) and its output: no turn of work.
                if text.starts_with("<command-") || text.starts_with("<local-command") {
                    return ended;
                }
                self.close_open("");
                // A task's notice or a teammate's message: it wakes the session, but it isn't your prompt.
                let prompt = if crate::logs::transcript::harness_text(text) || crate::logs::transcript::peer_text(text) { String::new() } else { cut(text.trim(), 1200) };
                // Your prompt: it went on working after its note, so that note isn't where it stopped.
                if !prompt.is_empty() {
                    self.note = None;
                    self.note_calls.clear();
                    self.prompted = (at, prompt.starts_with(HANDOFF_ASK_START));
                }
                self.turns.push(Turn { at_ms: at, prompt, items: Vec::new(), more: 0, hooked_ms: 0, tokens: 0, context: 0, usage: Usage::default(), work: Work::default() });
                ended // an empty turn isn't shown until something happens in it; a task that ended is
            }
            Some("assistant") => {
                let mut changed = false;
                for b in content_blocks(e) {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t = b.get("text").and_then(Value::as_str).unwrap_or("").trim();
                            if !t.is_empty() {
                                self.push(Item::Say { text: t.to_string(), at_ms: at });
                                changed = true;
                            }
                        }
                        Some("tool_use") => {
                            let file = b.pointer("/input/file_path").and_then(Value::as_str).unwrap_or("");
                            // Written, or (in the turn Cue's hand-off ask began) its older note edited into the new one.
                            let tool = b.get("name").and_then(Value::as_str).unwrap_or("");
                            let asked = self.turns.last().is_some_and(|t| t.prompt.starts_with(HANDOFF_ASK_START));
                            if !self.helper && (tool == "Write" || (asked && matches!(tool, "Edit" | "MultiEdit"))) && is_handoff_note(file) {
                                self.note_calls.insert(b.get("id").and_then(Value::as_str).unwrap_or("").to_string(), file.to_string());
                            }
                            self.push(step_of(&b, at));
                            if let Some(t) = self.turns.last_mut() {
                                t.work.count(&b);
                            }
                            changed = true;
                        }
                        _ => {}
                    }
                }
                changed
            }
            _ => false,
        }
    }

    fn push(&mut self, item: Item) {
        if self.turns.is_empty() {
            // Started mid-file, inside a turn whose prompt is further back.
            let at = match &item { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms };
            self.turns.push(Turn { at_ms: at, prompt: String::new(), items: Vec::new(), more: 0, hooked_ms: 0, tokens: 0, context: 0, usage: Usage::default(), work: Work::default() });
        }
        let turn = self.turns.last_mut().unwrap();
        turn.items.push(item);
        if turn.items.len() > MAX_ITEMS {
            turn.items.remove(0);
            turn.more += 1;
        }
    }

    /// A tool call's result came in: fill in how it went.
    fn finish(&mut self, block: &Value, extra: Option<&Value>, at: u64) -> bool {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { return false };
        // A hand-off note written: kept once the write went through.
        if let Some(note) = self.note_calls.remove(id) {
            if block.get("is_error").and_then(Value::as_bool) != Some(true) {
                self.note = Some((note, at));
            }
        }
        for turn in self.turns.iter_mut().rev().take(3) {
            for item in turn.items.iter_mut().rev() {
                if let Item::Step { id: sid, tool, verb, past, result, bad, done, subject, command, desc, .. } = item {
                    if sid == id {
                        let (r, b) = outcome(tool, block, extra);
                        *verb = past.to_string();
                        *result = r;
                        *bad = b;
                        *done = true;
                        let launched = extra.filter(|x| x.get("status").and_then(Value::as_str).is_some_and(in_background)).cloned();
                        let resumed = resumed_agent(extra);
                        let backgrounded = extra.filter(|x| x.get("backgroundTaskId").and_then(Value::as_str).is_some_and(|t| !t.is_empty())).cloned();
                        let (step, subject, command, desc) = (sid.clone(), subject.clone(), command.clone(), desc.clone());
                        if let Some(x) = launched {
                            self.add_helper(&step, &x, at);
                        } else if let Some(agent) = resumed {
                            self.resume_helper(&step, &agent, at);
                        } else if let Some(x) = backgrounded {
                            *result = "in the background".into();
                            *bad = false;
                            self.add_script(&step, &desc, &subject, &command, &result_text(block), &x, at);
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Steps still open in the current turn when it ends (stopped, or the next prompt came): done.
    fn close_open(&mut self, why: &str) -> bool {
        let Some(turn) = self.turns.last_mut() else { return false };
        let mut changed = false;
        for item in turn.items.iter_mut() {
            if let Item::Step { verb, past, result, done, .. } = item {
                if !*done {
                    *verb = past.to_string();
                    *result = why.to_string();
                    *done = true;
                    changed = true;
                }
            }
        }
        changed
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Claude,
    Pi,
    Codex,
}

/// Whose log this is, by its first line: Pi's starts `{"type":"session","version":…}`, Codex's
/// `{"type":"session_meta",…}`; anything else is Claude Code's.
fn kind_of(path: &str) -> Kind {
    let Ok(f) = std::fs::File::open(path) else { return Kind::Claude };
    let mut first = String::new();
    let _ = std::io::BufRead::read_line(&mut std::io::BufReader::new(f.take(256 * 1024)), &mut first);
    let e: Value = serde_json::from_str(&first).unwrap_or(Value::Null);
    match e.get("type").and_then(Value::as_str) {
        Some("session") if e.get("version").is_some() => Kind::Pi,
        Some("session_meta") => Kind::Codex,
        _ => Kind::Claude,
    }
}

/// Whose log this is, as a harness: "claude", "codex" or "pi" (see `kind_of`).
pub fn harness_of(path: &str) -> &'static str {
    match kind_of(path) {
        Kind::Claude => "claude",
        Kind::Codex => "codex",
        Kind::Pi => "pi",
    }
}

/// Every file the session edited or wrote, first edited first, as its log names it: Claude Code's Edit,
/// MultiEdit, Write and NotebookEdit, Codex's apply_patch (every file in a patch; paths relative to the
/// session's folder), Pi's edit and write. The whole log is read.
/// A Claude Code session's background agents edit under their own logs (`<session folder>/subagents/`):
/// those count as the session's edits too, after its own, in the order the agents started.
#[cfg_attr(not(test), allow(dead_code))]
pub fn edited_files(path: &str) -> Vec<String> {
    edited_files_by(path).into_iter().map(|(file, _)| file).collect()
}

/// `edited_files`, each with who edited it: "" for the session itself, else the background agent's
/// description (what the session called it; its type, or its id, when its meta file doesn't say).
/// A file both edited keeps whoever edited it first.
pub fn edited_files_by(path: &str) -> Vec<(String, String)> {
    let kind = kind_of(path);
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let mut patches = HashMap::new();
    let mut out: Vec<(String, String)> = vec![];
    fn add(out: &mut Vec<(String, String)>, p: &str, by: &str) {
        let p = p.trim();
        if !p.is_empty() && !out.iter().any(|(x, _)| x == p) {
            out.push((p.to_string(), by.to_string()));
        }
    }
    let mut scan = |text: &str, by: &str, out: &mut Vec<(String, String)>| {
        for line in text.lines() {
            let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
            for t in translate(kind, &e, &mut patches) {
                for b in t.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or("");
                    if b.get("type").and_then(Value::as_str) == Some("tool_use") && matches!(name, "Edit" | "MultiEdit" | "Write" | "NotebookEdit") {
                        let input = &b["input"];
                        add(out, input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str).unwrap_or(""), by);
                    }
                }
            }
        }
    };
    scan(&text, "", &mut out);
    if kind == Kind::Claude {
        if let Some(folder) = path.strip_suffix(".jsonl") {
            let mut logs: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(std::path::Path::new(folder).join("subagents")).into_iter().flatten().flatten()
                .map(|d| d.path())
                .filter(|p| p.extension().is_some_and(|x| x == "jsonl") && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("agent-")))
                .map(|p| (std::fs::metadata(&p).and_then(|m| m.created().or_else(|_| m.modified())).unwrap_or(std::time::UNIX_EPOCH), p))
                .collect();
            logs.sort();
            for (_, p) in logs {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    scan(&t, &helper_name(&p), &mut out);
                }
            }
        }
    }
    let mut raws: Vec<&String> = patches.values().collect();
    raws.sort();
    for raw in raws {
        for l in raw.lines() {
            for m in ["*** Update File: ", "*** Add File: ", "*** Delete File: ", "*** Move to: "] {
                if let Some(p) = l.strip_prefix(m) {
                    add(&mut out, p, "");
                }
            }
        }
    }
    out
}

/// What to call a background agent, from its log's path: the description its session gave it (its meta
/// file), else its type, else its id (`agent-<id>.jsonl`).
fn helper_name(log: &std::path::Path) -> String {
    let meta = helper_meta(log);
    let from_meta = ["description", "agentType"].iter().find_map(|k| meta.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()));
    match from_meta {
        Some(s) => s.to_string(),
        None => log.file_stem().and_then(|s| s.to_str()).unwrap_or("").trim_start_matches("agent-").to_string(),
    }
}

/// One log entry as Claude Code transcript entries (none, one, or a few).
fn translate(kind: Kind, e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
    match kind {
        Kind::Claude => vec![e.clone()],
        Kind::Pi => from_pi(e),
        Kind::Codex => from_codex(e, patches),
    }
}

/// One Codex rollout entry in the shape of Claude Code's transcript: your message (its `user_message`
/// event; the instructions Codex puts in as "user" messages aren't yours), its words, commands
/// (`exec_command`, `shell`) and patches (`apply_patch`), and their results. `write_stdin` (checking on
/// a command still running) is left out.
fn from_codex(e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
    let ts = e.get("timestamp").cloned().unwrap_or(Value::Null);
    let p = e.get("payload").cloned().unwrap_or(Value::Null);
    let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
    match (e.get("type").and_then(Value::as_str), ptype) {
        (Some("event_msg"), "user_message") => {
            let text = p.get("message").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if text.is_empty() || text.starts_with('<') {
                return vec![];
            }
            vec![json!({ "type": "user", "timestamp": ts, "message": { "content": text } })]
        }
        (Some("response_item"), "message") if p.get("role").and_then(Value::as_str) == Some("assistant") => {
            let text: String = p.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
            if text.trim().is_empty() {
                return vec![];
            }
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": [{ "type": "text", "text": text }] } })]
        }
        (Some("response_item"), "function_call" | "custom_tool_call") => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or("");
            let id = p.get("call_id").cloned().unwrap_or(Value::Null);
            let args: Value = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
            let raw = p.get("input").and_then(Value::as_str).unwrap_or("");
            let (tool, input) = match name {
                "write_stdin" => return vec![],
                "exec_command" => ("Bash".to_string(), json!({ "command": args.get("cmd").cloned().unwrap_or(Value::Null) })),
                "shell" | "local_shell" | "container.exec" => {
                    let argv: Vec<&str> = args.get("command").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                    let cmd = if argv.len() >= 3 && argv[1] == "-lc" { argv[2].to_string() } else { argv.join(" ") };
                    ("Bash".to_string(), json!({ "command": cmd }))
                }
                "apply_patch" => {
                    if let Some(i) = id.as_str() {
                        patches.insert(i.to_string(), raw.to_string());
                    }
                    let file = raw.lines().find_map(|l| ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|m| l.strip_prefix(m))).unwrap_or("").trim().to_string();
                    ("Edit".to_string(), json!({ "file_path": file }))
                }
                // The Codex app's own tool: a script it runs (JavaScript). Often it only runs a shell
                // command (`tools.exec_command({"cmd": "…"})`): then it reads as that command.
                "exec" => match script_command(raw) {
                    Some(cmd) => ("Bash".to_string(), json!({ "command": cmd })),
                    None => ("CodexScript".to_string(), json!({ "script": raw })),
                },
                n => (n.to_string(), if args.is_null() { json!({ "input": raw }) } else { args }),
            };
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": [{ "type": "tool_use", "id": id, "name": tool, "input": input }] } })]
        }
        (Some("response_item"), "function_call_output" | "custom_tool_call_output") => {
            let id = p.get("call_id").and_then(Value::as_str).unwrap_or("").to_string();
            let text = match p.get("output") {
                Some(Value::String(t)) => t.clone(),
                Some(Value::Array(a)) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
                Some(Value::Object(o)) => o.get("output").and_then(Value::as_str).unwrap_or("").to_string(),
                _ => String::new(),
            };
            // "Process exited with code 1" / "Exit code: 1" up top, then "Output:" and what it printed.
            let code = ["exited with code ", "Exit code: "].iter().find_map(|m| text.split(m).nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>())).and_then(|c| c.parse::<i64>().ok());
            let body = text.split_once("Output:\n").map(|(_, b)| b.to_string()).unwrap_or_else(|| text.clone());
            let failed = code.is_some_and(|c| c != 0);
            let extra = match patches.get(&id) {
                Some(patch) => json!({ "structuredPatch": codex_patch(patch) }),
                None => json!({ "stdout": body }),
            };
            let shown = if failed { format!("Exit code {}\n{body}", code.unwrap_or(1)) } else { body };
            vec![json!({
                "type": "user", "timestamp": ts, "toolUseResult": extra,
                "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": shown, "is_error": failed }] },
            })]
        }
        _ => vec![],
    }
}

/// The shell command a Codex app script runs, when that's all it is: `exec_command({"cmd": "…"})`.
fn script_command(script: &str) -> Option<String> {
    if script.matches("exec_command(").count() != 1 {
        return None;
    }
    let after = &script[script.find("\"cmd\"")? + 5..];
    let quote = after.find('"')?;
    let mut strings = serde_json::Deserializer::from_str(&after[quote..]).into_iter::<String>();
    strings.next()?.ok().filter(|c| !c.trim().is_empty())
}

/// An apply_patch patch as Claude Code's structured patch: one hunk per "@@" part, its +/−/context lines.
fn codex_patch(patch: &str) -> Value {
    let mut hunks: Vec<Vec<String>> = vec![];
    for l in patch.lines() {
        if l.starts_with("*** ") {
            continue;
        }
        if l.starts_with("@@") {
            hunks.push(vec![]);
            continue;
        }
        if hunks.is_empty() {
            hunks.push(vec![]);
        }
        let line = if l.starts_with('+') || l.starts_with('-') || l.starts_with(' ') { l.to_string() } else { format!(" {l}") };
        hunks.last_mut().unwrap().push(line);
    }
    Value::Array(hunks.into_iter().filter(|h| !h.is_empty()).map(|h| json!({ "lines": h })).collect())
}

/// One Pi log entry in the shape of Claude Code's transcript (so the steps read both alike): your
/// message, its words and tool calls, a tool's result. Pi's tool names map to Claude Code's.
fn from_pi(e: &Value) -> Vec<Value> {
    if e.get("type").and_then(Value::as_str) != Some("message") {
        return vec![];
    }
    let ts = e.get("timestamp").cloned().unwrap_or(Value::Null);
    let m = e.get("message").cloned().unwrap_or(Value::Null);
    let blocks = m.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
    let text_of = |bs: &[Value]| bs.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
    match m.get("role").and_then(Value::as_str) {
        Some("user") => {
            let text = match m.get("content") {
                Some(Value::String(t)) => t.clone(),
                _ => text_of(&blocks),
            };
            // What Pi itself puts in as a "user" message (a loaded skill, a reminder) starts with a tag.
            if text.trim_start().starts_with('<') || text.trim().is_empty() {
                return vec![];
            }
            vec![json!({ "type": "user", "timestamp": ts, "message": { "content": text } })]
        }
        Some("assistant") => {
            let content: Vec<Value> = blocks
                .iter()
                .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                    Some("text") => Some(json!({ "type": "text", "text": b.get("text").cloned().unwrap_or(Value::Null) })),
                    Some("toolCall") => {
                        let args = b.get("arguments").cloned().unwrap_or(Value::Null);
                        let (name, input) = pi_tool(b.get("name").and_then(Value::as_str).unwrap_or(""), &args);
                        Some(json!({ "type": "tool_use", "id": b.get("id").cloned().unwrap_or(Value::Null), "name": name, "input": input }))
                    }
                    _ => None,
                })
                .collect();
            if content.is_empty() {
                return vec![];
            }
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": content } })]
        }
        Some("toolResult") => {
            let text = text_of(&blocks);
            let tool = m.get("toolName").and_then(Value::as_str).unwrap_or("");
            let details = m.get("details").cloned().unwrap_or(Value::Null);
            let extra = match tool {
                "bash" => json!({ "stdout": text }),
                "read" => json!({ "type": "text", "file": { "numLines": text.lines().count() } }),
                "edit" => json!({ "structuredPatch": pi_patch(details.get("diff").and_then(Value::as_str).unwrap_or("")) }),
                _ => Value::Null,
            };
            vec![json!({
                "type": "user", "timestamp": ts, "toolUseResult": extra,
                "message": { "content": [{ "type": "tool_result", "tool_use_id": m.get("toolCallId").cloned().unwrap_or(Value::Null), "content": text, "is_error": m.get("isError").and_then(Value::as_bool).unwrap_or(false) }] },
            })]
        }
        _ => vec![],
    }
}

/// A model's context window from Pi's own model list (`<Pi's folder>/models-store.json`), read once.
fn pi_window(provider: &str, model: &str) -> Option<u64> {
    static STORE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let store = STORE.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let dir = std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent"));
        std::fs::read_to_string(dir.join("models-store.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
    });
    let find = |models: &Value| models.as_array()?.iter().find(|x| x.get("id").and_then(Value::as_str) == Some(model))?.get("contextWindow")?.as_u64();
    store.get(provider).and_then(|p| find(p.get("models")?)).or_else(|| store.as_object()?.values().find_map(|p| find(p.get("models")?)))
}

/// A Pi tool as the Claude Code tool it matches (its arguments renamed to fit).
fn pi_tool(name: &str, args: &Value) -> (String, Value) {
    let a = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    match name {
        "bash" => ("Bash".into(), json!({ "command": a("command") })),
        "read" => ("Read".into(), json!({ "file_path": a("path") })),
        "edit" => ("Edit".into(), json!({ "file_path": a("path") })),
        "write" => ("Write".into(), json!({ "file_path": a("path") })),
        "grep" => ("Grep".into(), json!({ "pattern": a("pattern"), "path": a("path") })),
        "find" => ("Glob".into(), json!({ "pattern": a("pattern") })),
        "ask_user" => ("AskUserQuestion".into(), json!({ "questions": [{ "question": a("question") }] })),
        n => (n.to_string(), args.clone()),
    }
}

/// Pi's edit diff ("-60  old", "+61  new", " 56  same", "    ..." between parts) as Claude Code's
/// structured patch: hunks of lines starting with "-", "+" or " ", line numbers dropped.
fn pi_patch(diff: &str) -> Value {
    let mut hunks: Vec<Vec<String>> = vec![vec![]];
    for l in diff.lines() {
        if l.trim() == "..." {
            if !hunks.last().unwrap().is_empty() {
                hunks.push(vec![]);
            }
            continue;
        }
        let (op, rest) = l.split_at(l.chars().next().map(char::len_utf8).unwrap_or(0));
        let op = if op == "+" || op == "-" { op } else { " " };
        // The line number, then one space, then the line as it is (its own indentation kept).
        let body = rest.trim_start().trim_start_matches(|c: char| c.is_ascii_digit());
        hunks.last_mut().unwrap().push(format!("{op}{}", body.strip_prefix(' ').unwrap_or(body)));
    }
    Value::Array(hunks.into_iter().filter(|h| !h.is_empty()).map(|h| json!({ "lines": h })).collect())
}

/// A tool call as a step line: its kind, the verb while running and once done, and its subject.
fn step_of(b: &Value, at: u64) -> Item {
    let tool = b.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let input = b.get("input").cloned().unwrap_or(Value::Null);
    let (kind, verb, past, subject, mono) = describe(&tool, &input);
    Item::Step {
        id: b.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        kind,
        verb: verb.to_string(),
        subject,
        mono,
        result: String::new(),
        bad: false,
        done: false,
        at_ms: at,
        command: if tool == "Bash" { input.get("command").and_then(Value::as_str).unwrap_or("").to_string() } else { String::new() },
        desc: if tool == "Bash" { input.get("description").and_then(Value::as_str).unwrap_or("").to_string() } else { String::new() },
        tool,
        past,
    }
}

fn describe(tool: &str, input: &Value) -> (&'static str, &'static str, &'static str, String, bool) {
    let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let file = |k: &str| arg(k).rsplit('/').next().unwrap_or("").to_string();
    let first = |t: String| t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
    match tool {
        "Bash" => ("run", "Running", "Ran", cut(&without_cd(&first(arg("command"))), 80), true),
        "Read" => ("read", "Reading", "Read", file("file_path"), true),
        "Edit" | "MultiEdit" => ("edit", "Editing", "Edited", file("file_path"), true),
        "Write" => ("edit", "Writing", "Wrote", file("file_path"), true),
        "NotebookEdit" => ("edit", "Editing", "Edited", file("notebook_path"), true),
        "Grep" => ("search", "Searching", "Searched", format!("“{}”", cut(&arg("pattern"), 60)), false),
        "Glob" => ("search", "Finding files", "Found files", cut(&arg("pattern"), 60), true),
        "WebSearch" => ("web", "Searching the web", "Searched the web", cut(&arg("query"), 70), false),
        "WebFetch" => ("web", "Reading", "Read", cut(arg("url").trim_start_matches("https://"), 70), true),
        "Agent" | "Task" => ("agent", "Delegating", "Delegated", cut(&arg("description"), 70), false),
        // A message to one of its agents (one that finished is put back to work on it): what it's about.
        "SendMessage" => ("agent", "Messaging its agent", "Messaged its agent", cut(&first(if arg("summary").is_empty() { arg("message") } else { arg("summary") }), 70), false),
        "CodexScript" => ("run", "Running a script", "Ran a script", cut(&first(arg("script")), 70), true),
        "TodoWrite" => ("todo", "Updating its to-do list", "Updated its to-do list", String::new(), false),
        "AskUserQuestion" => {
            let q = input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("");
            ("ask", "Asking you", "Asked you", cut(q, 70), false)
        }
        "Skill" => ("other", "Using", "Used", arg("skill"), false),
        t if t.starts_with("mcp__") => ("other", "Using", "Used", t.trim_start_matches("mcp__").replacen("__", ": ", 1), false),
        t => ("other", "Using", "Used", t.to_string(), false),
    }
}

/// A command without its leading `cd <folder> &&` (or `;`): the folder isn't what it ran.
fn without_cd(cmd: &str) -> String {
    let mut rest = cmd.trim();
    while let Some(after) = rest.strip_prefix("cd ") {
        let Some(i) = after.find("&&").map(|i| (i, 2)).into_iter().chain(after.find(';').map(|i| (i, 1))).min_by_key(|(i, _)| *i) else { break };
        let next = after[i.0 + i.1..].trim_start();
        if next.is_empty() {
            break;
        }
        rest = next;
    }
    rest.to_string()
}

/// A tool result's text (a string, or text blocks).
fn result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// "+6 −2" from an edit's structured patch.
fn patch_counts(patch: &Value) -> (usize, usize) {
    let mut add = 0;
    let mut del = 0;
    for hunk in patch.as_array().into_iter().flatten() {
        for l in hunk.get("lines").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if l.starts_with('+') {
                add += 1;
            } else if l.starts_with('-') {
                del += 1;
            }
        }
    }
    (add, del)
}

/// How a step went, in a few words, and whether it failed.
fn outcome(tool: &str, block: &Value, extra: Option<&Value>) -> (String, bool) {
    let text = result_text(block);
    let x = |k: &str| extra.and_then(|v| v.get(k));
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        let low = text.to_lowercase();
        if low.contains("denied") || low.contains("doesn't want to proceed") || low.contains("rejected") {
            return ("declined".into(), true);
        }
        let code_after = |mark: &str| text.split(mark).nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>()).filter(|c| !c.is_empty());
        if let Some(code) = code_after("Exit code ").or_else(|| code_after("exited with code ")) {
            return (format!("exit {code}"), true);
        }
        return ("failed".into(), true);
    }
    let r = match tool {
        "Read" => match x("type").and_then(Value::as_str) {
            Some("image") => "image".into(),
            _ => match x("file").and_then(|f| f.get("numLines")).and_then(Value::as_u64) {
                Some(n) => format!("{n} lines"),
                None => format!("{} lines", text.lines().count()),
            },
        },
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            if x("type").and_then(Value::as_str) == Some("create") {
                format!("new · {} lines", x("content").and_then(Value::as_str).unwrap_or("").lines().count())
            } else {
                match x("structuredPatch").map(patch_counts) {
                    Some((0, 0)) | None => "saved".into(),
                    Some((a, d)) => format!("+{a} −{d}"),
                }
            }
        }
        "Bash" => {
            if x("interrupted").and_then(Value::as_bool) == Some(true) {
                "stopped".into()
            } else {
                let out = x("stdout").and_then(Value::as_str).map(String::from).unwrap_or(text);
                // Its last line when that reads as a result ("Tests: 41 passed", "BUILD SUCCEEDED"); a
                // long line (a match, a log line) says less than "done".
                let last = strip_ansi(out.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or(""));
                if last.is_empty() || last.chars().count() > 32 { "done".into() } else { last }
            }
        }
        "Grep" | "Glob" => {
            let first = text.lines().next().unwrap_or("").trim();
            if first.starts_with("No ") {
                "none".into()
            } else if let Some(rest) = first.strip_prefix("Found ") {
                cut(rest.split(':').next().unwrap_or(rest).trim(), 30)
            } else {
                format!("{} results", text.lines().filter(|l| !l.trim().is_empty()).count())
            }
        }
        "Agent" | "Task" if x("status").and_then(Value::as_str).is_some_and(in_background) => "in the background".into(),
        "SendMessage" if resumed_agent(extra).is_some() => "back at work".into(),
        _ => "done".into(),
    };
    (r, false)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lines for an opened step: long outputs keep their start and end, with how many lines were skipped.
fn clip_lines(text: &str) -> (Vec<String>, usize) {
    let all: Vec<String> = strip_ansi(text).lines().map(|l| cut(l, LINE_CHARS)).collect();
    if all.len() <= OUT_HEAD + OUT_TAIL {
        return (all, 0);
    }
    let skipped = all.len() - OUT_HEAD - OUT_TAIL;
    let mut out = all[..OUT_HEAD].to_vec();
    out.extend_from_slice(&all[all.len() - OUT_TAIL..]);
    (out, skipped)
}

/// Opened steps: for each id, its full subject (the whole command, path, pattern), and its output
/// lines or diff. Read from the transcript on demand, never kept.
pub fn detail(path: &str, ids: &[String]) -> Value {
    let kind = kind_of(path);
    let mut patches = HashMap::new();
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(START_BYTES);
    let Ok(mut file) = std::fs::File::open(path) else { return json!({}) };
    let mut buf = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut buf).is_err() {
        return json!({});
    }
    let text = String::from_utf8_lossy(&buf);
    let mut calls: HashMap<&str, (String, Value)> = HashMap::new();
    let mut results: HashMap<&str, (Value, Value)> = HashMap::new();
    for line in text.lines() {
        let Some(id) = ids.iter().find(|id| line.contains(id.as_str())) else { continue };
        let Ok(raw) = serde_json::from_str::<Value>(line) else { continue };
        for e in translate(kind, &raw, &mut patches) {
        for b in content_blocks(&e) {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") if b.get("id").and_then(Value::as_str) == Some(id) => {
                    calls.insert(id.as_str(), (b.get("name").and_then(Value::as_str).unwrap_or("").to_string(), b.get("input").cloned().unwrap_or(Value::Null)));
                }
                Some("tool_result") if b.get("tool_use_id").and_then(Value::as_str) == Some(id) => {
                    results.insert(id.as_str(), (b.clone(), e.get("toolUseResult").cloned().unwrap_or(Value::Null)));
                }
                _ => {}
            }
        }
        }
    }
    let mut out = serde_json::Map::new();
    for id in ids {
        let Some((tool, input)) = calls.get(id.as_str()) else { continue };
        let (block, extra) = results.get(id.as_str()).cloned().unwrap_or((Value::Null, Value::Null));
        out.insert(id.clone(), one_detail(tool, input, &block, &extra));
    }
    Value::Object(out)
}

fn one_detail(tool: &str, input: &Value, block: &Value, extra: &Value) -> Value {
    let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
    let full = match tool {
        "Bash" => cut(&arg("command"), 4000),
        "CodexScript" => cut(&arg("script"), 4000),
        "Read" | "Edit" | "MultiEdit" | "Write" => arg("file_path"),
        "NotebookEdit" => arg("notebook_path"),
        "Grep" => [arg("pattern"), arg("path")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  in  "),
        "Glob" => arg("pattern"),
        "WebFetch" => arg("url"),
        "WebSearch" => arg("query"),
        "Agent" | "Task" => cut(&arg("prompt"), 1200),
        _ => cut(&input.to_string(), 1200),
    };
    // An edit (not a failed one): its diff, hunk by hunk.
    let edit = matches!(tool, "Edit" | "MultiEdit" | "Write" | "NotebookEdit") && !failed;
    if edit {
        let mut diff: Vec<[String; 2]> = Vec::new();
        let mut cut_lines = 0;
        if extra.get("type").and_then(Value::as_str) == Some("create") {
            let content = extra.get("content").and_then(Value::as_str).unwrap_or("");
            cut_lines = content.lines().count().saturating_sub(OUT_TAIL);
            for l in content.lines().take(OUT_TAIL) {
                diff.push(["+".into(), cut(l, LINE_CHARS)]);
            }
        } else {
            for (n, hunk) in extra.get("structuredPatch").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if n > 0 {
                    diff.push(["…".into(), String::new()]);
                }
                for l in hunk.get("lines").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    let (op, rest) = l.split_at(l.chars().next().map(char::len_utf8).unwrap_or(0));
                    diff.push([if op == "+" || op == "-" { op.to_string() } else { " ".into() }, cut(rest, LINE_CHARS)]);
                }
            }
        }
        cut_lines += diff.len().saturating_sub(OUT_HEAD + OUT_TAIL);
        diff.truncate(OUT_HEAD + OUT_TAIL);
        // A long diff is cut at the end: `skipped` lines more.
        return json!({ "full": full, "diff": diff, "skipped": cut_lines });
    }
    // A read shows no file contents: the line already says how much it read.
    if tool == "Read" && !failed {
        return json!({ "full": full });
    }
    let output = if tool == "Bash" && !failed {
        let mut o = extra.get("stdout").and_then(Value::as_str).unwrap_or("").to_string();
        let err = extra.get("stderr").and_then(Value::as_str).unwrap_or("").trim();
        // The shell's own note about its folder isn't the command's output.
        let err: String = err.lines().filter(|l| !l.starts_with("Shell cwd was reset")).collect::<Vec<_>>().join("\n");
        if !err.trim().is_empty() {
            if !o.is_empty() {
                o.push('\n');
            }
            o.push_str(&err);
        }
        if o.trim().is_empty() { result_text(block) } else { o }
    } else {
        result_text(block)
    };
    let (out, skipped) = clip_lines(&output);
    // `gap_at`: where the skipped lines were (after the first OUT_HEAD).
    json!({ "full": full, "output": out, "skipped": skipped, "gap_at": OUT_HEAD })
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_edited_file_is_in_the_area_under_its_session_folder() {
        assert_eq!(area_of("/r/cue", "/r/cue/ui/app.js").as_deref(), Some("ui"));
        assert_eq!(area_of("/r/cue/", "/r/cue/README.md").as_deref(), Some("."));
        assert_eq!(area_of("/r/cue", "/r/other/src/a.rs").as_deref(), Some("/r/other/src"), "outside it: its own folder");
        assert_eq!(area_of("/r/cue", "/Users/x/.claude/projects/m/memory/a.md"), None);
        assert_eq!(area_of("/r/cue", "/private/tmp/scratch/a.py"), None);
        assert_eq!(area_of("/r/cue", "/r/cuex/a.rs").as_deref(), Some("/r/cuex"), "a sibling folder isn't inside it");
    }

    #[test]
    fn a_turns_work_counts_its_looks_agents_relay_and_edits() {
        let mut w = Work::default();
        for b in [
            json!({"name":"Read","input":{"file_path":"/r/a.rs"}}),
            json!({"name":"Grep","input":{"pattern":"x"}}),
            json!({"name":"Bash","input":{"command":"cd /r && grep -n x a.rs"}}),
            json!({"name":"Bash","input":{"command":"cargo test"}}),
            json!({"name":"Bash","input":{"command":"relay spawn --topic x"}}),
            json!({"name":"Skill","input":{"skill":"relay:spawn"}}),
            json!({"name":"Agent","input":{"description":"look"}}),
            json!({"name":"Edit","input":{"file_path":"/r/a.rs"}}),
            json!({"name":"Write","input":{"file_path":"/r/a.rs"}}),
            json!({"name":"NotebookEdit","input":{"notebook_path":"/r/n.ipynb"}}),
        ] {
            w.count(&b);
        }
        assert_eq!((w.looks, w.agents, w.relay), (3, 1, 2));
        assert_eq!(w.edited, vec!["/r/a.rs", "/r/n.ipynb"], "each file once");
    }

    use super::*;

    #[test]
    fn edited_files_are_read_from_every_agents_log() {
        let dir = std::env::temp_dir().join(format!("cue-edited-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let claude = dir.join("c.jsonl");
        std::fs::write(&claude, [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/r/x.rs"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Write","input":{"file_path":"/other/b.md"}},{"type":"tool_use","id":"t4","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#,
        ].join("\n")).unwrap();
        assert_eq!(edited_files(claude.to_str().unwrap()), vec!["/r/a.rs", "/other/b.md"]);
        // A background agent's edits, from its own log under the session's folder, come after the session's own.
        let sub = dir.join("c").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-a1.jsonl"), r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"h1","name":"Edit","input":{"file_path":"/r/c.rs"}},{"type":"tool_use","id":"h2","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#).unwrap();
        assert_eq!(edited_files(claude.to_str().unwrap()), vec!["/r/a.rs", "/other/b.md", "/r/c.rs"]);
        // Who edited what: the session itself (""), or the agent by the description its session gave it; a
        // file both edited keeps the first. Without a meta file, the agent's id names it.
        let by = |p: &std::path::Path| edited_files_by(p.to_str().unwrap()).into_iter().map(|(f, b)| format!("{f} by {b}")).collect::<Vec<_>>();
        assert_eq!(by(&claude), vec!["/r/a.rs by ", "/other/b.md by ", "/r/c.rs by a1"]);
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","description":"Backend: names"}"#).unwrap();
        assert_eq!(by(&claude)[2], "/r/c.rs by Backend: names");
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore"}"#).unwrap();
        assert_eq!(by(&claude)[2], "/r/c.rs by Explore");
        let pi = dir.join("p.jsonl");
        std::fs::write(&pi, [
            r#"{"type":"session","version":3,"id":"s","cwd":"/a"}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"edit","arguments":{"path":"/a/src/f.ts"}}]}}"#,
        ].join("\n")).unwrap();
        assert_eq!(edited_files(pi.to_str().unwrap()), vec!["/a/src/f.ts"]);
        let codex = dir.join("x.jsonl");
        std::fs::write(&codex, [
            r#"{"timestamp":"2026-10-05T06:36:49.000Z","type":"session_meta","payload":{"id":"x","cwd":"/w"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:50.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","call_id":"p1","input":"*** Begin Patch\n*** Update File: src/one.rs\n@@\n-a\n+b\n*** Add File: src/two.rs\n+x\n*** End Patch"}}"#,
        ].join("\n")).unwrap();
        assert_eq!(edited_files(codex.to_str().unwrap()), vec!["src/one.rs", "src/two.rs"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_1m_session_continues_on_1m() {
        assert_eq!(model_for_window("claude-opus-5-5", 1_000_000), "claude-opus-5-5[1m]");
        assert_eq!(model_for_window("claude-opus-5-5[1m]", 1_000_000), "claude-opus-5-5[1m]");
        assert_eq!(model_for_window("claude-opus-5-5", 200_000), "claude-opus-5-5");
        assert_eq!(model_for_window("claude-opus-5-5", 0), "claude-opus-5-5");
        assert_eq!(model_for_window("", 1_000_000), "");
    }

    /// A teammate's message to its lead wakes it, but it's the teammate talking: the turn it starts has no prompt
    /// of yours, and Cue's Stop hook keeps the lead's reply before it in that turn's parts.
    #[test]
    fn a_teammates_message_is_never_your_prompt() {
        let log = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"look at the hook"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Asked reader to check it."}]}}
{"type":"user","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"text","text":"Another Claude session sent a message:\n<teammate-message teammate_id=\"reader\">It's fine.</teammate-message>"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"Reader says it's fine."}]}}
"#;
        let f = feed_of(log);
        assert_eq!(f.turns[0].prompt, "look at the hook");
        assert!(f.turns.iter().skip(1).all(|t| t.prompt.is_empty()), "no prompt of yours from the teammate");
        let path = std::env::temp_dir().join(format!("cue-peer-{}.jsonl", std::process::id()));
        std::fs::write(&path, log).unwrap();
        let parts = crate::hook::final_turn(&serde_json::json!({ "transcript_path": path.to_string_lossy(), "last_assistant_message": "Reader says it's fine." }));
        let _ = std::fs::remove_file(&path);
        let texts: Vec<&str> = parts.iter().filter_map(|v| v["text"].as_str()).collect();
        assert_eq!(texts, ["Asked reader to check it.", "Reader says it's fine."], "the lead's reply before the teammate's message stays in the turn");
    }

    fn feed_of(jsonl: &str) -> Feed {
        let mut f = Feed { path: String::new(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false) };
        for e in lines(jsonl) {
            f.take(&e);
        }
        f
    }
    fn step(item: &Item) -> (&str, &str, &str, bool, bool) {
        match item {
            Item::Step { verb, subject, result, bad, done, .. } => (verb, subject, result, *bad, *done),
            Item::Say { .. } => panic!("not a step"),
        }
    }

    #[test]
    fn a_hand_off_note_is_known_by_its_name() {
        for yes in ["/r/HANDOFF.md", "handoff.md", "/r/docs/mac-security-handoff.md", "/r/handoff-2026-10-18.md", "/r/Hand-Off.MD", "/r/next_hand_off.md"] {
            assert!(is_handoff_note(yes), "{yes}");
        }
        for no in ["/r/src/handoff.rs", "/r/handoff/notes.md", "/r/handoff/", "/r/HANDOFF.txt", "/r/README.md", ""] {
            assert!(!is_handoff_note(no), "{no}");
        }
    }

    #[test]
    fn a_hand_off_note_counts_when_written_whole_and_it_went_through() {
        let call = |id: &str, tool: &str, file: &str| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"{tool}","input":{{"file_path":"{file}"}}}}]}}}}"#);
        let result = |id: &str, at: &str, error: bool| format!(r#"{{"type":"user","timestamp":"{at}","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","is_error":{error},"content":"ok"}}]}}}}"#);
        let prompt = |text: &str| format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{{"content":"{text}"}}}}"#);
        let note = |log: Vec<String>| feed_of(&log.join("\n")).note;
        // Written, and it went through: the path as written, and when its result came.
        let at = crate::logs::transcript::iso_ms("2026-10-04T10:00:06.000Z").unwrap();
        assert_eq!(note(vec![prompt("hand off"), call("w1", "Write", "/r/HANDOFF.md"), result("w1", "2026-10-04T10:00:06.000Z", false)]), Some(("/r/HANDOFF.md".to_string(), at)));
        // Edited rather than written whole, or another file: not a hand-off.
        assert_eq!(note(vec![call("e1", "Edit", "/r/HANDOFF.md"), result("e1", "2026-10-04T10:00:06.000Z", false)]), None);
        assert_eq!(note(vec![call("e2", "MultiEdit", "/r/HANDOFF.md"), result("e2", "2026-10-04T10:00:06.000Z", false)]), None);
        assert_eq!(note(vec![call("w2", "Write", "/r/src/handoff.rs"), result("w2", "2026-10-04T10:00:06.000Z", false)]), None);
        // The write failed, or its result isn't in yet.
        assert_eq!(note(vec![call("w3", "Write", "/r/HANDOFF.md"), result("w3", "2026-10-04T10:00:06.000Z", true)]), None);
        assert_eq!(note(vec![call("w4", "Write", "/r/HANDOFF.md")]), None);
        // The newest one counts.
        let two = note(vec![call("w5", "Write", "/r/HANDOFF.md"), result("w5", "2026-10-04T10:00:06.000Z", false), call("w6", "Write", "/r/b-handoff.md"), result("w6", "2026-10-04T10:00:09.000Z", false)]);
        assert_eq!(two.map(|(p, _)| p).as_deref(), Some("/r/b-handoff.md"));
        // You gave it more to do after it: that note isn't where it stopped.
        assert_eq!(note(vec![call("w7", "Write", "/r/HANDOFF.md"), result("w7", "2026-10-04T10:00:06.000Z", false), prompt("now fix the test")]), None);
        // Cue's hand-off ask, pasted in and come back wrapped: its older note edited into the new one counts.
        let ask = format!(r#"<pasted_content id=\"a1\">\n{HANDOFF_ASK_START} so a fresh session can carry on.\n</pasted_content id=\"a1\">"#);
        let f = feed_of(&[prompt(&ask), call("e3", "Edit", "/r/HANDOFF-ab12cd34.md"), result("e3", "2026-10-04T10:00:06.000Z", false)].join("\n"));
        assert_eq!(f.note.map(|(p, _)| p).as_deref(), Some("/r/HANDOFF-ab12cd34.md"));
        assert!(f.prompted.1, "the wrapped ask is still known as Cue's");
    }

    const T: &str = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Looking at it."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"..."}]},"toolUseResult":{"type":"text","file":{"filePath":"/a/src/token.ts","numLines":84}}}
{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"structuredPatch":[{"lines":[" a","-b","+c","+d"]}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"npm test\n"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:20.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t3","is_error":true,"content":"Exit code 1\nTests: 3 failed"}]},"toolUseResult":"Error: Exit code 1"}
{"type":"assistant","timestamp":"2026-10-04T10:00:22.000Z","isSidechain":true,"message":{"content":[{"type":"text","text":"a helper's words"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:25.000Z","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"npm test"}}]}}"#;

    #[test]
    fn each_turn_counts_its_replies_tokens_once_and_the_cache_lifetime_is_kept() {
        // Claude Code writes a reply as one line per content block, each repeating the reply's usage.
        let path = std::env::temp_dir().join(format!("cue-turn-tokens-{}.jsonl", std::process::id()));
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"On it."}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":200,"ephemeral_5m_input_tokens":0},"output_tokens":50}}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/r/a.rs"}}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":200,"ephemeral_5m_input_tokens":0},"output_tokens":50}}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"and the total"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:01:09.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":5,"cache_read_input_tokens":3000,"cache_creation_input_tokens":0,"output_tokens":20}}}"#,
        ].join("\n") + "\n").unwrap();
        let mut f = Feed::open(path.to_str().unwrap(), 1);
        f.catch_up();
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.turns.iter().map(|t| t.tokens).collect::<Vec<_>>(), vec![1260, 3025]);
        assert_eq!(f.turns.iter().map(|t| t.context).collect::<Vec<_>>(), vec![1210, 3005], "each turn keeps the context its last reply carried");
        assert_eq!(f.meta.cache_ttl_s, 3600, "a reply that only read the cache keeps the lifetime it was written with");
        assert_eq!(f.meta.start_context, 1210, "the session's start is its first reply's context");
        assert_eq!(f.meta.last_call_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:09.000Z").unwrap());
    }

    #[test]
    fn a_compact_leaves_its_context_at_once_not_at_the_next_reply() {
        let path = std::env::temp_dir().join(format!("cue-compact-context-{}.jsonl", std::process::id()));
        let reply = |at: &str, id: &str, read: u64| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:{at}.000Z","message":{{"id":"{id}","model":"claude-opus-5-5","content":[{{"type":"text","text":"Done."}}],"usage":{{"input_tokens":5,"cache_read_input_tokens":{read},"cache_creation_input_tokens":0,"output_tokens":20}}}}}}"#);
        let mut log = [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#.to_string(),
            reply("00:05", "m1", 600_000),
            // A background agent compacting its own context isn't this session's.
            r#"{"type":"system","subtype":"compact_boundary","isSidechain":true,"timestamp":"2026-10-04T10:00:30.000Z","compactMetadata":{"trigger":"auto","preTokens":90000,"postTokens":4000}}"#.to_string(),
        ]
        .join("\n") + "\n";
        std::fs::write(&path, &log).unwrap();
        let mut f = Feed::open(path.to_str().unwrap(), 1);
        f.catch_up();
        assert_eq!(f.meta.context, 600_005);
        log += r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:01:00.000Z","compactMetadata":{"trigger":"manual","preTokens":600005,"postTokens":15959}}"#;
        log += "\n";
        std::fs::write(&path, &log).unwrap();
        f.catch_up();
        assert_eq!(f.meta.context, 15_959, "what the compact left, before any reply");
        log += &(reply("02:00", "m2", 18_000) + "\n");
        std::fs::write(&path, &log).unwrap();
        f.catch_up();
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.meta.context, 18_005, "then its replies again");
    }

    #[test]
    fn a_snippet_leaves_out_appended_instructions() {
        assert_eq!(snippet("No, the card did show up (Voice: listening, not reading. Answer in one or two short sentences.)"), "No, the card did show up");
        assert_eq!(snippet("Fix the fill (partial) price"), "Fix the fill (partial) price");
        assert_eq!(snippet("(only a long bracketed note, nothing before it at all here)"), "(only a long bracketed note, nothing before it at all here)");
    }

    #[test]
    fn a_turns_row_splits_its_tokens_and_knows_how_long_it_waited() {
        let path = std::env::temp_dir().join(format!("cue-turn-rows-{}.jsonl", std::process::id()));
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"On it."}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":150,"ephemeral_5m_input_tokens":50},"output_tokens":50}}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/r/a.rs"}}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":150,"ephemeral_5m_input_tokens":50},"output_tokens":50}}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T12:00:05.000Z","message":{"content":"no, the total column"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T12:00:09.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":3000,"output_tokens":20}}}"#,
        ].join("\n") + "\n").unwrap();
        let (rows, _) = turn_rows(path.to_str().unwrap(), 0, 1 << 20);
        let _ = std::fs::remove_file(&path);
        let u = rows[0].usage;
        assert_eq!((u.fresh, u.read, u.write_5m, u.write_1h, u.output), (10, 1000, 50, 150, 50), "each reply once, its writes by lifetime");
        assert_eq!(rows[0].waited_ms, 0, "nothing before the first turn");
        assert_eq!(rows[1].waited_ms, 2 * 3600 * 1000, "from its reply before to this prompt");
        assert_eq!((rows[1].usage.first_write, rows[1].usage.first_read), (3000, 0), "after two hours it wrote its context again");
        assert!(rows[0].corrected);
        assert_eq!((rows[0].prompt.as_str(), rows[0].correction.as_str()), ("add the tax column", "no, the total column"));
        assert_eq!((rows[0].looks, rows[1].looks), (1, 0), "its read counted on its own turn");
    }

    #[test]
    fn a_turns_row_keeps_where_claude_code_compacted_it_on_its_own() {
        let path = std::env::temp_dir().join(format!("cue-turn-auto-{}.jsonl", std::process::id()));
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"go on"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"cache_read_input_tokens":250000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
            r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:01:00.000Z","compactMetadata":{"trigger":"auto","preTokens":252123}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"and then"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:02:05.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"cache_read_input_tokens":9000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
            r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:03:00.000Z","compactMetadata":{"trigger":"manual","preTokens":9000}}"#,
        ].join("\n") + "\n").unwrap();
        let (rows, _) = turn_rows(path.to_str().unwrap(), 0, 1 << 20);
        let _ = std::fs::remove_file(&path);
        assert_eq!(rows.iter().map(|r| r.auto_compact).collect::<Vec<_>>(), vec![252123, 0], "on its own: kept with its turn; your /compact: not");
    }

    #[test]
    fn a_feed_opened_mid_file_still_knows_the_context_its_session_started_with() {
        // A long session: only its last START_BYTES are read for the chat, so its first reply is read
        // from the top of the log on its own.
        let path = std::env::temp_dir().join(format!("cue-start-ctx-{}.jsonl", std::process::id()));
        let reply = |id: &str, ctx: u64| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{{"id":"{id}","model":"claude-opus-5-5","content":[{{"type":"text","text":"ok"}}],"usage":{{"input_tokens":0,"cache_read_input_tokens":{ctx},"cache_creation_input_tokens":0,"output_tokens":5}}}}}}"#);
        let mut log = String::new();
        log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"start"}}"#);
        log.push('\n');
        log.push_str(&reply("m0", 20000));
        log.push('\n');
        let filler = format!(r#"{{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{{"content":"{}"}}}}"#, "x".repeat(4000));
        while (log.len() as u64) < START_BYTES + 200_000 {
            log.push_str(&filler);
            log.push('\n');
        }
        log.push_str(&reply("m9", 300000));
        log.push('\n');
        std::fs::write(&path, log).unwrap();
        let mut f = Feed::open(path.to_str().unwrap(), 1);
        f.catch_up();
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.meta.context, 300000);
        assert_eq!(f.meta.start_context, 20000, "read from the top of the log, not the first turn the chat sees");
        assert_eq!(f.meta.start_ms, crate::logs::transcript::iso_ms("2026-10-04T10:00:05.000Z").unwrap());
    }

    #[test]
    fn steps_read_as_lines_with_how_they_went() {
        let f = feed_of(T);
        assert_eq!(f.turns.len(), 1);
        let items = &f.turns[0].items;
        assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking at it."));
        assert_eq!(step(&items[1]), ("Read", "token.ts", "84 lines", false, true));
        assert_eq!(step(&items[2]), ("Edited", "token.ts", "+2 −1", false, true));
        assert_eq!(step(&items[3]), ("Ran", "npm test", "exit 1", true, true));
        // Still running: the present-tense verb, no result. The helper's words aren't here.
        assert_eq!(step(&items[4]), ("Running", "npm test", "", false, false));
        assert_eq!(items.len(), 5);
    }

    /// A turn a Stop hook sent back to work, as Claude Code writes it: the hook's entry is a meta one
    /// (`isMeta`), its content a plain string. The first line stands in for the partial line a tail read drops.
    const HOOKED: &str = r#"{"type":"summary","summary":"(the line a tail read skips)"}
{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"role":"user","content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Done: expiry fixed."}]}}
{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-10-04T10:00:02.500Z","hookCount":1}
{"type":"user","isMeta":true,"timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"Stop hook feedback:\n[bash rules-check-stop.sh]: Unaudited edits this turn. Read rules-check.md"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat rules.md"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"1. SHAPE: fine."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}
"#;

    #[test]
    fn a_stop_hooks_follow_up_stays_in_the_turn_it_continues_marked_from_where_the_hook_sent_it_back() {
        let mut log = HOOKED.to_string();
        log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:30.000Z","message":{"content":"and the refresh token?"}}"#);
        let f = feed_of(&log);
        assert_eq!(f.turns.iter().map(|t| t.prompt.as_str()).collect::<Vec<_>>(), ["fix the token expiry", "and the refresh token?"], "the hook's entry opens no turn");
        let t = &f.turns[0];
        assert_eq!(t.items.len(), 3, "answer, the hook's step, its checklist: all one turn");
        assert_eq!(t.hooked_ms, stamp(&serde_json::json!({"timestamp": "2026-10-04T10:00:03.000Z"})), "marked where the hook sent it back");
        let at = |i: &Item| match i { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms };
        let after: Vec<bool> = t.items.iter().map(|i| at(i) >= t.hooked_ms).collect();
        assert_eq!(after, [false, true, true], "the answer before it; the hook's step and checklist after");
        assert_eq!(step(&t.items[1]), ("Ran", "cat rules.md", "ok", false, true));
        assert_eq!(f.turns[1].hooked_ms, 0, "the next turn starts unhooked");
        // Sent to the window as hooked_ms, left out when there's none.
        assert!(serde_json::to_value(t).unwrap().get("hooked_ms").is_some() && serde_json::to_value(&f.turns[1]).unwrap().get("hooked_ms").is_none());
    }

    #[test]
    fn a_hook_read_before_its_turns_start_still_marks_what_follows() {
        // Opened mid-file (the last 4 MB): the hook's entry is the first thing read.
        let log = HOOKED.lines().skip(4).collect::<Vec<_>>().join("\n");
        let f = feed_of(&log);
        assert_eq!(f.turns.len(), 1);
        assert!(f.turns[0].hooked_ms > 0, "the turn it opens is hooked");
        assert!(f.turns[0].items.iter().all(|i| match i { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms >= f.turns[0].hooked_ms }), "all it reads after is the hook's");
    }

    #[test]
    fn a_background_task_reporting_after_the_hook_starts_unhooked_work() {
        let mut log = HOOKED.replace(r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}"#, "");
        log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:20.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n</task-notification>"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:22.000Z","message":{"content":[{"type":"text","text":"The agent reports: all three paths agree."}]}}
"#);
        let f = feed_of(&log);
        assert_eq!(f.turns.len(), 2, "the report starts new work");
        assert!(f.turns[0].hooked_ms > 0 && f.turns[1].hooked_ms == 0, "the hook's part stays with the turn it came in; the new work is an answer again");
    }

    /// You wrote to it while the hook's follow-up ran: what it does for your message is an answer again, in
    /// every reader (it was folded under "After a stop hook", and the card kept the reply from before the hook).
    #[test]
    fn your_message_after_the_hook_starts_unhooked_work_in_every_reader() {
        let mut log = HOOKED.replace(r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}"#, "");
        log.push_str(r#"{"type":"attachment","timestamp":"2026-10-04T10:00:06.500Z","attachment":{"type":"queued_command","prompt":"also check the hand-off","commandMode":"prompt","origin":{"kind":"human"},"humanTurn":true}}
{"type":"assistant","timestamp":"2026-10-04T10:00:09.000Z","message":{"content":[{"type":"text","text":"The hand-off note is found now."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:10.000Z","durationMs":10000}
"#);
        let f = feed_of(&log);
        assert_eq!(f.turns.len(), 2, "your message starts new work");
        assert!(f.turns[0].hooked_ms > 0 && f.turns[1].hooked_ms == 0);
        assert!(matches!(f.turns[1].items.last(), Some(Item::Say { text, .. }) if text == "The hand-off note is found now."));
        let path = std::env::temp_dir().join(format!("cue-hooked-yours-{}.jsonl", std::process::id()));
        std::fs::write(&path, &log).unwrap();
        let p = path.to_string_lossy().to_string();
        let (_, last, parts) = crate::logs::transcript::turn_ended_parts(&p).expect("the turn ended");
        let card = crate::hub::split_turn(last, &parts, "answer");
        let hook = crate::hook::final_turn(&serde_json::json!({ "transcript_path": p, "last_assistant_message": "The hand-off note is found now." }));
        let parts: Vec<crate::model::TurnPart> = hook.iter().map(|v| crate::model::TurnPart { text: v["text"].as_str().unwrap_or("").to_string(), hooked: v["hooked"].as_bool().unwrap_or(false) }).collect();
        let stop = crate::hub::split_turn("The hand-off note is found now.".into(), &parts, "answer");
        let _ = std::fs::remove_file(&path);
        assert_eq!(card.0, "The hand-off note is found now.", "a turn read from the transcript");
        assert_eq!(stop.0, "The hand-off note is found now.", "Cue's Stop hook");
        // A task's notice queued the same way isn't yours.
        assert!(!crate::logs::transcript::yours_mid_turn(&serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","prompt":"<task-notification>x</task-notification>","commandMode":"task-notification"}})));
    }

    /// Every reader of a hooked turn agrees on what's the answer and what's the hook's follow-up: the steps
    /// feed (the chat), the turn's parts read from the transcript (a card Cue learns of after a restart),
    /// and Cue's Stop hook (a card as the turn ends). One transcript, three readers, one split.
    #[test]
    fn every_reader_of_a_hooked_turn_splits_it_the_same_way() {
        let path = std::env::temp_dir().join(format!("cue-hooked-{}.jsonl", std::process::id()));
        std::fs::write(&path, HOOKED).unwrap();
        let p = path.to_string_lossy().to_string();
        // The steps feed: the last words before the mark are the answer, the words after it the follow-up.
        let f = feed_of(HOOKED);
        let t = &f.turns[0];
        let says = |hooked: bool| t.items.iter().filter_map(|i| match i { Item::Say { text, at_ms } if (*at_ms >= t.hooked_ms) == hooked => Some(text.clone()), _ => None }).collect::<Vec<_>>();
        let feed = (says(false).last().cloned().unwrap_or_default(), says(true).join("\n\n"));
        // The transcript's parts, through the card's split.
        let (_, last, parts) = crate::logs::transcript::turn_ended_parts(&p).expect("the turn ended");
        let card = crate::hub::split_turn(last, &parts, "answer");
        // Cue's Stop hook, as it ends the turn.
        let hook = crate::hook::final_turn(&serde_json::json!({ "transcript_path": p, "last_assistant_message": "1. SHAPE: fine." }));
        let parts: Vec<crate::model::TurnPart> = hook.iter().map(|v| crate::model::TurnPart { text: v["text"].as_str().unwrap_or("").to_string(), hooked: v["hooked"].as_bool().unwrap_or(false) }).collect();
        let stop = crate::hub::split_turn("1. SHAPE: fine.".into(), &parts, "answer");
        let _ = std::fs::remove_file(&path);
        let want = ("Done: expiry fixed.".to_string(), "1. SHAPE: fine.".to_string());
        assert_eq!(feed, want, "the steps feed");
        assert_eq!(card, want, "a turn read from the transcript");
        assert_eq!(stop, want, "Cue's Stop hook");
    }

    #[test]
    fn a_background_agent_shows_at_work_under_its_step_until_its_session_hears_it_finished() {
        let dir = std::env::temp_dir().join(format!("cue-steps-helper-{}", std::process::id()));
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","description":"Backend: names","toolUseId":"t1","requestShape":"background"}"#).unwrap();
        std::fs::write(sub.join("agent-a1.jsonl"), [
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"find every name"}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":100,"output_tokens":20},"content":[{"type":"tool_use","id":"h1","name":"Grep","input":{"pattern":"ai-title"}}]}}"#,
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"Found 3 files"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"model":"claude-opus-5-5","usage":{"input_tokens":5,"cache_read_input_tokens":1100,"output_tokens":30},"content":[{"type":"tool_use","id":"h2","name":"Read","input":{"file_path":"/a/hub.rs"}}]}}"#,
        ].join("\n") + "\n").unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"map the names"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Backend: names","subagent_type":"Explore","prompt":"find every name"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Backend: names","resolvedModel":"claude-opus-5-5"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"text","text":"An agent is on it."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.len(), 1);
        let h = &f.helpers[0];
        assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.kind.as_str(), h.model.as_str()), ("a1", "t1", "Backend: names", "Explore", "claude-opus-5-5"));
        assert!(!h.teammate && h.name.is_empty(), "a background agent isn't a teammate");
        assert!(h.running());
        assert_eq!(h.items.len(), 2, "its own steps, from its own log");
        assert_eq!(step(&h.items[0]), ("Searched", "“ai-title”", "3 files", false, true));
        assert_eq!(step(&h.items[1]).0, "Reading");
        assert_eq!(step(&f.turns[0].items[0]), ("Delegated", "Backend: names", "in the background", false, true));
        assert_eq!(h.tokens, 10 + 1000 + 100 + 20 + 5 + 1100 + 30, "its tokens: every reply's input, cache reads and writes, and output");
        let last = crate::logs::transcript::iso_ms("2026-10-04T10:00:06.000Z").unwrap();
        assert_eq!(h.last_ms, last, "last did something: its newest step");
        assert!(!h.stuck(last + STUCK_MS) && h.stuck(last + STUCK_MS + 1), "stuck once nothing happened for STUCK_MS");
        assert_eq!(running_helpers(p), vec!["Backend: names".to_string()]);
        assert_eq!(running_background(p), 1);
        let shown = &steps(p, 0)["helpers"][0];
        assert_eq!((shown["tokens"].as_u64(), shown["last_ms"].as_u64(), shown["running"].as_bool(), shown["stuck"].as_bool()), (Some(2265), Some(last), Some(true), Some(true)), "sent as such (and that step is years old by now): {shown}");
        // It writes its report: it's done, even before its session hears so.
        let v = f.version;
        let mut log = std::fs::OpenOptions::new().append(true).open(sub.join("agent-a1.jsonl")).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h2","content":"ok"}]}}"#, "\n",
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:08.000Z","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Names: project, name."}]}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert!(f.version > v, "its new steps count as a change");
        assert!(!f.helpers[0].running());
        assert_eq!(f.helpers[0].finished_ms, 0);
        // Its session is told: finished for good.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Backend: names\" finished</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:00.000Z").unwrap());
        assert!(running_helpers(p).is_empty());
        assert_eq!(running_background(p), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_teammate_shows_at_work_until_it_answers_and_again_when_its_session_messages_it() {
        // As Claude Code writes them: an Agent call given a name starts a teammate.
        let dir = std::env::temp_dir().join(format!("cue-steps-teammate-{}", std::process::id()));
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-aprobe-30f8.meta.json"), r#"{"agentType":"probe","description":"Probe teammate reply","name":"probe","requestShape":"background","taskKind":"in_process_teammate","teamName":"session-804699a9"}"#).unwrap();
        let agent_log = sub.join("agent-aprobe-30f8.jsonl");
        std::fs::write(&agent_log, concat!(r#"{"type":"user","isSidechain":true,"timestamp":"2026-10-20T20:34:45.000Z","message":{"role":"user","content":"<teammate-message teammate_id=\"team-lead\" summary=\"Probe teammate reply\">\nReply with exactly the text: probe ok\n</teammate-message>"}}"#, "\n")).unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-20T20:34:42.000Z","message":{"content":"start a teammate"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-20T20:34:44.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Probe teammate reply","name":"probe","prompt":"Reply with exactly the text: probe ok"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-20T20:34:45.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Spawned successfully.\nagent_id: aprobe-30f8\nname: probe\nThe agent is now running and will receive instructions via mailbox."}]}]},"toolUseResult":{"status":"teammate_spawned","agentId":"aprobe-30f8","resolvedModel":"claude-haiku-5-5","teammate_id":"probe@session-804699a9","name":"probe","team_name":"session-804699a9"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-20T20:34:46.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"It's running."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.len(), 1, "a teammate is one of its background agents");
        assert_eq!((f.helpers[0].id.as_str(), f.helpers[0].desc.as_str(), f.helpers[0].kind.as_str()), ("aprobe-30f8", "Probe teammate reply", "probe"));
        assert!(f.helpers[0].teammate && f.helpers[0].name == "probe", "known as a teammate, by its name");
        assert_eq!(steps(p, 0)["helpers"][0]["teammate"], true, "and sent to the window as one");
        assert_eq!(step(&f.turns[0].items[0]).2, "in the background");
        assert_eq!(running_helpers(p), vec!["Probe teammate reply".to_string()]);
        // It answers: done (its idle notice to the session says the same, and changes nothing).
        let append = |file: &std::path::Path, line: &str| std::io::Write::write_all(&mut std::fs::OpenOptions::new().append(true).open(file).unwrap(), format!("{line}\n").as_bytes()).unwrap();
        append(&agent_log, r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-20T20:34:48.000Z","message":{"model":"claude-haiku-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"probe ok"}]}}"#);
        append(&path, r#"{"type":"user","timestamp":"2026-10-20T20:34:49.000Z","message":{"role":"user","content":"Another Claude session sent a message:\n<teammate-message teammate_id=\"probe\" color=\"blue\">\n{\"type\":\"idle_notification\",\"from\":\"probe\",\"idleReason\":\"available\",\"result\":\"probe ok\"}\n</teammate-message>"}}"#);
        f.catch_up();
        assert!(!f.helpers[0].running());
        assert!(running_helpers(p).is_empty());
        // Its session messages it: at work again until it answers again.
        append(&agent_log, r#"{"type":"user","isSidechain":true,"timestamp":"2026-10-20T20:35:10.000Z","message":{"role":"user","content":"<teammate-message teammate_id=\"team-lead\">\nnow say it twice\n</teammate-message>"}}"#);
        f.catch_up();
        assert!(f.helpers[0].running(), "a message to it puts it back to work");
        append(&agent_log, r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-20T20:35:12.000Z","message":{"model":"claude-haiku-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"probe ok probe ok"}]}}"#);
        f.catch_up();
        assert!(!f.helpers[0].running());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_background_agent_waiting_on_its_own_background_work_is_still_at_work() {
        let dir = std::env::temp_dir().join(format!("cue-steps-waits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        // The agent starts a long command in the background and ends its turn to wait for it.
        std::fs::write(sub.join("agent-a1.jsonl"), [
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"rebase it"}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"h1","name":"Bash","input":{"command":"git rebase main","run_in_background":true}}]}}"#,
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"Command running in background with ID: b9."}]},"toolUseResult":{"backgroundTaskId":"b9"}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Waiting for the rebase."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let path = dir.join("s1.jsonl");
        // Its session is told "completed", but that it's waiting on its own background work.
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"rebase the branch"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Rebase","prompt":"rebase it","run_in_background":true}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Rebase"}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Rebase\" is waiting on its own background work</summary>\n<note>This agent stopped with background work of its own still running.</note>\n</task-notification>"}}"#,
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers[0].finished_ms, 0, "not finished on a notice that says it waits");
        assert!(f.helpers[0].running(), "its log ended its turn, but its own command still runs");
        assert_eq!(running_background(p), 1);
        // Its command ends, it writes its report: done, before its session hears so.
        let mut log = std::fs::OpenOptions::new().append(true).open(sub.join("agent-a1.jsonl")).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"<task-notification>\n<task-id>b9</task-id>\n<status>completed</status>\n</task-notification>"}}"#, "\n",
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:01.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Rebased."}]}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert!(!f.helpers[0].running());
        // Its session's second notice: finished for good.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:02:02.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Rebase\" finished</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:02:02.000Z").unwrap());
        assert_eq!(running_background(p), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_message_to_an_agent_cue_has_no_log_of_doesnt_leave_the_session_at_work() {
        let dir = std::env::temp_dir().join(format!("cue-steps-ghost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"ask it again"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"zz9","message":"and the rest"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"success":true,"resumedAgentId":"zz9"}}"#,
        ].join("\n") + "\n").unwrap();
        assert_eq!(running_background(path.to_str().unwrap()), 0, "no log of it here: not counted as at work");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_background_agent_sent_a_message_after_it_finished_is_at_work_again_until_it_reports_again() {
        let dir = std::env::temp_dir().join(format!("cue-steps-resume-{}", std::process::id()));
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"general-purpose","description":"Review the plan","toolUseId":"t1","requestShape":"background"}"#).unwrap();
        let agent_log = sub.join("agent-a1.jsonl");
        std::fs::write(&agent_log, [
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"review the plan"}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:08.000Z","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Don't build it as is."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"get the plan reviewed"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Review the plan","prompt":"review the plan"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Review the plan","resolvedModel":"claude-opus-5-5"}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#,
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.len(), 1);
        assert!(!f.helpers[0].running(), "finished: its session heard so");
        // The session sends it a message: it's put back to work, on the same log.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(
            r#"{"type":"assistant","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"a1","summary":"Review the revised plan","message":"Please look again."}}]}}"#, "\n",
            r#"{"type":"user","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"{\"success\":true}"}]},"toolUseResult":{"success":true,"message":"Resuming agent a1","resumedAgentId":"a1","revivedAgentId":"a1"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert_eq!(f.helpers.len(), 1, "the same agent, not a second one");
        let h = &f.helpers[0];
        assert!(h.running(), "at work again, before its log shows the message");
        assert_eq!((h.finished_ms, h.last_ms, h.step.as_str()), (0, crate::logs::transcript::iso_ms("2026-10-04T10:02:01.000Z").unwrap(), "t2"));
        assert_eq!(step(f.turns.iter().flat_map(|t| t.items.iter()).last().unwrap()), ("Messaged its agent", "Review the revised plan", "back at work", false, true));
        assert_eq!(running_helpers(p), vec!["Review the plan".to_string()]);
        assert_eq!(running_background(p), 1);
        // Its log goes on: its new steps are read on from where they were.
        let mut log = std::fs::OpenOptions::new().append(true).open(&agent_log).unwrap();
        std::io::Write::write_all(&mut log, concat!(
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:01.000Z","message":{"role":"user","content":"Please look again."}}"#, "\n",
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:05.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"h1","name":"Read","input":{"file_path":"/a/plan.md"}}]}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        let h = &f.helpers[0];
        assert!(h.running());
        assert_eq!(step(h.items.last().unwrap()).0, "Reading");
        // Its second report: finished again.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:05:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t2</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:05:00.000Z").unwrap());
        assert!(!f.helpers[0].running());
        assert!(running_helpers(p).is_empty());
        // Read from where the launch isn't seen: the message takes it up, named by its meta file.
        let only = dir.join("s1-tail.jsonl");
        std::fs::create_dir_all(dir.join("s1-tail").join("subagents")).unwrap();
        std::fs::copy(sub.join("agent-a1.meta.json"), dir.join("s1-tail").join("subagents").join("agent-a1.meta.json")).unwrap();
        std::fs::copy(&agent_log, dir.join("s1-tail").join("subagents").join("agent-a1.jsonl")).unwrap();
        std::fs::write(&only, [
            r#"{"type":"assistant","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"a1","summary":"Review the revised plan","message":"Please look again."}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"{\"success\":true}"}]},"toolUseResult":{"success":true,"message":"Resuming agent a1","revivedAgentId":"a1"}}"#,
        ].join("\n") + "\n").unwrap();
        let mut g = Feed::open(only.to_str().unwrap(), 1);
        g.catch_up();
        assert_eq!(g.helpers.len(), 1);
        let h = &g.helpers[0];
        assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.kind.as_str()), ("a1", "t2", "Review the plan", "general-purpose"));
        assert!(h.running());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_background_script_shows_its_output_tail_until_its_session_hears_it_ended() {
        let dir = std::env::temp_dir().join(format!("cue-steps-script-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("b1.output");
        std::fs::write(&out, "   Compiling cue v0.1.41\n").unwrap();
        let launch = format!("Command running in background with ID: b1. Output is being written to: {}. You will be notified when it completes.", out.display());
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"build it"}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cd /x && npx tauri build --bundles app","description":"Build Cue","run_in_background":true}}]}}"#.to_string(),
            format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t1","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"b1"}}}}"#),
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.len(), 1);
        let h = &f.helpers[0];
        assert!(h.script);
        assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.command.as_str()), ("b1", "t1", "Build Cue", "cd /x && npx tauri build --bundles app"));
        assert!(h.running());
        assert_eq!((h.output.clone(), h.lines), (vec!["   Compiling cue v0.1.41".to_string()], 1));
        assert_eq!(step(&f.turns[0].items[0]), ("Ran", "npx tauri build --bundles app", "in the background", false, true));
        assert!(running_helpers(p).is_empty(), "a script isn't an agent the session waits on");
        assert_eq!(running_background(p), 1, "but it's a background task the card counts");
        let written = mtime_ms(&out).unwrap();
        assert_eq!((h.last_ms, h.tokens), (written, 0), "last did something: its output file last written; no tokens");
        assert!(!h.stuck(written + STUCK_MS) && h.stuck(written + STUCK_MS + 1));
        // More output: shown on the next read.
        let v = f.version;
        std::fs::write(&out, "   Compiling cue v0.1.41\n\x1b[32m    Finished\x1b[0m release\n").unwrap();
        f.catch_up();
        assert!(f.version > v);
        assert_eq!(f.helpers[0].output[1], "    Finished release");
        // Its notice: ended, and how.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>b1</task-id>\n<status>failed</status>\n<summary>Background command failed</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        let h = &f.helpers[0];
        assert!(!h.running() && h.failed);
        assert_eq!(h.finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:00.000Z").unwrap());
        assert_eq!(running_background(p), 0);
        assert!(f.failures.is_empty(), "failed within a minute of starting: the session sees to it, no word");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Claude Code delivers a task notice three ways: as the prompt of a queued command (an `attachment`
    /// entry), wrapped in a system notice inside a user message's text (beside a tool result, two at a
    /// time sometimes), and as a user message of its own. And a script's output file ends on
    /// "[exited with code N]" whether or not a notice ever shows up.
    #[test]
    fn a_background_script_ends_on_any_form_of_its_notice_or_on_its_exit_line() {
        let dir = std::env::temp_dir().join(format!("cue-steps-notices-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let start = |id: &str, out: &std::path::Path| {
            let launch = format!("Command running in background with ID: {id}. Output is being written to: {}. You will be notified when it completes.", out.display());
            [
                format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{{"content":[{{"type":"tool_use","id":"t-{id}","name":"Bash","input":{{"command":"make {id}","description":"Build {id}","run_in_background":true}}}}]}}}}"#),
                format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-{id}","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"{id}"}}}}"#),
            ].join("\n")
        };
        // (As JSON string text: escaped newlines and quotes.)
        let notice = |id: &str, status: &str| format!("<task-notification>\\n<task-id>{id}</task-id>\\n<status>{status}</status>\\n<summary>Background command \\\"Build {id}\\\" {status}</summary>\\n</task-notification>");
        let outs: Vec<std::path::PathBuf> = ["b1", "b2", "b3", "b4"].iter().map(|id| dir.join(format!("{id}.output"))).collect();
        for o in &outs {
            std::fs::write(o, "building\n").unwrap();
        }
        let path = dir.join("s1.jsonl");
        let mut log = format!("{}\n{}\n{}\n{}\n{}\n", r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"build them"}}"#, start("b1", &outs[0]), start("b2", &outs[1]), start("b3", &outs[2]), start("b4", &outs[3]));
        std::fs::write(&path, &log).unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.iter().filter(|h| h.running()).count(), 4);
        // (a) A queued command, delivered mid-turn: an attachment entry with the notice as its prompt.
        log.push_str(&format!(r#"{{"type":"attachment","timestamp":"2026-10-04T10:02:00.000Z","attachment":{{"type":"queued_command","commandMode":"task-notification","prompt":"{}"}}}}"#, notice("b1", "completed")));
        log.push('\n');
        // (b) Two notices wrapped in a system notice, in a user message that also carries a tool result.
        log.push_str(&format!(r#"{{"type":"user","timestamp":"2026-10-04T10:03:00.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-none","content":"ok"}},{{"type":"text","text":"[SYSTEM NOTIFICATION - NOT USER INPUT]\\nThis is an automated background-task event.\\n\\n{}\\n\\n{}\\n\\n# Environment update"}}]}}}}"#, notice("b2", "completed"), notice("b3", "failed")));
        log.push('\n');
        let before = f.version;
        std::fs::write(&path, &log).unwrap();
        f.catch_up();
        assert!(f.version > before, "tasks ending count as a change");
        let by = |f: &Feed, id: &str| f.helpers.iter().find(|h| h.id == id).map(|h| (h.finished_ms, h.failed)).unwrap();
        assert_eq!(by(&f, "b1"), (crate::logs::transcript::iso_ms("2026-10-04T10:02:00.000Z").unwrap(), false), "the attachment's notice");
        assert_eq!(by(&f, "b2"), (crate::logs::transcript::iso_ms("2026-10-04T10:03:00.000Z").unwrap(), false), "the first wrapped notice");
        assert_eq!(by(&f, "b3"), (crate::logs::transcript::iso_ms("2026-10-04T10:03:00.000Z").unwrap(), true), "the second wrapped notice, beside a tool result");
        assert_eq!(f.turns.len(), 1, "a notice beside a tool result opens no turn");
        assert!(f.helpers.iter().find(|h| h.id == "b4").unwrap().running());
        // b3 failed three minutes after it started: worth a word, with its output's last line.
        assert_eq!(f.failures, vec![TaskFailure { path: p.to_string(), at_ms: by(&f, "b3").0, desc: "Build b3".into(), line: "building".into() }]);
        // (c) No notice for b4, but Claude Code ended its output file on the exit line: over, and failed.
        std::fs::write(&outs[3], "building\nerror: no rule\n[exited with code 2]\n").unwrap();
        f.catch_up();
        let h = f.helpers.iter().find(|h| h.id == "b4").unwrap();
        assert!(!h.running() && h.failed);
        assert_eq!(h.finished_ms, mtime_ms(&outs[3]).unwrap(), "finished when the file was last written");
        assert_eq!(h.last_line(), "error: no rule", "the exit line isn't what it last said");
        assert_eq!(f.failures.len(), 2, "it failed long after it started (the file was written just now): noted too");
        assert_eq!(f.failures[1].line, "error: no rule");
        // The hub takes the failures of every open feed, once.
        crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).insert(p.to_string(), f);
        let taken = take_task_failures();
        assert_eq!(taken.iter().filter(|t| t.path == p).count(), 2);
        assert!(take_task_failures().iter().all(|t| t.path != p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pi_log_reads_as_steps_too() {
        let log = [
            r#"{"type":"session","version":3,"id":"P1","timestamp":"2026-10-04T10:00:00.000Z","cwd":"/a"}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":"<skill name="x">loaded</skill>"}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:02.000Z","message":{"role":"user","content":[{"type":"text","text":"fix the filter"}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":"Looking."},{"type":"toolCall","id":"c1","name":"edit","arguments":{"path":"/a/src/filters.ts"}}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:04.000Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"edit","content":[{"type":"text","text":"ok"}],"isError":false,"details":{"diff":" 56   return [];\n-60 old line\n+60 new line\n+61   added\n    ...\n 90 tail"}}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:05.000Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c2","name":"bash","arguments":{"command":"npm test"}}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:09.000Z","message":{"role":"toolResult","toolCallId":"c2","toolName":"bash","content":[{"type":"text","text":"FAIL\n\nCommand exited with code 1"}],"isError":true}}"#,
        ]
        .join("\n");
        let dir = std::env::temp_dir().join(format!("cue-steps-pi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.jsonl");
        std::fs::write(&path, log + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        assert_eq!(f.kind, Kind::Pi);
        f.catch_up();
        assert_eq!(f.turns.len(), 1, "the skill Pi loaded isn't a turn");
        assert_eq!(f.turns[0].prompt, "fix the filter");
        let items = &f.turns[0].items;
        assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking."));
        assert_eq!(step(&items[1]), ("Edited", "filters.ts", "+2 −1", false, true));
        assert_eq!(step(&items[2]), ("Ran", "npm test", "exit 1", true, true));
        let d = detail(p, &["c1".into()]);
        assert_eq!(d["c1"]["diff"][0], json!([" ", "  return [];"]));
        assert_eq!(d["c1"]["diff"][3], json!(["+", "  added"]));
        assert_eq!(d["c1"]["diff"][4], json!(["…", ""]));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_command_reads_without_its_cd() {
        assert_eq!(without_cd("cd ~/code/app && npm test"), "npm test");
        assert_eq!(without_cd("cd /a; cd b && make"), "make");
        assert_eq!(without_cd("cd /a"), "cd /a");
        assert_eq!(without_cd("npm test && cd x"), "npm test && cd x");
    }

    #[test]
    fn a_new_prompt_starts_a_turn_and_closes_what_was_open() {
        let more = format!("{T}\n{}", r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"[Request interrupted by user]"}}
{"type":"user","timestamp":"2026-10-04T10:01:05.000Z","message":{"content":"<command-name>/rename</command-name>"}}
{"type":"user","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"now the docs"}}
{"type":"assistant","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"text","text":"On it."}]}}"#);
        let f = feed_of(&more);
        assert_eq!(f.turns.len(), 2, "the interrupt and the /rename don't start turns");
        assert_eq!(step(&f.turns[0].items[4]), ("Ran", "npm test", "stopped", false, true));
        assert_eq!(f.turns[1].items.len(), 1);
    }

    #[test]
    fn reads_only_whole_new_lines() {
        let dir = std::env::temp_dir().join(format!("cue-steps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let first: Vec<&str> = T.lines().collect();
        // Everything up to the Edit's result, then half of the next line.
        std::fs::write(&path, format!("{}\n{}", first[..5].join("\n"), &first[5][..20])).unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.turns[0].items.len(), 3);
        let v = f.version;
        std::fs::write(&path, format!("{}\n", first.join("\n"))).unwrap();
        f.catch_up();
        assert_eq!(f.turns[0].items.len(), 5, "the half line is read once it's whole");
        assert!(f.version > v);
        let d = detail(p, &["t2".into(), "t3".into()]);
        assert_eq!(d["t2"]["diff"][1], json!(["-", "b"]));
        assert_eq!(d["t3"]["full"], "npm test\n");
        assert_eq!(d["t3"]["output"][1], "Tests: 3 failed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_window_claude_code_reports_sets_the_context_share() {
        let dir = std::env::temp_dir().join(format!("cue-steps-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sid = format!("win-{}", std::process::id());
        let path = dir.join(format!("{sid}.jsonl"));
        // 174k tokens on a model whose name doesn't say its window is 1M.
        std::fs::write(&path, "{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{\"content\":\"hi\"}}\n{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{\"model\":\"claude-opus-5-5\",\"content\":[{\"type\":\"text\",\"text\":\"Hello.\"}],\"usage\":{\"input_tokens\":2,\"cache_read_input_tokens\":173376,\"cache_creation_input_tokens\":710}}}\n").unwrap();
        let p = path.to_str().unwrap();
        assert_eq!(context_pct(p), None, "without word from Claude Code: no share (a 200k guess is 5x off on a 1M window)");
        let before = steps(p, 0)["version"].as_u64().unwrap();
        set_window(&sid, 1_000_000, true);
        assert_eq!(context_pct(p), Some(17), "Claude Code's own window, as its status line shows");
        let after = steps(p, before);
        assert!(after["version"].as_u64().unwrap() > before && after["meta"]["window"] == 1_000_000, "the window redraws: {after}");
        // Another session on that model, before its own status line has run: the model's window, not the guess.
        let other = dir.join("win-other.jsonl");
        std::fs::write(&other, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
        let path2 = dir.join(format!("{sid}-2.jsonl"));
        std::fs::write(&path2, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
        assert_eq!(context_pct(path2.to_str().unwrap()), None, "a model nobody has reported yet: no share");
        set_window(&format!("{sid}-2"), 1_000_000, true);
        assert_eq!(context_pct(other.to_str().unwrap()), Some(17), "the window another session on its model reported");
        // A session on another machine runs that model with 200k (another provider): that's its own window
        // alone, and this Mac's sessions on the model keep the 1M theirs reported.
        let remote = dir.join(format!("{sid}-3.jsonl"));
        std::fs::write(&remote, std::fs::read_to_string(&path2).unwrap()).unwrap();
        assert_eq!(context_pct(remote.to_str().unwrap()), Some(17), "before it says: the model's window, as before");
        set_window(&format!("{sid}-3"), 200_000, false);
        assert_eq!(context_pct(remote.to_str().unwrap()), Some(87), "its own window, once its status line says");
        let mac = dir.join(format!("{sid}-4.jsonl"));
        std::fs::write(&mac, std::fs::read_to_string(&path2).unwrap()).unwrap();
        assert_eq!(context_pct(mac.to_str().unwrap()), Some(17), "this Mac's sessions on the model: still 1M");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_fresh_feed_answers_in_full_even_when_its_count_matches() {
        let dir = std::env::temp_dir().join(format!("cue-steps-fresh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let user = |text: &str| format!("{{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{{\"content\":\"{text}\"}}}}\n");
        let line = |model: &str| {
            format!(
                "{{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{{\"model\":\"{model}\",\"content\":[{{\"type\":\"text\",\"text\":\"Hello.\"}}],\"usage\":{{\"input_tokens\":10,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}}}}\n"
            )
        };
        let append = |path: &std::path::Path| {
            let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            std::io::Write::write_all(&mut file, format!("{}", user("switch")).as_bytes()).unwrap();
            std::io::Write::write_all(&mut file, line("claude-opus-5-5").as_bytes()).unwrap();
        };
        // While the feed lives, a model change is read and answered.
        let live = dir.join("live.jsonl");
        std::fs::write(&live, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
        let l1 = steps(live.to_str().unwrap(), 0);
        assert_eq!(l1["meta"]["model"], "claude-sonnet-5-5");
        append(&live);
        let l2 = steps(live.to_str().unwrap(), l1["version"].as_u64().unwrap());
        assert_eq!(l2["meta"]["model"], "claude-opus-5-5", "the model change is read while the feed lives");
        // A feed nobody asked for in a while is dropped: the caller still has its version, 2 here (the
        // first read). A fresh feed reads the whole file in one go and counts 2 again, so the "nothing
        // changed" answer would leave the caller on the old model. It must answer in full.
        let path = dir.join("t.jsonl");
        std::fs::write(&path, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
        let p = path.to_str().unwrap();
        let r1 = steps(p, 0);
        assert_eq!(r1["version"].as_u64(), Some(2));
        append(&path);
        crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).remove(p);
        let r2 = steps(p, r1["version"].as_u64().unwrap());
        assert_eq!(r2["version"].as_u64(), r1["version"].as_u64());
        assert_eq!(r2["meta"]["model"], "claude-opus-5-5", "a fresh feed answers with what's there now, whatever its count");
        assert!(r2["turns"].as_array().is_some_and(|t| !t.is_empty()));
        // Once it has answered, a feed that still counts the same answers with just the version.
        let r3 = steps(p, r2["version"].as_u64().unwrap());
        assert!(r3.get("turns").is_none() && r3.get("meta").is_none(), "only the version comes back when nothing changed");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod codex_tests {
    use super::*;

    #[test]
    fn a_codex_log_reads_as_steps_too() {
        let log = [
            r#"{"timestamp":"2026-10-05T06:36:45.800Z","type":"session_meta","payload":{"session_id":"X1","cwd":"/a"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:45.816Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"AGENTS.md instructions"}]}}"#,
            r#"{"timestamp":"2026-10-05T06:36:45.831Z","type":"event_msg","payload":{"type":"user_message","message":"fix the menu"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:46.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking at the menu code."}]}}"#,
            r#"{"timestamp":"2026-10-05T06:36:47.000Z","type":"event_msg","payload":{"type":"agent_message","message":"Looking at the menu code."}}"#,
            r#"{"timestamp":"2026-10-05T06:36:48.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"cd /a && rg --files\"}","call_id":"k1"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:48.500Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k1","output":"Chunk ID: 1\nWall time: 0.03 seconds\nProcess exited with code 0\nOutput:\napp.py\nREADME.md\n"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:49.000Z","type":"response_item","payload":{"type":"function_call","name":"write_stdin","arguments":"{\"session_id\":1,\"chars\":\"\"}","call_id":"k2"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:50.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /a/app.py\n@@\n-    old()\n+    new()\n+    more()\n*** End Patch","call_id":"k3"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:50.500Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"k3","output":"Exit code: 0\nWall time: 0.2 seconds\nOutput:\nSuccess. Updated the following files:\nM /a/app.py\n"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:51.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pytest -q\"}","call_id":"k4"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:55.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k4","output":"Chunk ID: 2\nProcess exited with code 1\nOutput:\n1 failed\n"}}"#,
        ]
        .join("\n");
        let dir = std::env::temp_dir().join(format!("cue-steps-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout.jsonl");
        std::fs::write(&path, log + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        assert_eq!(f.kind, Kind::Codex);
        f.catch_up();
        assert_eq!(f.turns.len(), 1, "Codex's instructions aren't a turn of yours");
        assert_eq!(f.turns[0].prompt, "fix the menu");
        let items = &f.turns[0].items;
        let line = |i: usize| match &items[i] {
            Item::Step { verb, subject, result, bad, .. } => (verb.as_str(), subject.as_str(), result.as_str(), *bad),
            Item::Say { text, .. } => ("say", text.as_str(), "", false),
        };
        assert_eq!(items.len(), 4, "its words once (not again from agent_message), no write_stdin");
        assert_eq!(line(0), ("say", "Looking at the menu code.", "", false));
        assert_eq!(line(1), ("Ran", "rg --files", "README.md", false));
        assert_eq!(line(2), ("Edited", "app.py", "+2 −1", false));
        assert_eq!(line(3), ("Ran", "pytest -q", "exit 1", true));
        let d = detail(p, &["k3".into(), "k4".into()]);
        assert_eq!(d["k3"]["diff"][1], json!(["+", "    new()"]));
        assert_eq!(d["k4"]["output"][1], "1 failed");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod script_tests {
    #[test]
    fn a_script_that_only_runs_a_command_reads_as_that_command() {
        assert_eq!(super::script_command(r#"const r = await tools.exec_command({"cmd":"git status --short","yield_time_ms":1000}); text(r);"#).as_deref(), Some("git status --short"));
        assert_eq!(super::script_command("const m = ALL_TOOLS.filter(x => x); text(m);"), None);
    }
}
