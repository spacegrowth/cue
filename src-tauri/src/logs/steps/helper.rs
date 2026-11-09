//! A background agent or shell command a session started: what the chat shows of it, how its log or
//! output file is read, and where its log and meta file are.

use super::feed::Feed;
use super::model::Item;
use crate::sessions::quiet::{judge, quiet_ms, Last, Quiet};
use super::reading::{HelperRead, Reads, ScriptRead};
use super::step_line::strip_ansi;
use super::LINE_CHARS;
use crate::logs::transcript::cut;
use serde::Serialize;
use serde_json::Value;

/// A background agent a Claude Code session started: it runs inside the session's process, with no
/// terminal of its own, and writes its log to `<session folder>/subagents/agent-<id>.jsonl`. Nothing
/// can be sent to it from outside; it reports to its session. Here so the chat can show it at work
/// (its steps, live) under the step that started it, and the session can be marked as waiting on it.
/// Also a shell command run in the background (`script`): the same place and the same ending (Claude
/// Code's task notice), but its body is the tail of its output file, not steps.
#[derive(Serialize)]
pub struct Helper {
    /// Claude Code's agent id.
    pub(super) id: String,
    /// The Delegating step that started it (its tool_use id).
    pub(super) step: String,
    /// The one-line description the session gave it.
    pub(super) desc: String,
    /// Its agent type ("Explore", "general-purpose"), from its meta file; "" until that's read.
    pub(super) kind: String,
    pub(super) model: String,
    pub(super) started_ms: u64,
    /// When the session was told it finished (its task-notification; a script also ends on the exit line
    /// at the end of its output file); 0 until then.
    pub(super) finished_ms: u64,
    /// When it last did something: an agent's newest step, a script's output file last written; its
    /// start until then. Quiet past the threshold (sessions/quiet.rs) is judged by its last line (`quiet`).
    pub(super) last_ms: u64,
    /// An agent's tokens so far (input, output, and cache reads and writes), summed over its log's replies. 0 for a script.
    pub(super) tokens: u64,
    /// Its own steps, from its log (the last MAX_HELPER_ITEMS; `more` says how many came before).
    pub(super) items: Vec<Item>,
    pub(super) more: usize,
    #[serde(skip)]
    pub(super) feed: Option<Box<Feed>>,
    /// A background shell command, not an agent: `command` ran, `output` is the tail of what it wrote
    /// (`lines` in all), `failed` once its notice or exit line said so, `stopped` when it was killed
    /// before it ended (not a failure: it was stopped on purpose).
    pub(super) script: bool,
    pub(super) command: String,
    pub(super) output: Vec<String>,
    pub(super) lines: usize,
    pub(super) failed: bool,
    pub(super) stopped: bool,
    #[serde(skip)]
    pub(super) out_path: Option<std::path::PathBuf>,
    #[serde(skip)]
    pub(super) out_len: u64,
    /// Script: its end has been seen to (its copy stopped, a long failure noted).
    #[serde(skip)]
    pub(super) closed: bool,
    /// When its session last sent it a message that put it back to work (SendMessage); 0 if never.
    #[serde(skip)]
    pub(super) resumed_ms: u64,
    /// A teammate (an agent started with a name): it stays after its task, idle, until its session gives it
    /// more; `name` is what its session calls it by. From its launch, else its meta file once that's there.
    pub(super) teammate: bool,
    pub(super) name: String,
    /// Its meta file has been read for what its launch didn't say (once it was there).
    #[serde(skip)]
    pub(super) meta_read: bool,
    /// A catch-up has read its log or output file (its meta file too) at least once.
    #[serde(skip)]
    pub(super) seen: bool,
    /// Taken up on a message to it, not its launch: kept only once its log is found there.
    #[serde(skip)]
    pub(super) unconfirmed: bool,
}

/// A background task that failed after running this long: worth a word (a quick failure is one the
/// session sees to itself).
pub(super) const FAILURE_WORTH_MS: u64 = 60_000;

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
pub(super) const MAX_HELPERS: usize = 12;
/// A background script's output shown: its last lines, read from the end of its file.
const MAX_SCRIPT_LINES: usize = 200;

impl Helper {
    /// Running: not reported finished, and its log doesn't end on an answer (one from before its session
    /// sent it a message doesn't count, nor one that waits on its own background work: a command or agent
    /// it started, which brings it back when it's done).
    /// An agent quiet past the threshold whose last line is its own words (sessions/quiet.rs) finished and only
    /// lingers: not running (its notice or `end_turn` never came: a fork, a resumed one that answered).
    pub fn running(&self) -> bool {
        self.at_work() && !self.lingering(crate::model::now_ms())
    }

    /// By its log and notice alone: not reported finished, and its log doesn't end on an answer.
    fn at_work(&self) -> bool {
        self.finished_ms == 0 && self.feed.as_ref().is_none_or(|f| f.ended_ms == 0 || f.ended_ms < self.resumed_ms || f.helpers.iter().any(Helper::running))
    }

    /// Finished and lingering as of `now`: an agent (not a script) waiting on none of its own, quiet, its last line
    /// its own words.
    fn lingering(&self, now: u64) -> bool {
        !self.script && !self.feed.as_ref().is_some_and(|f| f.helpers.iter().any(Helper::running)) && judge(self.last_kind(), self.heard_ms(), now) == Quiet::Finished
    }

    /// When it was last heard of: its newest line, or a message its session sent it since (that restarts the quiet).
    fn heard_ms(&self) -> u64 {
        self.last_ms.max(self.resumed_ms).max(self.feed.as_ref().map_or(0, |f| f.last_line.0))
    }

    /// Its log's last line: its own words, a step still open, or a step done (or nothing yet).
    fn last_kind(&self) -> Last {
        match self.feed.as_ref() {
            // A message its session sent it since its last line: it was to go on from there.
            Some(f) if f.last_line.0 > 0 && self.resumed_ms <= f.last_line.0 => f.last_line.1,
            _ => Last::Result,
        }
    }

    /// Running and quiet as of `now`: "tool" (a step open: a long run, left to it) or "stuck"; "" otherwise. A script
    /// with nothing written for the threshold is stuck (it has no steps to tell by).
    pub(super) fn quiet(&self, now: u64) -> &'static str {
        if !self.running() {
            return "";
        }
        if self.script {
            return if now.saturating_sub(self.heard_ms()) >= quiet_ms() { "stuck" } else { "" };
        }
        match judge(self.last_kind(), self.heard_ms(), now) {
            Quiet::Tool => "tool",
            Quiet::Stuck => "stuck",
            _ => "",
        }
    }

    /// Running, and quiet with nothing open as of `now` (`quiet`).
    pub(super) fn stuck(&self, now: u64) -> bool {
        self.quiet(now) == "stuck"
    }

    /// The step it has open, as its row says it ("Ran npm test"); "" when none is.
    pub(super) fn open_step(&self) -> String {
        match self.feed.as_ref().and_then(|f| f.turns.iter().rev().find_map(|t| t.items.last())) {
            Some(Item::Step { done: false, verb, subject, .. }) => format!("{verb} {subject}").trim().to_string(),
            _ => String::new(),
        }
    }

    /// Put in what a catch-up read of it found; true when what's shown of it changed.
    pub(super) fn put(&mut self, read: HelperRead) -> bool {
        let first = !self.seen;
        self.seen = true;
        match read {
            HelperRead::Script(r) => self.put_output(r),
            HelperRead::Agent { log, meta } => {
                let found = log.as_ref().is_some_and(|r| r.log.found);
                self.unconfirmed &= !found;
                if let Some(meta) = meta {
                    self.take_meta(&meta);
                }
                // Nothing said what it's doing (no description, none in a meta file, or no log path to find one
                // by): a teammate goes by its name.
                if self.desc.is_empty() {
                    self.desc = self.name.clone();
                }
                self.put_log(log) || first
            }
        }
    }

    /// Its log's read put in; true when its steps changed.
    fn put_log(&mut self, log: Option<Reads>) -> bool {
        let (Some(f), Some(log)) = (self.feed.as_mut(), log) else { return false };
        let before = f.version;
        f.apply(log);
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
        true
    }

    /// What its meta file says that its launch didn't: its type, its description, and whether it's a
    /// teammate. Its meta file can come after it started (copied from another machine).
    fn take_meta(&mut self, meta: &Value) {
        let m = |k: &str| meta.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        if self.kind.is_empty() {
            self.kind = m("agentType");
        }
        if !self.meta_read {
            self.meta_read = !meta.is_null();
            if self.desc.is_empty() {
                self.desc = m("description");
            }
            if m("taskKind") == "in_process_teammate" {
                self.teammate = true;
                self.name = m("name");
            }
        }
    }
}

impl Helper {
    /// A script's output file as read: its tail again when it grew (or shrank), and when it was last
    /// written. True when what's shown changed.
    fn put_output(&mut self, read: ScriptRead) -> bool {
        self.last_ms = read.mtime.unwrap_or(self.started_ms);
        let Some(tail) = read.tail.filter(|_| read.planned_len == self.out_len) else { return false };
        self.out_len = tail.len;
        let text = String::from_utf8_lossy(&tail.bytes);
        // Cut mid-file: the first partial line goes.
        let text = if tail.from > 0 { text.split_once('\n').map(|(_, r)| r).unwrap_or("") } else { &text };
        let all: Vec<&str> = text.lines().collect();
        self.lines = all.len() + usize::from(tail.from > 0);
        self.output = all.iter().rev().take(MAX_SCRIPT_LINES).rev().map(|l| cut(&strip_ansi(l), LINE_CHARS)).collect();
        // Claude Code ends the file on "[exited with code N]", or "[killed]" when it was stopped: over,
        // even though a stopped one never gets a notice and an ended one's may not show up.
        if self.finished_ms == 0 {
            if let Some(end) = self.output.iter().rev().find(|l| !l.trim().is_empty()).and_then(|l| end_line(l)) {
                self.finished_ms = read.mtime.unwrap_or_else(crate::model::now_ms).max(1);
                self.failed = matches!(end, End::Exited(code) if code != 0);
                self.stopped = end == End::Killed;
            }
        }
        true
    }

    /// The newest thing it did, in a few words: a script's last line, an agent's last step (else the first
    /// line of what it said last).
    pub(super) fn latest(&self) -> String {
        if self.script {
            return self.last_line();
        }
        match self.items.last() {
            Some(Item::Step { verb, subject, .. }) => format!("{verb} {subject}").trim().to_string(),
            Some(Item::Say { text, .. }) => text.lines().next().unwrap_or("").to_string(),
            None => String::new(),
        }
    }

    /// A script's output, its last line that says something (not the exit line).
    pub(super) fn last_line(&self) -> String {
        self.output.iter().rev().map(|l| l.trim()).find(|l| !l.is_empty() && end_line(l).is_none()).unwrap_or("").to_string()
    }
}

/// How a background command's output file ends: Claude Code's last line in it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    /// "[exited with code N]"
    Exited(i64),
    /// "[killed]": stopped before it ended.
    Killed,
}
fn end_line(line: &str) -> Option<End> {
    let line = line.trim();
    if line == "[killed]" {
        return Some(End::Killed);
    }
    line.strip_prefix("[exited with code ")?.strip_suffix(']')?.trim().parse().ok().map(End::Exited)
}

/// When a file was last written, in ms since 1970.
pub(super) fn mtime_ms(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// A background agent's meta file, beside its log (its type, the description it was given); Null when there's none (yet).
pub(super) fn helper_meta(log: &std::path::Path) -> Value {
    std::fs::read(log.with_extension("meta.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null)
}

/// An Agent call's launch status that leaves it working on its own: a background agent ("async_launched"),
/// or a teammate, one given a name ("teammate_spawned"). A teammate writes its log where the others do and
/// ends its turn the same way; it reports by a message (an idle notice), not a task notice, so it reads as
/// done once its log ends on its answer, and at work again when its session messages it.
pub(super) fn in_background(status: &str) -> bool {
    matches!(status, "async_launched" | "teammate_spawned")
}

/// The background agent a SendMessage result says it put back to work (`resumedAgentId`, or `revivedAgentId`).
pub(super) fn resumed_agent(extra: Option<&Value>) -> Option<String> {
    let x = extra?;
    ["resumedAgentId", "revivedAgentId"].iter().find_map(|k| x.get(*k).and_then(Value::as_str).filter(|id| !id.is_empty())).map(str::to_string)
}

/// Every `<task-notification>…</task-notification>` in a text, wherever it sits (Claude Code wraps them
/// in a system notice, and can put two in one message).
pub(super) fn notifications(text: &str) -> Vec<&str> {
    text.split("<task-notification>").skip(1).filter_map(|r| r.split("</task-notification>").next()).collect()
}

/// Where a session's background agent writes its log, and its meta file (`<session folder>/subagents/`).
#[cfg(test)]
pub fn helper_path_for_test(session_log: &str, agent_id: &str) -> Option<std::path::PathBuf> {
    helper_path(session_log, agent_id)
}
pub(super) fn helper_path(session_log: &str, agent_id: &str) -> Option<std::path::PathBuf> {
    let folder = session_log.strip_suffix(".jsonl")?;
    Some(std::path::Path::new(folder).join("subagents").join(format!("agent-{agent_id}.jsonl")))
}

/// What to call a background agent, from its log's path: the description its session gave it (its meta
/// file), else its type, else its id (`agent-<id>.jsonl`).
pub(super) fn helper_name(log: &std::path::Path) -> String {
    let meta = helper_meta(log);
    let from_meta = ["description", "agentType"].iter().find_map(|k| meta.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()));
    match from_meta {
        Some(s) => s.to_string(),
        None => log.file_stem().and_then(|s| s.to_str()).unwrap_or("").trim_start_matches("agent-").to_string(),
    }
}
