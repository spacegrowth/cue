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
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TheirEdit {
    at: u64,
    by: String,
    old: String,
    new: String,
    all: bool,
    blind: bool,
    /// Read from a shell command, not named by an edit: whose it is is a guess, so it's never named (#250).
    guess: bool,
}

impl TheirEdit {
    /// Who made it, as a wait says it: its session's name, or "someone else" when that's a guess.
    fn who(&self) -> String {
        if self.guess { super::desk_commit::SOMEONE_ELSE.to_string() } else { self.by.clone() }
    }
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
        let (logs, all): (Vec<(String, String, String)>, Vec<String>) = self.read(|st| {
            let here: Vec<_> = st.sessions.iter().filter(|s| s.origin.machine.is_empty() && !s.origin.transcript_path.is_empty()).collect();
            let all = here.iter().map(|s| s.origin.transcript_path.trim_end_matches(".jsonl").to_string()).collect();
            let logs = here.into_iter()
                .filter(|s| s.origin.session_id != sid && s.origin.session_id != lead)
                .map(|s| (s.origin.transcript_path.clone(), s.origin.cwd.clone(), super::desk::shown_name(s)))
                .collect();
            (logs, all)
        });
        let mut out = Theirs::default();
        for (log, cwd, name) in logs {
            let agents = crate::logs::steps::agent_logs(&log).into_iter().map(|p| p.to_string_lossy().into_owned());
            for path in std::iter::once(log.clone()).chain(agents) {
                out.edits.extend(read_edits(&path, &cwd, wanted).into_iter().map(|(f, e)| (f, TheirEdit { by: name.clone(), ..e })));
            }
        }
        // A log (or agent log) of a session gone from Cue isn't kept.
        crate::locking::lock(read_so_far(), "logs read for their edits").retain(|path, _| all.iter().any(|a| path.starts_with(a.as_str())));
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

/// What one log said so far: how far it was read (whole lines), the folder it was read in, and every change it
/// made to a file (`by` left empty: the session's name is put on when they're handed out).
#[derive(Default)]
struct SoFar {
    len: u64,
    cwd: String,
    edits: Vec<(String, TheirEdit)>,
}

/// Each log's changes so far, by path, so a finish check every 10 s reads only what each log added since (#284):
/// reading every session's whole log each time cost seconds of CPU a minute. Logs no session has any more are dropped.
fn read_so_far() -> &'static std::sync::Mutex<std::collections::HashMap<String, SoFar>> {
    static SO_FAR: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, SoFar>>> = std::sync::OnceLock::new();
    SO_FAR.get_or_init(Default::default)
}

/// The changes `log` (a session working in `cwd`) made to `wanted` files (absolute): what it said before, kept, and
/// the whole lines it added since. A log shorter than what was read, or read in another folder, is read again.
fn read_edits(log: &str, cwd: &str, wanted: &[String]) -> Vec<(String, TheirEdit)> {
    let kept = crate::locking::lock(read_so_far(), "logs read for their edits").remove(log);
    let mut so_far = kept.filter(|k| k.cwd == cwd).unwrap_or_else(|| SoFar { cwd: cwd.to_string(), ..SoFar::default() });
    let Ok(mut file) = std::fs::File::open(log) else { return vec![] };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < so_far.len {
        so_far = SoFar { cwd: cwd.to_string(), ..SoFar::default() };
    }
    let mut added: Vec<u8> = Vec::new();
    use std::io::{Read, Seek};
    if len > so_far.len && file.seek(std::io::SeekFrom::Start(so_far.len)).is_ok() && file.read_to_end(&mut added).is_ok() {
        // Only whole lines: a line still being written is read next time.
        if let Some(end) = added.iter().rposition(|b| *b == b'\n') {
            let dir = cwd.trim_end_matches('/');
            let resolve = |f: &str| if f.starts_with('/') { f.to_string() } else { format!("{dir}/{}", f.trim_start_matches("./")) };
            for line in String::from_utf8_lossy(&added[..=end]).lines().filter(|l| l.contains("tool_use")) {
                so_far.edits.extend(edits_in(line, &resolve));
            }
            so_far.len += end as u64 + 1;
        }
    }
    let out = so_far.edits.iter().filter(|(f, _)| wanted.contains(f)).cloned().collect();
    crate::locking::lock(read_so_far(), "logs read for their edits").insert(log.to_string(), so_far);
    out
}

/// The files one log line's tool calls changed, and how (`resolve` makes a path absolute).
fn edits_in(line: &str, resolve: &dyn Fn(&str) -> String) -> Vec<(String, TheirEdit)> {
    let Ok(e) = serde_json::from_str::<Value>(line) else { return vec![] };
    let at = crate::logs::transcript::stamp(&e);
    let edit = |old: &str, new: &str, all: bool, blind: bool| TheirEdit { at, by: String::new(), old: old.to_string(), new: new.to_string(), all, blind, guess: false };
    let mut found: Vec<(String, TheirEdit)> = Vec::new();
    for b in e.pointer("/message/content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
        let input = &b["input"];
        let file = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str).map(resolve).unwrap_or_default();
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
                found.extend(crate::sessions::shell_writes::writes(cmd).iter().map(|f| (resolve(f), TheirEdit { guess: true, ..edit("", "", false, true) })));
            }
            _ => {}
        }
    }
    found.retain(|(f, _)| !f.is_empty());
    found
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
            Some(e) => Own::Mixed(e.who()),
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
                return Err(e.who());
            }
            continue;
        }
        if e.new.is_empty() {
            // A cut: where it was can't be found again. Still in effect when its old text is in HEAD more often.
            if !e.old.is_empty() && head.matches(e.old.as_str()).count() > text.matches(e.old.as_str()).count() {
                return Err(e.who());
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
            return Err(e.who());
        }
        took = true;
    }
    Ok(took.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::{read_edits, take_out, TheirEdit};

    fn edit(at: u64, old: &str, new: &str) -> TheirEdit {
        TheirEdit { at, by: "tidier".into(), old: old.into(), new: new.into(), all: false, blind: false, guess: false }
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
        assert_eq!(news(read_edits(&path, &cwd, &wanted)), ["b"]);
        let blanked = " ".repeat(first.len() - 1) + "\n";
        let second = line("x.rs", "d");
        std::fs::write(&log, format!("{blanked}{other}{second}{}", &second[..20])).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted)), ["b", "d"], "the old line came from what was read before");
        std::fs::write(&log, format!("{blanked}{other}{second}{second}")).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted)), ["b", "d", "d"], "the half line, once whole");
        std::fs::write(&log, &second).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted)), ["d"], "cut shorter: read from the start");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
