//! #653: item sessions edited the main folder instead of their own copy, and the landing, made from the copy, left
//! those edits behind without a word. An item started now with every copy in use gets a copy of its own past the
//! count (never the main folder), and a copy's landing stops on files its holder changed in the main folder that the
//! copy doesn't have: sent back naming them once; the same ones still there at its next DONE land without them, its
//! lead told which. Nothing in the main folder is ever removed. #770: a file counts as its session's only while what
//! its own Edit or Write put there is still in it (#767 was named another session's ui/styles/desk-c.css), and each
//! is named to it once and to its lead once, however often it lands again.

use super::desk_commit::{ends, item, log, log_edit, routed_with};
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

/// Session `sid`'s Write of `text` to `path`, in its log.
fn log_write(w: &World, sid: &str, path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    log(w, sid).tool_use(&format!("toolu_w{}", crate::clock::now_ms()), "Write", serde_json::json!({ "file_path": path, "content": text }));
    w.settle();
}

/// Its holder edits a.txt in its copy and b.txt in the main folder (as #598's did before its copy was free).
fn edits_main_too(w: &World, dir: &Path, h: &str, path: &Path) {
    log_write(w, h, &path.join("a.txt"), "a\n");
    log_write(w, h, &dir.join("b.txt"), "b\n");
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
fn another_sessions_uncommitted_edit_in_the_main_folder_is_never_pinned_on_a_landing_item() {
    // #770: #767 was told three times that ui/styles/desk-c.css, another session's unfinished work, was its own.
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    // shared.txt stands for #767's ui/styles/desk-c.css (the test repository tracks only a few names).
    let css = Path::new("shared.txt");
    log_write(&w, &h, &path.join("a.txt"), "a\n");
    // Its holder tried to edit the main folder's copy of it; Cue stopped that, so the call failed.
    let stopped = format!("toolu_s{}", crate::clock::now_ms());
    log(&w, &h).tool_use(&stopped, "Edit", serde_json::json!({ "file_path": dir.join(css), "old_string": "", "new_string": ".rail { gap: 4px }" }));
    log(&w, &h).entry(serde_json::json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": stopped, "content": "Stopped: outside its copy", "is_error": true}]}}));
    // A shell command of its names the file by a relative path: a guess at what it wrote, never counted as its write.
    log(&w, &h).tool_use(&format!("toolu_b{}", crate::clock::now_ms()), "Bash", serde_json::json!({ "command": "printf '.rail { gap: 4px }\\n' > shared.txt" }));
    // Another session's edit of the same file, uncommitted in the main folder as this item lands.
    std::fs::write(dir.join(css), ".rail { gap: 8px }\n/* half done */\n").unwrap();
    w.settle();
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let n = w.hosted.sent().len();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed at its first DONE");
    let named = |to: &str| told(&w, to, n).into_iter().filter(|t| t.contains("shared.txt")).collect::<Vec<_>>();
    assert!(named(&h).is_empty() && named(&lead).is_empty(), "never pinned on it: {:?} {:?}", named(&h), named(&lead));
    assert!(item(&w, &project, "Header jumps")["main_left"].is_null());
}

#[test]
fn a_file_left_in_the_main_folder_is_named_to_its_session_once_and_to_its_lead_once_however_often_it_lands() {
    // #770: a landing tried again (here after a clash) named the same file again, back to its session and to its lead.
    let w = World::new();
    let (dir, project, id, h, path) = routed(&w);
    edits_main_too(&w, &dir, &h, &path);
    log_write(&w, &h, &path.join("shared.txt"), "theirs\n");
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let n = w.hosted.sent().len();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working", "named to it");
    // Main moves on with a shared.txt of its own: its next landing clashes after the lead is told.
    std::fs::write(dir.join("shared.txt"), "ours\n").unwrap();
    git(&dir, &["add", "shared.txt"]);
    git(&dir, &["commit", "-q", "-m", "Ours"]);
    ends(&w, &h, &format!("b.txt is another session's.\nDONE: #{id}"));
    assert!(told(&w, &h, n).iter().any(|t| t.contains("clashes")), "the clash goes back: {:?}", told(&w, &h, n));
    // Settled in its copy: it lands, without b.txt, and nobody hears of b.txt again.
    log_write(&w, &h, &path.join("shared.txt"), "ours\n");
    ends(&w, &h, &format!("Settled.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "landed");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "without b.txt");
    let named = |to: &str| told(&w, to, n).into_iter().filter(|t| t.contains("b.txt") && t.contains("main folder")).count();
    assert_eq!((named(&h), named(&lead)), (1, 1), "once each: {:?}", w.hosted.sent());
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

/// What Cue's own items are told of testing, after the words on their copy (tests/run.sh card, #678).
const CARD_TESTS: &str = " Cue stops an edit or git command outside it. Before you say done, run tests/run.sh card: it runs the tests for what you changed. Don't run the full suite: it runs after landing.";

#[test]
fn an_item_in_this_cues_own_source_is_told_to_run_its_card_tests() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, dir| super::desk_finish::as_this_cue(dir), |_, _, _| {});
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["tree"]["card_tests"], true, "{it}");
    let words = told(&w, &h, 0);
    assert!(words.iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("Work in its own copy") && t.ends_with(CARD_TESTS)), "told to run them: {words:?}");
}

#[test]
fn an_item_in_this_cues_own_source_is_told_what_its_desks_tests_setting_says() {
    // Every test (the Desk set so before the item starts, #780).
    let w = World::new();
    let set = |tests: &'static str| move |w: &World, h: &str, dir: &std::path::Path| {
        super::desk_finish::as_this_cue(dir);
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        crate::desk::lands::save(&project, crate::desk::lands::Place::Cue, &serde_json::json!({ "tests": tests })).unwrap();
    };
    let (_, project, id, h) = routed_with(&w, set("all"), |_, _, _| {});
    let words = told(&w, &h, 0);
    let brief = words.iter().find(|t| t.contains(&format!("Desk item #{id}")) && t.contains("Work in its own copy")).unwrap_or_else(|| panic!("{words:?}"));
    assert!(brief.contains("\nTests: run the tests for what you change as you work, and the full suite before you say done.\n"), "{brief}");
    assert!(brief.ends_with(" Cue stops an edit or git command outside it. Before you say done, run the full suite: CUE_FULL_RUN=1 tests/run.sh."), "{brief}");
    assert_eq!(item(&w, &project, "Header jumps")["tree"]["card_tests"], true);
    // Only what changed, then a full run just before DONE. The first World let go first: a second one waits on its turn.
    drop(w);
    let w = World::new();
    let (_, _, id, h) = routed_with(&w, set("changed_full"), |_, _, _| {});
    let words = told(&w, &h, 0);
    let brief = words.iter().find(|t| t.contains(&format!("Desk item #{id}")) && t.contains("Work in its own copy")).unwrap_or_else(|| panic!("{words:?}"));
    assert!(brief.contains("\nTests: while you work, run only the tests"), "{brief}");
    assert!(brief.ends_with(" While you work, run tests/run.sh card: it runs the tests for what you changed. Just before you say done, run the full suite once: CUE_FULL_RUN=1 tests/run.sh."), "{brief}");
}

#[test]
fn an_item_in_another_project_is_told_nothing_of_cues_tests() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let it = item(&w, &project, "Header jumps");
    assert!(it["tree"].get("card_tests").is_none(), "{it}");
    let words = told(&w, &h, 0);
    assert!(words.iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.ends_with("Cue stops an edit or git command outside it.")), "its copy's words as before: {words:?}");
    assert!(!words.iter().any(|t| t.contains("tests/run.sh")), "{words:?}");
}
