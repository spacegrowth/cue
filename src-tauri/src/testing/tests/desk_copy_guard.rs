//! Sessions stay in their own copy, and a failed landing goes to the lead, not to you (#483). #455's holder made a
//! worktree and a branch of its own; #456's merge stopped on edits sitting in the main checkout and showed as needing
//! you with git's text. Now Cue stops an edit or git command that leaves the copy (hub/desk_copy_guard.rs), and a stop
//! while landing is the Desk lead's, said plainly, the edits in the way named (hub/desk_stop_lead.rs).

use super::desk::{desk, second, sent_to};
use super::desk_commit::{done_with, ends, item, log, routed_with};
use super::desk_finish::crew_in_worktree;
use crate::testing::World;
use serde_json::{json, Value};

/// What a stopped call's hook was told: its "deny" and why.
fn denied(ask: Option<crate::testing::Ask>) -> String {
    let told: Value = ask.expect("stopped").told().expect("answered at once");
    assert_eq!(told["behavior"], "deny", "{told}");
    told["message"].as_str().unwrap().to_string()
}

#[test]
fn a_holder_is_stopped_from_leaving_its_copy_and_works_in_it_freely() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let it = item(&w, &project, "Header jumps");
    let copy = it["tree"]["path"].as_str().unwrap_or_else(|| panic!("given a copy: {it}")).to_string();
    let s = w.session(&h).in_folder(&dir);
    // #455: a worktree and a branch of its own.
    let why = denied(s.guard_tool("Bash", json!({ "command": format!("git worktree add -b desk/{id} ../mine") })));
    assert!(why.contains(&copy) && why.contains("a worktree of your own") && why.contains(&format!("#{id}")), "{why}");
    assert!(denied(s.guard_tool("Bash", json!({ "command": format!("cd {copy} && git switch -c fix-grip") }))).contains("a branch of your own"));
    // #456: the main checkout, by an edit or by git.
    let why = denied(s.guard_tool("Edit", json!({ "file_path": dir.join("a.txt") })));
    assert!(why.contains("a.txt is outside it") && why.contains(&copy), "{why}");
    denied(s.guard_tool("Bash", json!({ "command": "git commit -am 'The fix'" })));
    denied(s.guard_tool("Bash", json!({ "command": format!("git -C {} merge desk/{id}", dir.display()) })));
    // In its copy, reading anywhere, or outside the repository: it passes.
    assert!(s.guard_tool("Edit", json!({ "file_path": format!("{copy}/a.txt") })).is_none());
    assert!(s.guard_tool("Write", json!({ "file_path": format!("{copy}/new/b.txt") })).is_none());
    assert!(s.guard_tool("Bash", json!({ "command": format!("cd {copy} && git add a.txt && git commit -m 'The fix'") })).is_none());
    assert!(s.guard_tool("Bash", json!({ "command": "git status && git log -1 && git diff main" })).is_none());
    let elsewhere = std::env::temp_dir().join(format!("cue-notes-{}.md", std::process::id()));
    assert!(s.guard_tool("Write", json!({ "file_path": elsewhere })).is_none());
    // A session holding no item works where it likes.
    let free = second(&w, "free");
    assert!(w.session(&free).in_folder(&dir).guard_tool("Edit", json!({ "file_path": dir.join("a.txt") })).is_none());
}

#[test]
fn edits_in_main_blocking_a_merge_go_to_the_lead_named_never_to_you_with_gits_text() {
    let w = World::new();
    let other = std::cell::RefCell::new(String::new());
    let (dir, project, id) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        // Another session, on an item of its own, edits a.txt in the main checkout and leaves it uncommitted (#456).
        let o = second(w, "tidier");
        let project = w.snapshot().session(&o)["project"].as_str().unwrap().to_string();
        let mine = w.hub().desk_add(&project, "bug", "Tidy a", "", None).unwrap();
        w.hub().desk_give(&mine.id, &o).unwrap();
        w.settle();
        let a = dir.join("a.txt");
        std::fs::write(&a, "tidier's\n").unwrap();
        log(w, &o).tool_use("toolu_t1", "Write", json!({ "file_path": a, "content": "tidier's\n" }));
        w.settle();
        *other.borrow_mut() = mine.id;
    });
    let other = other.into_inner();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["stopped"].as_bool()), (Some("finishing"), Some(true)), "the lead's, not yours: {shown}");
    let latest = shown["latest"].as_str().unwrap();
    assert_eq!(latest, format!("Not merged: the main checkout has edits of its own in a.txt that aren't committed, made by tidier on #{other}"));
    assert!(!shown["finish"]["question"].as_str().unwrap().contains("overwritten"), "git's text isn't what you see: {shown}");
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let told = sent_to(&w, &lead).into_iter().find(|t| t.contains(&format!("#{id} (Header jumps) stopped while landing"))).unwrap_or_else(|| panic!("the lead is told: {:?}", sent_to(&w, &lead)));
    assert!(told.contains("made by tidier") && told.contains("git said:") && told.contains("ROUTE: #"), "{told}");
    // The lead clears the way and says DONE: it lands, and comes to your check.
    std::fs::remove_file(dir.join("a.txt")).unwrap();
    ends(&w, &lead, &format!("Cleared a.txt in main.\nDONE: #{id}"));
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "builder's\n", "merged");
}

#[test]
fn a_failed_landing_with_no_lead_waits_on_you_in_plain_words() {
    let w = World::new();
    let (_, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        std::fs::write(dir.join("a.txt"), "yours, not committed\n").unwrap();
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        let lead = desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
        w.hub().drop_session(&lead);
        w.settle();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "needs_you", "{shown}");
    assert_eq!(shown["latest"], "Not merged: the main checkout has edits of its own in a.txt that aren't committed", "{shown}");
}
