//! What a turn changed, from git: when a turn starts, Cue notes the folder's state (a `git stash create`
//! snapshot, which writes a commit object and touches nothing else, or HEAD when nothing's uncommitted, plus
//! the untracked files); when it ends, it compares the folder against that. So only this turn's edits count,
//! not work left uncommitted before it. Edits another session makes in the same folder during the turn
//! count too: git can't tell them apart.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// Where a turn started from.
#[derive(Clone, Debug)]
pub struct Base {
    root: String,
    commit: String,
    untracked: Vec<String>,
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

fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
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
    Some(Base { untracked: untracked(&root), root, commit })
}

/// What changed since `base`: edited tracked files, and files that are new and not ignored. None: nothing.
pub fn since(base: &Base) -> Option<Changes> {
    let mut files: Vec<File> = git(&base.root, &["diff", "--numstat", &base.commit])
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
    for path in untracked(&base.root).into_iter().filter(|p| !base.untracked.contains(p)) {
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

    fn repo() -> String {
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
    fn a_folder_outside_git_has_none() {
        let dir = format!("{}/cue-nogit-{}", std::env::temp_dir().display(), std::process::id());
        std::fs::create_dir_all(&dir).unwrap();
        assert!(base(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
