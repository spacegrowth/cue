//! Desk items worked by sessions on another machine (#500, desk/hub/landing/remote_item.rs): the holder there is told to
//! work on a branch of its own (`desk/<id>`) from the shared remote's main line; at its DONE Cue word-checks the
//! branch there, pushes it from there, fetches it here and lands it on the main line like any item. Played with a
//! machine ("box") whose git runs in place, on a clone of the same remote.

use super::desk_commit::{ends, item};
use super::team::{git, lead};
use crate::hosted::HostedEvent;
use crate::testing::World;
use std::path::{Path, PathBuf};

/// The main checkout pushed to a bare remote, and the machine's clone of it.
fn with_remote(dir: &Path) -> (PathBuf, PathBuf) {
    let bare = dir.join("shared.git");
    let there = dir.join("there");
    git(dir, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
    git(dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(dir, &["push", "-q", "-u", "origin", "main"]);
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &there.to_string_lossy()]);
    git(&there, &["config", "user.email", "t@example.com"]);
    git(&there, &["config", "user.name", "T"]);
    (bare, there)
}

/// An item routed to a session on "box", working in the machine's clone: (repo, bare remote, clone there, project,
/// id, holder), and the machine (kept for the test).
fn routed_there(w: &World) -> ((PathBuf, PathBuf, PathBuf, String, String, String), crate::testing::FakeSsh) {
    let ssh = crate::testing::FakeSsh::new();
    ssh.host("box", crate::testing::RUNS_IT);
    crate::machines::add("box", "").unwrap();
    let dir = super::team::main_repo(w);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    // The Desk's session that takes the item is on "box", in its clone there.
    let (bare, there) = with_remote(&dir);
    let cwd = there.to_string_lossy().to_string();
    w.hub().write(|st| {
        if let Some(s) = st.sessions.get_mut(&h) {
            (s.origin.machine, s.origin.cwd) = ("box".into(), cwd);
        }
    });
    w.advance(1000);
    ends(w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    let id = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    ((dir, bare, there, project, id, h), ssh)
}

/// The holder's work on its branch there, as it was told.
fn works_there(there: &Path, id: &str, file: &str, text: &str, message: &str) {
    let branch = format!("desk/{id}");
    if git(there, &["branch", "--list", &branch]).trim().is_empty() {
        git(there, &["fetch", "-q", "origin"]);
        git(there, &["switch", "-q", "-c", &branch, "origin/main"]);
    }
    std::fs::write(there.join(file), text).unwrap();
    git(there, &["add", "-f", file]);
    git(there, &["commit", "-q", "-m", message]);
}

#[test]
fn an_item_routed_to_a_session_on_another_machine_is_worked_on_a_branch_there_and_lands_here_at_its_done() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    // Told to work on a branch of its own there, and not to push; no copy here.
    let it = item(&w, &project, "Header jumps");
    assert!(it["tree"].is_null(), "no copy here for work on another machine: {it}");
    assert_eq!((it["remote"]["branch"].as_str(), it["remote"]["onto"].as_str(), it["machine"].as_str()), (Some(format!("desk/{id}").as_str()), Some("main"), Some("box")), "{it}");
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}"))).map(|(_, t)| t).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("git switch -c desk/{id} origin/main")) && told[0].contains("Don't push"), "{}", told[0]);
    assert!(!told[0].contains(".claude/worktrees"), "no copy here named: {}", told[0]);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    works_there(&there, &id, "b.txt", "b\n", "second go");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    // Pushed from there (its own branch only), landed here as one commit.
    let pushed = git(&there, &["rev-parse", &format!("desk/{id}")]).trim().to_string();
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/desk/{id}")]).trim(), pushed, "its branch on the remote");
    assert_eq!(git(&bare, &["rev-parse", "main"]).trim(), before, "main on the remote untouched");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), before, "one commit on the main line, no merge");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{it}");
    assert_eq!(it["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert!(it["tree"].is_null(), "its copy here is gone: {it}");
    assert!(git(&dir, &["for-each-ref", &format!("refs/cue/remote/{id}")]).trim().is_empty(), "and what was fetched");
    assert!(git(&dir, &["worktree", "list"]).lines().count() == 1, "no copy left here");
}

#[test]
fn a_never_commit_word_on_the_branch_there_hands_it_back_before_anything_is_pushed() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    w.hub().set_setting("project.never", serde_json::json!({ project.clone(): ["sekrit"] })).unwrap();
    works_there(&there, &id, "a.txt", "the sekrit plan\n", "first go");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"].as_str(), Some("working"), "back at work: {it}");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing landed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("sekrit") && back.contains("a.txt"), "{back}");
    assert!(back.contains("history"), "told every commit is pushed: {back}");
    // Taken out of the branch's history there, DONE again: it lands.
    git(&there, &["reset", "-q", "--hard", "origin/main"]);
    works_there(&there, &id, "a.txt", "the plan\n", "reworded");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(item(&w, &project, "Header jumps")["state"].as_str(), Some("to_check"));
}

#[test]
fn a_machine_that_doesnt_answer_at_done_waits_and_is_tried_again() {
    let w = World::new();
    let ((dir, _, there, project, id, h), ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    // The machine's gone quiet.
    ssh.unplug("box");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["wait"].as_str()), (Some("finishing"), Some("Machine not answering")), "{it}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1");
    // Back: the tick a minute on lands it.
    ssh.host("box", crate::testing::RUNS_IT);
    w.advance(61_000);
    super::desk_commit::tick(&w);
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
}

#[test]
fn a_route_on_a_machine_cue_doesnt_know_is_refused() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | on: nowhere | in: /src/app | why: there");
    let refused = w.hosted.sent().into_iter().any(|(s, t)| s == lead && t.contains("no machine called nowhere") && t.contains("box"));
    assert!(refused, "{:?}", w.hosted.sent());
    assert!(item(&w, &project, "Footer jumps")["on_machine"].is_null());
}

#[test]
fn a_commit_with_an_email_git_made_up_from_the_machine_is_handed_back_unpushed() {
    let w = World::new();
    let ((_, bare, there, project, id, h), _ssh) = routed_there(&w);
    git(&there, &["config", "user.email", "t@box"]);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"].as_str(), Some("working"));
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("t@box") && back.contains("user.email"), "{back}");
}

#[test]
fn a_holder_no_longer_on_that_machine_finishes_its_item_here_and_nothing_is_pushed_from_there() {
    let w = World::new();
    let ((_, bare, there, project, id, h), _ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    // Its session moved back to this Mac.
    w.hub().write(|st| {
        if let Some(s) = st.sessions.get_mut(&h) {
            s.origin.machine.clear();
        }
    });
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert!(it["remote"].is_null() && it["machine"].as_str().unwrap_or("").is_empty(), "{it}");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed from there");
}
