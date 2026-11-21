//! #653: item sessions edited the main folder instead of their own copy, and the landing, made from the copy, left
//! those edits behind without a word. An item started now with every copy in use gets a copy of its own past the
//! count (never the main folder), and a copy's landing stops on files its holder changed in the main folder that the
//! copy doesn't have: sent back naming them once; the same ones still there at its next DONE land without them, its
//! lead told which. Nothing in the main folder is ever removed.

use super::desk_commit::{ends, item, log_edit, routed_with};
use super::team::git;
use crate::testing::World;
use std::path::{Path, PathBuf};

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

fn told(w: &World, to: &str, from: usize) -> Vec<String> {
    w.hosted.sent().into_iter().skip(from).filter(|(s, _)| s == to).map(|(_, t)| t).collect()
}

/// Its holder edits a.txt in its copy and b.txt in the main folder (as #598's did before its copy was free).
fn edits_main_too(w: &World, dir: &Path, h: &str, path: &Path) {
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    log_edit(w, h, &path.join("a.txt"));
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    log_edit(w, h, &dir.join("b.txt"));
}

#[test]
fn a_landing_stops_on_files_its_session_changed_in_the_main_folder_and_lands_them_once_moved_into_its_copy() {
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    edits_main_too(&w, &dir, &h, &path);
    let n = w.hosted.sent().len();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]).trim(), before, "nothing landed");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "back with its holder: {it}");
    assert!(told(&w, &h, n).iter().any(|t| t.contains("b.txt") && t.contains("not in its copy") && t.contains(path.to_str().unwrap())), "named, with where to move it: {:?}", told(&w, &h, n));
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "b\n", "left in the main folder as it was");
    // Moved into its copy: both land, and the main folder's b.txt is the landed one.
    std::fs::write(path.join("b.txt"), "b\n").unwrap();
    ends(&w, &h, &format!("Moved.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt");
    assert!(git(&dir, &["status", "--porcelain", "--", "b.txt"]).trim().is_empty(), "the main folder's b.txt is the landed one");
}

#[test]
fn the_same_files_still_in_the_main_folder_at_its_next_done_land_without_them_and_its_lead_is_told() {
    let w = World::new();
    let (dir, project, id, h, _) = routed(&w);
    let path = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    edits_main_too(&w, &dir, &h, &path);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let n = w.hosted.sent().len();
    // Its holder says b.txt isn't this item's: DONE again, not moved.
    ends(&w, &h, &format!("b.txt is another session's.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "without b.txt");
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "b\n", "never removed from the main folder");
    assert!(told(&w, &lead, n).iter().any(|t| t.contains(&format!("#{id}")) && t.contains("b.txt") && t.contains("main folder")), "its lead is told: {:?}", told(&w, &lead, n));
}

#[test]
fn a_main_folder_file_its_session_never_touched_never_stops_its_landing() {
    let w = World::new();
    let (dir, _, id, h, path) = routed(&w);
    std::fs::write(dir.join("b.txt"), "another session's\n").unwrap();
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    log_edit(&w, &h, &path.join("a.txt"));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt");
}

#[test]
fn an_item_started_now_with_every_copy_in_use_gets_a_copy_of_its_own_never_the_main_folder() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&super::desk_unstick::top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let n = w.hosted.sent().len();
    ends(&w, &lead, &format!("START: #{id} | why: the user needs it now"));
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64(), it["tree_wait"].as_bool().unwrap_or(false)), (Some("working"), Some(4), false), "{it}");
    let path = PathBuf::from(it["tree"]["path"].as_str().unwrap());
    assert!(path.is_dir() && path != dir, "a copy of its own: {}", path.display());
    assert!(told(&w, &h, n).iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("Work in its own copy") && t.contains(path.to_str().unwrap())), "told where: {:?}", told(&w, &h, n));
    // An edit in the main folder is stopped, as in any copy.
    let edit = serde_json::json!({ "file_path": dir.join("a.txt") });
    let origin = w.hub().session_origin(&h).unwrap();
    assert!(w.hub().copy_guard(&origin, "Edit", &edit).is_some(), "an edit in the main folder is stopped");
}
