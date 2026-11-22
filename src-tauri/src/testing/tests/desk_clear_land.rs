//! Clear and land (#659): a landing stopped on edits in the main folder that are already on the item's branch offers
//! one OK that saves their diffs to work/, sets them back to main and lands it; a file that also holds another
//! session's edits is named and left alone (desk/hub/landing/clear_land.rs).

use super::desk::second;
use super::desk_commit::{done_with, item, log};
use super::desk_finish::crew_in_worktree;
use crate::testing::World;
use serde_json::json;

/// The patches Clear and land saved in `dir`'s work/ for item `id`.
fn saved(dir: &std::path::Path, id: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir.join("work")) else { return vec![] };
    rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with(&format!("{id}-cleared-"))).collect()
}

#[test]
fn a_landing_blocked_by_its_own_leftover_edits_is_cleared_and_lands_with_one_ok() {
    let w = World::new();
    let (dir, project, id) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        // A leftover copy of its own work, sitting uncommitted in the main folder.
        std::fs::write(dir.join("a.txt"), "builder's\n").unwrap();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "finishing", "{shown}");
    assert_eq!(shown["finish"]["clear"], json!(["a.txt"]), "offered: {shown}");
    assert!(shown["finish"].get("clear_keep").is_none(), "{shown}");
    let it = w.hub().desk_item_act(&id, "clear_land", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "landed: {shown} / {it:?}");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "builder's\n", "merged");
    let patches = saved(&dir, &id);
    assert_eq!(patches.len(), 1, "{patches:?}");
    let patch = std::fs::read_to_string(dir.join("work").join(&patches[0])).unwrap();
    assert!(patch.contains("a.txt") && patch.contains("+builder's"), "{patch}");
}

#[test]
fn a_file_with_another_items_edits_is_named_and_left_alone() {
    let w = World::new();
    let other = std::cell::RefCell::new(String::new());
    let (dir, project, id) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        let wt = crew_in_worktree(w, h, dir, "a.txt");
        std::fs::write(wt.join("b.txt"), "builder's b\n").unwrap();
        super::team::git(&wt, &["add", "b.txt"]);
        super::team::git(&wt, &["commit", "-q", "-m", "b"]);
        std::fs::write(dir.join("a.txt"), "builder's\n").unwrap();
        // Another session, on an item of its own, writes b.txt in the main folder.
        let o = second(w, "tidier");
        let project = w.snapshot().session(&o)["project"].as_str().unwrap().to_string();
        let mine = w.hub().desk_add(&project, "bug", "Tidy b", "", None).unwrap();
        w.hub().desk_give(&mine.id, &o).unwrap();
        w.settle();
        let b = dir.join("b.txt");
        std::fs::write(&b, "builder's b\n").unwrap();
        log(w, &o).tool_use("toolu_t1", "Write", json!({ "file_path": b, "content": "builder's b\n" }));
        w.settle();
        *other.borrow_mut() = mine.id;
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["finish"]["clear"], json!(["a.txt"]), "{shown}");
    assert_eq!(shown["finish"]["clear_keep"][0][0], "b.txt", "{shown}");
    assert!(shown["finish"]["clear_keep"][0][1].as_str().unwrap().contains("tidier"), "{shown}");
    w.hub().desk_item_act(&id, "clear_land", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    // a.txt set back; b.txt still in its way, untouched: it stops again, on b.txt only.
    assert_eq!(shown["state"], "finishing", "{shown}");
    assert!(shown["latest"].as_str().unwrap().contains("in b.txt that"), "{shown}");
    assert!(!dir.join("a.txt").exists(), "a.txt set back to main (it isn't there): {shown}");
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "builder's b\n");
    assert!(shown["finish"].get("clear").is_none(), "nothing left to clear: {shown}");
    assert_eq!(saved(&dir, &id).len(), 1);
}
