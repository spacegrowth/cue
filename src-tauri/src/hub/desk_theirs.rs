//! Other sessions' changes in an item's files, kept out of its commit. A file the item changed that another session
//! (here, not the item's holder) has changed too since it was last committed is committed with only the item's part:
//! the other session's edits, read from its log, taken back out of what's committed (newest first), while the file
//! itself stays as it is, so their edits stay there uncommitted. When their part can't be told apart (they wrote the
//! whole file, a shell command wrote it, or their text isn't found once and only once), the commit waits for it.

use super::*;
use crate::desk::DeskItem;
use serde_json::Value;

/// One change another session made to a file: when, who (as the Desk shows it), and what. `old`/`new` are an Edit's
/// strings (`all`: its replace_all); `blind` is a change whose text isn't known (Write, NotebookEdit, a shell command).
#[derive(Clone, Debug)]
pub(super) struct TheirEdit {
    at: u64,
    by: String,
    old: String,
    new: String,
    all: bool,
    blind: bool,
}

/// Other sessions' changes, by file (absolute).
#[derive(Default)]
pub(super) struct Theirs {
    edits: Vec<(String, TheirEdit)>,
}

/// What of one file is the item's to commit.
#[derive(Debug, PartialEq)]
pub(super) enum Own {
    /// No other session's change is in it: the file as it is.
    Whole,
    /// The file with others' changes taken out.
    Part(Vec<u8>),
    /// Only others' changes: nothing of the item's.
    Nothing,
    /// Others' changes that can't be told apart from the item's: who made them.
    Mixed(String),
}

/// A file committed with only the item's part: its folder, its path there, and what's committed.
#[derive(Clone, Debug)]
pub(super) struct Part {
    pub folder: String,
    pub path: String,
    pub content: Vec<u8>,
}

impl Hub {
    /// The changes sessions here other than `it`'s holder (and its crew) made to files, read from their logs (and
    /// their background agents'), for `wanted` (absolute paths): none of a file not in it.
    /// SHORTCUT: the holder's whole session is left out, so its other teammates' edits (not on this item) in the same
    /// file count as the item's; fine while one lead's crew splits work by file (as Cue tells it to). Upgrade: read
    /// those teammates' own logs and take their edits out too.
    pub(super) fn theirs(&self, it: &DeskItem, wanted: &[String]) -> Theirs {
        if wanted.is_empty() {
            return Theirs::default();
        }
        let (sid, lead) = it.holder.as_ref().map(|h| (h.sid.clone(), h.to().to_string())).unwrap_or_default();
        let logs: Vec<(String, String, String)> = self.read(|st| {
            st.sessions.iter()
                .filter(|s| s.origin.machine.is_empty() && !s.origin.transcript_path.is_empty() && s.origin.session_id != sid && s.origin.session_id != lead)
                .map(|s| (s.origin.transcript_path.clone(), s.origin.cwd.clone(), super::desk::shown_name(s)))
                .collect()
        });
        let mut out = Theirs::default();
        for (log, cwd, name) in logs {
            out.edits.extend(read_edits(&log, &cwd, &name, wanted));
            for path in crate::logs::steps::agent_logs(&log) {
                out.edits.extend(read_edits(&path.to_string_lossy(), &cwd, &name, wanted));
            }
        }
        out
    }

    /// Of `todo` (each folder's changed files), what's the item's to commit: the files to commit (their part in
    /// `parts` when others' changes are taken out of them; one with only others' changes left out), and the files
    /// whose others' changes can't be told apart ("<who> also changed <path>").
    pub(super) fn own_of(&self, it: &DeskItem, todo: Vec<(String, Vec<String>)>) -> (Vec<(String, Vec<String>)>, Vec<Part>, Vec<(String, String)>) {
        let abs = |folder: &str, p: &str| format!("{}/{p}", folder.trim_end_matches('/'));
        let wanted: Vec<String> = todo.iter().flat_map(|(f, ps)| ps.iter().map(|p| abs(f, p)).collect::<Vec<_>>()).collect();
        let theirs = self.theirs(it, &wanted);
        let mut keep: Vec<(String, Vec<String>)> = Vec::new();
        let mut parts: Vec<Part> = Vec::new();
        let mut mixed: Vec<(String, String)> = Vec::new();
        for (folder, paths) in todo {
            let mut left: Vec<String> = Vec::new();
            for p in paths {
                match own_part(&theirs, &folder, &p) {
                    Own::Whole => left.push(p),
                    Own::Part(content) => {
                        parts.push(Part { folder: folder.clone(), path: p.clone(), content });
                        left.push(p);
                    }
                    Own::Nothing => {}
                    Own::Mixed(by) => mixed.push((p, by)),
                }
            }
            if !left.is_empty() {
                keep.push((folder, left));
            }
        }
        (keep, parts, mixed)
    }
}

/// The changes `log` (a session working in `cwd`, shown as `by`) made to `wanted` files (absolute).
fn read_edits(log: &str, cwd: &str, by: &str, wanted: &[String]) -> Vec<(String, TheirEdit)> {
    let Ok(text) = std::fs::read_to_string(log) else { return vec![] };
    let names: Vec<&str> = wanted.iter().filter_map(|w| w.rsplit('/').next()).collect();
    let dir = cwd.trim_end_matches('/');
    let resolve = |f: &str| if f.starts_with('/') { f.to_string() } else { format!("{dir}/{}", f.trim_start_matches("./")) };
    let mut out: Vec<(String, TheirEdit)> = Vec::new();
    for line in text.lines().filter(|l| names.iter().any(|n| l.contains(n))) {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let at = crate::logs::transcript::stamp(&e);
        let edit = |old: &str, new: &str, all: bool, blind: bool| TheirEdit { at, by: by.to_string(), old: old.to_string(), new: new.to_string(), all, blind };
        for b in e.pointer("/message/content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
            let input = &b["input"];
            let file = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str).map(resolve).unwrap_or_default();
            let mut found: Vec<(String, TheirEdit)> = Vec::new();
            match b.get("name").and_then(Value::as_str).unwrap_or("") {
                "Edit" => {
                    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    found.push((file, edit(&s("old_string"), &s("new_string"), input.get("replace_all").and_then(Value::as_bool).unwrap_or(false), false)));
                }
                "MultiEdit" => {
                    for m in input.get("edits").and_then(Value::as_array).into_iter().flatten() {
                        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                        found.push((file.clone(), edit(&s("old_string"), &s("new_string"), m.get("replace_all").and_then(Value::as_bool).unwrap_or(false), false)));
                    }
                }
                "Write" | "NotebookEdit" => found.push((file, edit("", "", false, true))),
                "Bash" => {
                    let cmd = input.get("command").and_then(Value::as_str).unwrap_or("");
                    found.extend(crate::sessions::shell_writes::writes(cmd).iter().map(|f| (resolve(f), edit("", "", false, true))));
                }
                _ => {}
            }
            out.extend(found.into_iter().filter(|(f, _)| wanted.contains(f)));
        }
    }
    out
}

/// Of `path` in `folder`, the item's part: its text now with others' changes taken out, newest first. An Edit of
/// theirs counts while its new text is in the file more often than in HEAD (else it's committed already, or undone);
/// a change whose text isn't known counts when made since the file's last commit.
pub(super) fn own_part(theirs: &Theirs, folder: &str, path: &str) -> Own {
    let abs = format!("{}/{path}", folder.trim_end_matches('/'));
    let mut edits: Vec<&TheirEdit> = theirs.edits.iter().filter(|(f, _)| *f == abs).map(|(_, e)| e).collect();
    if edits.is_empty() {
        return Own::Whole;
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.at));
    let git = |args: &[&str]| super::desk_commit::git("", folder, &[], args);
    let committed = git(&["log", "-1", "--format=%ct", "--", path]).and_then(|t| t.trim().parse::<u64>().ok()).map_or(0, |s| s * 1000);
    let head = git(&["show", &format!("HEAD:./{path}")]).unwrap_or_default();
    let Ok(now) = std::fs::read_to_string(&abs) else {
        // Gone, or not text: theirs can't be taken out of it.
        return match edits.iter().find(|e| !e.blind || e.at >= committed) {
            Some(e) => Own::Mixed(e.by.clone()),
            None => Own::Whole,
        };
    };
    match take_out(&now, &head, &edits, committed) {
        Err(by) => Own::Mixed(by),
        Ok(None) => Own::Whole,
        Ok(Some(t)) if t == head => Own::Nothing,
        Ok(Some(t)) => Own::Part(t.into_bytes()),
    }
}

/// `now` with `edits` (newest first) taken out: None when none of them is in it, Err(who) when one can't be.
fn take_out(now: &str, head: &str, edits: &[&TheirEdit], committed: u64) -> Result<Option<String>, String> {
    let mut text = now.to_string();
    let mut took = false;
    for e in edits {
        if e.blind {
            if e.at >= committed {
                return Err(e.by.clone());
            }
            continue;
        }
        if e.new.is_empty() {
            // A cut: where it was can't be found again. Still in effect when its old text is in HEAD more often.
            if !e.old.is_empty() && head.matches(e.old.as_str()).count() > text.matches(e.old.as_str()).count() {
                return Err(e.by.clone());
            }
            continue;
        }
        let (here, before) = (text.matches(e.new.as_str()).count(), head.matches(e.new.as_str()).count());
        if here <= before {
            continue;
        }
        if e.all && before == 0 {
            text = text.replace(e.new.as_str(), &e.old);
        } else if here == 1 {
            text = text.replacen(e.new.as_str(), &e.old, 1);
        } else {
            return Err(e.by.clone());
        }
        took = true;
    }
    Ok(took.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::{take_out, TheirEdit};

    fn edit(at: u64, old: &str, new: &str) -> TheirEdit {
        TheirEdit { at, by: "tidier".into(), old: old.into(), new: new.into(), all: false, blind: false }
    }

    #[test]
    fn their_edit_is_taken_out_and_the_items_stays() {
        let head = "one\ntwo\nthree\n";
        let now = "ONE\ntwo\nthree!\n";
        let theirs = edit(1, "three\n", "three!\n");
        assert_eq!(take_out(now, head, &[&theirs], 0), Ok(Some("ONE\ntwo\nthree\n".into())));
    }

    #[test]
    fn chained_edits_come_out_newest_first_and_a_committed_one_is_left() {
        let head = "a\nb\n";
        let now = "a\nb3\n";
        let (first, second) = (edit(1, "b\n", "b2\n"), edit(2, "b2\n", "b3\n"));
        assert_eq!(take_out(now, head, &[&second, &first], 0), Ok(Some(head.into())));
        assert_eq!(take_out("a\nb\n", head, &[&edit(1, "x", "a\n")], 0), Ok(None), "its text is in HEAD: committed");
    }

    #[test]
    fn text_found_twice_or_a_write_since_the_commit_cant_be_told_apart() {
        let twice = edit(1, "x", "y");
        assert_eq!(take_out("y y", "", &[&twice], 0), Err("tidier".into()));
        let write = TheirEdit { blind: true, ..edit(5, "", "") };
        assert_eq!(take_out("z", "", &[&write], 1), Err("tidier".into()));
        assert_eq!(take_out("z", "", &[&write], 9), Ok(None), "written before its last commit");
    }
}
