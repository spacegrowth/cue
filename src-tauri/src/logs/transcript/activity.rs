//! What a working session is doing right now, in plain words: Claude Code's from its transcript,
//! Codex's from its rollout log.

use super::{content_blocks, cut, lines, read_from, stamp};
use serde_json::Value;

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
        // A subagent's own lines (inline, in older logs): not this session's, and its prompt isn't yours.
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        // Only this turn: stop at your prompt (not at a tool's result or a meta note).
        let tool_result = content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
        if kind == Some("user") && !tool_result && e.get("isMeta").and_then(Value::as_bool) != Some(true) {
            return None;
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
pub(super) fn describe_tool(b: &Value) -> String {
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
