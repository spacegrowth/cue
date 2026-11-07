//! Every file a session (and its background agents) edited, read from its whole log.

use super::helper::helper_name;
use super::kind::{kind_of, translate, Kind};
use serde_json::Value;
use std::collections::HashMap;

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
