//! Other sessions' changes in an item's files, kept out of its commit. A file the item changed that another session
//! (here, not the item's holder) has changed too since it was last committed is committed with only the item's part:
//! the other session's edits, read from its log, taken back out of what's committed (newest first), while the file
//! itself stays as it is, so their edits stay there uncommitted. When their part can't be told apart (they wrote the
//! whole file, a shell command wrote it, or their text isn't found once and only once), the commit waits for it.
//! The item's own sessions (its holder, its former holders, their crews) aren't others. Logs are read from Claude Code,
//! Codex and Pi alike, and from sessions that ended (#248). A wait names a session only when its log shows the change
//! by its text or as a whole-file write; a write read from a shell command is a guess, said as "someone else" (#250).

use super::*;
use crate::desk::DeskItem;
use super::desk_commit::SOMEONE_ELSE as SOMEONE;
use super::desk_theirs_log::{read_edits, read_so_far};

/// One change a session made to a file: when, who (as the Desk shows it), and what. `old`/`new` are an Edit's
/// strings (`all`: its replace_all); `blind` is a change whose text isn't known (Write, NotebookEdit, a shell command).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TheirEdit {
    pub(super) at: u64,
    pub(super) by: String,
    pub(super) old: String,
    pub(super) new: String,
    pub(super) all: bool,
    pub(super) blind: bool,
    /// Read from a shell command, not named by an edit: whose it is is a guess, so it's never named (#250).
    pub(super) guess: bool,
}

impl TheirEdit {
    /// Who made it, as a wait says it: its session's name, or "someone else" when that's a guess.
    fn who(&self) -> String {
        if self.guess { SOMEONE.to_string() } else { self.by.clone() }
    }
}

/// Other sessions' changes read from their logs, by file (absolute): not the item's own sessions'.
#[derive(Default)]
pub(super) struct Theirs {
    edits: Vec<(String, TheirEdit)>,
}

impl Theirs {
    /// The sessions these changes are named to (never a guess), once each.
    pub(super) fn named(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, e) in self.edits.iter().filter(|(_, e)| !e.guess) {
            if !out.contains(&e.by) {
                out.push(e.by.clone());
            }
        }
        out
    }
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

/// How long an ended session's log still counts (its saved record read back): a change older than that is in a
/// commit by now, or nobody's to take out (#248).
const ENDED_KEPT_MS: u64 = 3 * 24 * 3600 * 1000;
/// At most this many ended sessions read back.
const ENDED_KEPT_MAX: usize = 100;
/// The saved ended sessions are read back at most this often: each waiting item is checked every 10 s (#248).
const ENDED_READ_EVERY_MS: u64 = 60_000;

/// A session Cue knows of: id, log, folder, name as the Desk shows it, whether it's here, and its agent logs when
/// they were listed already.
#[derive(Clone)]
struct Known {
    id: String,
    log: String,
    cwd: String,
    name: String,
    here: bool,
    agents: Option<Vec<String>>,
}

fn known_of(s: &crate::sessions::Session) -> Known {
    let o = &s.origin;
    Known { id: o.session_id.clone(), log: o.transcript_path.clone(), cwd: o.cwd.clone(), name: super::desk::shown_name(s), here: o.machine.is_empty(), agents: None }
}

/// The agent logs of `log`, as paths.
fn agents_of(log: &str) -> Vec<String> {
    crate::logs::steps::agent_logs(log).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// Sessions that ended within ENDED_KEPT_MS as saved (at most ENDED_KEPT_MAX), with their agent logs: read back at
/// most every ENDED_READ_EVERY_MS. In tests each World has a store of its own, so it's read each time there.
fn ended_saved() -> Vec<Known> {
    static SAVED: std::sync::OnceLock<std::sync::Mutex<Option<(u64, Vec<Known>)>>> = std::sync::OnceLock::new();
    let saved = SAVED.get_or_init(Default::default);
    let now = crate::clock::now_mono_ms();
    if !cfg!(test) {
        let kept = crate::locking::lock(saved, "ended sessions read for their edits");
        if let Some((at, list)) = kept.as_ref().filter(|(at, _)| now.saturating_sub(*at) < ENDED_READ_EVERY_MS) {
            let _ = at;
            return list.clone();
        }
    }
    let list: Vec<Known> = crate::db::closed(now_ms().saturating_sub(ENDED_KEPT_MS), ENDED_KEPT_MAX)
        .iter()
        .map(|(_, s)| Known { agents: Some(agents_of(&s.origin.transcript_path)), ..known_of(s) })
        .collect();
    let mut kept = crate::locking::lock(saved, "ended sessions read for their edits");
    *kept = Some((now, list.clone()));
    list
}

impl Hub {
    /// The changes sessions here made to files, read from their logs (and their background agents'), for `wanted`
    /// (absolute paths): none of a file not in it. Those of `it`'s own sessions (its holder, its former holders and
    /// their crews) are its own; everyone else's are others'. Claude Code's logs, Codex's rollouts and Pi's alike, and
    /// a session that has ended still owns what it changed: parked, ended while Cue ran, or ended within
    /// ENDED_KEPT_MS as saved (#248).
    /// SHORTCUT: the holder's whole session counts as the item's, so its other teammates' edits (not on this item) in
    /// the same file count as the item's; fine while one lead's crew splits work by file (as Cue tells it to).
    /// Upgrade: read those teammates' own logs and take their edits out too.
    pub(super) fn theirs(&self, it: &DeskItem, wanted: &[String]) -> Theirs {
        if wanted.is_empty() {
            return Theirs::default();
        }
        let mut mine_ids: Vec<String> = it.former_holders.clone();
        if let Some(h) = it.holder.as_ref() {
            mine_ids.push(h.sid.clone());
            mine_ids.push(h.to().to_string());
        }
        mine_ids.retain(|i| !i.is_empty());
        // Live first, then ended.
        let mut known: Vec<Known> = self.read(|st| {
            st.sessions.iter().map(known_of)
                .chain(st.parked.iter().map(|p| known_of(&p.session)))
                .chain(st.ended.iter().map(|(_, s)| known_of(s)))
                .collect()
        });
        known.extend(ended_saved());
        // One of the item's own sessions that ended longer ago is looked up by itself.
        let missing: Vec<String> = mine_ids.iter().filter(|i| !known.iter().any(|k| k.id == **i)).cloned().collect();
        known.extend(missing.iter().filter_map(|i| crate::db::ended_session(i)).map(|(_, s)| known_of(&s)));
        let mut seen = std::collections::HashSet::new();
        known.retain(|k| k.here && !k.log.is_empty() && seen.insert(k.id.clone()));
        let all: Vec<String> = known.iter().map(|k| k.log.trim_end_matches(".jsonl").to_string()).collect();
        let mut out = Theirs::default();
        for k in known.into_iter().filter(|k| !mine_ids.contains(&k.id)) {
            let agents = k.agents.clone().unwrap_or_else(|| agents_of(&k.log));
            for path in std::iter::once(k.log.clone()).chain(agents) {
                let (edits, _) = read_edits(&path, &k.cwd, wanted);
                out.edits.extend(edits.into_iter().map(|(f, e)| (f, TheirEdit { by: k.name.clone(), ..e })));
            }
        }
        // A log (or agent log) of a session gone from Cue isn't kept.
        let mut so_far = crate::locking::lock(read_so_far(), "logs read for their edits");
        so_far.retain(|path, _| all.iter().any(|a| path.starts_with(a.as_str())));
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
                    Own::Mixed(by) => mixed.push((p, if by.is_empty() { SOMEONE.to_string() } else { by })),
                }
            }
            if !left.is_empty() {
                keep.push((folder, left));
            }
        }
        (keep, parts, mixed)
    }
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
    let git = |args: &[&str]| super::desk_git::git("", folder, &[], args);
    let committed = git(&["log", "-1", "--format=%ct", "--", path]).and_then(|t| t.trim().parse::<u64>().ok()).map_or(0, |s| s * 1000);
    let head = git(&["show", &format!("HEAD:./{path}")]).unwrap_or_default();
    let Ok(now) = std::fs::read_to_string(&abs) else {
        // Gone, or not text: theirs can't be taken out of it. Only a change since its last commit is named (#248).
        return match edits.iter().find(|e| e.at >= committed) {
            Some(e) => Own::Mixed(e.who()),
            None => Own::Whole,
        };
    };
    match take_out(&now, &head, &edits, committed) {
        Err(by) => Own::Mixed(by),
        Ok(None) => Own::Whole,
        Ok(Some(rest)) if rest == head => Own::Nothing,
        Ok(Some(rest)) => Own::Part(rest.into_bytes()),
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
    use super::{take_out, TheirEdit};

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

    /// A file gone (or not text) names only a change made since its last commit: an older edit of theirs isn't
    /// shown as the reason it waits (#248).
    #[test]
    fn a_gone_file_names_only_a_change_since_its_last_commit() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args)
            .env("GIT_COMMITTER_DATE", "@2000 +0000").env("GIT_AUTHOR_DATE", "@2000 +0000").output().unwrap().status.success(), "{args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-q", "-m", "a"]);
        std::fs::remove_file(dir.join("a.txt")).unwrap();
        let folder = dir.to_string_lossy().into_owned();
        let theirs = |at: u64| super::Theirs { edits: vec![(format!("{folder}/a.txt"), edit(at, "a\n", "b\n"))], ..Default::default() };
        assert_eq!(super::own_part(&theirs(1_000_000), &folder, "a.txt"), super::Own::Whole, "made before its commit");
        assert_eq!(super::own_part(&theirs(3_000_000), &folder, "a.txt"), super::Own::Mixed("tidier".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
