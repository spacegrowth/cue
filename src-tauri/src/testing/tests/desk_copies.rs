//! Copies cleared away once their work is in the app (desk/hub/landing/copies.rs, #376): as an item lands, its helpers'
//! copies whose work is on the main line go with its own; one with work that isn't stays. Copies left from before are
//! listed with their sizes, and removed only once you say so; one with work not on the main line, or in use, never is.

use super::desk::{desk, sent_to};
use super::desk_commit::{ends, item, routed_with};
use super::team::{git, says, starts};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;
use std::path::{Path, PathBuf};

/// The repository's main checkout, as git names it.
fn top(w: &World) -> String {
    let out = git(w.home(), &["worktree", "list", "--porcelain"]);
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// A copy of `dir` at `.claude/worktrees/<name>` on a new branch `branch`.
fn copy(dir: &Path, name: &str, branch: &str) -> PathBuf {
    let at = dir.join(".claude/worktrees").join(name);
    git(dir, &["worktree", "add", "-q", "-b", branch, &at.to_string_lossy()]);
    at
}

/// Lead `sid`'s teammate (agent `id`) finished.
fn finishes(w: &World, sid: &str, id: &str) {
    says(w, sid, [HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": id, "status": "completed", "summary": "Done." }) }]);
}

#[test]
fn as_an_item_lands_its_helpers_copies_whose_work_is_in_it_go_and_one_with_other_work_stays() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    // Two helpers in copies of their own: one's work the holder took into the item, the other's it didn't.
    let a = copy(&dir, "agent-agentA", "worktree-agent-agentA");
    let b = copy(&dir, "agent-agentB", "worktree-agent-agentB");
    starts(&w, &h, "toolu_A", "agentA", "rows", true);
    starts(&w, &h, "toolu_B", "agentB", "side", true);
    std::fs::write(a.join("a.txt"), "a\n").unwrap();
    git(&a, &["add", "a.txt"]);
    git(&a, &["commit", "-q", "-m", "rows"]);
    std::fs::write(b.join("b.txt"), "not taken\n").unwrap();
    finishes(&w, &h, "agentA");
    finishes(&w, &h, "agentB");
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "landed");
    assert!(!path.exists(), "the item's copy is gone");
    assert!(!a.exists(), "the helper whose work landed: its copy is gone");
    assert!(git(&dir, &["branch", "--list", "worktree-agent-agentA"]).trim().is_empty(), "and its branch");
    assert_eq!(std::fs::read_to_string(b.join("b.txt")).unwrap(), "not taken\n", "the one with work not on main stays, as it was");
}

/// Desk `project`'s lead's session.
fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

/// The lead's card asking about old working folders, if any.
fn old_card(w: &World, lead: &str) -> Option<serde_json::Value> {
    w.snapshot().cards_of(lead).into_iter().find(|c| c["tool_input"]["desk_old"].is_string()).cloned()
}

fn answer(w: &World, card: &serde_json::Value, a: &str) {
    let q = card["tool_input"]["questions"][0]["question"].as_str().unwrap().to_string();
    assert!(w.hub().respond(card["id"].as_str().unwrap(), crate::model::Decision { behavior: "allow".into(), answers: Some(json!({ q: a })), ..Default::default() }));
    w.settle();
    w.advance(1000);
    w.settle();
}

#[test]
fn old_copies_whose_work_is_on_main_are_asked_about_once_in_the_leads_chat_and_removed_only_on_yes() {
    let w = World::new();
    let (dir, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let lead = lead_of(&w, &project);
    let item_copy = PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    // Landed as a squash: its commit isn't on main, but what it changed is.
    let landed = copy(&dir, "desk-9", "desk/9");
    std::fs::write(landed.join("a.txt"), "landed\n").unwrap();
    git(&landed, &["add", "a.txt"]);
    git(&landed, &["commit", "-q", "-m", "wip"]);
    std::fs::write(dir.join("a.txt"), "landed\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "Its item (#9)"]);
    // Nothing in it (and build output, which git ignores, doesn't count).
    let empty = copy(&dir, "agent-old", "worktree-agent-old");
    std::fs::write(empty.join("build.log"), "x".repeat(10_000)).unwrap();
    // Work that isn't on main: never listed.
    let work = copy(&dir, "agent-work", "worktree-agent-work");
    std::fs::write(work.join("b.txt"), "mine\n").unwrap();

    let found = w.hub().desk_old_copies_find(&top(&w));
    let mut listed: Vec<String> = found.copies.iter().map(|c| Path::new(&c.path).file_name().unwrap().to_string_lossy().to_string()).collect();
    listed.sort();
    assert_eq!(listed, ["agent-old", "desk-9"], "not the item's copy (in use), not the one with work");
    assert!(found.kb > 0 && found.copies.iter().all(|c| c.kb > 0), "{found:?}");
    assert!(desk(&w, &project).get("old_copies").is_none(), "not in the item list");

    // Asked in the lead's chat: how many, how big.
    w.hub().desk_old_copies_look();
    w.settle();
    let card = old_card(&w, &lead).expect("the lead asks");
    let q = card["tool_input"]["questions"][0]["question"].as_str().unwrap();
    assert!(q.starts_with("2 old working folders (") && q.contains(" KB) have work that's already in the app"), "{q}");
    let choices: Vec<&str> = card["tool_input"]["questions"][0]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap()).collect();
    assert_eq!(choices, ["Yes", "Not now"]);
    assert!(landed.exists() && empty.exists(), "nothing removed before you say so");

    // Meanwhile one gets work: looked at again, it stays.
    std::fs::write(empty.join("b.txt"), "new\n").unwrap();
    let before = sent_to(&w, &lead).len();
    answer(&w, &card, "Yes");
    assert!(!landed.exists() && git(&dir, &["branch", "--list", "desk/9"]).trim().is_empty(), "removed with its branch");
    assert!(empty.join("b.txt").exists() && work.join("b.txt").exists() && item_copy.exists(), "the rest untouched");
    assert_eq!(sent_to(&w, &lead).len(), before, "Cue's question: nothing sent to the lead");
    let said = desk(&w, &project)["decisions"].to_string();
    assert!(said.contains("Removed 1 old working folder") && said.contains("1 kept: it has work not in the app yet"), "{said}");

    // Asked once: not again, even with old copies left.
    std::fs::remove_file(empty.join("b.txt")).unwrap();
    w.hub().desk_old_copies_look();
    w.settle();
    assert!(old_card(&w, &lead).is_none(), "asked once");
}

#[test]
fn not_now_removes_nothing_and_how_work_lands_lists_them_again() {
    let w = World::new();
    let (dir, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let lead = lead_of(&w, &project);
    let old = copy(&dir, "agent-old", "worktree-agent-old");
    w.hub().desk_old_copies_look();
    w.settle();
    let card = old_card(&w, &lead).expect("the lead asks");
    assert!(card["tool_input"]["questions"][0]["question"].as_str().unwrap().starts_with("1 old working folder ("));
    answer(&w, &card, "Not now");
    assert!(old.exists());
    // How work lands: Remove old working folders looks again, and removes on yes.
    let list = w.hub().desk_old_copies_list(&project).unwrap();
    assert_eq!(list.copies.len(), 1);
    assert_eq!(w.hub().desk_old_copies_answer(&project, true).unwrap()["removed"], 1);
    assert!(!old.exists());
}
