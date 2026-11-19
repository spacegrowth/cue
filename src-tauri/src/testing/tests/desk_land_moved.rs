//! An item's copy whose branch was moved onto a rewritten main line (hub/desk_trees.rs, land): the commit it was
//! made from is gone from its history, so it lands from where its branch meets the main line now, not as a clash.

use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::testing::World;
use std::path::PathBuf;

#[test]
fn a_copy_rebased_onto_a_rewritten_main_line_lands_from_where_it_meets_it_now() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().expect("a copy"));
    // The main line's history rewritten: the commit the copy started from replaced by one with the same files, then
    // a change to README on top.
    let tree = git(&dir, &["rev-parse", "HEAD^{tree}"]).trim().to_string();
    let rewritten = git(&dir, &["commit-tree", &tree, "-m", "rewritten"]).trim().to_string();
    git(&dir, &["reset", "-q", "--hard", &rewritten]);
    std::fs::write(dir.join("README"), "main's\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "main's readme"]);
    let main = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    // Its branch moved onto it, and its work next to main's change: measured from where it started, a clash.
    git(&path, &["reset", "-q", "--hard", &main]);
    std::fs::write(path.join("README"), "main's\nthe item's\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_ne!(it["finish"]["step"], "conflict", "{it}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "{it}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), main, "on top of the rewritten main line");
    assert_eq!(std::fs::read_to_string(dir.join("README")).unwrap(), "main's\nthe item's\n");
}

/// Main moved on with a change to README; its copy brought main in (`merge`, or moved onto it) and changed that same
/// line again. Measured from where it started, its copy of main's change would clash with main's own (#358).
fn lands_after_bringing_main_in(merge: bool) {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().expect("a copy"));
    std::fs::write(dir.join("README"), "main's\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "main's readme"]);
    let main = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    if merge {
        // Work of its own committed first, so bringing main in is a real merge, not a fast-forward.
        std::fs::write(path.join("NOTES"), "the item's notes\n").unwrap();
        git(&path, &["add", "-f", "NOTES"]);
        git(&path, &["commit", "-q", "-m", "the item's notes"]);
        git(&path, &["merge", "-q", "--no-edit", &main]);
    } else {
        git(&path, &["reset", "-q", "--hard", &main]);
    }
    std::fs::write(path.join("README"), "main's, and the item's\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_ne!(it["finish"]["step"], "conflict", "{it}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "{it}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), main, "one commit on top of main");
    assert_eq!(std::fs::read_to_string(dir.join("README")).unwrap(), "main's, and the item's\n");
    if merge {
        assert_eq!(std::fs::read_to_string(dir.join("NOTES")).unwrap(), "the item's notes\n");
    }
}

#[test]
fn a_copy_that_merged_main_lands_without_a_false_clash() {
    lands_after_bringing_main_in(true);
}

#[test]
fn a_copy_moved_onto_main_as_it_is_now_lands_without_a_false_clash() {
    lands_after_bringing_main_in(false);
}
