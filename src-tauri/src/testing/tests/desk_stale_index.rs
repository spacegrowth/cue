//! One item commits a file, then another item's session commits from a git index it built before that (#807): #796's
//! 5cff09a, made from an index read before #791's 80e1f7c, put the older copies back, the exact reverse of it. Now
//! the guard stops that commit (desk/hub/landing/stale_index.rs), so the earlier commit stays; an index built from
//! HEAD again, or a commit of named paths, passes. Cue's own commit at a DONE keeps it too.

use super::desk_commit::{done_with, log_edit};
use super::team::{git, lead, main_repo};
use crate::testing::World;
use serde_json::{json, Value};

/// What a stopped call's hook was told.
fn denied(ask: Option<crate::testing::Ask>) -> String {
    let told: Value = ask.expect("stopped").told().expect("answered at once");
    assert_eq!(told["behavior"], "deny", "{told}");
    told["message"].as_str().unwrap().to_string()
}

#[test]
fn a_commit_from_an_index_older_than_head_is_stopped_and_the_earlier_commit_stays() {
    let w = World::new();
    let dir = main_repo(&w);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "a"]);
    let b = lead(&w, "hello");
    let s = w.session(&b).in_folder(&dir);
    // Item B's session builds its own index from HEAD, with its file in it.
    let scratch = dir.join(".git/scratch-b");
    std::fs::create_dir_all(&scratch).unwrap();
    let index = scratch.join("index");
    let with_index = |args: &[&str]| {
        let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(&dir).args(args).env("GIT_INDEX_FILE", &index).output().unwrap();
        assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    std::fs::write(dir.join("b.txt"), "B's\n").unwrap();
    with_index(&["read-tree", "HEAD"]);
    with_index(&["add", "b.txt"]);
    // Item A lands its change to a.txt meanwhile.
    std::fs::write(dir.join("a.txt"), "two\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "A's change"]);
    let a_sha = git(&dir, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    // B commits from its index, set the way #796 did: stopped, A's commit named.
    let cmd = format!("S={} && export GIT_INDEX_FILE=$S/index && git commit -q -m \"B's change\"", scratch.display());
    let why = denied(s.guard_tool("Bash", json!({ "command": cmd })));
    assert!(why.contains("a.txt") && why.contains(&a_sha) && why.contains("read-tree HEAD"), "{why}");
    assert!(denied(s.guard_tool("Bash", json!({ "command": format!("GIT_INDEX_FILE={} git write-tree", index.display()) }))).contains("a.txt"));
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "two\n", "A's commit stays");
    // Built from HEAD again in the same command, or committing named paths: it passes.
    let fresh = format!("export GIT_INDEX_FILE={} && git read-tree HEAD && git add b.txt && git commit -q -m x", index.display());
    assert!(s.guard_tool("Bash", json!({ "command": fresh })).is_none());
    assert!(s.guard_tool("Bash", json!({ "command": format!("GIT_INDEX_FILE={} git commit -q -m x -- b.txt", index.display()) })).is_none());
    // Its own index built again from HEAD: it passes, and its commit keeps A's change.
    with_index(&["read-tree", "HEAD"]);
    with_index(&["add", "b.txt"]);
    assert!(s.guard_tool("Bash", json!({ "command": cmd })).is_none());
    with_index(&["commit", "-q", "-m", "B's change"]);
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "two\n");
    assert_eq!(git(&dir, &["show", "HEAD:b.txt"]), "B's\n");
}

#[test]
fn the_shared_index_left_behind_by_a_commit_made_without_it_is_stopped_too() {
    let w = World::new();
    let dir = main_repo(&w);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "a"]);
    // Item A commits a.txt with an index of its own and update-ref: the folder's index still has the old a.txt.
    std::fs::write(dir.join("a.txt"), "two\n").unwrap();
    let index = dir.join(".git/a-index");
    let with_index = |args: &[&str]| -> String {
        let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(&dir).args(args).env("GIT_INDEX_FILE", &index).output().unwrap();
        assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    with_index(&["read-tree", "HEAD"]);
    with_index(&["add", "a.txt"]);
    let tree = with_index(&["write-tree"]);
    let old = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let new = git(&dir, &["commit-tree", &tree, "-p", &old, "-m", "A's change"]).trim().to_string();
    git(&dir, &["update-ref", "refs/heads/main", &new, &old]);
    // Item B stages its own file and commits from the folder's index: stopped, told to bring a.txt up to HEAD.
    std::fs::write(dir.join("b.txt"), "B's\n").unwrap();
    git(&dir, &["add", "b.txt"]);
    let b = lead(&w, "hello");
    let s = w.session(&b).in_folder(&dir);
    let why = denied(s.guard_tool("Bash", json!({ "command": "git commit -q -m \"B's change\"" })));
    assert!(why.contains("a.txt") && why.contains("git reset -q -- a.txt"), "{why}");
    // `-am` takes every tracked file from the folder: it passes.
    assert!(s.guard_tool("Bash", json!({ "command": "git commit -qam x" })).is_none());
    git(&dir, &["reset", "-q", "--", "a.txt"]);
    assert!(s.guard_tool("Bash", json!({ "command": "git commit -q -m \"B's change\"" })).is_none());
    git(&dir, &["commit", "-q", "-m", "B's change"]);
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "two\n", "A's commit stays");
}

#[test]
fn cues_own_commit_at_a_done_keeps_a_commit_made_after_the_folders_index_was() {
    let w = World::new();
    let (dir, _, _) = done_with(&w, |_, _, dir| {
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        git(dir, &["add", "a.txt", "b.txt"]);
        git(dir, &["commit", "-q", "-m", "a and b"]);
    }, |w, h, dir| {
        // Another item commits a.txt without the folder's index, leaving it behind HEAD.
        std::fs::write(dir.join("a.txt"), "two\n").unwrap();
        let blob = git(dir, &["hash-object", "-w", "a.txt"]).trim().to_string();
        let index = dir.join(".git/a-index");
        let run = |args: &[&str]| {
            let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(dir).args(args).env("GIT_INDEX_FILE", &index).output().unwrap();
            assert!(o.status.success(), "{args:?}");
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        run(&["read-tree", "HEAD"]);
        run(&["update-index", "--cacheinfo", &format!("100644,{blob},a.txt")]);
        let tree = run(&["write-tree"]);
        let old = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
        let new = git(dir, &["commit-tree", &tree, "-p", &old, "-m", "Another item's a.txt"]).trim().to_string();
        git(dir, &["update-ref", "refs/heads/main", &new, &old]);
        // This item changes b.txt.
        std::fs::write(dir.join("b.txt"), "b fixed\n").unwrap();
        log_edit(w, h, &dir.join("b.txt"));
    });
    assert_eq!(git(&dir, &["show", "HEAD:b.txt"]), "b fixed\n", "the item's change is committed");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "two\n", "the other item's earlier commit stays");
}
