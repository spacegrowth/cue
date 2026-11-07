//! An opened step: its full subject and its output or diff, read from the transcript on demand.

use super::kind::{kind_of, translate};
use super::step_line::{result_text, strip_ansi};
use super::{LINE_CHARS, START_BYTES};
use crate::logs::transcript::{content_blocks, cut};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

/// A step's output, opened: this many lines at most (the start and, mostly, the end).
const OUT_HEAD: usize = 40;
const OUT_TAIL: usize = 160;

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
        return edit_detail(full, extra);
    }
    // A read shows no file contents: the line already says how much it read.
    if tool == "Read" && !failed {
        return json!({ "full": full });
    }
    let output = if tool == "Bash" && !failed { bash_output(block, extra) } else { result_text(block) };
    let (out, skipped) = clip_lines(&output);
    // `gap_at`: where the skipped lines were (after the first OUT_HEAD).
    json!({ "full": full, "output": out, "skipped": skipped, "gap_at": OUT_HEAD })
}

/// An edit's diff, hunk by hunk (a new file: its lines, all added), cut at the end when long.
fn edit_detail(full: String, extra: &Value) -> Value {
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
    json!({ "full": full, "diff": diff, "skipped": cut_lines })
}

/// What a command printed: its output, then its errors (not the shell's own note about its folder);
/// the result's text when it printed nothing.
fn bash_output(block: &Value, extra: &Value) -> String {
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
}
