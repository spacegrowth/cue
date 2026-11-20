//! Other sessions' changes, read from their logs (split from desk/hub/theirs/theirs.rs): each log read once and then only what
//! it adds, Claude Code's and Pi's lines (Codex's are in desk/hub/theirs/theirs_codex.rs), the newest changes kept (#248).

use crate::desk::hub::theirs::theirs::TheirEdit;
use crate::desk::hub::theirs::theirs_codex::{codex_line, shell_changes, CODEX_MARKS};
use crate::desk::hub::prelude::*;
use serde_json::Value;

/// What one log keeps of what it changed, newest kept: this many changes, this much of their text (#248).
pub(crate) const KEPT_EDITS: usize = 1000;

pub(crate) const KEPT_TEXT: usize = 256 * 1024;

/// What one log said so far: how far it was read (whole lines), the folder it was read in, whose log it is
/// ("claude", "codex" or "pi"), the folder its last line ran in, every change it made to a file (`by` left empty:
/// the session's name is put on when they're handed out; the newest KEPT_EDITS and KEPT_TEXT kept), when it last
/// ran a command in each folder, and the patches it sent whose outcome it hasn't said yet (by call id).
#[derive(Default)]
pub(crate) struct SoFar {
    pub(crate) len: u64,
    pub(crate) cwd: String,
    pub(crate) kind: &'static str,
    pub(crate) at: String,
    pub(crate) edits: Vec<(String, TheirEdit)>,
    pub(crate) ran: HashMap<String, u64>,
    pub(crate) pending: HashMap<String, Vec<(String, TheirEdit)>>,
}

impl SoFar {
    pub(crate) fn new(cwd: &str, kind: &'static str) -> SoFar {
        SoFar { cwd: cwd.to_string(), at: cwd.to_string(), kind, ..SoFar::default() }
    }

    /// A shell command `cmd` run in `dir` at `at`: what it writes, as changes whose text isn't known, and that it ran.
    pub(crate) fn shell(&mut self, cmd: &str, dir: &str, at: u64) {
        if cmd.trim().is_empty() {
            return;
        }
        self.edits.extend(shell_changes(cmd, dir, at));
        let last = self.ran.entry(dir.trim_end_matches('/').to_string()).or_default();
        *last = (*last).max(at);
    }

    /// A change that's only made once its call says it worked: kept by call id until then (none: made now).
    pub(crate) fn held(&mut self, id: Option<&str>, changes: Vec<(String, TheirEdit)>) {
        match id.filter(|i| !i.is_empty()) {
            // Some calls never say: a few are kept, not all.
            Some(i) if self.pending.len() < 64 => {
                self.pending.insert(i.to_string(), changes);
            }
            Some(_) => {}
            None => self.edits.extend(changes),
        }
    }

    /// Call `id` said whether it worked: its changes are made, or dropped.
    pub(crate) fn settled(&mut self, id: &str, worked: bool) -> bool {
        match self.pending.remove(id) {
            Some(changes) => {
                if worked {
                    self.edits.extend(changes);
                }
                true
            }
            None => false,
        }
    }

    /// Only the newest KEPT_EDITS changes and KEPT_TEXT of their text are kept.
    pub(crate) fn trim(&mut self) {
        let mut text = 0;
        let keep = self.edits.iter().rev().take_while(|(_, e)| {
            text += e.old.len() + e.new.len();
            text <= KEPT_TEXT
        }).count().min(KEPT_EDITS);
        let cut = self.edits.len() - keep;
        self.edits.drain(..cut);
    }
}

/// Each log's changes so far, by path, so a finish check every 10 s reads only what each log added since (#284):
/// reading every session's whole log each time cost seconds of CPU a minute. Logs no session has any more are dropped.
pub(crate) fn read_so_far() -> &'static std::sync::Mutex<std::collections::HashMap<String, SoFar>> {
    static SO_FAR: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, SoFar>>> = std::sync::OnceLock::new();
    SO_FAR.get_or_init(Default::default)
}

/// The changes `log` (a session working in `cwd`) made to `wanted` files (absolute), and when it last ran a command
/// in a folder holding one of them: what it said before, kept, and the whole lines it added since. A log shorter than
/// what was read, or read in another folder, is read again. A relative path is taken in the folder its own line says
/// it ran in (a helper in a separate copy of the repo writes there, not in the shared folder: #248), else in the last
/// one said, else in `cwd`.
pub(crate) fn read_edits(log: &str, cwd: &str, wanted: &[String]) -> (Vec<(String, TheirEdit)>, Vec<(String, u64)>) {
    let kept = {
        let mut so_far = crate::locking::lock(read_so_far(), "logs read for their edits");
        so_far.remove(log)
    };
    let fresh = || SoFar::new(cwd, crate::logs::steps::harness_of(log));
    let mut so_far = kept.filter(|k| k.cwd == cwd).unwrap_or_else(fresh);
    let Ok(mut file) = std::fs::File::open(log) else { return (vec![], vec![]) };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < so_far.len {
        so_far = fresh();
    }
    let mut added: Vec<u8> = Vec::new();
    use std::io::{Read, Seek};
    if len > so_far.len && file.seek(std::io::SeekFrom::Start(so_far.len)).is_ok() && file.read_to_end(&mut added).is_ok() {
        // Only whole lines: a line still being written is read next time.
        if let Some(end) = added.iter().rposition(|b| *b == b'\n') {
            for line in String::from_utf8_lossy(&added[..=end]).lines() {
                let worth = match so_far.kind {
                    "codex" => CODEX_MARKS.iter().any(|m| line.contains(m)) || so_far.pending.keys().any(|i| line.contains(i.as_str())),
                    "pi" => ["\"toolCall\"", "\"toolResult\"", "\"cwd\""].iter().any(|m| line.contains(m)),
                    _ => line.contains("tool_use"),
                };
                let Some(e) = worth.then(|| serde_json::from_str::<Value>(line).ok()).flatten() else { continue };
                match so_far.kind {
                    "codex" => codex_line(&e, &mut so_far),
                    "pi" => pi_line(&e, &mut so_far),
                    _ => claude_line(&e, &mut so_far),
                }
            }
            so_far.len += end as u64 + 1;
            so_far.edits.retain(|(f, _)| !f.is_empty());
            so_far.trim();
        }
    }
    let edits = so_far.edits.iter().filter(|(f, _)| wanted.contains(f)).cloned().collect();
    let ran = so_far.ran.iter().filter(|(d, _)| wanted.iter().any(|f| f.starts_with(&format!("{d}/")))).map(|(d, at)| (d.clone(), *at)).collect();
    let mut all = crate::locking::lock(read_so_far(), "logs read for their edits");
    all.insert(log.to_string(), so_far);
    (edits, ran)
}

/// `f` made absolute in folder `dir`.
pub(crate) fn resolve(dir: &str, f: &str) -> String {
    if f.starts_with('/') {
        f.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), f.trim_start_matches("./"))
    }
}

/// A change whose text is known (`old` to `new`) or isn't (`blind`), at `at`, nobody's yet.
pub(crate) fn change(at: u64, old: &str, new: &str, all: bool, blind: bool) -> TheirEdit {
    TheirEdit { at, by: String::new(), old: old.to_string(), new: new.to_string(), all, blind, guess: false }
}

/// One Claude Code log line: the folder it ran in, and the files its tool calls changed, and how (a relative path
/// taken in that folder).
pub(crate) fn claude_line(e: &Value, so: &mut SoFar) {
    if let Some(here) = e.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
        so.at = here.to_string();
    }
    let at = crate::logs::transcript::stamp(e);
    let dir = so.at.clone();
    let edit = |old: &str, new: &str, all: bool, blind: bool| change(at, old, new, all, blind);
    // An edit without its old and new text changes nothing (Claude Code refuses it): it isn't a change of anyone's.
    // An empty old text makes the file: a write.
    let one = |m: &Value| match (m.get("old_string").and_then(Value::as_str), m.get("new_string").and_then(Value::as_str)) {
        (Some(""), Some(_)) => Some(edit("", "", false, true)),
        (Some(old), Some(new)) => Some(edit(old, new, m.get("replace_all").and_then(Value::as_bool).unwrap_or(false), false)),
        _ => None,
    };
    for b in e.pointer("/message/content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
        let input = &b["input"];
        let file = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str).map(|f| resolve(&dir, f)).unwrap_or_default();
        match b.get("name").and_then(Value::as_str).unwrap_or("") {
            "Edit" => so.edits.extend(one(input).map(|c| (file, c))),
            "MultiEdit" => {
                for m in input.get("edits").and_then(Value::as_array).into_iter().flatten() {
                    so.edits.extend(one(m).map(|c| (file.clone(), c)));
                }
            }
            "Write" | "NotebookEdit" => so.edits.push((file, edit("", "", false, true))),
            "Bash" => so.shell(input.get("command").and_then(Value::as_str).unwrap_or(""), &dir, at),
            _ => {}
        }
    }
}

/// One Pi log line (#248): its session line's folder; an assistant's edit (each of its `edits`, or its one
/// oldText/newText) and write, made once their result says they worked; its bash commands' writes.
pub(crate) fn pi_line(e: &Value, so: &mut SoFar) {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if e.get("type").and_then(Value::as_str) == Some("session") {
        if let Some(c) = e.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
            so.at = c.to_string();
        }
        return;
    }
    let at = crate::logs::transcript::stamp(e);
    let m = e.get("message").unwrap_or(&Value::Null);
    match m.get("role").and_then(Value::as_str) {
        Some("assistant") => {
            let dir = so.at.clone();
            for b in m.get("content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("toolCall")) {
                let args = b.get("arguments").unwrap_or(&Value::Null);
                let file = resolve(&dir, &s(args, "path"));
                let id = s(b, "id");
                match b.get("name").and_then(Value::as_str).unwrap_or("") {
                    "edit" => {
                        let one = [args.clone()];
                        let list = args.get("edits").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&one);
                        let changes = list.iter().map(|x| (file.clone(), match (x.get("oldText").and_then(Value::as_str), x.get("newText").and_then(Value::as_str)) {
                            (Some(old), Some(new)) => change(at, old, new, false, false),
                            _ => change(at, "", "", false, true),
                        })).collect();
                        so.held(Some(&id), changes);
                    }
                    "write" => so.held(Some(&id), vec![(file, change(at, "", "", false, true))]),
                    "bash" => so.shell(&s(args, "command"), &dir, at),
                    _ => {}
                }
            }
        }
        Some("toolResult") => {
            so.settled(&s(m, "toolCallId"), m.get("isError").and_then(Value::as_bool) != Some(true));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(at: u64, old: &str, new: &str) -> TheirEdit {
        TheirEdit { at, by: "tidier".into(), old: old.into(), new: new.into(), all: false, blind: false, guess: false }
    }

    /// (file, old, new, blind) of each change.
    fn shown(got: Vec<(String, TheirEdit)>) -> Vec<(String, String, String, bool)> {
        got.into_iter().map(|(f, e)| (f, e.old, e.new, e.blind)).collect()
    }

    /// A log read again gives what it said before plus only its new whole lines: its old lines aren't read again
    /// (here they're blanked on disk and their edit is still there), a half-written line waits, and a log cut
    /// shorter is read from the start.
    #[test]
    fn a_log_is_read_once_and_then_only_what_it_adds() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("s.jsonl");
        let line = |file: &str, new: &str| format!("{{\"timestamp\":\"2026-11-18T12:00:00Z\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{{\"file_path\":\"{file}\",\"old_string\":\"a\",\"new_string\":\"{new}\"}}}}]}}}}\n");
        let (first, other) = (line("x.rs", "b"), line("/elsewhere/y.rs", "c"));
        std::fs::write(&log, format!("{first}{other}")).unwrap();
        let (path, cwd) = (log.to_string_lossy().into_owned(), dir.to_string_lossy().into_owned());
        let wanted = vec![format!("{cwd}/x.rs")];
        let news = |got: Vec<(String, TheirEdit)>| got.into_iter().map(|(_, e)| e.new).collect::<Vec<_>>();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b"]);
        let blanked = " ".repeat(first.len() - 1) + "\n";
        let second = line("x.rs", "d");
        std::fs::write(&log, format!("{blanked}{other}{second}{}", &second[..20])).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b", "d"], "the old line came from what was read before");
        std::fs::write(&log, format!("{blanked}{other}{second}{second}")).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b", "d", "d"], "the half line, once whole");
        std::fs::write(&log, &second).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["d"], "cut shorter: read from the start");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Pi log: an edit (each of its edits) by its text and a write, once their result says they worked; a bash
    /// command's write; relative paths in the folder its session line says (#248).
    #[test]
    fn a_pi_logs_edits_writes_and_commands_are_read() {
        let mut so = super::SoFar::new("/start", "pi");
        let mut line = |v: serde_json::Value| super::pi_line(&v, &mut so);
        line(serde_json::json!({ "type": "session", "version": 3, "id": "P", "cwd": "/a" }));
        line(serde_json::json!({ "type": "message", "timestamp": "2026-11-18T12:00:01Z", "message": { "role": "assistant", "content": [
            { "type": "toolCall", "id": "c1", "name": "edit", "arguments": { "path": "x.ts", "edits": [{ "oldText": "a", "newText": "b" }] } },
            { "type": "toolCall", "id": "c2", "name": "edit", "arguments": { "path": "/a/y.ts", "oldText": "c", "newText": "d" } },
            { "type": "toolCall", "id": "c3", "name": "write", "arguments": { "path": "z.ts", "content": "z" } },
            { "type": "toolCall", "id": "c4", "name": "bash", "arguments": { "command": "echo hi > w.txt" } } ] } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c1", "isError": false } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c2", "isError": true } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c3", "isError": false } }));
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/a/w.txt".into(), "".into(), "".into(), true),
            ("/a/x.ts".into(), "a".into(), "b".into(), false),
            ("/a/z.ts".into(), "".into(), "".into(), true),
        ], "the failed edit of y.ts isn't one");
    }

    /// A Claude Code Edit without its old and new text changed nothing and isn't read; one with an empty old text
    /// made the file, read as a write (#248).
    #[test]
    fn a_claude_edit_without_its_text_is_no_change_and_one_with_empty_old_text_is_a_write() {
        let mut so = super::SoFar::new("/r", "claude");
        let edit = |input: serde_json::Value| serde_json::json!({ "message": { "content": [{ "type": "tool_use", "name": "Edit", "input": input }] } });
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/a.txt" })), &mut so);
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/b.txt", "old_string": "", "new_string": "b" })), &mut so);
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/c.txt", "old_string": "c", "new_string": "d" })), &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/r/b.txt".into(), "".into(), "".into(), true),
            ("/r/c.txt".into(), "c".into(), "d".into(), false),
        ]);
    }

    /// A log keeps only its newest changes: KEPT_EDITS of them, and KEPT_TEXT of their text (#248).
    #[test]
    fn a_log_keeps_only_its_newest_changes() {
        let mut so = super::SoFar::new("/r", "claude");
        so.edits = (0..super::KEPT_EDITS as u64 + 5).map(|i| ("/r/a".to_string(), edit(i, "", "x"))).collect();
        so.trim();
        assert_eq!((so.edits.len(), so.edits[0].1.at), (super::KEPT_EDITS, 5));
        so.edits = (0..4u64).map(|i| ("/r/a".to_string(), edit(i, "", &"y".repeat(super::KEPT_TEXT / 3)))).collect();
        so.trim();
        assert_eq!(so.edits.iter().map(|(_, e)| e.at).collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn a_relative_write_is_taken_in_the_folder_its_line_ran_in() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-cwd-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("agent-h.jsonl");
        let bash = |cwd: &str| format!("{{\"timestamp\":\"2026-11-18T12:00:00Z\",\"cwd\":\"{cwd}\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{{\"command\":\"echo hi > a.txt\"}}}}]}}}}\n");
        std::fs::write(&log, bash("/copy")).unwrap();
        let path = log.to_string_lossy().into_owned();
        let wanted = vec!["/shared/a.txt".to_string(), "/copy/a.txt".to_string()];
        let files = |got: Vec<(String, TheirEdit)>| got.into_iter().map(|(f, _)| f).collect::<Vec<_>>();
        assert_eq!(files(read_edits(&path, "/shared", &wanted).0), ["/copy/a.txt"], "the copy's file, not the shared one");
        std::fs::write(&log, format!("{}{}", bash("/copy"), bash("/shared"))).unwrap();
        assert_eq!(files(read_edits(&path, "/shared", &wanted).0), ["/copy/a.txt", "/shared/a.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
