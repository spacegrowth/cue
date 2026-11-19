//! Copies cleared away once their work is in the app (hub/desk_copies.rs, #376): as an item lands, its helpers'
//! copies whose work is on the main line go with its own; one with work that isn't stays. Copies left from before are
//! listed with their sizes, and removed only once you say so; one with work not on the main line, or in use, never is.

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use super::team::{git, says, starts};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;
use std::path::{Path, PathBuf};

/// The repository's main checkout, as git names it.
fn top(w: &World) -> String {
    let out = git(w.home(), &["worktree", "list", "--porcelain"]);
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// A copy of `dir` at `.claude/worktrees/<name>` on a new branch `branch`.
fn copy(dir: &Path, name: &str, branch: &str) -> PathBuf {
    let at = dir.join(".claude/worktrees").join(name);
    git(dir, &["worktree", "add", "-q", "-b", branch, &at.to_string_lossy()]);
    at
}

/// Lead `sid`'s teammate (agent `id`) finished.
fn finishes(w: &World, sid: &str, id: &str) {
    says(w, sid, [HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": id, "status": "completed", "summary": "Done." }) }]);
}

#[test]
fn as_an_item_lands_its_helpers_copies_whose_work_is_in_it_go_and_one_with_other_work_stays() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    // Two helpers in copies of their own: one's work the holder took into the item, the other's it didn't.
    let a = copy(&dir, "agent-agentA", "worktree-agent-agentA");
    let b = copy(&dir, "agent-agentB", "worktree-agent-agentB");
    starts(&w, &h, "toolu_A", "agentA", "rows", true);
    starts(&w, &h, "toolu_B", "agentB", "side", true);
    std::fs::write(a.join("a.txt"), "a\n").unwrap();
    git(&a, &["add", "a.txt"]);
    git(&a, &["commit", "-q", "-m", "rows"]);
    std::fs::write(b.join("b.txt"), "not taken\n").unwrap();
    finishes(&w, &h, "agentA");
    finishes(&w, &h, "agentB");
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "landed");
    assert!(!path.exists(), "the item's copy is gone");
    assert!(!a.exists(), "the helper whose work landed: its copy is gone");
    assert!(git(&dir, &["branch", "--list", "worktree-agent-agentA"]).trim().is_empty(), "and its branch");
    assert_eq!(std::fs::read_to_string(b.join("b.txt")).unwrap(), "not taken\n", "the one with work not on main stays, as it was");
}

#[test]
fn old_copies_whose_work_is_on_main_are_listed_with_their_size_and_removed_only_once_you_say_so() {
    let w = World::new();
    let (dir, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let item_copy = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    // Landed as a squash: its commit isn't on main, but what it changed is.
    let landed = copy(&dir, "desk-9", "desk/9");
    std::fs::write(landed.join("a.txt"), "landed\n").unwrap();
    git(&landed, &["add", "a.txt"]);
    git(&landed, &["commit", "-q", "-m", "wip"]);
    std::fs::write(dir.join("a.txt"), "landed\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "Its item (#9)"]);
    // Nothing in it (and build output, which git ignores, doesn't count).
    let empty = copy(&dir, "agent-old", "worktree-agent-old");
    std::fs::write(empty.join("build.log"), "x".repeat(10_000)).unwrap();
    // Work that isn't on main: never listed.
    let work = copy(&dir, "agent-work", "worktree-agent-work");
    std::fs::write(work.join("b.txt"), "mine\n").unwrap();

    let found = w.hub().desk_old_copies_find(&top(&w));
    let mut listed: Vec<String> = found.copies.iter().map(|c| Path::new(&c.path).file_name().unwrap().to_string_lossy().to_string()).collect();
    listed.sort();
    assert_eq!(listed, ["agent-old", "desk-9"], "not the item's copy (in use), not the one with work");
    assert!(found.kb > 0 && found.copies.iter().all(|c| c.kb > 0), "{found:?}");
    let shown = &desk(&w, &project)["old_copies"];
    assert_eq!(shown["copies"].as_array().map(|c| c.len()), Some(2), "listed on its Desk: {shown}");
    assert!(landed.exists() && empty.exists(), "nothing removed before you say so");

    // Meanwhile one gets work: looked at again, it stays.
    std::fs::write(empty.join("b.txt"), "new\n").unwrap();
    let r = w.hub().desk_old_copies_answer(&project, true).unwrap();
    assert_eq!(r["removed"], 1, "{r}");
    assert_eq!(r["kept"][0]["why"], "it has work not in the app yet", "{r}");
    assert!(!landed.exists() && git(&dir, &["branch", "--list", "desk/9"]).trim().is_empty(), "removed with its branch");
    assert!(empty.join("b.txt").exists() && work.join("b.txt").exists() && item_copy.exists(), "the rest untouched");
    assert!(desk(&w, &project)["old_copies"].is_null(), "answered: the list goes");
}

#[test]
fn not_now_hides_the_list_and_removes_nothing() {
    let w = World::new();
    let (dir, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let old = copy(&dir, "agent-old", "worktree-agent-old");
    assert_eq!(w.hub().desk_old_copies_find(&top(&w)).copies.len(), 1);
    w.hub().desk_old_copies_answer(&project, false).unwrap();
    assert!(desk(&w, &project)["old_copies"].is_null());
    assert!(old.exists());
}
