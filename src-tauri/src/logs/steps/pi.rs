//! Pi's log entries in the shape of Claude Code's transcript, and its models' context windows.

use serde_json::{json, Value};

/// One Pi log entry in the shape of Claude Code's transcript (so the steps read both alike): your
/// message, its words and tool calls, a tool's result. Pi's tool names map to Claude Code's.
pub(super) fn from_pi(e: &Value) -> Vec<Value> {
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
pub(super) fn pi_window(provider: &str, model: &str) -> Option<u64> {
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
