//! Codex's rollout entries in the shape of Claude Code's transcript.

use serde_json::{json, Value};
use std::collections::HashMap;

/// One Codex rollout entry in the shape of Claude Code's transcript: your message (its `user_message`
/// event; the instructions Codex puts in as "user" messages aren't yours), its words, commands
/// (`exec_command`, `shell`) and patches (`apply_patch`), and their results. `write_stdin` (checking on
/// a command still running) is left out.
pub(super) fn from_codex(e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
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
        (Some("response_item"), "function_call" | "custom_tool_call") => codex_call(&p, ts, patches),
        (Some("response_item"), "function_call_output" | "custom_tool_call_output") => codex_output(&p, ts, patches),
        _ => vec![],
    }
}

/// A Codex tool call as Claude Code's tool_use: its commands as Bash, its patches as Edit (kept by call
/// id for their result), anything else by its own name.
fn codex_call(p: &Value, ts: Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
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

/// A Codex tool's output as Claude Code's tool_result: its exit code, what it printed, and a patch's diff.
fn codex_output(p: &Value, ts: Value, patches: &HashMap<String, String>) -> Vec<Value> {
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

/// The shell command a Codex app script runs, when that's all it is: `exec_command({"cmd": "…"})`.
pub(super) fn script_command(script: &str) -> Option<String> {
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
