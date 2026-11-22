//! Work its holder did outside the copy Cue gave it (desk/hub/landing/own_work.rs, #481): an item started without a copy,
//! whose holder works on a branch it named itself, in a folder it made, or commits and removes the folder, is landed
//! at its DONE like any other; and one whose commit Cue can't land never goes to check: back to its session,
//! saying which commit and where.

use super::desk_commit::{ends, item, log, routed_with};
use super::team::git;
use crate::testing::World;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `dir`'s main checkout as git names it.
fn top(dir: &Path) -> String {
    git(dir, &["worktree", "list", "--porcelain"]).lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// An item routed while every copy is in use, then started without one (as #455 was, before #653 gave one started now
/// a copy past the count, and #712 a copy within the count you raise; an item saved so is still at work that way):
/// (repo, project, id, holder).
fn started_without_a_copy(w: &World) -> (PathBuf, String, String, String) {
    let (dir, project, id, h) = routed_with(w, |w, _, dir| w.hub().desk_hold_copies(&top(dir), &[1, 2, 3]), |_, _, _| {});
    w.hub().desk_item_act(&id, "start_now_raise", "").unwrap();
    w.settle();
    w.hub().desk_at_work_without_copy(&id, &top(&dir));
    let it = item(w, &project, "Header jumps");
    assert!(it["state"] == "working" && it["tree"].is_null(), "{it}");
    (dir, project, id, h)
}

/// Its holder at work in `folder`, as Claude Code logs it (each line's `cwd`).
fn works_in(w: &World, h: &str, folder: &Path) {
    log(w, h).entry(json!({ "type": "assistant", "cwd": folder, "message": { "content": [{ "type": "text", "text": "On it." }] } }));
}

/// Its holder commits `file` in `folder` from its shell, git's line for it in its log.
fn commits(w: &World, h: &str, folder: &Path, file: &str) {
    std::fs::write(folder.join(file), "fixed\n").unwrap();
    git(folder, &["add", file]);
    git(folder, &["commit", "-q", "-m", "The fix"]);
    let branch = git(folder, &["symbolic-ref", "--short", "HEAD"]).trim().to_string();
    let sha = git(folder, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    let call = format!("toolu_c{}", crate::clock::now_ms());
    let l = log(w, h);
    l.entry(json!({ "type": "assistant", "cwd": folder, "message": { "content": [{ "type": "tool_use", "id": call, "name": "Bash", "input": { "command": format!("git add {file} && git commit -m 'The fix'") } }] } }));
    l.entry(json!({ "type": "user", "cwd": folder, "message": { "content": [{ "type": "tool_result", "tool_use_id": call, "content": format!("[{branch} {sha}] The fix\n 1 file changed, 1 insertion(+)") }] } }));
    w.settle();
}

#[test]
fn a_branch_its_holder_named_itself_in_a_folder_it_made_is_landed_at_its_done() {
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let own = w.home().join("my-grip");
    git(&dir, &["worktree", "add", "-q", "-b", "fix-grip", own.to_str().unwrap()]);
    works_in(&w, &h, &own);
    commits(&w, &h, &own, "b.txt");
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).ok().as_deref(), Some("fixed\n"), "its branch is merged into main");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), "Merge fix-grip: Header jumps");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn a_commit_on_a_branch_whose_folder_is_gone_is_landed_at_its_done() {
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let own = w.home().join("scratch");
    git(&dir, &["worktree", "add", "-q", "-b", "quick-fix", own.to_str().unwrap()]);
    commits(&w, &h, &own, "b.txt");
    git(&dir, &["worktree", "remove", own.to_str().unwrap()]);
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).ok().as_deref(), Some("fixed\n"), "its branch is merged into main");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn a_done_whose_commit_cue_cant_land_goes_back_to_its_session_not_to_check() {
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]);
    // Committed in a clone of its own, on a branch there: not in this repository, so nothing Cue can merge.
    let clone = w.home().join("clone");
    git(&dir, &["clone", "-q", dir.to_str().unwrap(), clone.to_str().unwrap()]);
    git(&clone, &["checkout", "-q", "-b", "side"]);
    git(&clone, &["config", "user.email", "t@example.com"]);
    git(&clone, &["config", "user.name", "T"]);
    works_in(&w, &h, &clone);
    commits(&w, &h, &clone, "b.txt");
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before, "nothing landed");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "back with its session, never to check: {it}");
    let told = w.hosted.sent().into_iter().find(|(s, t)| *s == h && t.contains("isn't on main yet"));
    let (_, told) = told.unwrap_or_else(|| panic!("its session is told why: {:?}", w.hosted.sent()));
    assert!(told.contains("on side in") && told.contains("clone"), "which commit and where: {told}");
}

#[test]
fn a_done_with_nothing_changed_anywhere_still_goes_to_check() {
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]);
    works_in(&w, &h, &dir);
    ends(&w, &h, &format!("Nothing needed changing: here is why.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

/// Another item's copy (one of the stand-ins holding the copies) made for real: its own branch with a commit to
/// a.txt that clashes with one main has.
fn another_items_copy_that_clashes(dir: &Path) -> PathBuf {
    let copy = PathBuf::from(format!("{}/.claude/worktrees/held-1", top(dir)));
    git(dir, &["worktree", "add", "-q", "-b", "held/1", copy.to_str().unwrap()]);
    std::fs::write(copy.join("a.txt"), "theirs\n").unwrap();
    git(&copy, &["add", "a.txt"]);
    git(&copy, &["commit", "-q", "-m", "Theirs"]);
    std::fs::write(dir.join("a.txt"), "ours\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "Ours"]);
    copy
}

#[test]
fn a_holder_that_looked_in_another_items_copy_never_gets_that_items_clash() {
    // #551: #406, started without a copy, kept coming back "isn't done: Its work clashes with main … in desk-1, merge
    // main into desk/510" four times: its holder had looked in #510's copy, and Cue merged that copy as #406's work.
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let copy = another_items_copy_that_clashes(&dir);
    works_in(&w, &h, &copy.join("ui"));
    works_in(&w, &h, &copy);
    works_in(&w, &h, &dir);
    let before = git(&dir, &["rev-parse", "HEAD"]);
    for round in 1..=4 {
        ends(&w, &h, &format!("Built.\nDONE: #{id}"));
        let it = item(&w, &project, "Header jumps");
        assert_ne!(it["state"], "working", "round {round}: never sent back over another item's copy: {it}");
        assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("isn't done")), "round {round}: {:?}", w.hosted.sent());
    }
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before, "the other item's work stays on its own branch");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "ours\n");
}

#[test]
fn a_closed_item_is_never_sent_back() {
    // #551: #406 was closed as on main while a landing of it still ran; that landing's clash sent it back anyway.
    let w = World::new();
    let (dir, project, id, h) = started_without_a_copy(&w);
    let it = w.hub().read(|st| st.desk.item(&id).cloned()).unwrap();
    // The lead closes it as on main, as #406 was.
    let sha = git(&dir, &["rev-parse", "--short=9", "HEAD"]).trim().to_string();
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(&w, &lead, &format!("CLOSE: #{id} | commit: {sha} | why: already on main"));
    w.advance(1000);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done", "closed");
    // Then the landing that was still running finds its clash.
    w.hub().finish_clash(&it, "Its work clashes with main in a.txt");
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("isn't done")), "{:?}", w.hosted.sent());
}
