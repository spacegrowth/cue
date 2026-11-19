//! Each item in its own copy, landed as one squash (hub/desk_trees.rs): an item routed to a session here gets a git
//! worktree of its own from the main line, and its holder is told to work there; at its DONE everything in it is
//! squashed into one commit, brought up to the main line, checked, and only then put on the main line (a
//! fast-forward); the copy goes once it landed. Three items of a repository have copies at once; a fourth waits. A
//! clash or a failed check goes back to its holder, nothing landed. A dropped item's copy with work in it is kept
//! and listed; one with none is removed.

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::desk::{InstallStatus, Installer};
use crate::testing::World;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// An installer whose check says what each test sets (None: the repository has no check), and that installs
/// nothing: what it was asked to check.
#[derive(Default)]
struct Checker {
    says: Mutex<Option<Result<(), String>>>,
    checked: Mutex<Vec<String>>,
    /// While it checks: this item sent back on this hub.
    sends_back: Mutex<Option<(std::sync::Weak<crate::hub::Hub>, String)>>,
}

impl Installer for Checker {
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
        crate::locking::lock(&self.checked, "checker checked").push(commit.to_string());
        let back = crate::locking::lock(&self.sends_back, "checker sends back").take();
        if let Some((hub, id)) = back {
            if let Some(hub) = hub.upgrade() {
                hub.desk_item_act(&id, "send_back", "Not like that").unwrap();
            }
        }
        crate::locking::lock(&self.says, "checker says").clone()
    }
}

/// The item routed: (repo, project, id, holder, its copy's folder).
fn routed(w: &World) -> (PathBuf, String, String, String, PathBuf) {
    let (dir, project, id, h) = routed_with(w, |_, _, _| {}, |_, _, _| {});
    let it = item(w, &project, "Header jumps");
    let path = PathBuf::from(it["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {it}")));
    (dir, project, id, h, path)
}

fn subject(dir: &Path) -> String {
    git(dir, &["log", "-1", "--format=%s"]).trim().to_string()
}

fn tree_of(w: &World, project: &str) -> Value {
    item(w, project, "Header jumps")["tree"].clone()
}

#[test]
fn a_routed_item_gets_its_own_copy_from_the_main_line_and_its_holder_is_told_to_work_there() {
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    assert!(path.join("README").is_file(), "a checkout of the main line");
    assert_eq!(git(&path, &["branch", "--show-current"]).trim(), format!("desk/{id}"));
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), git(&dir, &["rev-parse", "HEAD"]), "from the main line's tip");
    assert!(path.starts_with(std::fs::canonicalize(&dir).unwrap()) || path.starts_with(&dir), "inside the main checkout: {}", path.display());
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}"))).map(|(_, t)| t).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&path.to_string_lossy().to_string()) && told[0].contains("never in the main checkout"), "{}", told[0]);
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty(), "the copy isn't a change in the main checkout");
    assert_eq!(tree_of(&w, &project)["slot"], 1);
}

#[test]
fn done_lands_everything_in_the_copy_as_one_commit_and_removes_the_copy() {
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    // One file committed in the copy, one not.
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    git(&path, &["add", "a.txt"]);
    git(&path, &["commit", "-q", "-m", "wip"]);
    std::fs::write(path.join("b.txt"), "b\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), before, "one commit on the main line, no merge");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt");
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "b\n", "the main checkout's files moved with it");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{it}");
    assert_eq!(it["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert_eq!(it["finish"]["unchecked"], true, "no check to run: landed, not checked");
    assert!(it["tree"].is_null() && !path.exists(), "the copy is gone");
    assert!(git(&dir, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "and its branch");
}

#[test]
fn a_main_line_that_moved_meanwhile_is_brought_up_to_and_landed_on_top_of() {
    let w = World::new();
    let (dir, _, id, h, path) = routed(&w);
    std::fs::write(dir.join("shared.txt"), "someone else's\n").unwrap();
    git(&dir, &["add", "shared.txt"]);
    git(&dir, &["commit", "-q", "-m", "meanwhile"]);
    let meanwhile = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), meanwhile, "on top of what landed meanwhile");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "only its own work in its commit");
    assert!(dir.join("shared.txt").is_file());
}

#[test]
fn a_clash_with_the_main_line_goes_back_to_its_holder_and_nothing_lands() {
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    std::fs::write(dir.join("README"), "main's\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "main's readme"]);
    let main = git(&dir, &["rev-parse", "HEAD"]);
    std::fs::write(path.join("README"), "the item's\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), main, "nothing landed");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["finish"]["step"], "conflict", "{it}");
    let q = it["finish"]["question"].as_str().unwrap();
    assert!(q.contains("README") && q.contains(&path.to_string_lossy().to_string()), "{q}");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("isn't done") && t.contains("README")), "its holder is told");
    assert!(path.exists() && !tree_of(&w, &project).is_null(), "its copy stays for the fix");
}

#[test]
fn the_check_runs_before_landing_and_a_failed_one_lands_nothing_and_goes_back_with_its_output() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Err("window: deskFoo is declared twice".into()));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h, path) = routed(&w);
    let main = git(&dir, &["rev-parse", "HEAD"]);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), main, "nothing landed");
    let checked = crate::locking::lock(&checker.checked, "checker checked").clone();
    assert_eq!(checked.len(), 1, "checked once, on the commit it would land");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", &checked[0]]).trim(), format!("Header jumps (#{id})\n\na.txt"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["finish"]["step"], "check_failed", "{it}");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("Its check failed") && t.contains("deskFoo is declared twice")), "its holder gets the output");
    // Fixed and said DONE again: checked again, and landed.
    *crate::locking::lock(&checker.says, "checker says") = Some(Ok(()));
    std::fs::write(path.join("a.txt"), "a, fixed\n").unwrap();
    ends(&w, &h, &format!("Fixed it.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["unchecked"].as_bool()), (Some("to_check"), None), "checked: {it}");
}

#[test]
fn a_fourth_item_waits_for_a_free_copy_and_starts_in_it_once_one_is_free() {
    let w = World::new();
    let main = std::fs::canonicalize(w.home()).unwrap().to_string_lossy().to_string();
    // Three of the repository's copies taken by other items.
    let hold = |w: &World| {
        let top = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        top.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap_or(&main).to_string()
    };
    let (_, project, id, h) = routed_with(
        &w,
        |w, _, _| {
            let top = hold(w);
            w.hub().desk_hold_copies(&top, &[1, 2, 3]);
        },
        |_, _, _| {},
    );
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool(), it["tree"].is_null()), (Some("queued"), Some(true), true), "{it}");
    assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}"))), "not sent yet");
    let top = hold(&w);
    w.hub().desk_let_go_copy(&top, 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}")) && t.contains(&path)), "sent now, with its copy");
}

#[test]
fn an_item_waiting_for_a_copy_but_started_now_forgets_that_wait_once_it_has_one() {
    let w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, id, _) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["tree_wait"].as_bool(), Some(true));
    w.hub().desk_let_go_copy_quietly(2);
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
    // Otherwise a later queue (after another item) would read as waiting for a copy, and a freed copy would start it.
    assert_ne!(it["tree_wait"].as_bool(), Some(true), "{it}");
    assert!(it["tree_wait_repo"].as_str().unwrap_or("").is_empty(), "{it}");
}

#[test]
fn a_dropped_items_copy_with_work_is_kept_and_listed_and_one_without_is_removed() {
    let w = World::new();
    let (dir, project, id, _, path) = routed(&w);
    std::fs::write(path.join("a.txt"), "half done\n").unwrap();
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    let kept = desk(&w, &project)["kept"].clone();
    assert_eq!(kept.as_array().map(Vec::len), Some(1), "{kept}");
    let at = PathBuf::from(kept[0]["path"].as_str().unwrap());
    assert!(!path.exists() && at.ends_with(format!("kept-{id}")), "moved aside: {}", at.display());
    assert_eq!(std::fs::read_to_string(at.join("a.txt")).unwrap(), "half done\n", "its work is there");
    assert_eq!(crate::db::desk_kept().len(), 1, "saved");
    // Remove: the copy and its branch go, and it's off the list.
    w.hub().desk_item_act(&id, "remove_kept", "").unwrap();
    assert!(!at.exists() && git(&dir, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty());
    assert!(desk(&w, &project)["kept"].as_array().is_none_or(Vec::is_empty) && crate::db::desk_kept().is_empty());
}

#[test]
fn a_dropped_items_copy_with_nothing_in_it_is_removed() {
    let w = World::new();
    let (dir, project, id, _, path) = routed(&w);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert!(!path.exists() && git(&dir, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty());
    assert!(desk(&w, &project)["kept"].as_array().is_none_or(Vec::is_empty));
}

#[test]
fn cues_own_copies_share_its_node_modules_and_one_build_folder_and_removing_one_leaves_them() {
    let w = World::new();
    let (dir, _, id, h, path) = routed_with(
        &w,
        |_, _, dir| {
            std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
            std::fs::write(dir.join("src-tauri/tauri.conf.json"), "{ \"identifier\": \"dev.spacegrowth.cue\" }\n").unwrap();
            // On the main line, as in Cue's own repository: its copies have the folder the build link goes in.
            git(dir, &["add", "-f", "src-tauri/tauri.conf.json"]);
            git(dir, &["commit", "-q", "-m", "this Cue"]);
            std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
            std::fs::write(dir.join("node_modules/x/index.js"), "1\n").unwrap();
        },
        |_, _, _| {},
    )
    .pipe(|(dir, project, id, h)| {
        let it = item(&w, &project, "Header jumps");
        let path = PathBuf::from(it["tree"]["path"].as_str().unwrap());
        (dir, project, id, h, path)
    });
    let modules = std::fs::read_link(path.join("node_modules")).expect("node_modules linked");
    assert!(modules.ends_with("node_modules") && modules.join("x/index.js").is_file());
    let target = std::fs::read_link(path.join("src-tauri/target")).expect("the build folder linked");
    assert!(target.ends_with(".claude/worktrees/desk-target"), "{}", target.display());
    std::fs::write(target.join("built"), "1\n").unwrap();
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "the links aren't in its commit");
    assert!(!path.exists(), "the copy is gone");
    assert!(dir.join("node_modules/x/index.js").is_file() && target.join("built").is_file(), "what they pointed at stays");
}

trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}

#[test]
fn a_copy_left_with_work_in_its_folder_is_never_removed_and_the_item_starts_beside_it() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(
        &w,
        |_, _, dir| {
            // Left by a run that stopped: not a copy git knows, with someone's work in it.
            std::fs::create_dir_all(dir.join(".claude/worktrees/desk-1")).unwrap();
            std::fs::write(dir.join(".claude/worktrees/desk-1/notes.txt"), "half done\n").unwrap();
        },
        |_, _, _| {},
    );
    assert_eq!(std::fs::read_to_string(dir.join(".claude/worktrees/desk-1/notes.txt")).unwrap(), "half done\n", "left as it is");
    let path = tree_of(&w, &project)["path"].as_str().unwrap_or_else(|| panic!("no copy: {}", item(&w, &project, "Header jumps"))).to_string();
    assert!(path.ends_with(&format!("desk-1-{id}")), "{path}");
    assert!(Path::new(&path).join("README").is_file());
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}")) && t.contains(&path)), "its holder is told the folder it got");
}

#[test]
fn an_item_left_checking_when_cue_stopped_goes_back_in_the_line_and_lands() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Err("failed".into()));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h, path) = routed(&w);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    // Cue stopped while its check ran; on the next start the check passes.
    w.hub().desk_cut_check_short(&id);
    *crate::locking::lock(&checker.says, "checker says") = Some(Ok(()));
    w.advance(5000);
    w.hub().desk_tick_now();
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn a_failed_check_tried_again_lands() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Err("failed".into()));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h, path) = routed(&w);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["step"], "check_failed");
    *crate::locking::lock(&checker.says, "checker says") = Some(Ok(()));
    w.advance(5000);
    w.hub().desk_rerun_landing(&id).unwrap();
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
}

#[test]
fn an_item_sent_back_while_it_was_checked_doesnt_land_and_its_copy_stays() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Ok(()));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h, path) = routed_with(&w, |_, _, _| {}, |_, _, _| {}).pipe(|(dir, project, id, h)| {
        let path = PathBuf::from(tree_of(&w, &project)["path"].as_str().unwrap());
        (dir, project, id, h, path)
    });
    let main = git(&dir, &["rev-parse", "HEAD"]);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    *crate::locking::lock(&checker.sends_back, "checker sends back") = Some((Arc::downgrade(w.hub()), id.clone()));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(crate::locking::lock(&checker.checked, "checker checked").len(), 1, "it was checked");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), main, "nothing landed");
    let it = item(&w, &project, "Header jumps");
    assert_ne!(it["state"], "finishing", "{it}");
    assert!(path.join("a.txt").is_file() && it["tree"]["path"].as_str() == Some(path.to_str().unwrap()), "its holder's copy stays: {it}");
}

#[test]
fn a_freed_copy_goes_to_an_item_waiting_for_that_repository_not_one_waiting_elsewhere() {
    let w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, id, _) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued");
    // Queued longer, for another repository's copies.
    w.hub().desk_queue_for("/elsewhere/repo");
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "#{id}: {it}");
}

#[test]
fn an_item_in_the_line_whose_copy_turns_out_empty_is_finished_where_it_was() {
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    // The main checkout off any branch: it waits in the line.
    git(&dir, &["checkout", "-q", "--detach"]);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("finishing"), Some("line")), "{it}");
    git(&dir, &["checkout", "-q", "main"]);
    w.hub().desk_tick_now();
    let it = item(&w, &project, "Header jumps");
    assert_ne!(it["state"], "finishing", "not left finishing: {it}");
    assert!(it["tree"].is_null() && !path.exists(), "its empty copy is gone");
}

#[test]
fn an_item_an_older_cue_left_at_commit_wait_is_committed_after_the_upgrade() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Err("failed".into()));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h, path) = routed(&w);
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    w.hub().desk_set_step(&id, "commit_wait");
    *crate::locking::lock(&checker.says, "checker says") = Some(Ok(()));
    w.advance(5000);
    w.hub().desk_tick_now();
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}
