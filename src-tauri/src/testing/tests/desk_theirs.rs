//! Whose edits are whose in an item's commit (desk/hub/theirs/theirs.rs, #248): another session's change in the item's file
//! is read from its log, Claude Code's or Codex's, live or ended, and taken out of the commit or left out of it; a helper's
//! write in its own copy of the repo isn't one in the shared folder; a session is named only when its log shows the
//! change.

use super::desk::desk;
use super::desk_commit::{done_after_tidier_changed, done_with, ends, item, log, routed_with};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use super::team::{git, says, writes};
use crate::testing::transcript::Transcript;
use crate::testing::World;
use serde_json::json;

/// An item whose crew wrote a.txt and b.txt, said DONE after a Codex session in the same folder changed a.txt as
/// `change` does to it and its rollout, and then ended: (repo, project).
fn done_after_codex_changed(w: &World, change: impl Fn(&Transcript, &std::path::Path)) -> (std::path::PathBuf, String) {
    let (dir, project, _) = done_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
        let cx = w.session("cx").codex().in_folder(dir);
        let rollout = cx.transcript();
        rollout.entry(json!({ "type": "session_meta", "payload": { "id": "cx", "cwd": dir } }));
        cx.prompt("tidy a");
        w.advance(1000);
        change(&rollout, &dir.join("a.txt"));
        cx.stop("Tidied.");
        cx.ended();
        w.settle();
    });
    (dir, project)
}

/// A Codex session's patch to the item's file, read from its rollout after it ended: only the item's part is
/// committed, the patch stays in the file uncommitted.
#[test]
fn an_ended_codex_sessions_patch_stays_out_of_the_commit_and_in_the_file() {
    let w = World::new();
    let (dir, project) = done_after_codex_changed(&w, |rollout, a| {
        std::fs::write(a, "fixer wrote this\ncodex line\n").unwrap();
        rollout.entry(json!({ "type": "event_msg", "payload": { "type": "item_completed", "item": { "type": "FileChange",
            "changes": { a.to_string_lossy(): { "type": "update", "unified_diff": "@@ -1 +1,2 @@\n fixer wrote this\n+codex line\n", "move_path": null } } } } }));
    });
    assert!(w.snapshot().find("cx").is_none(), "it has ended");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "fixer wrote this\n", "only the item's part is committed");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "fixer wrote this\ncodex line\n", "the file stays as it is");
    assert_eq!(git(&dir, &["status", "--porcelain", "--", "a.txt", "b.txt"]).trim(), "M a.txt", "the Codex line stays uncommitted");
}

/// A Codex shell command that wrote the item's file can't be told apart: that file is left out of the commit, in the
/// folder as it is, and the rest lands (an item's commit never waits on another's, #251).
#[test]
fn a_codex_shell_write_leaves_its_file_out_of_the_commit() {
    let w = World::new();
    let (dir, project) = done_after_codex_changed(&w, |rollout, a| {
        std::fs::write(a, "fixer wrote this\nappended\n").unwrap();
        let folder = format!("file://{}", a.parent().unwrap().to_string_lossy());
        rollout.entry(json!({ "type": "event_msg", "payload": { "type": "item_completed", "item": { "type": "CommandExecution",
            "command": ["/bin/zsh", "-lc", "echo appended >> a.txt"], "cwd": folder } } }));
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert!(!git(&dir, &["show", "HEAD:b.txt"]).is_empty(), "the item's other file is committed");
    assert!(git(&dir, &["ls-files", "a.txt"]).trim().is_empty(), "a.txt isn't committed");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "fixer wrote this\nappended\n", "a.txt stays in the folder as it is");
}

/// The review's case: a helper of another session worked in a separate copy of the repo and wrote its own a.txt
/// there with a shell command. Its log line says that folder, so it isn't a write of the shared a.txt and the item's
/// commit is made with it; the same line run in the shared folder leaves a.txt out of the commit.
#[test]
fn a_helpers_write_in_its_own_copy_of_the_repo_isnt_a_write_in_the_shared_folder() {
    for in_copy in [true, false] {
        let w = World::new();
        let (dir, project) = done_after_tidier_changed(&w, |w, o, a| {
            let shared = a.parent().unwrap();
            let copy = shared.join("copy");
            std::fs::create_dir_all(&copy).unwrap();
            let ran_in = if in_copy { copy.clone() } else { shared.to_path_buf() };
            std::fs::write(ran_in.join("a.txt"), if in_copy { "hi\n" } else { "fixer wrote this\nhi\n" }).unwrap();
            let main = w.hub().session_origin(o).unwrap().transcript_path;
            let helper = Transcript::at(format!("{}/subagents/agent-h1.jsonl", main.trim_end_matches(".jsonl")));
            helper.entry(json!({ "type": "assistant", "cwd": ran_in, "message": { "content": [{ "type": "tool_use", "id": "toolu_h1", "name": "Bash", "input": { "command": "echo hi >> a.txt" } }] } }));
            log(w, o).assistant("Tidied.");
            w.settle();
        });
        let shown = item(&w, &project, "Header jumps");
        if in_copy {
            assert_eq!(shown["state"], "to_check", "{shown}");
            assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "fixer wrote this\n");
        } else {
            assert_eq!(shown["state"], "to_check", "{shown}");
            assert!(git(&dir, &["status", "--porcelain", "--", "a.txt"]).trim().ends_with("a.txt"), "a.txt left in the folder, uncommitted");
        }
    }
}

/// The item's holder wrote a.txt, its log showing it, and was then moved off it: the lead routed it to
/// another session on the Desk, which said DONE. The first holder's change is still the item's: it's committed, nothing waits on it.
#[test]
fn a_former_holders_change_stays_the_items_and_is_committed() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |w, h, dir| {
        let a = dir.join("a.txt");
        says(w, h, writes("toolu_A", "agentA", "fixer", &[a.to_string_lossy().to_string()]));
        log(w, h).tool_use("toolu_w1", "Write", json!({ "file_path": a, "content": "fixer wrote this\n" }));
        w.settle();
    });
    let d = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(&w, &h, "Over to someone else.");
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &dir.to_string_lossy(), "hello", "checker", &opts).unwrap();
    let b = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&b, HostedEvent::Taken { text: "hello".into() });
    ends(&w, &b, "Hi.");
    w.hub().desk_set_member(&project, &b, true).unwrap();
    ends(&w, &d, &format!("ROUTE: #{id} -> checker | why: a second look"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["holder"]["sid"].as_str(), Some(b.as_str()), "{it}");
    w.advance(1000);
    // The item's files are the ones its holder's work touched (a route starts that list over): its crew writes a.txt.
    says(&w, &b, writes("toolu_B", "agentB", "fixer", &[dir.join("a.txt").to_string_lossy().to_string()]));
    ends(&w, &b, &format!("Checked it.\nDONE: #{id}"));
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["former_holders"], json!([h]), "{shown}");
    assert_eq!(shown["state"], "to_check", "not waiting on cue-teams: {shown}");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "fixer wrote this\n", "the first holder's change is committed");
}

/// Another session's edit made by an inline script (`python3 - <<'EOF'`), the way most sessions edit: its file is
/// left out of the item's commit, in the folder as it is (#653: e7361cace took such edits into #650's commit).
#[test]
fn another_sessions_python_heredoc_edit_leaves_its_file_out_of_the_commit() {
    let w = World::new();
    let (dir, project) = done_after_tidier_changed(&w, |w, o, a| {
        std::fs::write(a, "fixer wrote this\ntidier's line\n").unwrap();
        let script = format!("cd {} && python3 - <<'EOF'\np='a.txt'\ns=open(p).read()\nopen(p,'w').write(s+\"tidier's line\\n\")\nEOF", dir_of(a));
        log(w, o).tool_use("toolu_t2", "Bash", json!({ "command": script }));
        w.settle();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert!(!git(&dir, &["show", "HEAD:b.txt"]).is_empty(), "the item's other file is committed");
    assert!(git(&dir, &["ls-files", "a.txt"]).trim().is_empty(), "a.txt isn't committed");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "fixer wrote this\ntidier's line\n", "a.txt stays in the folder as it is");
}

/// Another session's shell edit made before the item's holder committed its own part of the same file ("… (#id)"):
/// that commit isn't the file's last for the other session's edit, so it's still theirs and left out (#653: #650's
/// holder committed its hunk of trees.rs, and others' earlier edits there were then taken as committed).
#[test]
fn another_sessions_shell_edit_before_the_holders_own_commit_of_the_file_still_stays_out() {
    let w = World::new();
    let (dir, project, id) = super::desk_commit::done_with(&w, |_, _, _| {}, |w, h, dir| {
        let a = dir.join("a.txt");
        says(w, h, writes("toolu_A", "agentA", "fixer", &[a.to_string_lossy().to_string(), dir.join("b.txt").to_string_lossy().to_string()]));
        let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
        let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy a", "tidier", &opts).unwrap();
        let o = r["session_id"].as_str().unwrap().to_string();
        w.hosted_says(&o, HostedEvent::Taken { text: "tidy a".into() });
        w.advance(1000);
        // The tidier's line, by a shell command, before the holder's own commit.
        std::fs::write(&a, "tidier's line\n").unwrap();
        log(w, &o).tool_use("toolu_t3", "Bash", json!({ "command": format!("echo \"tidier's line\" > {}", a.display()) }));
        ends(w, &o, "Tidied.");
        w.advance(2000);
        // The holder commits only its own part of a.txt, then puts the file back as it is.
        std::fs::write(&a, "fixer wrote this\n").unwrap();
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        let id = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
        let at = format!("@{} +0000", crate::clock::now_ms() / 1000);
        git(dir, &["add", "a.txt"]);
        let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(dir).args(["commit", "-q", "-m", &format!("Its part (#{id})")])
            .env("GIT_COMMITTER_DATE", &at).env("GIT_AUTHOR_DATE", &at).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        std::fs::write(&a, "fixer wrote this\ntidier's line\n").unwrap();
        w.advance(1000);
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "fixer wrote this\n", "the tidier's line isn't committed: {id}");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "fixer wrote this\ntidier's line\n", "a.txt stays in the folder as it is");
}

fn dir_of(a: &std::path::Path) -> String {
    a.parent().unwrap().to_string_lossy().to_string()
}
