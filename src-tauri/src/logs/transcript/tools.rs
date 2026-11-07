//! Tool calls in the transcript: finding the one a request is about, whether it got its result, and
//! how many times a slash command ran.

use super::{content_blocks, lines, read_from, tail_offset};
use serde_json::Value;

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
    // The session's own calls: a subagent's (inline, in older logs) aren't what its terminal is asking about.
    for entry in lines(&text).filter(|e| e.get("isSidechain").and_then(Value::as_bool) != Some(true)) {
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

/// How many times Claude Code has logged running "/name". A local command such as /rename logs a
/// "local_command" entry; newer Claude Code (2.1.29x) logs it as a "user" entry whose content is the
/// same "<command-name>/model</command-name>" record; /compact logs what you typed ("/compact") the
/// moment it starts, and its own record only when it's done, a minute later. An unknown command
/// ("/reomte") logs nothing, so a count that doesn't move means it didn't run.
pub fn command_count(path: &str, name: &str) -> usize {
    let Some(text) = read_from(path, tail_offset(path)) else { return 0 };
    let tag = format!("<command-name>/{name}</command-name>");
    let typed = |c: &str| c.strip_prefix('/').and_then(|t| t.split_whitespace().next()) == Some(name);
    lines(&text)
        .filter(|e| match (e.get("type").and_then(Value::as_str), e.get("subtype").and_then(Value::as_str)) {
            (Some("system"), Some("local_command")) => e.get("content").and_then(Value::as_str).is_some_and(|c| c.contains(&tag)),
            (Some("user"), _) => e.pointer("/message/content").and_then(Value::as_str).is_some_and(|c| typed(c) || c.contains(&tag)),
            _ => false,
        })
        .count()
}
