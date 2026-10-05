//! What a session did, step by step, read from its Claude Code transcript for the chat's step lines
//! (the window's Active pane, and anything else that asks): the agent's words between steps, and each
//! tool call as one line (what it did, how it went). Read only while someone has the session open:
//! each feed remembers how far into the file it got and reads just the new lines, and a feed nobody
//! asked for in a couple of minutes is dropped. Nothing is saved; the transcript is the only copy.
//!
//! A step's full output or diff isn't part of the feed: `detail` fetches it when you open the step.
//!
//! SHORTCUT: viewers poll (every 0.5 s while the session works) rather than being
//! pushed new steps. Fine for a few viewers, since a poll with nothing new reads no file and answers
//! with just a version; push new steps instead if many viewers watch at once.
//! SHORTCUT: only the last 4 MB of a transcript is read, so very old turns of a huge session aren't
//! there. That's past anything the chat shows; read further back on demand if that changes.

use crate::transcript::{content_blocks, cut, lines, stamp};
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
}

struct Feed {
    path: String,
    offset: u64,
    /// Started mid-file: the first (partial) line still has to be skipped.
    align: bool,
    turns: Vec<Turn>,
    version: u64,
    used: Instant,
}

static FEEDS: Mutex<Option<HashMap<String, Feed>>> = Mutex::new(None);

/// The session's turns and steps, newest last. `known`: the version the caller already has; when
/// nothing changed, only the version comes back.
pub fn steps(path: &str, known: u64) -> Value {
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    if feed.version == known {
        return json!({ "version": feed.version });
    }
    let turns: Vec<&Turn> = feed.turns.iter().filter(|t| !t.items.is_empty()).collect();
    json!({ "version": feed.version, "turns": turns })
}

impl Feed {
    fn open(path: &str, version: u64) -> Feed {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let offset = len.saturating_sub(START_BYTES);
        Feed { path: path.to_string(), offset, align: offset > 0, turns: Vec::new(), version, used: Instant::now() }
    }

    /// Read what was added since last time (whole lines only; a half-written line waits for the next call).
    fn catch_up(&mut self) {
        let Ok(mut file) = std::fs::File::open(&self.path) else { return };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // The file was replaced: start over (with a new version, so callers redraw).
            *self = Feed::open(&self.path, self.version + 1);
        }
        if len <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return;
        }
        let mut from = 0;
        if self.align {
            let Some(nl) = buf.iter().position(|&b| b == b'\n') else { return };
            from = nl + 1;
            self.align = false;
        }
        let Some(end) = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1).filter(|&e| e > from) else {
            self.offset += from as u64;
            return;
        };
        let text = String::from_utf8_lossy(&buf[from..end]);
        let mut changed = false;
        for e in lines(&text) {
            changed |= self.take(&e);
        }
        self.offset += end as u64;
        if changed {
            if self.turns.len() > MAX_TURNS {
                self.turns.drain(..self.turns.len() - MAX_TURNS);
            }
            self.version += 1;
        }
    }

    /// One transcript entry. Returns whether anything shown changed.
    fn take(&mut self, e: &Value) -> bool {
        // A helper agent's own steps (older transcripts keep them inline) aren't this session's.
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let at = stamp(e);
        match e.get("type").and_then(Value::as_str) {
            Some("user") => {
                let blocks = content_blocks(e);
                let results: Vec<&Value> = blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
                if !results.is_empty() {
                    let extra = e.get("toolUseResult");
                    return results.into_iter().fold(false, |any, b| self.finish(b, extra) || any);
                }
                if e.get("isMeta").and_then(Value::as_bool) == Some(true) || e.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
                    return false;
                }
                let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
                let text = text.trim_start();
                if text.starts_with("[Request interrupted") {
                    return self.close_open("stopped");
                }
                // A local command (/rename, /model) and its output: no turn of work.
                if text.starts_with("<command-") || text.starts_with("<local-command") {
                    return false;
                }
                self.close_open("");
                let prompt = if crate::transcript::harness_text(text) { String::new() } else { cut(text.trim(), 1200) };
                self.turns.push(Turn { at_ms: at, prompt, items: Vec::new(), more: 0 });
                false // an empty turn isn't shown until something happens in it
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
                            self.push(step_of(&b, at));
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
            self.turns.push(Turn { at_ms: at, prompt: String::new(), items: Vec::new(), more: 0 });
        }
        let turn = self.turns.last_mut().unwrap();
        turn.items.push(item);
        if turn.items.len() > MAX_ITEMS {
            turn.items.remove(0);
            turn.more += 1;
        }
    }

    /// A tool call's result came in: fill in how it went.
    fn finish(&mut self, block: &Value, extra: Option<&Value>) -> bool {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { return false };
        for turn in self.turns.iter_mut().rev().take(3) {
            for item in turn.items.iter_mut().rev() {
                if let Item::Step { id: sid, tool, verb, past, result, bad, done, .. } = item {
                    if sid == id {
                        let (r, b) = outcome(tool, block, extra);
                        *verb = past.to_string();
                        *result = r;
                        *bad = b;
                        *done = true;
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
        if let Some(code) = text.split("Exit code ").nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>()).filter(|c| !c.is_empty()) {
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
        "Agent" | "Task" if x("status").and_then(Value::as_str) == Some("async_launched") => "in the background".into(),
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
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
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
    use super::*;

    fn feed_of(jsonl: &str) -> Feed {
        let mut f = Feed { path: String::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now() };
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
}
