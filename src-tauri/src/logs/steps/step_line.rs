//! A tool call as a step line: what it did (its kind, verb and subject) and how it went.

use super::helper::{in_background, resumed_agent};
use super::model::Item;
use crate::logs::transcript::cut;
use serde_json::Value;

/// A tool call as a step line: its kind, the verb while running and once done, and its subject.
pub(super) fn step_of(b: &Value, at: u64) -> Item {
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
pub(super) fn without_cd(cmd: &str) -> String {
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
pub(super) fn result_text(block: &Value) -> String {
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
pub(super) fn outcome(tool: &str, block: &Value, extra: Option<&Value>) -> (String, bool) {
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

pub(super) fn strip_ansi(s: &str) -> String {
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
