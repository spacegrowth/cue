//! Desk spec (#498), copies: each item works in a copy of its own (a git worktree on desk/<id>, made from main),
//! lands on main as one squash commit of only its own work, and its copy goes once it has landed; a copy whose work
//! hasn't landed stays. Sources: docs/desk-landing-design.md parts 1 and 2, docs/desk-finish-breaks-main-2026-11-18.md
//! ("The fix"), #376.

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::desk::{InstallStatus, Installer};
use crate::testing::World;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// An installer that installs nothing and whose check says what the test sets (None: no check), keeping each commit
/// it was asked to check.
#[derive(Default)]
pub(super) struct SpecChecker {
    pub(super) says: Mutex<Option<Result<(), String>>>,
    pub(super) checked: Mutex<Vec<String>>,
}

impl SpecChecker {
    pub(super) fn say(&self, r: Option<Result<(), String>>) {
        *crate::locking::lock(&self.says, "spec checker says") = r;
    }

    pub(super) fn checked(&self) -> Vec<String> {
        crate::locking::lock(&self.checked, "spec checker checked").clone()
    }
}

impl Installer for SpecChecker {
    fn has(&self, _repo: &str, _commit: &str) -> bool {
        false
    }

    fn start(&self, _repo: &str, _commit: &str, _tag: &str) -> Result<(), String> {
        Ok(())
    }

    fn status(&self) -> Option<InstallStatus> {
        None
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::self_install::is_ancestor(repo, commit, of)
    }

    fn check(&self, _repo: &str, commit: &str) -> Option<Result<(), String>> {
        crate::locking::lock(&self.checked, "spec checker checked").push(commit.to_string());
        crate::locking::lock(&self.says, "spec checker says").clone()
    }
}

/// The Desk lead's session of `project`.
pub(super) fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

/// A second item, `title`, routed by the lead to the same holder as "Header jumps": its id.
pub(super) fn another(w: &World, project: &str, title: &str) -> String {
    ends(w, &lead_of(w, project), &format!("ITEM: bug | {title} | done when: it shows\nROUTE: #new -> cue-teams | why: it made it"));
    w.advance(1000);
    item(w, project, title)["id"].as_str().unwrap().to_string()
}

/// Item `title`'s copy, as the Desk shows it.
pub(super) fn copy_of(w: &World, project: &str, title: &str) -> PathBuf {
    let it = item(w, project, title);
    PathBuf::from(it["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {it}")))
}

/// The files `commit` changed, one per line.
pub(super) fn files_of(dir: &Path, commit: &str) -> String {
    git(dir, &["show", "--name-only", "--format=", commit]).trim().to_string()
}

pub(super) fn subject(dir: &Path, commit: &str) -> String {
    git(dir, &["log", "-1", "--format=%s", commit]).trim().to_string()
}

pub(super) fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

pub(super) fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

/// Whether the Desk lists an item `id` at all.
pub(super) fn listed(w: &World, project: &str, id: &str) -> bool {
    desk(w, project)["items"].as_array().is_some_and(|a| a.iter().any(|i| i["id"] == id))
}

// Landing design part 1 ("Own worktree": one copy per item, on its own branch) and the 18 Nov fix (finishing
// "commits everything in that worktree", nobody else's work).
#[test]
fn two_items_at_once_work_in_copies_of_their_own_and_each_lands_only_its_own_work() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let f = another(&w, &project, "Footer");
    let (a, b) = (copy_of(&w, &project, "Header jumps"), copy_of(&w, &project, "Footer"));
    assert_ne!(a, b, "a copy each");
    assert_eq!(git(&a, &["branch", "--show-current"]).trim(), format!("desk/{id}"));
    assert_eq!(git(&b, &["branch", "--show-current"]).trim(), format!("desk/{f}"));
    std::fs::write(a.join("a.txt"), "header\n").unwrap();
    std::fs::write(b.join("b.txt"), "footer\n").unwrap();
    let start = head(&dir);

    ends(&w, &h, &format!("Fixed the header.\nDONE: #{id}"));
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), start, "one commit");
    assert_eq!(files_of(&dir, "HEAD"), "a.txt", "only the header's work");
    assert!(!dir.join("b.txt").exists(), "the footer's unfinished work isn't on main");
    assert!(!a.exists(), "the landed item's copy is gone");
    assert_eq!(std::fs::read_to_string(b.join("b.txt")).ok().as_deref(), Some("footer\n"), "the other copy is left as it was");
    assert_eq!(item(&w, &project, "Footer")["state"], "working");

    ends(&w, &h, &format!("Fixed the footer.\nDONE: #{f}"));
    assert_eq!(subject(&dir, "HEAD"), format!("Footer (#{f})"));
    assert_eq!(subject(&dir, "HEAD~1"), format!("Header jumps (#{id})"), "on top of the first");
    assert_eq!(files_of(&dir, "HEAD"), "b.txt", "only the footer's work");
    assert!(!b.exists());
}

// Landing design part 1 ("Kept while the item is open"; removed once its commit is on main) and part 3 (a failed
// check: nothing reaches main, the item goes back to its holder).
#[test]
fn a_copy_whose_check_failed_keeps_its_work_and_goes_only_once_the_fix_lands() {
    let w = World::new();
    let checker = std::sync::Arc::new(SpecChecker::default());
    checker.say(Some(Err("window: deskFoo is missing".into())));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = copy_of(&w, &project, "Header jumps");
    let start = head(&dir);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(head(&dir), start, "nothing landed");
    assert_eq!(std::fs::read_to_string(path.join("a.txt")).ok().as_deref(), Some("a\n"), "its work stays in its copy");
    assert_eq!(copy_of(&w, &project, "Header jumps"), path, "the same copy, still its");
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty(), "nothing of it in the main checkout");

    checker.say(Some(Ok(())));
    std::fs::write(path.join("a.txt"), "a, fixed\n").unwrap();
    ends(&w, &h, &format!("Fixed it.\nDONE: #{id}"));
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "a, fixed\n");
    assert!(!path.exists() && item(&w, &project, "Header jumps")["tree"].is_null(), "landed: its copy goes");
}

// 18 Nov fix, "What it costs": "Desk tests for a leak (another session edits the same file in the shared folder; the
// item's commit must not change)"; a new file another session left in the shared folder isn't the item's either.
#[test]
fn another_sessions_new_file_and_edit_in_the_shared_folder_stay_out_of_the_items_commit() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = copy_of(&w, &project, "Header jumps");
    std::fs::write(path.join("a.txt"), "the item's\n").unwrap();
    // Someone else, in the shared folder, at the same time: a new file and an edit to the item's file name.
    std::fs::write(dir.join("shared.txt"), "not the item's\n").unwrap();
    std::fs::write(dir.join("README"), "hi\nsomeone else's line\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"), "landed: {}", item(&w, &project, "Header jumps"));
    assert_eq!(files_of(&dir, "HEAD"), "a.txt", "the commit holds only the item's work");
    assert_eq!(git(&dir, &["show", "HEAD:README"]), "hi\n", "main's README is untouched");
    assert_eq!(std::fs::read_to_string(dir.join("shared.txt")).ok().as_deref(), Some("not the item's\n"), "their file is left where it was");
    assert_eq!(git(&dir, &["status", "--porcelain", "--", "shared.txt", "README"]).trim(), "M README\n?? shared.txt", "and still theirs, unsaved");
}
