//! Desk spec (#498), landing after DONE: one item at a time through the landing line, each checked against main as it
//! will be once it lands; a landing that fails never makes the item done; a later commit that takes landed work back
//! out opens it again; nothing is pushed unless asked. Sources: docs/desk-landing-design.md parts 2 to 4, #213, #247.

use super::desk_commit::{ends, item, routed_with, tick};
use super::desk_spec_copies::{another, copy_of, files_of, head, sent_to, subject, SpecChecker};
use super::team::git;
use crate::testing::World;
use std::sync::Arc;

// Landing design part 2, "One at a time": "Landings go through a single line, one after another. Each is checked
// against main as it will be after it lands."
#[test]
fn the_second_item_in_the_landing_line_is_checked_with_the_first_already_on_main() {
    let w = World::new();
    let checker = Arc::new(SpecChecker::default());
    checker.say(Some(Ok(())));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let f = another(&w, &project, "Footer");
    std::fs::write(copy_of(&w, &project, "Header jumps").join("a.txt"), "a\n").unwrap();
    std::fs::write(copy_of(&w, &project, "Footer").join("b.txt"), "b\n").unwrap();
    // The main checkout mid-merge: both wait in the line.
    std::fs::write(dir.join(".git/MERGE_HEAD"), head(&dir)).unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    w.advance(1000);
    ends(&w, &h, &format!("Fixed too.\nDONE: #{f}"));
    assert_eq!(item(&w, &project, "Footer")["finish"]["step"], "line", "{}", item(&w, &project, "Footer"));
    std::fs::remove_file(dir.join(".git/MERGE_HEAD")).unwrap();
    w.hub().desk_tick_now();
    w.settle();
    tick(&w);
    assert_eq!(subject(&dir, "HEAD~1"), format!("Header jumps (#{id})"), "said DONE first, landed first");
    assert_eq!(subject(&dir, "HEAD"), format!("Footer (#{f})"));
    let first = git(&dir, &["rev-parse", "HEAD~1"]).trim().to_string();
    let checked = checker.checked();
    let footers: Vec<&String> = checked.iter().filter(|c| subject(&dir, c) == format!("Footer (#{f})")).collect();
    assert!(!footers.is_empty(), "the footer was checked: {checked:?}");
    let last = footers.last().unwrap();
    assert_eq!(git(&dir, &["rev-parse", &format!("{last}~1")]).trim(), first, "checked on top of the header's landed commit");
    assert_eq!(files_of(&dir, last), "b.txt");
    assert_eq!(head(&dir), git(&dir, &["rev-parse", last]).trim(), "what landed is what was checked");
}

// Landing design part 4: "Looks right appears only from Landed on. Never before." Part 3: a failed check goes back to
// its holder and nothing reaches main. #247: "an item reaches to check ... only after its commit is on main".
#[test]
fn an_item_whose_check_failed_is_never_to_check_and_looks_right_on_it_is_refused() {
    let w = World::new();
    let checker = Arc::new(SpecChecker::default());
    checker.say(Some(Err("cargo check: error[E0425]".into())));
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    std::fs::write(copy_of(&w, &project, "Header jumps").join("a.txt"), "a\n").unwrap();
    let start = head(&dir);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert!(!matches!(it["state"].as_str(), Some("to_check" | "done")), "{it}");
    assert!(w.hub().desk_looks_right(&id).is_err(), "Looks right isn't there before it landed");
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert!(!matches!(it["state"].as_str(), Some("to_check" | "done")), "still not done: {it}");
    assert_eq!(head(&dir), start, "nothing on main");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("E0425")), "its holder has the check's output");
}

// #371 and landing design part 2 ("If the two clash, the item goes back to its holder ... The Desk never resolves a
// clash itself"); part 4 (no Looks right before Landed).
#[test]
fn an_item_that_clashed_with_main_is_never_to_check_and_looks_right_on_it_is_refused() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = copy_of(&w, &project, "Header jumps");
    std::fs::write(dir.join("README"), "main's\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "main's readme"]);
    let main = head(&dir);
    std::fs::write(path.join("README"), "the item's\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert!(w.hub().desk_looks_right(&id).is_err(), "Looks right isn't there before it landed");
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    assert_eq!(head(&dir), main, "nothing landed");
    assert_eq!(git(&dir, &["show", "HEAD:README"]), "main's\n", "the clash isn't resolved by the Desk");
    assert_eq!(std::fs::read_to_string(path.join("README")).unwrap(), "the item's\n", "nor in its copy");
}

// #213: "A commit that takes another item's work back out reopens that item and says so", for an item landed from
// its own copy and closed with Looks right.
#[test]
fn a_later_commit_on_main_that_takes_a_copied_items_work_out_opens_it_again() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    std::fs::write(copy_of(&w, &project, "Header jumps").join("a.txt"), "the fix\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    tick(&w);
    std::fs::remove_file(dir.join("a.txt")).unwrap();
    git(&dir, &["commit", "-q", "-am", "Back to how it was"]);
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "open again: {it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("is open again") && t.contains("took its work back out")), "its holder is told: {:?}", sent_to(&w, &h));
}

// Landing design part 2, "Who may push": sessions never push main, a push to main asks the owner first; part 7: a
// new project's push is "ask each time". Nothing set for the project: the landing pushes nothing.
#[test]
fn a_landing_with_nothing_set_never_pushes_main() {
    let w = World::new();
    let bare = std::env::temp_dir().join(format!("cue-spec-origin-{}-{}.git", std::process::id(), crate::clock::now_ms()));
    let (dir, project, id, h) = routed_with(
        &w,
        |_, _, dir| {
            git(dir, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
            git(dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
            git(dir, &["push", "-q", "origin", "main"]);
        },
        |_, _, _| {},
    );
    let pushed = git(&bare, &["rev-parse", "main"]);
    std::fs::write(copy_of(&w, &project, "Header jumps").join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    tick(&w);
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"), "landed");
    assert_eq!(git(&bare, &["rev-parse", "main"]), pushed, "origin's main is as it was");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nor is its branch pushed");
    let _ = std::fs::remove_dir_all(&bare);
}

/// Item "Header jumps" said DONE with a.txt while the main checkout is mid-merge: waiting in the landing line.
/// (main checkout, project, id, holder).
fn in_the_line(w: &World) -> (std::path::PathBuf, String, String, String) {
    let (dir, project, id, h) = routed_with(w, |_, _, _| {}, |_, _, _| {});
    std::fs::write(copy_of(w, &project, "Header jumps").join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.join(".git/MERGE_HEAD"), head(&dir)).unwrap();
    ends(w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(w, &project, "Header jumps")["finish"]["step"], "line", "{}", item(w, &project, "Header jumps"));
    (dir, project, id, h)
}

/// The main checkout free again, and Cue's tick run.
fn line_frees(w: &World, dir: &std::path::Path) {
    std::fs::remove_file(dir.join(".git/MERGE_HEAD")).unwrap();
    w.hub().desk_tick_now();
    w.settle();
    tick(w);
}

// Drop takes an item off the Desk: one waiting in the landing line never lands after it (landing design part 6: only
// the Desk's rules land an item, and a dropped one is no longer the Desk's).
#[test]
fn an_item_dropped_while_it_waits_in_the_landing_line_never_lands() {
    let w = World::new();
    let (dir, project, id, _) = in_the_line(&w);
    let start = head(&dir);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    line_frees(&w, &dir);
    assert_eq!(head(&dir), start, "nothing landed");
    assert!(!dir.join("a.txt").exists());
    assert!(!super::desk_spec_copies::listed(&w, &project, &id));
}

// #371/#396: Send back works on an item waiting on Cue too; sent back, it's at work again and lands only at its next
// DONE, never on its own when the line frees (landing design part 6: "Send it back to its holder").
#[test]
fn an_item_sent_back_while_it_waits_in_the_landing_line_doesnt_land_till_its_next_done() {
    let w = World::new();
    let (dir, project, id, h) = in_the_line(&w);
    let start = head(&dir);
    w.hub().desk_item_act(&id, "send_back", "Use the other colour").unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("Use the other colour")), "its holder has your words");
    line_frees(&w, &dir);
    assert_eq!(head(&dir), start, "not landed while it's being fixed");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    let path = copy_of(&w, &project, "Header jumps");
    assert_eq!(std::fs::read_to_string(path.join("a.txt")).ok().as_deref(), Some("a\n"), "its work stays in its copy");
    std::fs::write(path.join("a.txt"), "a, other colour\n").unwrap();
    ends(&w, &h, &format!("Changed the colour.\nDONE: #{id}"));
    assert_eq!(subject(&dir, "HEAD"), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "a, other colour\n");
}
