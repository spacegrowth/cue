//! A Codex rollout's changes (split from desk/hub/theirs/theirs.rs): its patches by their text, each hunk an edit, and what its
//! shell commands write, each in the folder it ran in (#248).

use crate::desk::hub::theirs::theirs::TheirEdit;
use crate::desk::hub::theirs::theirs_log::{change, resolve, SoFar};
use serde_json::Value;

/// What a Codex rollout line must say to be read for changes (the rest is skipped unparsed, but for a patch's
/// outcome, found by its call id).
pub(crate) const CODEX_MARKS: [&str; 10] = ["FileChange", "CommandExecution", "apply_patch", "patch_apply", "session_meta", "turn_context", "exec_command", "\"name\":\"shell", "\"name\":\"container.exec\"", "local_shell_call"];

/// One Codex rollout line (#248): its patches (a FileChange item, a patch_apply event, an apply_patch call) by their
/// text, each hunk an edit, counted only when it worked; a file added, deleted or moved, and what its shell commands
/// write, as changes whose text isn't known. The folder is what its session_meta and turn_context lines say: a
/// relative path is taken there (a command's own folder, when it says one). Typing into a running command
/// (write_stdin) isn't read: what it writes isn't known.
pub(crate) fn codex_line(e: &Value, so: &mut SoFar) {
    let at = crate::logs::transcript::stamp(e);
    let p = e.get("payload").unwrap_or(&Value::Null);
    let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let dir = so.at.clone();
    match (e.get("type").and_then(Value::as_str).unwrap_or(""), ptype) {
        ("session_meta" | "turn_context", _) => {
            if let Some(c) = p.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
                so.at = c.to_string();
            }
        }
        ("event_msg", "item_completed") => {
            let item = p.get("item").unwrap_or(&Value::Null);
            match item.get("type").and_then(Value::as_str) {
                // A patch that failed or was declined changed nothing.
                Some("FileChange") if matches!(item.get("status").and_then(Value::as_str), None | Some("completed")) => {
                    so.edits.extend(codex_changes(item.get("changes").unwrap_or(&Value::Null), &dir, at));
                }
                Some("CommandExecution") => {
                    let here = folder_of_url(&s(item, "cwd")).unwrap_or(dir);
                    so.shell(&command_of(item.get("command").unwrap_or(&Value::Null)), &here, at);
                }
                _ => {}
            }
        }
        ("event_msg", "patch_apply_end") => {
            let worked = p.get("success").and_then(Value::as_bool) != Some(false);
            // The call it ends has its changes already; else they're its own.
            if !so.settled(&s(p, "call_id"), worked) && worked {
                so.edits.extend(codex_changes(p.get("changes").unwrap_or(&Value::Null), &dir, at));
            }
        }
        ("response_item", "function_call" | "custom_tool_call") => {
            let args: Value = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
            let here = args.get("workdir").and_then(Value::as_str).map(|w| resolve(&dir, w)).unwrap_or(dir);
            match p.get("name").and_then(Value::as_str).unwrap_or("") {
                "apply_patch" => {
                    let raw = p.get("input").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| s(&args, "input"));
                    let changes = patch_changes(&raw, &here, at);
                    so.held(Some(&s(p, "call_id")), changes);
                }
                "exec_command" => so.shell(&s(&args, "cmd"), &here, at),
                "shell" | "shell_command" | "local_shell" | "container.exec" => so.shell(&command_of(args.get("command").unwrap_or(&Value::Null)), &here, at),
                _ => {}
            }
        }
        ("response_item", "local_shell_call") => {
            let action = p.get("action").unwrap_or(&Value::Null);
            let here = action.get("working_directory").and_then(Value::as_str).map(|w| resolve(&dir, w)).unwrap_or(dir);
            so.shell(&command_of(action.get("command").unwrap_or(&Value::Null)), &here, at);
        }
        ("response_item", "function_call_output" | "custom_tool_call_output") => {
            so.settled(&s(p, "call_id"), patch_worked(p.get("output").unwrap_or(&Value::Null)));
        }
        _ => {}
    }
}

/// Whether an apply_patch call's output says it worked: its exit code 0, or "Success." (as its text says).
pub(crate) fn patch_worked(output: &Value) -> bool {
    let text = match output {
        Value::String(t) => t.clone(),
        Value::Array(a) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        v => v.to_string(),
    };
    if let Ok(j) = serde_json::from_str::<Value>(&text) {
        if let Some(code) = j.pointer("/metadata/exit_code").and_then(Value::as_i64) {
            return code == 0;
        }
    }
    let t = text.trim_start();
    t.starts_with("Exit code: 0") || t.starts_with("Success.") || t.contains("\"output\":\"Success.")
}

/// A Codex command as one line: a string as it is, an argv as its script (`zsh -lc "<script>"`) or its words.
pub(crate) fn command_of(v: &Value) -> String {
    if let Some(c) = v.as_str() {
        return c.to_string();
    }
    let argv: Vec<&str> = v.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if argv.len() >= 3 && matches!(argv[1], "-lc" | "-c") { argv[2].to_string() } else { argv.join(" ") }
}

/// A folder as a `file://` URL says it (its %-escapes put back), or a plain absolute path.
pub(crate) fn folder_of_url(u: &str) -> Option<String> {
    let path = u.strip_prefix("file://").unwrap_or(u);
    if !path.starts_with('/') {
        return None;
    }
    let (b, mut out, mut i) = (path.as_bytes(), Vec::new(), 0);
    while i < b.len() {
        match (b[i], path.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(x)) => {
                out.push(x);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// What a shell command run in `dir` writes, as changes whose text isn't known.
pub(crate) fn shell_changes(cmd: &str, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    crate::logs::shell_writes::writes(cmd).iter().map(|f| (resolve(dir, f), TheirEdit { guess: true, ..change(at, "", "", false, true) })).collect()
}

/// A Codex change list (by path: `{type, unified_diff, move_path}`, or `{update: {unified_diff, move_path}}` and
/// the like; or a list of `{path, kind, diff}`): an update's hunks as edits, anything else not known by its text.
pub(crate) fn codex_changes(changes: &Value, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    let listed: Vec<(String, &Value)> = match changes {
        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Value::Array(a) => a.iter().map(|c| (c.get("path").and_then(Value::as_str).unwrap_or("").to_string(), c)).collect(),
        _ => vec![],
    };
    let mut found: Vec<(String, TheirEdit)> = Vec::new();
    for (path, c) in listed.into_iter().filter(|(p, _)| !p.is_empty()) {
        let file = resolve(dir, &path);
        let kind = c.get("type").or_else(|| c.pointer("/kind/type")).and_then(Value::as_str).map(str::to_string)
            .or_else(|| c.as_object().and_then(|o| o.keys().find(|k| matches!(k.as_str(), "update" | "add" | "delete")).cloned()))
            .unwrap_or_default();
        let body = c.get(kind.as_str()).filter(|v| v.is_object()).unwrap_or(c);
        let moved = body.get("move_path").and_then(Value::as_str).filter(|m| !m.is_empty());
        let diff = body.get("unified_diff").or_else(|| body.get("diff")).and_then(Value::as_str);
        match (kind.as_str(), moved, diff) {
            ("update", None, Some(d)) => found.extend(hunks(d).into_iter().map(|(old, new)| (file.clone(), change(at, &old, &new, false, false)))),
            _ => {
                found.push((file, change(at, "", "", false, true)));
                if let Some(m) = moved {
                    found.push((resolve(dir, m), change(at, "", "", false, true)));
                }
            }
        }
    }
    found
}

/// An apply_patch patch's changes (paths taken in `dir`): an updated file's hunks as edits; a file added, deleted or
/// moved not known by its text.
pub(crate) fn patch_changes(patch: &str, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    let mut found: Vec<(String, TheirEdit)> = Vec::new();
    let mut updating: Option<(String, Vec<&str>)> = None;
    let done = |u: Option<(String, Vec<&str>)>, found: &mut Vec<(String, TheirEdit)>| {
        if let Some((file, body)) = u {
            found.extend(hunks(&body.join("\n")).into_iter().map(|(old, new)| (file.clone(), change(at, &old, &new, false, false))));
        }
    };
    for l in patch.lines() {
        if let Some(f) = l.strip_prefix("*** Update File: ") {
            done(updating.take(), &mut found);
            updating = Some((resolve(dir, f.trim()), Vec::new()));
        } else if let Some(f) = l.strip_prefix("*** Move to: ") {
            // Moved: neither end known by its text.
            if let Some((file, _)) = updating.take() {
                found.push((file, change(at, "", "", false, true)));
            }
            found.push((resolve(dir, f.trim()), change(at, "", "", false, true)));
        } else if let Some(f) = ["*** Add File: ", "*** Delete File: "].iter().find_map(|m| l.strip_prefix(m)) {
            done(updating.take(), &mut found);
            found.push((resolve(dir, f.trim()), change(at, "", "", false, true)));
        } else if let Some((_, body)) = updating.as_mut() {
            // "*** End of File" and the like end a hunk, as "@@" does.
            body.push(if l.starts_with("*** ") { "@@" } else { l });
        }
    }
    done(updating, &mut found);
    found
}

/// A diff's hunks (split at "@@"; a hunk's context and removed lines its old text, context and added lines its new):
/// each (old, new), whole lines, those that change nothing left out.
pub(crate) fn hunks(diff: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let (mut old, mut new): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let joined = |v: &[&str]| if v.is_empty() { String::new() } else { v.join("\n") + "\n" };
    let mut flush = |old: &mut Vec<&str>, new: &mut Vec<&str>| {
        if old != new {
            out.push((joined(old), joined(new)));
        }
        old.clear();
        new.clear();
    };
    for l in diff.lines() {
        if l.starts_with("@@") || l.starts_with("*** ") {
            flush(&mut old, &mut new);
            continue;
        }
        match l.as_bytes().first() {
            // "\ No newline at end of file"
            Some(b'\\') => {}
            Some(b'+') => new.push(&l[1..]),
            Some(b'-') => old.push(&l[1..]),
            Some(b' ') => {
                old.push(&l[1..]);
                new.push(&l[1..]);
            }
            // A blank line: an empty context line.
            _ => {
                old.push(l);
                new.push(l);
            }
        }
    }
    flush(&mut old, &mut new);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desk::hub::theirs::theirs_log::read_edits;

    /// (file, old, new, blind) of each change.
    fn shown(got: Vec<(String, TheirEdit)>) -> Vec<(String, String, String, bool)> {
        got.into_iter().map(|(f, e)| (f, e.old, e.new, e.blind)).collect()
    }

    /// A Codex apply_patch: each hunk of an updated file is an edit by its text (relative paths taken in the
    /// session's folder); an added, deleted or moved file is a change whose text isn't known (#248).
    #[test]
    fn a_codex_patch_reads_as_one_edit_per_hunk_and_an_added_or_moved_file_as_unknown_text() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@ fn a\n keep\n-old\n+new\n@@\n-gone\n*** Add File: b.rs\n+x\n*** Update File: c.rs\n*** Move to: d.rs\n@@\n-c\n+d\n*** End Patch";
        assert_eq!(shown(super::patch_changes(patch, "/r/", 1)), [
            ("/r/src/a.rs".into(), "keep\nold\n".into(), "keep\nnew\n".into(), false),
            ("/r/src/a.rs".into(), "gone\n".into(), "".into(), false),
            ("/r/b.rs".into(), "".into(), "".into(), true),
            ("/r/c.rs".into(), "".into(), "".into(), true),
            ("/r/d.rs".into(), "".into(), "".into(), true),
        ]);
    }

    /// The Codex app's rollout: a FileChange item's diff by its text, a command's writes taken in the folder it ran
    /// in (its file:// URL), and a relative path in the folder its session_meta says.
    #[test]
    fn a_codex_rollouts_file_changes_and_shell_writes_are_read_with_their_folders() {
        let mut so = super::SoFar::new("/start", "codex");
        let meta = serde_json::json!({ "timestamp": "2026-11-18T12:00:00Z", "type": "session_meta", "payload": { "cwd": "/repo" } });
        super::codex_line(&meta, &mut so);
        assert!(so.edits.is_empty());
        assert_eq!(so.at, "/repo");
        let change = serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "FileChange", "status": "completed", "changes": { "/repo/x.rs": { "type": "update", "unified_diff": "@@ -1,2 +1,3 @@\n a\n+b\n c\n", "move_path": null }, "y.rs": { "type": "add", "content": "z" } } } } });
        super::codex_line(&change, &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/repo/x.rs".into(), "a\nc\n".into(), "a\nb\nc\n".into(), false),
            ("/repo/y.rs".into(), "".into(), "".into(), true),
        ]);
        let shell = serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "CommandExecution", "command": ["/bin/zsh", "-lc", "echo hi > out.txt"], "cwd": "file:///other%20copy" } } });
        super::codex_line(&shell, &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [("/other copy/out.txt".into(), "".into(), "".into(), true)]);
        assert!(so.ran.contains_key("/other copy"), "it ran a command there");
    }

    /// A Codex log written to disk and read through the quick filter: every kind of shell call it may make is read
    /// (shell, shell_command, container.exec, local_shell_call, exec_command), each in its own folder (#248).
    #[test]
    fn every_codex_shell_call_is_read_through_the_quick_filter() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-cxsh-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("rollout.jsonl");
        let call = |name: &str, args: serde_json::Value| serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "response_item",
            "payload": { "type": "function_call", "name": name, "arguments": args.to_string(), "call_id": format!("c-{name}") } }).to_string();
        let lines = [
            serde_json::json!({ "timestamp": "2026-11-18T12:00:00Z", "type": "session_meta", "payload": { "cwd": "/repo" } }).to_string(),
            call("shell", serde_json::json!({ "command": ["bash", "-lc", "echo 1 > a.txt"], "workdir": "/repo" })),
            call("shell_command", serde_json::json!({ "command": "echo 2 > b.txt", "workdir": "/repo/sub" })),
            call("container.exec", serde_json::json!({ "command": ["bash", "-lc", "echo 3 > c.txt"] })),
            call("exec_command", serde_json::json!({ "cmd": "echo 4 > d.txt" })),
            serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "response_item", "payload": { "type": "local_shell_call",
                "action": { "type": "exec", "command": ["bash", "-lc", "echo 5 > e.txt"], "working_directory": "/repo" } } }).to_string(),
        ];
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        let wanted: Vec<String> = ["/repo/a.txt", "/repo/sub/b.txt", "/repo/c.txt", "/repo/d.txt", "/repo/e.txt"].iter().map(|s| s.to_string()).collect();
        let (edits, ran) = read_edits(&log.to_string_lossy(), "/repo", &wanted);
        assert_eq!(edits.into_iter().map(|(f, e)| (f, e.blind)).collect::<Vec<_>>(), wanted.iter().map(|f| (f.clone(), true)).collect::<Vec<_>>());
        assert!(ran.iter().any(|(d, _)| d == "/repo") && ran.iter().any(|(d, _)| d == "/repo/sub"), "{ran:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Codex patch that failed changed nothing: a FileChange item that failed, an apply_patch call whose output says
    /// it failed, a patch_apply_end that says so. One that worked counts, once its outcome is read (#248).
    #[test]
    fn a_codex_patch_that_failed_is_no_change() {
        let mut so = super::SoFar::new("/repo", "codex");
        let failed = serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "FileChange", "status": "failed", "changes": { "/repo/x.rs": { "type": "update", "unified_diff": "@@\n-a\n+b\n" } } } } });
        super::codex_line(&failed, &mut so);
        assert!(so.edits.is_empty(), "a failed FileChange");
        let patch = |id: &str| serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "response_item", "payload": { "type": "custom_tool_call",
            "name": "apply_patch", "call_id": id, "input": "*** Begin Patch\n*** Update File: x.rs\n@@\n-a\n+b\n*** End Patch" } });
        let output = |id: &str, text: &str| serde_json::json!({ "timestamp": "2026-11-18T12:00:03Z", "type": "response_item", "payload": { "type": "custom_tool_call_output", "call_id": id, "output": text } });
        super::codex_line(&patch("p1"), &mut so);
        assert!(so.edits.is_empty(), "not yet: its outcome isn't read");
        super::codex_line(&output("p1", "apply_patch verification failed: Failed to find expected lines"), &mut so);
        assert!(so.edits.is_empty() && so.pending.is_empty(), "its output says it failed");
        super::codex_line(&patch("p2"), &mut so);
        let end = serde_json::json!({ "timestamp": "2026-11-18T12:00:03Z", "type": "event_msg", "payload": { "type": "patch_apply_end", "call_id": "p2", "success": false, "changes": {} } });
        super::codex_line(&end, &mut so);
        assert!(so.edits.is_empty(), "patch_apply_end says it failed");
        super::codex_line(&patch("p3"), &mut so);
        super::codex_line(&output("p3", "Exit code: 0\nWall time: 0 seconds\nOutput:\nSuccess. Updated the following files:\nM x.rs\n"), &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [("/repo/x.rs".into(), "a\n".into(), "b\n".into(), false)]);
    }
}
