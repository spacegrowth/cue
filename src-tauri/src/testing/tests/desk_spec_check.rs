//! Desk spec (#498), to check and after: Looks right closes a landed item; Send back with a note puts it to work again
//! with the note at its holder, its landed commit kept and the fix landing as a second commit; Reopen takes a done item
//! back to check; Drop takes an item off the Desk and lands nothing more of it. Sources: docs/desk-landing-design.md
//! part 4, #181, #396, #484.

use super::desk_commit::{ends, item, routed_with};
use super::desk_spec_copies::{copy_of, files_of, head, listed, sent_to, subject};
use super::team::git;
use crate::testing::World;

/// An item landed from its copy with a.txt, now to check: (main checkout, project, id, holder, its commit).
fn landed(w: &World) -> (std::path::PathBuf, String, String, String, String) {
    let (dir, project, id, h) = routed_with(w, |_, _, _| {}, |_, _, _| {});
    std::fs::write(copy_of(w, &project, "Header jumps").join("a.txt"), "first\n").unwrap();
    ends(w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    let c = head(&dir);
    (dir, project, id, h, c)
}

// Landing design part 4: Looks right from Landed on closes it; "Only the owner closes an item"; #181 "Looks right only
// closes" (nothing else happens to main).
#[test]
fn looks_right_on_a_landed_item_closes_it_and_leaves_main_as_it_is() {
    let w = World::new();
    let (dir, project, id, _, c) = landed(&w);
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "done", "{it}");
    assert_eq!(head(&dir), c, "no new commit");
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty());
}

// Landing design part 4, Send back after landing, "Fix it": "leaves the commit on main and reopens the item, and the
// fix lands as a second commit"; #396 (your exact words reach its holder); #484 (at work until it says DONE again).
#[test]
fn send_back_after_landing_keeps_its_commit_sends_your_words_and_the_fix_lands_as_a_second_commit() {
    let w = World::new();
    let (dir, project, id, h, first) = landed(&w);
    let note = "The header still jumps on narrow windows";
    w.hub().desk_item_act(&id, "send_back", note).unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "at work again: {it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains(note)), "your words reach its holder: {:?}", sent_to(&w, &h));
    assert_eq!(head(&dir), first, "its first commit stays on main");
    let path = copy_of(&w, &project, "Header jumps");
    assert_eq!(std::fs::read_to_string(path.join("a.txt")).ok().as_deref(), Some("first\n"), "it works on from what landed");

    std::fs::write(path.join("a.txt"), "first\nnarrow too\n").unwrap();
    ends(&w, &h, &format!("Fixed narrow windows.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), first, "the fix on top of the first commit");
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"));
    assert_eq!(files_of(&dir, "HEAD"), "a.txt");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    let shas: Vec<String> = it["commit"].as_str().unwrap().split_whitespace().map(str::to_string).collect();
    let short = |c: &str| git(&dir, &["rev-parse", "--short", c]).trim().to_string();
    assert!(shas.contains(&short("HEAD")) && shas.contains(&short(&first)), "both of its commits are its: {it}");
}

// Reopen (Undo on a done item, desk_act.rs "reopen"): back to check, its commit untouched; it can be closed again.
#[test]
fn reopen_on_a_done_item_puts_it_back_to_check_with_its_commit_and_it_closes_again() {
    let w = World::new();
    let (dir, project, id, _, c) = landed(&w);
    assert!(w.hub().desk_item_act(&id, "reopen", "").is_err(), "not done yet: nothing to reopen");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert!(w.hub().desk_item_act(&id, "reopen", "").is_ok());
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_eq!(head(&dir), c, "main untouched");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(head(&dir), c);
}

// Drop: the item leaves the Desk and its holder is told to stop; whatever its holder says after, nothing of it lands.
// Landing design part 1: a dropped item's copy with work in it is kept (desk_land.rs covers the keeping).
#[test]
fn a_dropped_item_at_work_tells_its_holder_to_stop_and_a_late_done_lands_nothing() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    std::fs::write(copy_of(&w, &project, "Header jumps").join("a.txt"), "half\n").unwrap();
    let start = head(&dir);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert!(!listed(&w, &project, &id), "off the Desk");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id} was dropped"))), "{:?}", sent_to(&w, &h));
    ends(&w, &h, &format!("Finished anyway.\nDONE: #{id}"));
    w.settle();
    assert_eq!(head(&dir), start, "nothing landed");
    assert!(!dir.join("a.txt").exists());
    assert!(!listed(&w, &project, &id), "not back on the Desk");
}

// Drop on an item to check: it leaves the Desk without being done, and the commit that already landed stays (Cue
// never takes work off main by itself: only "Undo it" on Send back would, landing design part 4).
#[test]
fn a_dropped_item_to_check_leaves_the_desk_and_its_landed_commit_stays() {
    let w = World::new();
    let (dir, project, id, _, c) = landed(&w);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    assert!(!listed(&w, &project, &id), "off the Desk");
    assert_eq!(head(&dir), c, "its commit stays on main");
    assert!(dir.join("a.txt").is_file());
}
