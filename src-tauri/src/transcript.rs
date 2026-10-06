//! Reads Claude Code session transcripts (JSONL). Two jobs:
//! 1. Context: the last few things said, so a request isn't shown cold.
//! 2. "Answered in the terminal": Claude never tells the hook when you answer in the terminal,
//!    but the transcript gets a tool_result for that tool_use within about a second.

use crate::model::Ctx;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const TAIL_BYTES: u64 = 1024 * 1024;
const CTX_CHARS: usize = 1500;

/// Read from `from` to EOF; drop a partial first line when starting mid-file.
fn read_from(path: &str, from: u64) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = from.min(len);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf).to_string();
    if start > 0 {
        Some(s.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default())
    } else {
        Some(s)
    }
}

pub fn tail_offset(path: &str) -> u64 {
    std::fs::metadata(path).map(|m| m.len().saturating_sub(TAIL_BYTES)).unwrap_or(0)
}

pub(crate) fn lines(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
}

pub(crate) fn content_blocks(entry: &Value) -> Vec<Value> {
    match entry.pointer("/message/content") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => vec![serde_json::json!({"type": "text", "text": s})],
        _ => vec![],
    }
}

/// The id of the most recent tool_use matching this request, if it's in the transcript yet.
pub fn find_tool_use_id(path: &str, from: u64, tool_name: &str, tool_input: &Value) -> Option<String> {
    let text = read_from(path, from)?;
    let mut found = None;
    for entry in lines(&text) {
        if entry.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        for b in content_blocks(&entry) {
            if b.get("type").and_then(Value::as_str) == Some("tool_use")
                && b.get("name").and_then(Value::as_str) == Some(tool_name)
                && b.get("input") == Some(tool_input)
            {
                found = b.get("id").and_then(Value::as_str).map(String::from);
            }
        }
    }
    found
}

/// True once a tool_result for `tool_use_id` exists — i.e. the request got answered somewhere.
pub fn has_result(path: &str, from: u64, tool_use_id: &str) -> bool {
    let Some(text) = read_from(path, from) else { return false };
    // Cheap pre-filter before parsing: most polls find nothing new.
    if !text.contains(tool_use_id) {
        return false;
    }
    let found = lines(&text).any(|entry| {
        content_blocks(&entry).iter().any(|b| {
            b.get("type").and_then(Value::as_str) == Some("tool_result")
                && b.get("tool_use_id").and_then(Value::as_str) == Some(tool_use_id)
        })
    });
    found
}

/// The newest tool call still waiting for its result (id, tool, input): what a prompt in the terminal
/// is about, when Claude Code tells Cue only that one is showing.
pub fn waiting_tool_use(path: &str) -> Option<(String, String, Value)> {
    let text = read_from(path, tail_offset(path))?;
    let (mut calls, mut done) = (Vec::new(), std::collections::HashSet::new());
    for entry in lines(&text) {
        for b in content_blocks(&entry) {
            let id = || b.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => calls.push((id(), b.get("name").and_then(Value::as_str).unwrap_or("").to_string(), b.get("input").cloned().unwrap_or(Value::Null))),
                Some("tool_result") => {
                    done.insert(b.get("tool_use_id").and_then(Value::as_str).unwrap_or("").to_string());
                }
                _ => {}
            }
        }
    }
    calls.into_iter().rev().find(|(id, ..)| !id.is_empty() && !done.contains(id))
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= CTX_CHARS {
        return s.to_string();
    }
    let head: String = s.chars().take(CTX_CHARS).collect();
    format!("{head}…")
}

/// What a working Claude Code session is doing right now, from the newest entry of this turn in its
/// transcript, and since when: a tool running ("Running: cargo test", "Editing app.js"), "Thinking…"
/// (a tool just finished, or it's reasoning) or "Writing…" (its reply). Never the words themselves:
/// a line it wrote ("It works live") reads like a status, not an activity.
pub fn activity(path: &str) -> Option<(String, u64)> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(256 * 1024))?;
    let entries: Vec<Value> = lines(&text).collect();
    for e in entries.iter().rev() {
        let kind = e.get("type").and_then(Value::as_str);
        // Only this turn: stop at your prompt (not at a tool's result or a meta note).
        let tool_result = content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
        if kind == Some("user") && !tool_result && e.get("isMeta").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if kind == Some("user") && tool_result {
            return Some(("Thinking…".into(), stamp(e))); // a step just finished: it's working out the next
        }
        if kind != Some("assistant") {
            continue;
        }
        let blocks = content_blocks(e);
        if let Some(b) = blocks.iter().rev().find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
            return Some((describe_tool(b), stamp(e)));
        }
        match blocks.last().and_then(|b| b.get("type")).and_then(Value::as_str) {
            Some("text") => return Some(("Writing…".into(), stamp(e))),
            Some("thinking" | "redacted_thinking") => return Some(("Thinking…".into(), stamp(e))),
            _ => {}
        }
    }
    None
}

/// The same for any agent Cue reads a log for: Claude Code's transcript, or Codex's rollout log.
pub fn activity_of(harness: &str, path: &str) -> Option<(String, u64)> {
    match harness {
        "codex" => codex_activity(path),
        _ => activity(path),
    }
}

/// A working Codex session's latest step in this turn, from its rollout log
/// (~/.codex/sessions/…/rollout-*.jsonl): the command or patch it's running, or its last words.
fn codex_activity(path: &str) -> Option<(String, u64)> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(256 * 1024))?;
    let entries: Vec<Value> = lines(&text).collect();
    for e in entries.iter().rev() {
        let p = e.get("payload").cloned().unwrap_or(Value::Null);
        let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
        match e.get("type").and_then(Value::as_str) {
            // This turn only: it starts at your message.
            Some("event_msg") if ptype == "user_message" || ptype == "task_started" => return None,
            Some("response_item") => match ptype {
                "function_call" => {
                    let args = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str::<Value>(a).ok()).unwrap_or(Value::Null);
                    return Some((describe_codex(p.get("name").and_then(Value::as_str).unwrap_or(""), &args, ""), stamp(e)));
                }
                "custom_tool_call" => {
                    return Some((describe_codex(p.get("name").and_then(Value::as_str).unwrap_or(""), &Value::Null, p.get("input").and_then(Value::as_str).unwrap_or("")), stamp(e)));
                }
                "function_call_output" | "custom_tool_call_output" | "reasoning" => return Some(("Thinking…".into(), stamp(e))),
                "message" if p.get("role").and_then(Value::as_str) == Some("assistant") => return Some(("Writing…".into(), stamp(e))),
                _ => {}
            },
            _ => {}
        }
    }
    None
}

/// When a transcript entry was written ("2026-10-04T14:08:39.507Z" → ms), 0 if it doesn't say.
pub(crate) fn stamp(e: &Value) -> u64 {
    e.get("timestamp").and_then(Value::as_str).and_then(iso_ms).unwrap_or(0)
}

pub(crate) fn iso_ms(t: &str) -> Option<u64> {
    let num = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (num(0, 4)?, num(5, 7)?, num(8, 10)?, num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let ms = t.get(19..).and_then(|r| r.strip_prefix('.')).map(|r| r.chars().take_while(char::is_ascii_digit).take(3).collect::<String>()).filter(|f| !f.is_empty())
        .map(|f| f.parse::<i64>().unwrap_or(0) * 10_i64.pow(3 - f.len() as u32)).unwrap_or(0);
    // Days since 1970-01-01 for a UTC date (Howard Hinnant's days_from_civil).
    let (y, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + d - 1;
    let days = era * 146097 + doe - 719468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60 * 1000 + sec * 1000 + ms).ok()
}

/// A Codex tool call in plain words.
fn describe_codex(name: &str, args: &Value, raw: &str) -> String {
    match name {
        "exec_command" | "shell" | "local_shell" | "container.exec" => {
            // `cmd` is a string; `command` an argv, often ["bash", "-lc", "<script>"].
            let cmd = args.get("cmd").and_then(Value::as_str).map(String::from).or_else(|| {
                let argv: Vec<&str> = args.get("command")?.as_array()?.iter().filter_map(Value::as_str).collect();
                Some(if argv.len() >= 3 && argv[1] == "-lc" { argv[2].to_string() } else { argv.join(" ") })
            });
            let first = cmd.unwrap_or_default().lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
            format!("Running: {}", cut(&first, 90))
        }
        "apply_patch" => {
            let file = raw.lines().find_map(|l| ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|p| l.strip_prefix(p)));
            match file {
                Some(f) => format!("Editing {}", f.trim().rsplit('/').next().unwrap_or(f)),
                None => "Editing files".into(),
            }
        }
        "update_plan" => "Updating its plan".into(),
        "web_search" => format!("Searching the web: {}", cut(args.get("query").and_then(Value::as_str).unwrap_or(""), 70)),
        "" => "Working".into(),
        n => format!("Using {n}"),
    }
}

/// A tool call in plain words: what it's doing and to what.
fn describe_tool(b: &Value) -> String {
    let name = b.get("name").and_then(Value::as_str).unwrap_or("a tool");
    let arg = |k: &str| b.pointer(&format!("/input/{k}")).and_then(Value::as_str).unwrap_or("").to_string();
    let file = |k: &str| arg(k).rsplit('/').next().unwrap_or("").to_string();
    let first = |t: String| t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
    match name {
        "Bash" => format!("Running: {}", cut(&first(arg("command")), 90)),
        "Edit" | "MultiEdit" | "Write" => format!("Editing {}", file("file_path")),
        "NotebookEdit" => format!("Editing {}", file("notebook_path")),
        "Read" => format!("Reading {}", file("file_path")),
        "Grep" => format!("Searching: {}", cut(&arg("pattern"), 60)),
        "Glob" => format!("Finding files: {}", cut(&arg("pattern"), 60)),
        "Agent" | "Task" => format!("Delegating: {}", cut(&arg("description"), 80)),
        "WebSearch" => format!("Searching the web: {}", cut(&arg("query"), 70)),
        "WebFetch" => format!("Reading {}", cut(&arg("url"), 70)),
        "TodoWrite" => "Updating its to-do list".into(),
        n if n.starts_with("mcp__") => format!("Using {}", n.trim_start_matches("mcp__").replace("__", ": ")),
        n => format!("Using {n}"),
    }
}

pub(crate) fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

/// How many times Claude Code has logged running "/name". A local command such as /rename logs a
/// "local_command" entry; /compact logs what you typed ("/compact") the moment it starts, and its own
/// record only when it's done, a minute later. An unknown command ("/reomte") logs nothing, so a
/// count that doesn't move means it didn't run.
pub fn command_count(path: &str, name: &str) -> usize {
    let Some(text) = read_from(path, tail_offset(path)) else { return 0 };
    let tag = format!("<command-name>/{name}</command-name>");
    let typed = |c: &str| c.strip_prefix('/').and_then(|t| t.split_whitespace().next()) == Some(name);
    lines(&text)
        .filter(|e| match (e.get("type").and_then(Value::as_str), e.get("subtype").and_then(Value::as_str)) {
            (Some("system"), Some("local_command")) => e.get("content").and_then(Value::as_str).is_some_and(|c| c.contains(&tag)),
            (Some("user"), _) => e.pointer("/message/content").and_then(Value::as_str).is_some_and(typed),
            _ => false,
        })
        .count()
}

/// Who sent a prompt, as Claude records it in `origin`.
#[derive(Debug, PartialEq)]
pub enum Sender {
    /// You: typed, queued, or a suggestion you accepted. Also any transcript too old to say.
    Human,
    /// Another agent session messaged this one: Claude's name for it ("cue-3e") and its process.
    Peer { name: String, pid: Option<i32>, body: String },
    /// Claude Code's own plumbing: a background agent's task-notification, a subagent's hand-back.
    Harness,
}

/// Who sent prompt `prompt_id`; None until Claude has written it to the transcript.
pub fn prompt_sender(path: &str, prompt_id: &str) -> Option<Sender> {
    let text = read_from(path, tail_offset(path))?;
    // The prompt and the tool results that follow it all carry its promptId; only the prompt says
    // who sent it. A transcript too old to carry `origin` at all counts as you.
    let mine: Vec<Value> = lines(&text)
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("user") && e.get("promptId").and_then(Value::as_str) == Some(prompt_id))
        .collect();
    let e = mine.iter().find(|e| e.get("origin").is_some()).or(mine.first())?;
    let o = |k: &str| e.pointer(&format!("/origin/{k}")).cloned().unwrap_or(Value::Null);
    Some(match o("kind").as_str() {
        None | Some("human") => Sender::Human,
        Some("peer") if o("handback").as_bool() != Some(true) => Sender::Peer {
            name: o("name").as_str().unwrap_or("").to_string(),
            pid: o("verifiedPeerPid").as_i64().map(|p| p as i32),
            body: o("body").as_str().unwrap_or("").to_string(),
        },
        _ => Sender::Harness,
    })
}

/// What a session is about, from its own transcript, no model: Claude Code's AI title (set early in
/// a session, never updated) and your latest request (`last-prompt`). Claude re-writes both every
/// turn, so the last 256 KB has them; only those lines are parsed.
pub fn about(path: &str) -> (String, String) {
    let from = std::fs::metadata(path).map(|m| m.len().saturating_sub(256 * 1024)).unwrap_or(0);
    let Some(tail) = read_from(path, from) else { return (String::new(), String::new()) };
    let last = |kind: &str, key: &str| {
        tail.lines()
            .rev()
            .filter(|l| l.contains(kind))
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find_map(|e| e.get(key).and_then(Value::as_str).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()))
            .unwrap_or_default()
    };
    (last("\"type\":\"ai-title\"", "aiTitle"), last("\"type\":\"last-prompt\"", "lastPrompt"))
}

/// Whether `text` reached the session from Claude Code's own queue, not from you. A background
/// task that finishes while the session is busy is queued, then handed over when the turn ends as
/// a `queued_command` attachment: there is no prompt entry with the hook's promptId to read
/// `origin` from, so `prompt_sender` never finds one. The attachment carries the text and origin.
pub fn queued_from_harness(path: &str, text: &str) -> bool {
    let Some(tail) = read_from(path, tail_offset(path)) else { return false };
    let text = text.trim();
    !text.is_empty()
        && lines(&tail).any(|e| {
            let a = |k: &str| e.pointer(&format!("/attachment/{k}")).and_then(Value::as_str).map(str::to_string);
            e.get("type").and_then(Value::as_str) == Some("attachment")
                && a("type").as_deref() == Some("queued_command")
                && a("prompt").is_some_and(|p| p.trim() == text)
                && a("origin/kind").is_some_and(|k| k != "human")
        })
}

/// Text only Claude Code itself sends: a background task's `<task-notification>`.
pub fn harness_text(text: &str) -> bool {
    text.trim_start().starts_with("<task-notification>")
}

/// A Claude Code session's transcript, wherever its folder is: ~/.claude/projects/<folder>/<id>.jsonl.
pub fn find_claude(session_id: &str) -> Option<String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let home = std::env::var("HOME").ok()?;
    std::fs::read_dir(format!("{home}/.claude/projects")).ok()?.flatten().map(|d| d.path().join(format!("{session_id}.jsonl"))).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

/// The conversation as Cue's chat shows it, from the transcript: your prompts, and the agent's last
/// text reply of each turn (not its tool calls or what it said between steps). Only what's before
/// `before_ms`, the last `limit` of it, oldest first. For what's older than Cue's own log (a session
/// from before Cue, or one you resumed): read only when you ask for it, from the start of the file,
/// stopping at `before_ms`.
pub fn conversation_before(path: &str, before_ms: u64, limit: usize) -> Vec<Value> {
    use std::io::BufRead;
    let Ok(f) = File::open(path) else { return vec![] };
    let mut out: Vec<Value> = vec![];
    let mut reply: Option<(u64, String)> = None;   // the turn's latest text reply so far
    let msg = |at: u64, role: &str, text: &str| serde_json::json!({ "at_ms": at, "role": role, "text": cut(text.trim(), 6000), "images": [] });
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        let Ok(e) = serde_json::from_str::<Value>(&line) else { continue };
        let at = stamp(&e);
        if at >= before_ms {
            break;
        }
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) || e.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let text: String = content_blocks(&e).iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n\n");
        // Slash-command plumbing, notices and "[Request interrupted…]" aren't conversation.
        if text.trim().is_empty() || text.starts_with('<') || text.starts_with("[Request interrupted") {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("assistant") => reply = Some((at, text)),
            Some("user") => {
                if let Some((t, r)) = reply.take() {
                    out.push(msg(t, "agent", &r));
                }
                out.push(msg(at, "you", &text));
            }
            _ => {}
        }
    }
    if let Some((t, r)) = reply {
        out.push(msg(t, "agent", &r));
    }
    let skip = out.len().saturating_sub(limit);
    out.into_iter().skip(skip).collect()
}

/// Where a message you queued for a busy Claude Code session stands, by its queue log: still in the
/// queue, taken into the running turn between two steps, or taken as the next turn's prompt (when).
#[derive(Debug, PartialEq)]
pub enum Queued {
    Waiting,
    Absorbed,
    Taken(Option<u64>),
}

/// The queue log says `enqueue` (with the text), then `remove` (reason "absorbed_mid_turn", with the
/// text) or `dequeue` (the oldest, no text). None: the log doesn't mention this message.
/// Whether `got` (what the agent received) is the message `sent`, perhaps with image paths after it.
/// Only the start is compared, with spacing ignored: a long paste typed into a terminal comes back
/// with its line breaks, tabs or trailing spaces changed, and must still count as the same message.
pub fn is_message(got: &str, sent: &str) -> bool {
    let start = |t: &str| t.chars().filter(|c| !c.is_whitespace()).take(300).collect::<String>();
    let sent = start(sent);
    !sent.is_empty() && start(got).starts_with(&sent)
}

pub fn queued_state(path: &str, text: &str) -> Option<Queued> {
    if text.trim().is_empty() {
        return None;
    }
    let len = std::fs::metadata(path).ok()?.len();
    let log = read_from(path, len.saturating_sub(256 * 1024))?;
    let mine = |c: &str| is_message(c, text);
    let (mut pending, mut last): (Vec<String>, Option<Queued>) = (vec![], None);
    for e in lines(&log) {
        if e.get("type").and_then(Value::as_str) != Some("queue-operation") {
            continue;
        }
        let content = e.get("content").and_then(Value::as_str).unwrap_or("").to_string();
        match e.get("operation").and_then(Value::as_str) {
            Some("enqueue") => {
                if mine(&content) {
                    last = Some(Queued::Waiting);
                }
                pending.push(content);
            }
            Some("remove") => {
                if let Some(i) = pending.iter().position(|c| *c == content) {
                    pending.remove(i);
                }
                if mine(&content) {
                    last = Some(Queued::Absorbed);
                }
            }
            Some("dequeue") if !pending.is_empty() => {
                if mine(&pending.remove(0)) {
                    last = Some(Queued::Taken(e.get("timestamp").and_then(Value::as_str).and_then(iso_ms)));
                }
            }
            _ => {}
        }
    }
    last
}

/// Where a message you queued for a busy session stands, from its agent's own log: Claude Code's
/// queue log, or Codex's rollout. None: no log that says (Pi reads it only once its turn is over).
pub fn read_state(harness: &str, path: &str, text: &str, sent_ms: u64) -> Option<Queued> {
    match harness {
        "claude" => queued_state(path, text),
        "codex" => codex_queued(path, text, sent_ms),
        _ => None,
    }
}

/// Codex logs each message it takes as a `user_message`: right after `task_started` when it's the
/// next turn's prompt, mid-turn when it was steered into the running turn. Only messages since you
/// sent it count (an older "yes" isn't this one).
fn codex_queued(path: &str, text: &str, sent_ms: u64) -> Option<Queued> {
    let len = std::fs::metadata(path).ok()?.len();
    let log = read_from(path, len.saturating_sub(512 * 1024))?;
    let (mut prev, mut state) = (String::new(), Queued::Waiting);
    for e in lines(&log).filter(|e| e.get("type").and_then(Value::as_str) == Some("event_msg")) {
        let p = &e["payload"];
        let kind = p.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let at = e.get("timestamp").and_then(Value::as_str).and_then(iso_ms);
        if kind == "user_message" && at.map_or(true, |a| a + 5000 >= sent_ms) && is_message(p.get("message").and_then(Value::as_str).unwrap_or(""), text) {
            state = if prev == "task_started" { Queued::Taken(at) } else { Queued::Absorbed };
        }
        if kind != "token_count" {
            prev = kind;
        }
    }
    Some(state)
}

/// A turn that was interrupted (Esc: "[Request interrupted by user]") with nothing since: Claude Code
/// sends no Stop for that, so the session would look busy while it waits at "What should Claude do
/// instead?". Returns when.
pub fn interrupted_at(path: &str) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(64 * 1024))?;
    let mut at = None;
    for e in lines(&text) {
        if !matches!(e.get("type").and_then(Value::as_str), Some("user" | "assistant")) || e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let said = content_blocks(&e).iter().any(|b| b.get("text").and_then(Value::as_str).is_some_and(|t| t.starts_with("[Request interrupted by user")));
        at = (e.get("type").and_then(Value::as_str) == Some("user") && said).then(|| stamp(&e)).filter(|t| *t > 0);
    }
    at
}

/// Whether a compaction started at `since_ms` has finished: Claude Code writes the command, then (when the
/// summary is done, or it couldn't compact) a `<local-command-stdout>` / `-stderr` entry after it. One it
/// started itself (context full) has no command: its `compact_boundary` marks the end.
pub fn compact_finished(path: &str, since_ms: u64) -> bool {
    let Some(text) = std::fs::metadata(path).ok().and_then(|m| read_from(path, m.len().saturating_sub(256 * 1024))) else { return false };
    let mut typed = false;
    for e in lines(&text) {
        if stamp(&e) + 2000 < since_ms {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("system") if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => return true,
            Some("user") => {}
            _ => continue,
        }
        let said = content_blocks(&e).iter().filter_map(|b| b.get("text").and_then(Value::as_str).map(String::from)).collect::<String>();
        if said.contains("<command-name>/compact</command-name>") {
            typed = true;
        } else if typed && (said.contains("<local-command-stdout>") || said.contains("<local-command-stderr>")) {
            return true;
        }
    }
    false
}

/// A turn that has ended, by the transcript: Claude Code writes a `turn_duration` entry once a turn is
/// really over (after its Stop hooks, and not when one sends it back to work). Returns when, and the
/// turn's last text reply, if nothing has happened since. Catches a Stop hook that never reached Cue
/// (Cue was restarting at that moment).
pub fn turn_ended(path: &str) -> Option<(u64, String)> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(256 * 1024))?;
    let (mut ended, mut reply, mut last_reply) = (None, String::new(), String::new());
    for e in lines(&text) {
        match e.get("type").and_then(Value::as_str) {
            Some("system") if e.get("subtype").and_then(Value::as_str) == Some("turn_duration") => {
                ended = Some(stamp(&e));
                last_reply = reply.clone();
            }
            Some("user" | "assistant") if e.get("isSidechain").and_then(Value::as_bool) != Some(true) => {
                ended = None; // a new turn (or more of this one) since
                if e.get("type").and_then(Value::as_str) == Some("assistant") {
                    if let Some(t) = content_blocks(&e).iter().filter_map(|b| (b.get("type").and_then(Value::as_str) == Some("text")).then(|| b.get("text").and_then(Value::as_str).unwrap_or(""))).filter(|t| !t.trim().is_empty()).last() {
                        reply = t.to_string();
                    }
                }
            }
            _ => {}
        }
    }
    ended.filter(|t| *t > 0).map(|t| (t, last_reply))
}

/// The last `n` human-readable turns: your prompts and Claude's text replies (no tool noise).
pub fn recent_context(path: &str, n: usize) -> Vec<Ctx> {
    let Some(text) = read_from(path, tail_offset(path)) else { return vec![] };
    let mut out: Vec<Ctx> = Vec::new();
    for entry in lines(&text) {
        let role = match entry.get("type").and_then(Value::as_str) {
            Some("assistant") => "assistant",
            Some("user") => "user",
            _ => continue,
        };
        if entry.get("isMeta").and_then(Value::as_bool) == Some(true)
            || entry.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        for b in content_blocks(&entry) {
            if b.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let t = b.get("text").and_then(Value::as_str).unwrap_or("");
            // Slash-command plumbing and system reminders aren't conversation.
            if t.trim().is_empty() || t.starts_with('<') {
                continue;
            }
            out.push(Ctx { role: role.into(), text: clip(t) });
        }
    }
    let skip = out.len().saturating_sub(n);
    out.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_transcript(lines: &[Value]) -> tempfile_path::TempPath {
        let p = tempfile_path::TempPath::new();
        let mut f = File::create(&p.0).unwrap();
        for l in lines {
            writeln!(f, "{}", l).unwrap();
        }
        p
    }

    #[test]
    fn the_tool_call_a_terminal_prompt_is_about_is_the_newest_without_a_result() {
        let call = |id: &str, cmd: &str| json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":cmd}}]}});
        let result = |id: &str| json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}});
        let p = write_transcript(&[call("a", "ls"), result("a"), call("b", "curl -sI https://example.com"), call("c", "echo x"), result("c")]);
        let (id, name, input) = waiting_tool_use(&p.0).unwrap();
        assert_eq!((id.as_str(), name.as_str(), input["command"].as_str().unwrap()), ("b", "Bash", "curl -sI https://example.com"));
        let done = write_transcript(&[call("a", "ls"), result("a")]);
        assert!(waiting_tool_use(&done.0).is_none(), "every call answered: nothing waiting");
    }

    #[test]
    fn the_conversation_before_a_time_is_prompts_and_each_turns_last_reply() {
        let at = |m: u32| format!("2026-10-04T08:{m:02}:00.000Z");
        let p = write_transcript(&[
            json!({"type":"user","timestamp":at(1),"message":{"content":"fix the bar"}}),
            json!({"type":"assistant","timestamp":at(2),"message":{"content":[{"type":"text","text":"Looking at it."},{"type":"tool_use","name":"Read","input":{}}]}}),
            json!({"type":"user","timestamp":at(3),"message":{"content":[{"type":"tool_result","content":"…"}]}}),
            json!({"type":"assistant","timestamp":at(4),"message":{"content":[{"type":"text","text":"Fixed: the bar now fills."}]}}),
            json!({"type":"user","timestamp":at(5),"isMeta":true,"message":{"content":"<local-command-stdout>x</local-command-stdout>"}}),
            json!({"type":"user","timestamp":at(6),"message":{"content":"thanks, now commit"}}),
            json!({"type":"assistant","timestamp":at(7),"message":{"content":[{"type":"text","text":"Committed."}]}}),
            json!({"type":"user","timestamp":at(9),"message":{"content":"later message"}}),
        ]);
        let got = |before: &str, limit| conversation_before(&p.0, iso_ms(before).unwrap(), limit).iter().map(|e| format!("{}: {}", e["role"].as_str().unwrap(), e["text"].as_str().unwrap())).collect::<Vec<_>>();
        assert_eq!(got(&at(8), 10), ["you: fix the bar", "agent: Fixed: the bar now fills.", "you: thanks, now commit", "agent: Committed."]);
        assert_eq!(got(&at(8), 2), ["you: thanks, now commit", "agent: Committed."], "the last `limit` before that time");
        assert_eq!(got(&at(1), 10), Vec::<String>::new());
    }

    #[test]
    fn an_interrupt_counts_only_while_nothing_follows_it() {
        let stop = json!({"type":"user","timestamp":"2026-10-04T19:47:22.000Z","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}});
        let work = json!({"type":"assistant","timestamp":"2026-10-04T19:47:20.000Z","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}});
        let p = write_transcript(&[work.clone(), stop.clone(), json!({"type":"file-history-snapshot"})]);
        assert_eq!(interrupted_at(&p.0), iso_ms("2026-10-04T19:47:22.000Z"));
        // You said something after it (or it went on): no longer interrupted.
        let p = write_transcript(&[work.clone(), stop.clone(), json!({"type":"user","timestamp":"2026-10-04T19:50:27.000Z","message":{"content":"continue"}})]);
        assert_eq!(interrupted_at(&p.0), None);
        let p = write_transcript(&[work]);
        assert_eq!(interrupted_at(&p.0), None);
    }

    #[test]
    fn follows_a_queued_message_through_the_queue_log() {
        let op = |op: &str, c: Option<&str>| match c {
            Some(c) => json!({"type":"queue-operation","operation":op,"content":c}),
            None => json!({"type":"queue-operation","operation":op}),
        };
        let p = write_transcript(&[op("enqueue", Some("is it done?"))]);
        assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Waiting));
        // Taken into the running turn between steps: Esc now would interrupt the turn that has it.
        let p = write_transcript(&[op("enqueue", Some("is it done?")), json!({"type":"queue-operation","operation":"remove","content":"is it done?","reason":"absorbed_mid_turn"})]);
        assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Absorbed));
        // dequeue takes the oldest: here someone else's first, then ours.
        let p = write_transcript(&[op("enqueue", Some("<task-notification>")), op("enqueue", Some("is it done? /tmp/a.png")), op("dequeue", None)]);
        assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Waiting));
        let p = write_transcript(&[op("enqueue", Some("<task-notification>")), op("enqueue", Some("is it done? /tmp/a.png")), op("dequeue", None), op("dequeue", None)]);
        assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Taken(None)));
        assert_eq!(queued_state(&p.0, "something else"), None);
        // A long paste comes back with its spacing changed: still the same message.
        let long = format!("first line\n\n{}\tend", "word ".repeat(1200));
        let p = write_transcript(&[op("enqueue", Some(&long.replace('\n', "\r\n").replace('\t', "    "))), json!({"type":"queue-operation","operation":"remove","content":long.replace('\n', "\r\n").replace('\t', "    "),"reason":"absorbed_mid_turn"})]);
        assert_eq!(queued_state(&p.0, &long), Some(Queued::Absorbed));
    }

    #[test]
    fn follows_a_message_queued_for_codex_through_its_rollout() {
        let ev = |t: &str, kind: &str, msg: Option<&str>| {
            let mut p = json!({"type": kind});
            if let Some(m) = msg { p["message"] = json!(m); }
            json!({"timestamp": format!("2026-10-05T06:{t}Z"), "type":"event_msg", "payload": p})
        };
        let sent = iso_ms("2026-10-05T06:40:00.000Z").unwrap();
        let start = [ev("39:00.000", "task_started", None), ev("39:00.010", "user_message", Some("fix it")), ev("39:30.000", "agent_message", Some("on it"))];
        let p = write_transcript(&start);
        assert_eq!(read_state("codex", &p.0, "fix it", sent), Some(Queued::Waiting), "the same words, before you sent this one");
        // Steered into the running turn.
        let mut l = start.to_vec();
        l.push(ev("40:01.000", "user_message", Some("fix it\n")));
        assert_eq!(read_state("codex", &write_transcript(&l).0, "fix it", sent), Some(Queued::Absorbed));
        // Or taken as the next turn's prompt.
        let mut l = start.to_vec();
        l.extend([ev("40:05.000", "task_complete", None), ev("40:05.100", "task_started", None), ev("40:05.200", "token_count", None), ev("40:05.300", "user_message", Some("fix it"))]);
        assert_eq!(read_state("codex", &write_transcript(&l).0, "fix it", sent), Some(Queued::Taken(iso_ms("2026-10-05T06:40:05.300Z"))));
        assert_eq!(read_state("pi", &p.0, "fix it", sent), None);
    }

    #[test]
    fn a_turn_has_ended_only_when_nothing_follows_its_turn_duration() {
        let reply = json!({"type":"assistant","timestamp":"2026-10-04T13:35:00.000Z","message":{"content":[{"type":"text","text":"Done: header moved."}]}});
        let end = json!({"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T13:35:01.000Z"});
        let p = write_transcript(&[json!({"type":"user","message":{"content":"move the header"}}), reply.clone(), end.clone()]);
        assert_eq!(turn_ended(&p.0), Some((iso_ms("2026-10-04T13:35:01.000Z").unwrap(), "Done: header moved.".to_string())));
        // A Stop hook sent it back to work, or you sent the next message: not ended.
        let p = write_transcript(&[reply.clone(), end.clone(), json!({"type":"user","message":{"content":"next"}})]);
        assert_eq!(turn_ended(&p.0), None);
        let p = write_transcript(&[json!({"type":"user","message":{"content":"go"}}), reply]);
        assert_eq!(turn_ended(&p.0), None);
    }

    #[test]
    fn compact_is_finished_once_claude_code_reports_it() {
        let at = |t: &str| format!("2026-10-05T13:{t}Z");
        let typed = json!({"type":"user","timestamp":at("02:30.869"),"message":{"content":"<command-name>/compact</command-name>\n<command-message>compact</command-message>"}});
        let since = iso_ms(&at("02:30.500")).unwrap();
        let p = write_transcript(&[json!({"type":"user","timestamp":at("01:00.000"),"message":{"content":"<local-command-stdout>an older one</local-command-stdout>"}}), typed.clone()]);
        assert!(!compact_finished(&p.0, since), "typed, still summarizing");
        // One it started itself has no command, only the boundary when it's done.
        let p2 = write_transcript(&[json!({"type":"system","subtype":"compact_boundary","timestamp":at("03:04.877")})]);
        assert!(compact_finished(&p2.0, since));
        let done = json!({"type":"user","timestamp":at("03:04.976"),"message":{"content":"<local-command-stdout>Compacted (ctrl+o to see full summary)</local-command-stdout>"}});
        let p = write_transcript(&[typed.clone(), json!({"type":"system","subtype":"compact_boundary","timestamp":at("03:04.877")}), done]);
        assert!(compact_finished(&p.0, since));
        // It couldn't compact: that's the end of it too.
        let p = write_transcript(&[typed, json!({"type":"user","timestamp":at("02:31.000"),"message":{"content":"<local-command-stderr>Error: Not enough messages to compact.</local-command-stderr>"}})]);
        assert!(compact_finished(&p.0, since));
    }

    // Tiny self-cleaning temp file so tests need no extra crate.
    mod tempfile_path {
        pub struct TempPath(pub String);
        impl TempPath {
            pub fn new() -> Self {
                static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Self(format!("{}/cue-test-{}-{}.jsonl", std::env::temp_dir().display(), std::process::id(), n))
            }
        }
        impl Drop for TempPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }

    fn tool_use(id: &str, cmd: &str) -> Value {
        json!({"type":"assistant","message":{"content":[
            {"type":"text","text":"Let me run it."},
            {"type":"tool_use","id":id,"name":"Bash","input":{"command":cmd,"description":"d"}}]}})
    }
    fn tool_result(id: &str) -> Value {
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}})
    }

    #[test]
    fn matches_the_right_tool_use_and_its_result() {
        let input = json!({"command":"touch b","description":"d"});
        let t = write_transcript(&[tool_use("t1", "touch a"), tool_result("t1"), tool_use("t2", "touch b")]);
        let id = find_tool_use_id(&t.0, 0, "Bash", &input).unwrap();
        assert_eq!(id, "t2");
        assert!(!has_result(&t.0, 0, "t2"), "t2 not answered yet");
        assert!(has_result(&t.0, 0, "t1"), "t1 answered");

        let mut f = std::fs::OpenOptions::new().append(true).open(&t.0).unwrap();
        writeln!(f, "{}", tool_result("t2")).unwrap();
        assert!(has_result(&t.0, 0, "t2"), "t2 answered after result appended");
    }

    #[test]
    fn no_match_when_input_differs() {
        let t = write_transcript(&[tool_use("t1", "touch a")]);
        assert!(find_tool_use_id(&t.0, 0, "Bash", &json!({"command":"touch zzz","description":"d"})).is_none());
    }

    #[test]
    fn context_keeps_prose_drops_tool_noise_and_meta() {
        let t = write_transcript(&[
            json!({"type":"user","message":{"content":"please fix the bug"}}),
            json!({"type":"user","isMeta":true,"message":{"content":"meta stuff"}}),
            json!({"type":"user","message":{"content":"<command-name>/model</command-name>"}}),
            tool_use("t1", "touch a"),
            tool_result("t1"),
        ]);
        let c = recent_context(&t.0, 5);
        assert_eq!(
            c,
            vec![
                Ctx { role: "user".into(), text: "please fix the bug".into() },
                Ctx { role: "assistant".into(), text: "Let me run it.".into() },
            ]
        );
    }

    #[test]
    fn tells_who_sent_each_prompt() {
        let t = write_transcript(&[
            json!({"type":"user","promptId":"p1","origin":{"kind":"human"},"message":{"content":"fix it"}}),
            json!({"type":"user","promptId":"p2","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>…"}}),
            json!({"type":"user","promptId":"p3","message":{"content":"from an older Claude"}}),
            json!({"type":"user","promptId":"p4","origin":{"kind":"peer","from":"uds:/tmp/cc-socks/37929.sock","verifiedPeerPid":37929,"name":"cue-3e","body":"done, go ahead"},"message":{"content":"…"}}),
            json!({"type":"user","promptId":"p5","origin":{"kind":"peer","from":"a1","body":"report","handback":true},"message":{"content":"…"}}),
            // The tool results after a prompt share its promptId but carry no origin.
            json!({"type":"user","promptId":"p4","message":{"content":[{"type":"tool_result","tool_use_id":"t9","content":"ok"}]}}),
        ]);
        assert_eq!(prompt_sender(&t.0, "p1"), Some(Sender::Human));
        assert_eq!(prompt_sender(&t.0, "p2"), Some(Sender::Harness));
        assert_eq!(prompt_sender(&t.0, "p3"), Some(Sender::Human));
        assert_eq!(prompt_sender(&t.0, "p4"), Some(Sender::Peer { name: "cue-3e".into(), pid: Some(37929), body: "done, go ahead".into() }));
        assert_eq!(prompt_sender(&t.0, "p5"), Some(Sender::Harness));
        assert_eq!(prompt_sender(&t.0, "not-written-yet"), None);
    }

    #[test]
    fn about_reads_the_ai_title_and_the_last_prompt() {
        let t = write_transcript(&[
            json!({"type":"ai-title","aiTitle":"Old title","sessionId":"s"}),
            json!({"type":"user","message":{"content":"first ask"}}),
            json!({"type":"last-prompt","lastPrompt":"first ask","sessionId":"s"}),
            json!({"type":"ai-title","aiTitle":"Agent decision notification system","sessionId":"s"}),
            json!({"type":"last-prompt","lastPrompt":"see it looks odd on sides of keyboard top","sessionId":"s"}),
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"\"type\":\"ai-title\" mentioned in prose"}]}}),
        ]);
        assert_eq!(about(&t.0), ("Agent decision notification system".into(), "see it looks odd on sides of keyboard top".into()));
        let empty = write_transcript(&[json!({"type":"user","message":{"content":"hi"}})]);
        assert_eq!(about(&empty.0), (String::new(), String::new()));
    }

    #[test]
    fn a_task_notification_handed_over_from_the_queue_is_not_yours() {
        // A background task finished while the session was busy: no prompt entry, an attachment instead.
        let note = "<task-notification>\n<task-id>bgkpah4ly</task-id>\n<status>completed</status>\n</task-notification>";
        let t = write_transcript(&[
            json!({"type":"queue-operation","operation":"enqueue","content":note}),
            json!({"type":"queue-operation","operation":"remove","content":note}),
            json!({"type":"attachment","attachment":{"type":"queued_command","prompt":note,"commandMode":"task-notification","origin":{"kind":"task-notification","producer":"session-task"}}}),
            json!({"type":"attachment","attachment":{"type":"queued_command","prompt":"also check the logs","commandMode":"prompt","origin":{"kind":"human"}}}),
        ]);
        assert_eq!(prompt_sender(&t.0, "p-hook"), None, "no prompt entry for it");
        assert!(queued_from_harness(&t.0, note));
        assert!(queued_from_harness(&t.0, &format!("{note}\n")), "the hook's copy may differ in trailing space");
        assert!(!queued_from_harness(&t.0, "also check the logs"), "something you queued while it worked is yours");
        assert!(!queued_from_harness(&t.0, "never queued"));
        assert!(!queued_from_harness(&t.0, ""));
    }

    #[test]
    fn says_what_a_working_session_is_doing() {
        let tool = |name: &str, input: Value| json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":name,"input":input}]}});
        let t = write_transcript(&[
            json!({"type":"user","message":{"content":"fix it"}}),
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"## Looking at the hub first\nthen the UI"}]}}),
            tool("Bash", json!({"command":"cd src-tauri && cargo test\n"})),
        ]);
        assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Running: cd src-tauri && cargo test"));
        let t = write_transcript(&[tool("Edit", json!({"file_path":"/x/ui/app.js"}))]);
        assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Editing app.js"));
        // Its words aren't shown ("It works live" isn't what it's doing): just that it's writing.
        let t = write_transcript(&[json!({"type":"assistant","message":{"content":[{"type":"text","text":"It works live."}]}})]);
        assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Writing…"));
        // A step that finished: it's thinking about the next one, not still running the last.
        let t = write_transcript(&[tool("Bash", json!({"command":"cargo test"})), json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}})]);
        assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Thinking…"));
        assert_eq!(iso_ms("2026-10-04T14:08:39.507Z"), Some(1_791_122_919_507));
        assert_eq!(describe_tool(&json!({"name":"mcp__claude-in-chrome__navigate"})), "Using claude-in-chrome: navigate");
        // Just after a new prompt: nothing from the last turn.
        let t = write_transcript(&[tool("Bash", json!({"command":"old"})), json!({"type":"user","message":{"content":"next task"}})]);
        assert_eq!(activity(&t.0), None);
    }

    #[test]
    fn says_what_a_codex_session_is_doing() {
        let call = |name: &str, args: Value| json!({"type":"response_item","payload":{"type":"function_call","name":name,"arguments":args.to_string()}});
        let t = write_transcript(&[
            json!({"type":"event_msg","payload":{"type":"user_message","message":"fix it"}}),
            call("exec_command", json!({"cmd":"rg --files\n","workdir":"/x"})),
        ]);
        assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Running: rg --files"));
        let t = write_transcript(&[call("shell", json!({"command":["bash","-lc","cargo test"]}))]);
        assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Running: cargo test"));
        let t = write_transcript(&[json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /x/src/hub.rs\n@@"}})]);
        assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Editing hub.rs"));
        let t = write_transcript(&[call("exec_command", json!({"cmd":"ls"})), json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"ok"}})]);
        assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Thinking…"));
        // A new turn: nothing from the last one.
        let t = write_transcript(&[call("exec_command", json!({"cmd":"old"})), json!({"type":"event_msg","payload":{"type":"task_started"}})]);
        assert_eq!(activity_of("codex", &t.0), None);
    }

    #[test]
    fn counts_commands_that_ran() {
        let t = write_transcript(&[
            json!({"type":"system","subtype":"local_command","content":"<command-name>/rename</command-name>\n<command-args>cue-2</command-args>"}),
            json!({"type":"system","subtype":"local_command","content":"<command-name>/rename</command-name>\n<command-args>cue-ios</command-args>"}),
            json!({"type":"user","message":{"content":"<command-name>/rename</command-name>"}}),
        ]);
        assert_eq!(command_count(&t.0, "rename"), 2);
        assert_eq!(command_count(&t.0, "reomte"), 0);
        // /compact: what you typed is logged as it starts.
        let t = write_transcript(&[json!({"type":"user","message":{"role":"user","content":"/compact"}}), json!({"type":"user","message":{"content":"/compacting is a word"}})]);
        assert_eq!(command_count(&t.0, "compact"), 1);
    }

    #[test]
    fn reading_mid_file_drops_the_partial_line() {
        let t = write_transcript(&[tool_use("t1", "touch a"), tool_result("t1")]);
        // Offset 5 lands inside line 1: it must be skipped, not mis-parsed.
        assert!(find_tool_use_id(&t.0, 5, "Bash", &json!({"command":"touch a","description":"d"})).is_none());
        assert!(has_result(&t.0, 5, "t1"));
    }

    #[test]
    fn a_task_notification_is_never_yours() {
        assert!(harness_text("<task-notification>\n<task-id>ba2xv14y3</task-id>\n<status>failed</status>\n</task-notification>"));
        assert!(harness_text("  \n<task-notification><task-id>x</task-id></task-notification>"));
        assert!(!harness_text("why is <task-notification> showing in Cue?"));
    }
}
