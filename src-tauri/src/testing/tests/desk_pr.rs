//! PR landing and the push choices (desk/hub/landing/pr.rs). No network: `origin` is a bare repository in a temp folder, and
//! gh is a fake script that keeps its arguments and answers from files the test writes.

use super::desk_commit::{ends, item, routed_with};
use super::desk_lands::{set, Checker};
use super::team::git;
use crate::testing::World;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A bare repository as the main checkout's `origin`, with main pushed to it, and a fake gh: (the bare repository,
/// the fake's folder).
fn remote(w: &World, dir: &Path) -> (PathBuf, PathBuf) {
    let at = std::env::temp_dir().join(format!("cue-pr-{}-{}", std::process::id(), crate::clock::now_ms()));
    let bare = at.join("origin.git");
    let fake = at.join("gh");
    std::fs::create_dir_all(&fake).unwrap();
    git(&at, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
    git(dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(dir, &["push", "-q", "origin", "main"]);
    let script = fake.join("gh");
    let f = fake.to_string_lossy();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{f}/args'\nif [ -f '{f}/hang' ]; then exec sleep 30; fi\ncase \"$2\" in\n  list) cat '{f}/open' 2>/dev/null || echo '[]';;\n  create) if [ -f '{f}/fail' ]; then cat '{f}/fail' >&2; exit 1; fi; echo 'https://example.test/o/r/pull/7';;\n  view) cat '{f}/state' || exit 1;;\nesac\n"
        ),
    )
    .unwrap();
    std::fs::write(fake.join("state"), r#"{"state":"OPEN","mergeCommit":null}"#).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    w.hub().desk_set_gh(&script.to_string_lossy());
    (bare, fake)
}

/// An item of a project set to `lands`, its copy holding a.txt, said DONE (checked by `Checker`): (main checkout,
/// project, item, its holder, the bare origin and the fake gh's folder; None for both without a remote).
fn done(w: &World, lands: Value, with_remote: bool, fail: &str) -> (PathBuf, String, String, String, Option<(PathBuf, PathBuf)>) {
    w.hub().desk_set_installer(Arc::new(Checker::default()));
    let made = std::sync::Mutex::new(None);
    let (dir, project, id, h) = routed_with(
        w,
        |w, h, dir| {
            set(w, h, lands.clone());
            if with_remote {
                let r = remote(w, dir);
                if !fail.is_empty() {
                    std::fs::write(r.1.join("fail"), fail).unwrap();
                }
                *crate::locking::lock(&made, "test remote") = Some(r);
            }
        },
        |_, _, _| {},
    );
    let path = item(w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(Path::new(&path).join("a.txt"), "a\n").unwrap();
    ends(w, &h, &format!("Fixed.\nDONE: #{id}"));
    let made = crate::locking::lock(&made, "test remote").take();
    (dir, project, id, h, made)
}

fn fin(w: &World, project: &str) -> Value {
    item(w, project, "Header jumps")["finish"].clone()
}

#[test]
fn a_pull_request_is_opened_from_its_pushed_branch_and_merged_it_lands() {
    let w = World::new();
    let (dir, project, id, _, r) = done(&w, json!({ "how": "pr" }), true, "");
    let (bare, fake) = r.unwrap();
    let before = git(&dir, &["rev-parse", "main"]);
    let f = fin(&w, &project);
    assert_eq!((f["step"].as_str(), f["pr"].as_str()), (Some("review"), Some("https://example.test/o/r/pull/7")), "{f}");
    assert_eq!(git(&dir, &["rev-parse", "main"]), before, "main isn't touched");
    let branch = format!("desk/{id}");
    let pushed = git(&bare, &["rev-parse", &branch]).trim().to_string();
    assert_eq!(git(&bare, &["show", &format!("{pushed}:a.txt")]), "a\n", "its work is on its branch at origin");
    assert_eq!(git(&bare, &["rev-parse", "main"]), before, "origin's main isn't pushed");
    let args = std::fs::read_to_string(fake.join("args")).unwrap();
    assert!(args.contains(&format!("pr create --head {branch} --base main --title Header jumps (#{id})")), "{args}");
    // Still open: in review as it was.
    w.hub().desk_pr_look(&id);
    assert_eq!(fin(&w, &project)["step"], "review");
    // Merged on the remote: landed once its commit is on main.
    git(&bare, &["update-ref", "refs/heads/main", &pushed]);
    std::fs::write(fake.join("state"), json!({ "state": "MERGED", "mergeCommit": { "oid": pushed } }).to_string()).unwrap();
    w.hub().desk_pr_look(&id);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("to_check"), true), "landed, its copy removed: {it}");
    assert_eq!(git(&dir, &["rev-parse", "main"]).trim(), pushed, "the main checkout brought up to it");
}

#[test]
fn a_pull_request_closed_without_merging_goes_back_to_its_holder() {
    let w = World::new();
    let (_, project, id, h, r) = done(&w, json!({ "how": "pr" }), true, "");
    let (_, fake) = r.unwrap();
    std::fs::write(fake.join("state"), r#"{"state":"CLOSED","mergeCommit":null}"#).unwrap();
    w.hub().desk_pr_look(&id);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["finish"]["step"].as_str(), it["finish"]["question"].as_str()), (Some("pr_closed"), Some(crate::desk::hub::landing::pr::PR_CLOSED)), "{it}");
    assert!(!it["tree"].is_null(), "its copy kept for the fix");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(crate::desk::hub::landing::pr::PR_CLOSED)), "its holder is told");
}

#[test]
fn with_no_remote_no_pull_request_is_opened_and_the_item_says_why() {
    let w = World::new();
    let (dir, project, _, _, _) = done(&w, json!({ "how": "pr" }), false, "");
    let before = git(&dir, &["rev-parse", "main"]);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["finish"]["step"], "pr_failed", "{it}");
    assert!(it["finish"]["question"].as_str().unwrap().contains("no remote"), "{it}");
    assert!(!it["tree"].is_null(), "nothing lost: its copy kept");
    assert_eq!(git(&dir, &["rev-parse", "main"]), before);
}

#[test]
fn a_pull_request_gh_refuses_says_why_in_its_words_and_runs_again() {
    let w = World::new();
    let (_, project, id, _, r) = done(&w, json!({ "how": "pr" }), true, "HTTP 422: Validation Failed");
    let (_, fake) = r.unwrap();
    let f = fin(&w, &project);
    assert_eq!((f["step"].as_str(), f["question"].as_str()), (Some("pr_failed"), Some("The pull request couldn't be opened: HTTP 422: Validation Failed")), "{f}");
    std::fs::remove_file(fake.join("fail")).unwrap();
    w.hub().desk_rerun_landing(&id).unwrap();
    w.settle();
    assert_eq!(fin(&w, &project)["step"], "review", "run again, it's opened");
}

#[test]
fn push_after_each_landing_pushes_main_without_asking() {
    let w = World::new();
    let (dir, project, id, _, r) = done(&w, json!({ "push": "after" }), true, "");
    let (bare, _) = r.unwrap();
    let main = git(&dir, &["rev-parse", "main"]);
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "landed");
    assert_eq!(git(&bare, &["rev-parse", "main"]), main, "origin's main has it");
    assert_eq!(fin(&w, &project)["pushed"], "Pushed to origin");
}

#[test]
fn push_never_and_ask_each_time_push_nothing() {
    for push in ["never", "ask"] {
        let w = World::new();
        let (dir, project, id, _, r) = done(&w, json!({ "push": push }), true, "");
        let (bare, _) = r.unwrap();
        assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "{push}: landed");
        assert_ne!(git(&bare, &["rev-parse", "main"]), git(&dir, &["rev-parse", "main"]), "{push}: origin's main isn't pushed");
        assert!(fin(&w, &project)["pushed"].is_null(), "{push}");
        assert_eq!(crate::desk::one_way::kinds(&project), ["main"], "{push}: a session's push to main is asked about as before");
    }
}

/// Item in review, its pull request merged on the remote: (main checkout, project, item, the bare origin, the fake
/// gh's folder, the merged commit).
fn merged(w: &World) -> (PathBuf, String, String, PathBuf, PathBuf, String) {
    let (dir, project, id, _, r) = done(w, json!({ "how": "pr" }), true, "");
    let (bare, fake) = r.unwrap();
    assert_eq!(fin(w, &project)["step"], "review");
    assert_eq!(fin(w, &project)["base"], "main", "its base branch is kept");
    let pushed = git(&bare, &["rev-parse", &format!("desk/{id}")]).trim().to_string();
    git(&bare, &["update-ref", "refs/heads/main", &pushed]);
    std::fs::write(fake.join("state"), json!({ "state": "MERGED", "mergeCommit": { "oid": pushed } }).to_string()).unwrap();
    (dir, project, id, bare, fake, pushed)
}

#[test]
fn a_merged_pull_request_main_cant_move_to_isnt_landed_and_says_why() {
    let w = World::new();
    let (dir, project, id, _, _, _) = merged(&w);
    // Main moved here meanwhile, not pushed: no fast-forward to the merged commit.
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    git(&dir, &["add", "b.txt"]);
    git(&dir, &["commit", "-q", "-m", "here only"]);
    w.hub().desk_pr_look(&id);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("finishing"), Some("review")), "not landed: {it}");
    let wait = it["finish"]["wait"].as_str().unwrap_or("");
    assert!(wait.starts_with("Merged; waiting to bring it onto main: git wouldn't move main"), "{wait}");
    assert!(!it["tree"].is_null(), "its copy kept");
}

#[test]
fn a_merged_pull_request_waits_while_the_main_checkout_is_on_another_branch() {
    let w = World::new();
    let (dir, project, id, _, _, pushed) = merged(&w);
    git(&dir, &["checkout", "-q", "-b", "other"]);
    let other = git(&dir, &["rev-parse", "other"]);
    w.hub().desk_pr_look(&id);
    w.settle();
    let f = fin(&w, &project);
    assert_eq!((f["step"].as_str(), f["wait"].as_str()), (Some("review"), Some("Merged; waiting to bring it onto main: the main checkout is on other, not main")), "{f}");
    assert_eq!(git(&dir, &["rev-parse", "other"]), other, "the branch it's on isn't moved");
    // Back on main: it's brought up and lands.
    git(&dir, &["checkout", "-q", "main"]);
    w.hub().desk_pr_look(&id);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert_eq!(git(&dir, &["rev-parse", "main"]).trim(), pushed);
}

#[test]
fn the_landing_isnt_run_again_while_its_pull_request_is_open() {
    let w = World::new();
    let (_, project, id, _, r) = done(&w, json!({ "how": "pr" }), true, "");
    let (_, fake) = r.unwrap();
    let no = w.hub().desk_rerun_landing(&id).unwrap_err();
    assert!(no.contains("open for review"), "{no}");
    assert_eq!(fin(&w, &project)["step"], "review");
    let args = std::fs::read_to_string(fake.join("args")).unwrap();
    assert_eq!(args.matches("pr create").count(), 1, "{args}");
}

#[test]
fn an_open_pull_request_for_its_branch_is_used_again() {
    let w = World::new();
    let (_, project, id, _, r) = done(&w, json!({ "how": "pr" }), true, "HTTP 422: Validation Failed");
    let (_, fake) = r.unwrap();
    assert_eq!(fin(&w, &project)["step"], "pr_failed");
    std::fs::write(fake.join("open"), r#"[{"url":"https://example.test/o/r/pull/3"}]"#).unwrap();
    w.hub().desk_rerun_landing(&id).unwrap();
    w.settle();
    let f = fin(&w, &project);
    assert_eq!((f["step"].as_str(), f["pr"].as_str()), (Some("review"), Some("https://example.test/o/r/pull/3")), "{f}");
    let args = std::fs::read_to_string(fake.join("args")).unwrap();
    assert!(args.contains(&format!("pr list --head desk/{id} --state open --json url")), "{args}");
    assert_eq!(args.matches("pr create").count(), 1, "only the first try created one: {args}");
}

#[test]
fn a_gh_that_hangs_is_cut_off() {
    let w = World::new();
    let (_, project, id, _, r) = done(&w, json!({ "how": "pr" }), true, "");
    let (_, fake) = r.unwrap();
    w.hub().desk_set_net_cap(300);
    std::fs::write(fake.join("hang"), "").unwrap();
    let began = std::time::Instant::now();
    for _ in 0..3 {
        w.hub().desk_pr_look(&id);
    }
    assert!(began.elapsed() < std::time::Duration::from_secs(10), "{:?}", began.elapsed());
    let f = fin(&w, &project);
    assert_eq!(f["step"], "review");
    assert_eq!(f["wait"], "Cue can't read its pull request: gh didn't answer in 300ms", "said from the third miss: {f}");
}

#[test]
fn a_push_that_hangs_is_cut_off() {
    let w = World::new();
    w.hub().desk_set_net_cap(300);
    let at = std::env::temp_dir().join(format!("cue-pr-hang-{}-{}", std::process::id(), crate::clock::now_ms()));
    std::fs::create_dir_all(&at).unwrap();
    // A remote (a folder here) whose receiving end never answers.
    let stuck = at.join("stuck");
    std::fs::write(&stuck, "#!/bin/sh\nexec sleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (repo, bare) = (at.join("r"), at.join("origin.git"));
    git(&at, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
    git(&at, &["init", "-q", "-b", "main", &repo.to_string_lossy()]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "one"]);
    git(&repo, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(&repo, &["config", "remote.origin.receivepack", &stuck.to_string_lossy()]);
    let began = std::time::Instant::now();
    let said = w.hub().net_git(&repo.to_string_lossy(), &["push", "-q", "origin", "main"]);
    assert!(began.elapsed() < std::time::Duration::from_secs(10), "{:?}", began.elapsed());
    assert_eq!(said.unwrap_err(), "git didn't answer in 300ms");
}
