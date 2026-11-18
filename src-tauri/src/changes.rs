//! What a turn changed, from git: when a turn starts, Cue notes the folder's state (a `git stash create`
//! snapshot, which writes a commit object and touches nothing else, or HEAD when nothing's uncommitted, plus
//! the untracked files); when it ends, it compares the folder against that. So only this turn's edits count,
//! not work left uncommitted before it. Edits another session makes in the same folder during the turn
//! count too: git can't tell them apart.

use serde::{Deserialize, Serialize};
use crate::safe_git;

// The optional add-on's part (built in with the `ext` feature).
#[cfg(feature = "ext")]
#[path = "../ext/changes.rs"]
pub(crate) mod ext;

/// Where a turn started from: the folder's root, a snapshot of it, and its untracked files (extensions see it too).
/// A folder git doesn't track at all is one entry ending in "/" (as `git status` shows it), never its every file:
/// a build folder nobody ignored held ten thousand, sent with every session's state to the window several times a
/// second, and typing lagged while the app's main thread passed it on (#270).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Base {
    root: String,
    commit: String,
    #[serde(deserialize_with = "saved_untracked")]
    untracked: Vec<String>,
    /// The machine the folder is on; empty: this Mac. Its git runs there, over Cue's SSH.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    machine: String,
}

/// Untracked files listed with each wholly untracked folder as one "dir/" entry.
const UNTRACKED_AT_BASE: [&str; 5] = ["ls-files", "--others", "--exclude-standard", "--directory", "--no-empty-directory"];
/// A saved list longer than this is from before folders were one entry: it's dropped (`saved_untracked`).
const OLD_LIST_MAX: usize = 2000;

/// A saved base's untracked files. One from before folders were one entry can hold thousands of a build folder's
/// files; it's dropped rather than kept. All it costs: a session started before that may show files that were
/// already there in its whole-session changes.
fn saved_untracked<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let list = Vec::<String>::deserialize(d)?;
    Ok(if list.len() > OLD_LIST_MAX { vec![] } else { list })
}

impl Base {
    /// `path` (from the repo's top) was untracked already when this base was noted: listed, or inside a folder
    /// that was untracked as a whole.
    pub fn had(&self, path: &str) -> bool {
        self.untracked.iter().any(|u| u == path || (u.ends_with('/') && path.starts_with(u.as_str())))
    }
}

/// The folder's state now, on one of Cue's machines: git runs there, and the folder is never looked
/// up on this Mac. None outside a git repo, or when the machine can't be reached.
pub fn base_on(machine: &str, cwd: &str) -> Option<Base> {
    if cwd.is_empty() {
        return None;
    }
    // Git runs as safe_git runs it (no program the repo sets up), in three calls: its top folder, the
    // filters its own config defines (to switch them off), then the snapshot.
    let (top, code) = safe_git::run(machine, cwd, &[], &["rev-parse", "--show-toplevel"]).ok()?;
    let root = top.trim().to_string();
    if code != 0 || root.is_empty() {
        return None;
    }
    let f = safe_git::no_repo_filters(machine, &root);
    let g = |rest: &[&str]| safe_git::sh_words(&safe_git::words(&root, &f, rest));
    let script = format!(
        "c=$({} 2>/dev/null)\n[ -n \"$c\" ] || c=$({} 2>/dev/null) || exit 0\nprintf '%s\\n%s\\n' {} \"$c\"\n{}",
        g(&["stash", "create"]),
        g(&["rev-parse", "HEAD"]),
        safe_git::sh_quote(&root),
        g(&UNTRACKED_AT_BASE)
    );
    let out = safe_git::on_machine(machine, &script).ok()?;
    let mut lines = out.lines();
    let root = lines.next()?.trim().to_string();
    let commit = lines.next()?.trim().to_string();
    if root.is_empty() || commit.is_empty() {
        return None;
    }
    Some(Base { root, commit, untracked: lines.map(String::from).collect(), machine: machine.to_string() })
}

/// What changed since `base`, on its machine: one SSH call, git and wc there.
fn since_on(base: &Base) -> Option<Changes> {
    let f = safe_git::no_repo_filters(&base.machine, &base.root);
    let g = |rest: &[&str]| safe_git::sh_words(&safe_git::words(&base.root, &f, rest));
    let script = format!(
        "cd {} || exit 1\n{}\necho @@new\n{} | while IFS= read -r f; do printf '%s\\t%s\\n' \"$(wc -l < \"$f\" 2>/dev/null | tr -d ' ')\" \"$f\"; done",
        safe_git::sh_quote(&base.root),
        g(&[&["diff", "--numstat", &base.commit][..], &safe_git::QUIET_DIFF[..]].concat()),
        g(&["ls-files", "--others", "--exclude-standard"])
    );
    let out = safe_git::on_machine(&base.machine, &script).ok()?;
    let (num, new) = out.split_once("@@new\n").unwrap_or((out.as_str(), ""));
    let mut files: Vec<File> = num
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            let (a, d, path) = (p.next()?, p.next()?, p.next()?);
            Some(File { path: path.to_string(), add: a.parse().unwrap_or(0), del: d.parse().unwrap_or(0) })
        })
        .collect();
    for l in new.lines() {
        let Some((n, path)) = l.split_once('\t') else { continue };
        if !base.had(path) {
            files.push(File { path: path.to_string(), add: n.parse().unwrap_or(0), del: 0 });
        }
    }
    if files.is_empty() {
        return None;
    }
    let (add, del) = files.iter().fold((0, 0), |(a, d), f| (a + f.add, d + f.del));
    Some(Changes { files, add, del })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct File {
    pub path: String,
    pub add: u64,
    pub del: u64,
}

/// A turn's changes: its files (paths from the repo's root), and lines added and removed in all.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Changes {
    pub files: Vec<File>,
    pub add: u64,
    pub del: u64,
}

/// Git on this Mac as safe_git runs it: never a program the repository sets up.
fn git(dir: &str, args: &[&str]) -> Option<String> {
    let (out, code) = safe_git::run("", dir, &safe_git::no_repo_filters("", dir), args).ok()?;
    (code == 0).then_some(out)
}

fn untracked(root: &str) -> Vec<String> {
    git(root, &["ls-files", "--others", "--exclude-standard"]).map(|s| s.lines().map(String::from).collect()).unwrap_or_default()
}

/// The folder's state now; None outside a git repo.
pub fn base(cwd: &str) -> Option<Base> {
    if cwd.is_empty() {
        return None;
    }
    let root = git(cwd, &["rev-parse", "--show-toplevel"])?.trim().to_string();
    let snap = git(&root, &["stash", "create"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let commit = if snap.is_empty() { git(&root, &["rev-parse", "HEAD"])?.trim().to_string() } else { snap };
    let untracked = git(&root, &UNTRACKED_AT_BASE).map(|s| s.lines().map(String::from).collect()).unwrap_or_default();
    Some(Base { untracked, root, commit, machine: String::new() })
}

/// What changed since `base`: edited tracked files, and files that are new and not ignored. None: nothing.
pub fn since(base: &Base) -> Option<Changes> {
    if !base.machine.is_empty() {
        return since_on(base);
    }
    let mut files: Vec<File> = git(&base.root, &[&["diff", "--numstat", &base.commit][..], &crate::safe_git::QUIET_DIFF[..]].concat())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            let (a, d, path) = (p.next()?, p.next()?, p.next()?);
            // A binary file shows "-": it changed, with no lines to count.
            Some(File { path: path.to_string(), add: a.parse().unwrap_or(0), del: d.parse().unwrap_or(0) })
        })
        .collect();
    // SHORTCUT: a new file's lines are counted by reading it whole; fine for source files, slow for a big
    // new binary or data file. Upgrade: skip files over a size limit (count them as changed, no lines).
    for path in untracked(&base.root).into_iter().filter(|p| !base.had(p)) {
        let lines = std::fs::read(std::path::Path::new(&base.root).join(&path)).map(|b| b.iter().filter(|c| **c == b'\n').count() as u64).unwrap_or(0);
        files.push(File { path, add: lines, del: 0 });
    }
    if files.is_empty() {
        return None;
    }
    let (add, del) = files.iter().fold((0, 0), |(a, d), f| (a + f.add, d + f.del));
    Some(Changes { files, add, del })
}


#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn repo() -> String {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = format!("{}/cue-changes-{}-{}", std::env::temp_dir().display(), std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        std::fs::create_dir_all(&dir).unwrap();
        for args in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"]] {
            git(&dir, &args).unwrap();
        }
        std::fs::write(format!("{dir}/a.txt"), "one\ntwo\n").unwrap();
        git(&dir, &["add", "."]).unwrap();
        git(&dir, &["commit", "-qm", "first"]).unwrap();
        dir
    }

    #[test]
    fn only_what_the_turn_changed() {
        let dir = repo();
        // Uncommitted before the turn: not the turn's.
        std::fs::write(format!("{dir}/a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(format!("{dir}/old.txt"), "x\n").unwrap();
        let b = base(&dir).unwrap();
        assert_eq!(since(&b), None, "nothing changed during the turn");
        // The turn: edits a.txt again, adds new.txt.
        std::fs::write(format!("{dir}/a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
        std::fs::write(format!("{dir}/new.txt"), "1\n2\n3\n").unwrap();
        let c = since(&b).unwrap();
        assert_eq!(c.files, vec![File { path: "a.txt".into(), add: 2, del: 1 }, File { path: "new.txt".into(), add: 3, del: 0 }]);
        assert_eq!((c.add, c.del), (5, 1));
        // The snapshot didn't touch the folder or the stash list.
        assert_eq!(git(&dir, &["stash", "list"]).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_nobody_tracks_is_one_entry_and_its_files_were_there_before() {
        let dir = repo();
        std::fs::create_dir_all(format!("{dir}/build/deep")).unwrap();
        for i in 0..50 {
            std::fs::write(format!("{dir}/build/deep/f{i}.o"), "x\n").unwrap();
        }
        let b = base(&dir).unwrap();
        assert_eq!(b.untracked, vec!["build/".to_string()], "the folder, not its fifty files");
        assert!(b.had("build/deep/f3.o") && !b.had("builder.txt"));
        std::fs::write(format!("{dir}/new.txt"), "1\n").unwrap();
        let c = since(&b).unwrap();
        assert_eq!(c.files, vec![File { path: "new.txt".into(), add: 1, del: 0 }], "the build folder's files aren't the turn's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_saved_list_of_thousands_of_files_is_dropped_on_load() {
        let many: Vec<String> = (0..=OLD_LIST_MAX).map(|i| format!("target-wt/f{i}")).collect();
        let b: Base = serde_json::from_value(serde_json::json!({ "root": "/r", "commit": "c", "untracked": many })).unwrap();
        assert!(b.untracked.is_empty());
        let b: Base = serde_json::from_value(serde_json::json!({ "root": "/r", "commit": "c", "untracked": ["notes.md", "out/"] })).unwrap();
        assert_eq!(b.untracked, vec!["notes.md".to_string(), "out/".to_string()]);
    }

    #[test]
    fn a_folder_outside_git_has_none() {
        let dir = format!("{}/cue-nogit-{}", std::env::temp_dir().display(), std::process::id());
        std::fs::create_dir_all(&dir).unwrap();
        assert!(base(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

