//! "A branch" (desk/lands.rs, desk/hub/landing/in_place.rs): an item works on a branch of its own in the project's own
//! folder, one at a time. Cue switches the folder onto `desk/<item>` as it starts, lands its work on main as one
//! commit at DONE, puts the folder back on main and removes the branch. Meanwhile other items land on main, never on
//! its branch, and never touch the folder's files.

use super::desk_commit::{ends, item, log, routed_with, tick};
use super::desk_lands::set;
use super::desk_spec_copies::{another, copy_of, sent_to};
use super::team::git;
use crate::testing::World;
use serde_json::json;
use std::path::Path;

fn on(dir: &Path) -> String {
    git(dir, &["branch", "--show-current"]).trim().to_string()
}

fn tracked_changes(dir: &Path) -> String {
    git(dir, &["status", "--porcelain", "--untracked-files=no"]).trim().to_string()
}

fn has_branch(dir: &Path, b: &str) -> bool {
    !git(dir, &["branch", "--list", b]).trim().is_empty()
}

/// An item routed with the project set to a branch: (repo, project, id, holder).
fn on_a_branch(w: &World) -> (std::path::PathBuf, String, String, String) {
    routed_with(w, |w, h, _| set(w, h, json!({ "where": "branch" })), |_, _, _| {})
}

#[test]
fn a_started_item_puts_the_folder_on_its_own_branch_and_a_second_item_waits() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["in_place"].as_bool()), (Some("working"), Some(true)), "{it}");
    assert_eq!(on(&dir), format!("desk/{id}"), "the folder is on the item's branch");
    let told: Vec<String> = sent_to(&w, &h).into_iter().filter(|t| t.contains(&format!("Desk item #{id}"))).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("branch desk/{id}")) && told[0].contains("project's own folder"), "{}", told[0]);
    let main_line = crate::desk::hub::landing::main_line::main_checkout(&dir.to_string_lossy()).map(|(_, b)| b);
    assert_eq!(main_line.as_deref(), Some("main"), "main is still the main line");
    let (f, _) = another(&w, &project, "Footer");
    let it = item(&w, &project, "Footer");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("queued"), true), "one item at a time: {it}");
    assert_eq!(on(&dir), format!("desk/{id}"), "#{f} didn't move the folder");
}

#[test]
fn done_lands_one_commit_on_main_puts_the_folder_back_and_the_next_item_starts_on_its_branch() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let before = git(&dir, &["rev-parse", "main"]).trim().to_string();
    let (f, _) = another(&w, &project, "Footer");
    // One file committed on the branch, one not.
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "wip"]);
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s", "main"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["rev-parse", "main~1"]).trim(), before, "one commit on main, no merge");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "main"]).trim(), "a.txt\nb.txt");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert!(it["tree"].is_null(), "{it}");
    assert!(!has_branch(&dir, &format!("desk/{id}")), "its branch is gone");
    // The next item took the folder: it's on that one's branch now, cut from the landed main.
    let next = item(&w, &project, "Footer");
    assert_eq!((next["state"].as_str(), next["tree"]["in_place"].as_bool()), (Some("working"), Some(true)), "{next}");
    assert_eq!(on(&dir), format!("desk/{f}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), git(&dir, &["rev-parse", "main"]), "from main's tip");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "b\n");
}

#[test]
fn done_with_nobody_waiting_leaves_the_folder_on_main_and_clean() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert_eq!(on(&dir), "main");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert!(!has_branch(&dir, &format!("desk/{id}")));
    assert!(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(&dir).args(["config", "--get", "cue.mainline"]).output().unwrap().stdout.is_empty(), "the main line isn't recorded any more");
}

#[test]
fn another_items_copy_lands_on_main_without_touching_the_folder_or_the_items_branch() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "half done\n").unwrap();
    let branch_tip = git(&dir, &["rev-parse", &format!("desk/{id}")]);
    // Another Desk's item on the same repository, in an own copy.
    set(&w, &h, json!({ "where": "copy" }));
    let (f, fh) = another(&w, &project, "Footer");
    let path = copy_of(&w, &project, "Footer");
    std::fs::write(path.join("b.txt"), "footer\n").unwrap();
    ends(&w, &fh, &format!("Fixed.\nDONE: #{f}"));
    assert_eq!(item(&w, &project, "Footer")["state"], "to_check", "{}", item(&w, &project, "Footer"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s", "main"]).trim(), format!("Footer (#{f})"), "main moved");
    assert_eq!(on(&dir), format!("desk/{id}"), "the folder stays on the item's branch");
    assert_eq!(git(&dir, &["rev-parse", &format!("desk/{id}")]), branch_tip, "its branch untouched");
    assert!(!dir.join("b.txt").exists(), "the folder's files aren't touched");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "half done\n");
    // The branch item then lands on top of it.
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["log", "-2", "--format=%s", "main"]).trim(), format!("Header jumps (#{id})\nFooter (#{f})"));
    assert_eq!((on(&dir), std::fs::read_to_string(dir.join("b.txt")).unwrap()), ("main".to_string(), "footer\n".to_string()));
    assert!(tracked_changes(&dir).is_empty());
}

#[test]
fn a_folder_with_uncommitted_edits_works_in_place_and_says_why() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(
        &w,
        |w, h, dir| {
            set(w, h, json!({ "where": "branch" }));
            std::fs::write(dir.join("README"), "someone's edit\n").unwrap();
        },
        |_, _, _| {},
    );
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("working"), true), "{it}");
    assert_eq!(on(&dir), "main", "left on main");
    assert_eq!(std::fs::read_to_string(dir.join("README")).unwrap(), "someone's edit\n");
    let told = sent_to(&w, &h).join("\n");
    assert!(told.contains(&format!("Desk item #{id}: Cue couldn't put")) && told.contains("changes not committed"), "{told}");
}

#[test]
fn a_dropped_item_puts_the_folder_back_on_main_and_keeps_its_work_on_its_branch() {
    let w = World::new();
    let (dir, _project, id, _) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "half done\n").unwrap();
    std::fs::write(dir.join("README"), "changed\n").unwrap();
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert_eq!(on(&dir), "main");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert!(!dir.join("a.txt").exists());
    assert_eq!(std::fs::read_to_string(dir.join("README")).unwrap(), "hi\n");
    let b = format!("desk/{id}");
    assert_eq!(git(&dir, &["show", &format!("{b}:a.txt")]), "half done\n", "its work is kept on its branch");
    assert_eq!(git(&dir, &["show", &format!("{b}:README")]), "changed\n");
}

#[test]
fn a_dropped_item_with_no_work_leaves_no_branch() {
    let w = World::new();
    let (dir, _project, id, _) = on_a_branch(&w);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert_eq!(on(&dir), "main");
    assert!(!has_branch(&dir, &format!("desk/{id}")));
}

fn recorded(dir: &Path) -> String {
    String::from_utf8(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(dir).args(["config", "--get", "cue.mainline"]).output().unwrap().stdout).unwrap().trim().to_string()
}

/// Session "tidier", working in the project's folder, wrote `file` there (its Write in its log): its id.
fn tidier_wrote(w: &World, dir: &Path, file: &str) -> String {
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy", "tidier", &opts).unwrap();
    let o = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&o, crate::hosted::HostedEvent::Taken { text: "tidy".into() });
    w.advance(1000);
    std::fs::write(dir.join(file), "tidier's\n").unwrap();
    log(w, &o).tool_use("toolu_t1", "Write", json!({ "file_path": dir.join(file), "content": "tidier's\n" }));
    w.settle();
    o
}

#[test]
fn a_new_file_not_committed_in_the_folder_keeps_the_item_in_place_and_says_why() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(
        &w,
        |w, h, dir| {
            set(w, h, json!({ "where": "branch" }));
            // a.txt: the World ignores every file but a few, and an ignored file is never taken in.
            std::fs::write(dir.join("a.txt"), "mine, not committed\n").unwrap();
        },
        |_, _, _| {},
    );
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("working"), true), "{it}");
    assert_eq!(on(&dir), "main", "left on main");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "mine, not committed\n");
    let told = sent_to(&w, &h).join("\n");
    assert!(told.contains(&format!("Desk item #{id}: Cue couldn't put")) && told.contains("a.txt"), "{told}");
}

#[test]
fn another_sessions_file_in_the_folder_holds_the_landing_and_is_neither_landed_nor_removed() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let repo = std::path::PathBuf::from(item(&w, &project, "Header jumps")["tree"]["repo"].as_str().unwrap());
    let before = git(&dir, &["rev-parse", "main"]);
    std::fs::write(dir.join("a.txt"), "the item's\n").unwrap();
    tidier_wrote(&w, &repo, "b.txt");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "finishing", "{it}");
    let wait = it["finish"]["wait"].as_str().unwrap_or_default();
    assert!(wait.contains("tidier") && wait.contains("b.txt"), "names who and which: {it}");
    assert_eq!(git(&dir, &["rev-parse", "main"]), before, "nothing landed");
    assert_eq!(on(&dir), format!("desk/{id}"), "the folder stays as it is");
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "tidier's\n");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "the item's\n");
    // Dropped meanwhile: the folder stays on its branch with both files, and the lead is told, once.
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    tick(&w);
    tick(&w);
    assert_eq!(on(&dir), format!("desk/{id}"));
    assert_eq!((std::fs::read_to_string(dir.join("b.txt")).unwrap(), std::fs::read_to_string(dir.join("a.txt")).unwrap()), ("tidier's\n".into(), "the item's\n".into()));
    let lead = super::desk_spec_copies::lead_of(&w, &project);
    let told: Vec<String> = sent_to(&w, &lead).into_iter().filter(|t| t.contains("tidier changed b.txt")).collect();
    assert_eq!(told.len(), 1, "{told:?}");
}

#[test]
fn a_copy_landing_while_the_folder_is_on_a_branch_finishes_on_mains_commit_its_after_steps_waiting_for_the_folder() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "half done\n").unwrap();
    let mark = std::env::temp_dir().join(format!("cue-after-{}-{}", std::process::id(), crate::clock::now_ms()));
    set(&w, &h, json!({ "where": "copy", "after": "command", "after_steps": [format!("echo ran >> {}", mark.display())] }));
    let (f, fh) = another(&w, &project, "Footer");
    std::fs::write(copy_of(&w, &project, "Footer").join("b.txt"), "footer\n").unwrap();
    ends(&w, &fh, &format!("Fixed.\nDONE: #{f}"));
    let it = item(&w, &project, "Footer");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s", "main"]).trim(), format!("Footer (#{f})"), "on main: {it}");
    assert_eq!(it["state"], "finishing", "{it}");
    assert_eq!(it["finish"]["commit"].as_str(), Some(git(&dir, &["rev-parse", "main"]).trim()), "main's commit, not the branch's: {it}");
    assert!(it["finish"]["wait"].as_str().unwrap_or_default().contains(&format!("desk/{id}")), "{it}");
    assert!(!mark.exists(), "its after steps didn't run on the other item's files");
    tick(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], "finishing");
    // The branch item lands; the folder is back on main, and the copy's steps run on the tick.
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(on(&dir), "main");
    tick(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], "to_check", "{}", item(&w, &project, "Footer"));
    assert!(mark.exists());
    let _ = std::fs::remove_file(&mark);
}

#[test]
fn a_folder_that_cant_go_back_holds_the_item_till_it_can_then_it_lands() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    // git can't write the folder's index while this is there.
    let lock = std::path::PathBuf::from(git(&dir, &["rev-parse", "--absolute-git-dir"]).trim()).join("index.lock");
    std::fs::write(&lock, "").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "finishing", "not landed while its folder isn't back: {it}");
    assert!(it["finish"]["wait"].as_str().unwrap_or_default().contains("couldn't go back to main"), "{it}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s", "main"]).trim(), format!("Header jumps (#{id})"), "its work is on main");
    assert_eq!(on(&dir), format!("desk/{id}"));
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
    std::fs::remove_file(&lock).unwrap();
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_eq!(on(&dir), "main");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert_eq!(git(&dir, &["log", "-2", "--format=%s", "main"]).trim().lines().filter(|l| l.contains(&format!("#{id}"))).count(), 1, "landed once");
    assert!(recorded(&dir).is_empty());
}

#[test]
fn a_folder_left_on_a_branch_with_no_item_at_work_goes_back_to_main_on_the_tick() {
    let w = World::new();
    let (dir, project, id, _) = on_a_branch(&w);
    std::fs::write(dir.join("a.txt"), "half done\n").unwrap();
    // Cue lost the item's record of its folder (stopped midway).
    w.hub().write(|st| {
        if let Some(i) = st.desk.item_mut(&id) {
            (i.tree, i.state) = (None, "dropped".into());
        }
    });
    assert_eq!(recorded(&dir), "main", "{}", item(&w, &project, "Header jumps"));
    tick(&w);
    assert_eq!(on(&dir), "main");
    assert!(recorded(&dir).is_empty());
    assert!(!dir.join("a.txt").exists());
    assert_eq!(git(&dir, &["show", &format!("desk/{id}:a.txt")]), "half done\n", "its work kept on its branch");
}

#[test]
fn an_item_the_lead_starts_while_the_folder_is_on_a_branch_waits_for_it() {
    let w = World::new();
    let (dir, project, id, _) = on_a_branch(&w);
    let (f, _) = another(&w, &project, "Footer");
    let lead = super::desk_spec_copies::lead_of(&w, &project);
    ends(&w, &lead, &format!("START: #{f} | why: the user needs it now"));
    w.advance(1000);
    let it = item(&w, &project, "Footer");
    assert!(it["tree"].is_null() && it["state"] == "queued", "{it}");
    assert_eq!(on(&dir), format!("desk/{id}"));
}

/// The project's folder as the item's record has it (its real path: others' edits are matched by it).
fn repo_of(w: &World, project: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(item(w, project, "Header jumps")["tree"]["repo"].as_str().unwrap())
}

#[test]
fn a_drop_whose_let_go_had_to_wait_is_finished_by_the_tick_after_the_item_is_gone_its_holders_work_its_own() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let repo = repo_of(&w, &project);
    std::fs::write(repo.join("a.txt"), "half done\n").unwrap();
    log(&w, &h).tool_use("toolu_h1", "Write", json!({ "file_path": repo.join("a.txt"), "content": "half done\n" }));
    w.settle();
    // A landing under way: the drop can't put the folder back now.
    w.hub().write(|st| st.desk.run.finish_claims.insert("landing".into()));
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert_eq!(on(&dir), format!("desk/{id}"));
    assert!(w.hub().read(|st| st.desk.item(&id).is_none()), "the item is gone from the Desk");
    w.hub().write(|st| st.desk.run.finish_claims.remove("landing"));
    tick(&w);
    assert_eq!(on(&dir), "main", "its holder's file is its own work, not someone else's");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert!(!dir.join("a.txt").exists());
    assert_eq!(git(&dir, &["show", &format!("desk/{id}:a.txt")]), "half done\n", "its work kept on its branch");
    assert!(recorded(&dir).is_empty());
}

#[test]
fn a_dropped_copy_item_picked_up_again_on_a_branch_starts_its_branch_from_the_kept_copy() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "where": "copy" })), |_, _, _| {});
    let path = copy_of(&w, &project, "Header jumps");
    std::fs::write(path.join("a.txt"), "half done\n").unwrap();
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    let kept = std::path::PathBuf::from(super::desk::desk(&w, &project)["kept"][0]["path"].as_str().unwrap());
    set(&w, &h, json!({ "where": "branch" }));
    w.hub().desk_item_act(&id, "pickup_kept", "").unwrap();
    let lead = super::desk_spec_copies::lead_of(&w, &project);
    ends(&w, &lead, &format!("ROUTE: #{id} -> cue-teams | why: it had started it"));
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["in_place"].as_bool()), (Some("working"), Some(true)), "{it}");
    assert!(it["kept_copy"].is_null(), "{it}");
    let b = it["tree"]["branch"].as_str().unwrap().to_string();
    assert!(b.starts_with(&format!("desk/{id}")), "{b}");
    assert_eq!(on(&dir), b, "the folder is on its branch");
    assert!(!kept.exists(), "the kept copy went, not moved onto the folder");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "half done\n", "its kept work is in the folder");
    assert_eq!(git(&dir, &["worktree", "list"]).lines().count(), 1, "no worktree left");
    let holder = it["holder"]["sid"].as_str().unwrap().to_string();
    ends(&w, &holder, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert_eq!(git(&dir, &["show", "main:a.txt"]), "half done\n");
    assert_eq!(on(&dir), "main");
}

/// The item ends while its folder can't go back (git can't write its index), so its landing waits for that.
fn landed_but_not_back(w: &World, dir: &Path, h: &str, id: &str) -> std::path::PathBuf {
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    let lock = std::path::PathBuf::from(git(dir, &["rev-parse", "--absolute-git-dir"]).trim()).join("index.lock");
    std::fs::write(&lock, "").unwrap();
    ends(w, h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(dir, &["log", "-1", "--format=%s", "main"]).trim().to_string(), format!("Header jumps (#{id})"));
    lock
}

#[test]
fn its_holders_change_after_it_landed_is_kept_on_its_branch_and_the_item_finishes_with_the_folder_on_main() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let repo = repo_of(&w, &project);
    let lock = landed_but_not_back(&w, &dir, &h, &id);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
    // Its holder writes on after its DONE.
    std::fs::write(repo.join("b.txt"), "late\n").unwrap();
    log(&w, &h).tool_use("toolu_h2", "Write", json!({ "file_path": repo.join("b.txt"), "content": "late\n" }));
    w.settle();
    std::fs::remove_file(&lock).unwrap();
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_eq!(on(&dir), "main");
    assert!(tracked_changes(&dir).is_empty(), "clean: {}", tracked_changes(&dir));
    assert!(!dir.join("b.txt").exists() && git(&dir, &["ls-tree", "--name-only", "main"]).lines().all(|l| l != "b.txt"), "not on main");
    let b = format!("desk/{id}-late");
    assert_eq!(git(&dir, &["show", &format!("{b}:b.txt")]), "late\n", "kept on a branch of its own");
    assert_eq!(git(&dir, &["show", &format!("{b}:a.txt")]), "a\n", "with the landed work under it");
    let told = sent_to(&w, &h).join("\n");
    assert!(told.contains(&format!("kept on branch {b}")) && told.contains("b.txt"), "{told}");
}

#[test]
fn another_sessions_change_after_it_landed_holds_the_folder_and_the_lead_is_told_once() {
    let w = World::new();
    let (dir, project, id, h) = on_a_branch(&w);
    let repo = repo_of(&w, &project);
    let lock = landed_but_not_back(&w, &dir, &h, &id);
    tidier_wrote(&w, &repo, "b.txt");
    std::fs::remove_file(&lock).unwrap();
    tick(&w);
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "finishing", "{it}");
    assert!(it["finish"]["wait"].as_str().unwrap_or_default().contains("tidier changed b.txt"), "{it}");
    assert_eq!(on(&dir), format!("desk/{id}"));
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "tidier's\n");
    let lead = super::desk_spec_copies::lead_of(&w, &project);
    assert_eq!(sent_to(&w, &lead).into_iter().filter(|t| t.contains("tidier changed b.txt")).count(), 1);
}

#[test]
fn a_commit_in_another_worktree_of_the_repository_isnt_refused_while_an_item_works_on_its_branch() {
    let w = World::new();
    let (dir, project, id, _) = on_a_branch(&w);
    let repo = repo_of(&w, &project);
    let other = repo.join(".claude/worktrees/elsewhere");
    git(&repo, &["worktree", "add", "-q", "-b", "elsewhere", &other.to_string_lossy()]);
    let not = |f: &Path| crate::desk::hub::landing::commit::not_on_item_branch("", &f.to_string_lossy(), "it");
    assert!(not(&other).is_ok(), "{:?}", not(&other));
    assert!(not(&repo).is_err_and(|e| e.contains(&format!("desk/{id}"))));
    let _ = dir;
}
