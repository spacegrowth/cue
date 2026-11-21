//! Landing checks only the item's own commits, and finds them on main however they got there (#578): main's history
//! rewritten after the item's commit was made (#501), or another item's branch in a worktree its holder looked in (#560).

use super::desk_commit::{done_with, ends, item, log, log_edit, routed_with, tick};
use super::desk_finish::{as_this_cue, Fake};
use super::team::{git, says, writes};
use crate::testing::World;
use serde_json::json;
use std::sync::Arc;

/// #501: its commit was made, then main's history was rewritten (the same change, a new commit) and the old one is on
/// no branch. It goes on to its install instead of waiting "not on main".
#[test]
fn an_item_whose_commit_main_has_under_a_new_commit_goes_on_to_install() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, _id) = done_with(&w, |_, _, dir| {
        as_this_cue(dir);
        git(dir, &["checkout", "-q", "-b", "side"]);
    }, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["step"].as_str(), shown["finish"]["wait"].as_str()), (Some("landing"), Some("on side, not main")), "{shown}");
    let old = git(&dir, &["rev-parse", "side"]).trim().to_string();
    // Main's history rewritten: the same change on main as a new commit, the old one on no branch.
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["cherry-pick", "-x", &old]);
    git(&dir, &["branch", "-q", "-D", "side"]);
    let new = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(new, old);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("finishing"), Some("queued"), Some(new.as_str())), "{shown}");
}

/// A commit that isn't on main in any form still holds it back: the check above isn't a pass for everything.
#[test]
fn an_item_whose_change_main_lacks_still_waits() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, _id) = done_with(&w, |_, _, dir| {
        as_this_cue(dir);
        git(dir, &["checkout", "-q", "-b", "side"]);
    }, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt")]));
    });
    git(&dir, &["checkout", "-q", "main"]);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["step"].as_str(), shown["finish"]["wait"].as_str()), (Some("landing"), Some("on side, not main")), "{shown}");
}

/// #560: started without a copy, its holder also worked in a worktree on another item's branch (desk/77: that item
/// landed and its copy not handed on yet, its commit never on main). This item's own work goes to main and on to check,
/// never waiting on desk/77, and #77's work isn't merged with it.
#[test]
fn an_item_never_waits_on_or_takes_another_items_branch_its_holder_worked_in() {
    let w = World::new();
    let top = |d: &std::path::Path| git(d, &["worktree", "list", "--porcelain"]).lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string();
    let (dir, project, id, h) = routed_with(&w, |w, _, dir| w.hub().desk_hold_copies(&top(dir), &[1, 2, 3]), |_, _, _| {});
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let other = w.home().join("desk-9");
    git(&dir, &["worktree", "add", "-q", "-b", "desk/77", other.to_str().unwrap()]);
    std::fs::write(other.join("shared.txt"), "#77's work\n").unwrap();
    git(&other, &["add", "shared.txt"]);
    git(&other, &["commit", "-q", "-m", "#77's work"]);
    // Its holder looks around in #77's copy, then fixes a.txt in the main checkout.
    log(&w, &h).entry(json!({ "type": "assistant", "cwd": other, "message": { "content": [{ "type": "text", "text": "Looking." }] } }));
    log(&w, &h).entry(json!({ "type": "assistant", "cwd": dir, "message": { "content": [{ "type": "text", "text": "On it." }] } }));
    std::fs::write(dir.join("a.txt"), "fixed\n").unwrap();
    log_edit(&w, &h, &dir.join("a.txt"));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    let on_main = git(&dir, &["log", "--format=%s", "main"]);
    assert!(!on_main.contains("#77's work"), "#77's work stays on its branch: {on_main}");
    assert!(on_main.lines().next().is_some_and(|l| l.contains("Header jumps")), "its own commit is on main: {on_main}");
}
