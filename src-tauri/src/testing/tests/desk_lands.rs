//! How work lands, per project (lands.rs): the Desk shows the setting with its defaults, refuses what it can't do
//! yet, and works as it says: the shared folder (no copy), items at once (the pool), you commit (the Desk commits
//! nothing) and no check (landed, not checked).

use super::desk::desk;
use super::desk_commit::{commits, ends, item, log_edit, routed_with};
use super::team::git;
use crate::desk::{InstallStatus, Installer};
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// An installer whose check passes, and that keeps what it was asked to check.
#[derive(Default)]
pub(super) struct Checker {
    pub(super) checked: Mutex<Vec<String>>,
}

impl Installer for Checker {
    fn has(&self, _repo: &str, _commit: &str) -> bool {
        false
    }

    fn start(&self, _repo: &str, _commit: &str, _tag: &str) -> Result<(), String> {
        Ok(())
    }

    fn status(&self) -> Option<InstallStatus> {
        None
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::self_install::is_ancestor(repo, commit, of)
    }

    fn check(&self, _repo: &str, commit: &str) -> Option<Result<(), String>> {
        crate::locking::lock(&self.checked, "checker checked").push(commit.to_string());
        Some(Ok(()))
    }
}

/// Session `h`'s project set to `lands`, as the Desk's setting saves it.
pub(super) fn set(w: &World, h: &str, lands: Value) {
    let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
    crate::lands::save(&project, crate::lands::Place::Repo, &lands).unwrap();
}

/// The repository's main checkout (where its copies are made from).
fn top(w: &World) -> String {
    let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// The holder edits a.txt in the folder it works in.
fn edits(w: &World, h: &str, dir: &std::path::Path) {
    std::fs::write(dir.join("a.txt"), "its work\n").unwrap();
    log_edit(w, h, &dir.join("a.txt"));
}

#[test]
fn a_new_project_shows_the_defaults_and_what_cant_be_picked_yet_says_so() {
    let w = World::new();
    let (_, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = &desk(&w, &project)["lands"];
    assert_eq!(l["set"], json!({ "where": "copy", "how": "squash", "check": "none", "after": "nothing", "push": "ask", "at_once": 3 }), "{l}");
    assert_eq!(l["summary"], "own copy, not checked, one commit per item");
    let choice = |part: &str, key: &str| l["parts"].as_array().unwrap().iter().find(|p| p["key"] == part).unwrap()["choices"].as_array().unwrap().iter().find(|c| c["key"] == key).cloned().unwrap();
    for (part, key) in [("where", "branch"), ("how", "merge"), ("check", "command"), ("after", "command")] {
        assert_eq!((choice(part, key)["ok"].as_bool(), choice(part, key)["why"].as_str()), (Some(false), Some("not yet")), "{part} {key}");
    }
    assert_eq!(choice("check", "cue")["why"], "only for Cue's own project");
    for (part, key) in [("how", "pr"), ("push", "after"), ("push", "never")] {
        assert_eq!(choice(part, key)["ok"], true, "{part} {key} can be picked");
    }
    let err = w.hub().desk_lands_set(&project, &json!({ "how": "merge" })).unwrap_err();
    assert!(err.contains("not yet"), "{err}");
    let saved = w.hub().desk_lands_set(&project, &json!({ "where": "shared", "at_once": 2 })).unwrap();
    assert_eq!((saved["set"]["where"].as_str(), saved["set"]["at_once"].as_u64(), saved["summary"].as_str()), (Some("shared"), Some(2), Some("shared folder, not checked, one commit per item")));
    assert_eq!(desk(&w, &project)["lands"]["set"]["where"], "shared", "kept in Cue's settings");
}

#[test]
fn a_folder_that_isnt_a_git_repository_works_in_place_and_says_why() {
    let _w = World::new();
    let dir = std::env::temp_dir().join(format!("cue-lands-plain-{}-{}", std::process::id(), crate::clock::now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let folder = dir.to_string_lossy().to_string();
    crate::logs::repos::first_look(&folder);
    let v = crate::lands::desk_view("plain", &folder);
    assert_eq!((v["place"].as_str(), v["summary"].as_str()), (Some("folder"), Some("shared folder, not checked, you commit")), "{v}");
    assert!(v["why"].as_str().unwrap().contains("isn't a git repository"), "{v}");
    let err = crate::lands::save("plain", crate::lands::place_of(&folder), &json!({ "where": "copy" })).unwrap_err();
    assert!(err.contains("needs a git repository"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_the_shared_folder_an_item_gets_no_copy() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "where": "shared" })), edits);
    let it = item(&w, &project, "Header jumps");
    assert!(it["tree"].is_null(), "no copy: {it}");
    assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(".claude/worktrees/desk-")), "its holder isn't sent to a copy");
    let before = commits(&dir);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_ne!(commits(&dir), before, "the Desk committed its files in the shared folder");
}

#[test]
fn items_at_once_is_the_pool_of_copies() {
    let w = World::new();
    let (_, project, id, _) = routed_with(
        &w,
        |w, h, _| {
            set(w, h, json!({ "at_once": 1 }));
            w.hub().desk_hold_copies(&top(w), &[1]);
        },
        |_, _, _| {},
    );
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("queued"), true), "one copy, held: {it}");
    let summary = w.hub().desk_summary(&project, "", false).0;
    let line = summary.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("{summary}"));
    assert!(line.contains("(1 of 1 held, by #held1)"), "{line}");
    w.hub().desk_let_go_copy_quietly(1);
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(1)), "{it}");
}

#[test]
fn no_check_lands_without_running_one_and_says_not_checked() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "check": "none" })), |_, _, _| {});
    let path = item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "landed");
    assert!(crate::locking::lock(&checker.checked, "checker checked").is_empty(), "no check run");
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["unchecked"], true, "landed, not checked");
}

#[test]
fn you_commit_means_the_desk_commits_nothing() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "where": "shared", "how": "owner" })), edits);
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "works in the shared folder");
    let before = commits(&dir);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"].is_null()), (Some("to_check"), true), "{it}");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(commits(&dir), before, "nothing committed");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "its work\n", "its work left for you");
}

#[test]
fn a_folder_that_isnt_a_git_repository_with_nothing_saved_gets_no_copy_and_you_commit() {
    // As routed_with, with no repository: the session works in a plain folder.
    let w = World::new();
    let dir = w.home().to_path_buf();
    assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "--show-toplevel"]).output().is_ok_and(|o| !o.status.success()), "a plain folder");
    let h = super::team::lead(&w, "hello");
    ends(&w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, crate::hosted::HostedEvent::Taken { text: "hello".into() });
    ends(&w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    edits(&w, &h, &dir);
    let id = item(&w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "no copy");
    assert_eq!(desk(&w, &project)["lands"]["summary"], "shared folder, not checked, you commit");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"].is_null()), (Some("to_check"), true), "yours to commit: {it}");
    let r = w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!((r["files"].as_u64(), r["commit"].as_str()), (Some(0), Some("")), "nothing committed: {r}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "its work\n", "its work left for you");
}
