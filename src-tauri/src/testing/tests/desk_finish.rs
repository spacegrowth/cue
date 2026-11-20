//! Done means it's in your app (desk/hub/landing/finish.rs): an item that changed the Cue this app is built from, once its
//! commit is made, is in the next install: half an hour after the first DONE waiting (or at once on Install now), never
//! while a session that ends with Cue is mid-turn, built from the main line's HEAD. Once the running Cue has its commit
//! it's to check; a failed build, or a new Cue that didn't start, asks you and sends its holder the log. A crew's work
//! on a branch of its own is merged into the main checkout's branch first, never forced.

use super::desk::desk;
use super::desk_commit::{commits, done_with, ends, item, routed_with, tick};
use super::team::{git, runs, says, starts, writes};
use crate::desk::{InstallStatus, Installer};
use crate::hosted::HostedEvent;
use super::desk_install::{install_goes, mid_turn};
use crate::testing::World;
use serde_json::json;
use std::sync::{Arc, Mutex};

/// An installer that installs nothing: what it was asked to start, how its install goes, and the commits the "running
/// Cue" has, as each test sets them.
#[derive(Default)]
pub(super) struct Fake {
    pub(super) starts: Mutex<Vec<(String, String, String)>>,
    pub(super) status: Mutex<Option<InstallStatus>>,
    pub(super) has: Mutex<Vec<String>>,
    pub(super) refuse: Mutex<String>,
}

impl Installer for Fake {
    fn has(&self, _repo: &str, commit: &str) -> bool {
        let has = crate::locking::lock(&self.has, "fake installer has");
        has.iter().any(|c| c == commit)
    }

    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String> {
        let refuse = crate::locking::lock(&self.refuse, "fake installer refuse").clone();
        if !refuse.is_empty() {
            return Err(refuse);
        }
        crate::locking::lock(&self.starts, "fake installer starts").push((repo.into(), commit.into(), tag.into()));
        *crate::locking::lock(&self.status, "fake installer status") = Some(InstallStatus { tag: tag.into(), commit: commit.into(), step: "building".into(), ..Default::default() });
        Ok(())
    }

    fn status(&self) -> Option<InstallStatus> {
        crate::locking::lock(&self.status, "fake installer status").clone()
    }

    fn contains(&self, _repo: &str, commit: &str, of: &str) -> bool {
        commit == of
    }
}

impl Fake {
    pub(super) fn started(&self) -> Vec<(String, String, String)> {
        crate::locking::lock(&self.starts, "fake installer starts").clone()
    }

    /// Its latest install now at `step`, with `log`.
    pub(super) fn goes(&self, step: &str, log: &str) {
        let mut s = crate::locking::lock(&self.status, "fake installer status");
        if let Some(s) = s.as_mut() {
            (s.step, s.log) = (step.into(), log.into());
        }
    }

    /// An install of `commit` begun outside the Desk (a session ran the helper itself), called `tag`, building.
    pub(super) fn begun_elsewhere(&self, tag: &str, commit: &str) {
        *crate::locking::lock(&self.status, "fake installer status") = Some(InstallStatus { tag: tag.into(), commit: commit.into(), step: "building".into(), ..Default::default() });
    }

    /// The running Cue has `commit` now.
    pub(super) fn runs(&self, commit: &str) {
        crate::locking::lock(&self.has, "fake installer has").push(commit.into());
    }
}

/// `dir` made to hold this Cue's own source, as builds_this_cue reads it.
pub(super) fn as_this_cue(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
    std::fs::write(dir.join("src-tauri/tauri.conf.json"), "{ \"identifier\": \"dev.spacegrowth.cue\" }\n").unwrap();
}

/// An item done in this Cue's source (its crew wrote a.txt and b.txt), with a fake installer: (fake, repo, project, id).
pub(super) fn done_in_cue(w: &World) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    (fake, dir, project, id)
}

pub(super) fn step(w: &World, project: &str) -> String {
    item(w, project, "Header jumps")["finish"]["step"].as_str().unwrap_or("").to_string()
}

/// The World's repository with a crew teammate in a worktree of its own (branch worktree-agent-agentW), which writes
/// `file` there: the lead, the worktree.
pub(super) fn crew_in_worktree(w: &World, h: &str, dir: &std::path::Path, file: &str) -> std::path::PathBuf {
    let wt = dir.join(".claude/worktrees/agent-agentW");
    git(dir, &["worktree", "add", "-q", "-b", "worktree-agent-agentW", &wt.to_string_lossy()]);
    starts(w, h, "toolu_W", "agentW", "builder", true);
    std::fs::write(wt.join(file), "builder's\n").unwrap();
    runs(w, h, "toolu_W", 1, &format!("printf 'builder\\n' > {file}"));
    says(w, h, [HostedEvent::Task { what: "task_notification".into(), detail: json!({ "subtype": "task_notification", "tool_use_id": "toolu_W", "task_id": "agentW", "status": "completed", "summary": "Done." }) }]);
    wt
}

#[test]
fn crew_work_on_a_branch_of_its_own_is_merged_into_the_main_checkout_never_pushed() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["branch", "--show-current"]).trim(), "main");
    assert!(git(&dir, &["log", "-1", "--format=%s"]).trim().starts_with("Merge worktree-agent-agentW"), "{}", git(&dir, &["log", "-3", "--format=%s"]));
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "builder's\n", "in the main checkout");
    assert!(git(&dir, &["remote"]).trim().is_empty(), "never pushed");
}

#[test]
fn a_merge_that_conflicts_goes_back_to_its_holder_and_leaves_the_main_checkout_as_it_was() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        std::fs::write(dir.join("a.txt"), "yours\n").unwrap();
        git(dir, &["add", "a.txt"]);
        git(dir, &["commit", "-q", "-m", "yours"]);
    });
    let shown = item(&w, &project, "Header jumps");
    // #371: a clash in its work goes back to the session that did it, not to you.
    assert_eq!(shown["state"], "working", "{shown}");
    assert!(shown["finish"].is_null(), "{shown}");
    let h = holder(&w, &project);
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("isn't done") && t.contains("a.txt") && t.contains("settle the clash")), "its holder is told what clashed");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), "yours", "nothing merged");
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty(), "nothing left half-merged");
}

/// The holder of the item "Header jumps".
pub(super) fn holder(w: &World, project: &str) -> String {
    item(w, project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string()
}

/// The Desk's lead (#248).
pub(super) fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

/// A route back to its holder while its finish is under way: it stays in the install, and the why goes to its holder
/// as a note (#338), the lead told so; with no why, it's refused.
#[test]
fn a_route_of_a_finishing_item_under_way_goes_to_its_holder_as_a_note_and_it_stays_in_the_install() {
    let w = World::new();
    let (_fake, _dir, project, id) = done_in_cue(&w);
    let (h, d) = (holder(&w, &project), lead_of(&w, &project));
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: one more thing"));
    let told = sent_to(&w, &d).last().cloned().unwrap();
    assert_eq!(told, format!("Noted: #{id} is committed and waiting for the next install, so it stays as it is; your why went to cue-teams as a note."));
    assert!(sent_to(&w, &h).last().unwrap().ends_with(": one more thing"), "{:?}", sent_to(&w, &h));
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams"));
    let told = sent_to(&w, &d).last().cloned().unwrap();
    assert_eq!(told, format!("Refused: #{id} is committed and waiting for the next install. You can wait for it, tell its holder something (NOTE: #{id} | …), or ask the user (ASK: #{id} | …)."));
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("queued")), "{shown}");
}

/// Its finish under way, a route to a session off the Desk is refused at once: the user isn't asked (#248).
#[test]
fn a_finishing_item_under_way_routed_off_the_desk_is_refused_without_asking_you() {
    let w = World::new();
    let (_fake, _dir, project, id) = done_in_cue(&w);
    let d = lead_of(&w, &project);
    let o = mid_turn(&w);
    assert_eq!(w.snapshot().session(&o)["project"].as_str(), Some(project.as_str()), "a session of the project, off its Desk");
    ends(&w, &d, &format!("ROUTE: #{id} -> tidier | why: it knows the build"));
    assert_eq!(sent_to(&w, &d).last().unwrap(), &format!("Refused: #{id} is committed and waiting for the next install. You can wait for it, tell its holder something (NOTE: #{id} | …), or ask the user (ASK: #{id} | …)."));
    let snap = w.snapshot();
    let asked: Vec<_> = snap.cards_of(&d).into_iter().filter(|c| !c["tool_input"]["desk_use"].is_null()).collect();
    assert!(asked.is_empty(), "the user isn't asked to add tidier: {asked:?}");
    assert_eq!(step(&w, &project), "queued");
}

#[test]
fn rerun_on_a_word_you_took_out_commits_it() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let before = |w: &World, h: &str, dir: &std::path::Path| {
        as_this_cue(dir);
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        w.hub().set_setting("project.never", json!({ project: ["Fixer"] })).unwrap();
    };
    let (dir, project, id) = done_with(&w, before, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    assert_eq!((step(&w, &project), commits(&dir)), ("word".to_string(), "1".to_string()));
    w.hub().set_setting("project.never", json!({})).unwrap();
    w.hub().desk_item_act(&id, "rerun", "").unwrap();
    w.settle();
    assert_eq!((step(&w, &project), commits(&dir)), ("queued".to_string(), "2".to_string()));
}

#[test]
fn a_remote_sessions_work_waits_on_looks_right_for_its_commit_never_closed_uncommitted() {
    let w = World::new();
    let ssh = crate::testing::FakeSsh::new();
    ssh.host("box", crate::testing::RUNS_IT);
    crate::machines::add("box", "").unwrap();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
        // Its session is on another machine (played here, its git run in place).
        w.hub().desk_session_on(h, "box");
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"].is_null()), (Some("to_check"), true), "{shown}");
    assert_eq!(commits(&dir), "1", "nothing committed at DONE");
    assert!(fake.started().is_empty(), "nothing to install here");
    let r = w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(commits(&dir), "2", "committed on Looks right: {r}");
    assert_eq!((item(&w, &project, "Header jumps")["state"].as_str(), r["commit"].as_str()), (Some("done"), Some(git(&dir, &["rev-parse", "--short", "HEAD"]).trim())));
    w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!(commits(&dir), "1", "Undo takes it back");
}

#[test]
fn a_restart_before_its_commit_was_made_makes_it_after() {
    let mut w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id, h) = super::desk_commit::routed_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    // DONE, and Cue stops before its commit runs.
    w.hosted_says(&h, HostedEvent::TurnEnded { text: format!("Fixed.\nDONE: #{id}"), failed: false, cost_usd: 0.0, unprompted: false });
    w.relaunch_cut();
    w.hub().desk_set_installer(fake.clone());
    assert_eq!((step(&w, &project), commits(&dir)), ("commit".to_string(), "1".to_string()));
    tick(&w);
    assert_eq!((step(&w, &project), commits(&dir)), ("queued".to_string(), "2".to_string()));
}

#[test]
fn a_word_in_a_crews_merged_work_or_its_merge_message_stops_the_merge() {
    for (word, at) in [("builder", "a.txt"), ("agentW", "the merge's message")] {
        let w = World::new();
        let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
            let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
            w.hub().set_setting("project.never", json!({ project: [word] })).unwrap();
            crew_in_worktree(w, h, dir, "a.txt");
        });
        let shown = item(&w, &project, "Header jumps");
        assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("word")), "{shown}");
        assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
        assert!(shown["latest"].as_str().unwrap().contains(at), "{shown}");
        assert!(!git(&dir, &["log", "-1", "--format=%s"]).starts_with("Merge"), "nothing merged");
    }
}

#[test]
fn a_main_checkout_mid_merge_is_left_alone_and_goes_to_the_lead() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        // Your own merge, stopped on a conflict in shared.txt.
        git(dir, &["switch", "-q", "-c", "yours"]);
        std::fs::write(dir.join("shared.txt"), "yours\n").unwrap();
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "-m", "yours"]);
        git(dir, &["switch", "-q", "main"]);
        std::fs::write(dir.join("shared.txt"), "mine\n").unwrap();
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "-m", "mine"]);
        let o = std::process::Command::new("git").arg("-C").arg(dir).args(["merge", "-q", "yours"]).output().unwrap();
        assert!(!o.status.success(), "a conflict");
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("conflict")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    assert!(shown["latest"].as_str().unwrap().contains("in the middle of a merge"), "{shown}");
    assert!(dir.join(".git/MERGE_HEAD").exists(), "your merge still in progress");
}

#[test]
fn a_crew_merge_onto_a_file_with_your_uncommitted_changes_is_refused_and_leaves_them() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        std::fs::write(dir.join("a.txt"), "yours, not committed\n").unwrap();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("conflict")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    assert_eq!(shown["latest"], "Not merged: the main checkout has edits of its own in a.txt that aren't committed", "{shown}");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "yours, not committed\n");
    assert!(!dir.join(".git/MERGE_HEAD").exists());
    assert!(!git(&dir, &["log", "-1", "--format=%s"]).starts_with("Merge"), "nothing merged");
}

/// What was sent to session `sid`.
pub(super) fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

/// The item's crew writes a.txt and b.txt in the repository.
pub(super) fn writes_both(w: &World, h: &str, dir: &std::path::Path) {
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
}

#[test]
fn a_holder_that_stops_with_its_work_uncommitted_goes_to_the_lead_as_not_committed_never_to_you() {
    let w = World::new();
    // Its holder edits the files itself (no crew: a crew's finish asks it for a review).
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |w, h, dir| {
        for n in ["a.txt", "b.txt"] {
            std::fs::write(dir.join(n), "its holder wrote this\n").unwrap();
            super::desk_commit::log_edit(w, h, &dir.join(n));
        }
    });
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let before = commits(&dir);
    w.advance(1000);
    ends(&w, &h, "Both files are written.\nShall I commit them?");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["latest"].as_str()), (Some("working"), Some(true), Some("Not committed: a.txt, b.txt")), "the lead's, not yours: {it}");
    assert!(it["ask"].is_null(), "{it}");
    let told: Vec<String> = sent_to(&w, &lead).into_iter().filter(|t| t.starts_with(&format!("#{id} (Header jumps) stopped, not committed:"))).collect();
    assert_eq!(told.len(), 1, "{:?}", sent_to(&w, &lead));
    assert!(told[0].contains("a.txt, b.txt"), "{}", told[0]);
    // Its time to settle runs out: still the lead's while its work isn't committed.
    w.advance(11 * 60_000);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool()), (Some("working"), Some(true)), "{it}");
    assert_eq!(commits(&dir), before, "Cue commits nothing without a DONE");

    // Committed now: a question its next turn ends on is yours.
    git(&dir, &["add", "a.txt", "b.txt"]);
    git(&dir, &["commit", "-q", "-m", "Header jumps"]);
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: format!("#{id}: go on") });
    w.settle();
    w.advance(1000);
    ends(&w, &h, "Committed.\nShall I fix the footer too?");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("Shall I fix the footer too?")), "{it}");
}

#[test]
fn work_its_holder_committed_itself_still_waits_until_the_running_cue_has_it() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id, h) = routed_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        writes_both(w, h, dir);
        git(dir, &["add", "a.txt", "b.txt"]);
        git(dir, &["commit", "-q", "-m", "Header jumps, by its holder"]);
    });
    ends(&w, &h, &format!("Fixed and committed.\nDONE: #{id}"));
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("finishing"), Some("queued"), Some(head.as_str())), "not to check before the app has it: {shown}");
    fake.runs(&head);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

#[test]
fn a_commit_that_takes_an_items_work_back_out_reopens_it_and_says_so() {
    let w = World::new();
    let (dir, project, id) = done_with(&w, |_, _, _| {}, writes_both);
    let h = item(&w, &project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string();
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    tick(&w);
    // A commit that leaves its work alone: nothing reopens.
    std::fs::write(dir.join("a.txt"), "fixer wrote this\nand one more line here\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "One more line"]);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");

    // One that takes it back out.
    std::fs::write(dir.join("a.txt"), "and one more line here\n").unwrap();
    std::fs::remove_file(dir.join("b.txt")).unwrap();
    git(&dir, &["commit", "-q", "-am", "Back to how it was"]);
    let sha = git(&dir, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    tick(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"].is_null()), (Some("working"), true), "{it}");
    let why = format!("Commit {sha} (Back to how it was) took its work back out");
    let told: Vec<String> = sent_to(&w, &h).into_iter().filter(|t| t.starts_with(&format!("Desk item #{id} (Header jumps) is open again: {why}"))).collect();
    assert_eq!(told.len(), 1, "its holder is told: {:?}", sent_to(&w, &h));
    let steps = desk(&w, &project)["decisions"].as_array().cloned().unwrap_or_default();
    assert!(steps.iter().any(|d| d["text"].as_str() == Some(why.as_str()) && d["item_id"].as_str() == Some(id.as_str())), "its page says so: {steps:?}");
    tick(&w);
    assert_eq!(sent_to(&w, &h).iter().filter(|t| t.contains("took its work back out")).count(), 1, "once");
}

#[test]
fn a_commit_that_moves_an_items_work_to_other_files_leaves_it_alone() {
    let w = World::new();
    let (dir, project, id) = done_with(&w, |_, _, _| {}, writes_both);
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    tick(&w);
    // Its files moved to a folder of their own, as a refactor does (#475's work moved by #509): still there.
    std::fs::create_dir_all(dir.join("moved")).unwrap();
    git(&dir, &["mv", "a.txt", "moved/a.txt"]);
    git(&dir, &["mv", "b.txt", "moved/b.txt"]);
    git(&dir, &["commit", "-q", "-m", "Move the files"]);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done", "{}", item(&w, &project, "Header jumps"));
    assert!(!desk(&w, &project)["decisions"].as_array().cloned().unwrap_or_default().iter().any(|d| d["text"].as_str().is_some_and(|t| t.contains("took its work back out"))));
}

/// `dir`'s pre-commit hook: refuses every commit saying `words` (none: it's taken out).
pub(super) fn hook_says(dir: &std::path::Path, words: Option<&str>) {
    let hook = dir.join(".git/hooks/pre-commit");
    match words {
        Some(w) => {
            std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
            std::fs::write(&hook, format!("#!/bin/sh\necho '{w}' >&2\nexit 1\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        None => std::fs::remove_file(&hook).unwrap(),
    }
}

#[test]
fn a_commit_git_refuses_says_why_in_gits_words_and_a_rerun_says_it_again() {
    let w = World::new();
    let (dir, project, id) = done_with(&w, |_, _, dir| hook_says(dir, Some("lint: a.txt has a tab")), writes_both);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("error")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    let q = shown["finish"]["question"].as_str().unwrap();
    assert!(q.starts_with("Not committed: git didn't commit the files in") && q.ends_with(": lint: a.txt has a tab"), "{q}");

    // Run the landing again, stopped once more: what stopped it this time.
    hook_says(&dir, Some("lint: b.txt is too long"));
    let it = w.hub().desk_item_act(&id, "rerun", "").unwrap();
    let it = serde_json::to_value(it).unwrap();
    assert!(it["finish"]["question"].as_str().unwrap().ends_with(": lint: b.txt is too long"), "{it}");

    // The hook happy: running the landing again commits it.
    let before = commits(&dir);
    hook_says(&dir, None);
    w.hub().desk_item_act(&id, "rerun", "").unwrap();
    assert_ne!(commits(&dir), before, "committed");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

#[test]
fn gits_last_words_drop_hints_and_keep_the_last_three_lines() {
    let said = "error: one\nhint: do this\n\nerror: two\nerror: three\nfatal: four\n";
    assert_eq!(crate::safe_git::last_words(said), "error: two / error: three / fatal: four");
    assert_eq!(crate::safe_git::last_words(""), "");
    assert!(crate::safe_git::last_words(&"x".repeat(400)).ends_with('…'));
}

/// How work lands, step 2 (#247): an item is to check, with Looks right, only once its commit is on main. Its main
/// checkout on another branch, it waits at "landing" saying where its work is: not to check, not in an install, no
/// Looks right. Once main has it, it goes on (here into the next install, then to check once Cue has it).
#[test]
fn an_item_shows_ready_only_once_its_commit_is_on_main_and_says_where_it_is_till_then() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(&w, |_, _, dir| {
        as_this_cue(dir);
        git(dir, &["checkout", "-q", "-b", "side"]);
    }, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    let side = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(commits(&dir), "2", "committed at DONE, on the branch the main checkout is on");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["wait"].as_str()), (Some("finishing"), Some("landing"), Some("on side, not main")), "{shown}");
    assert!(w.hub().desk_looks_right(&id).is_err(), "no Looks right before it's on main");
    w.advance(31 * 60_000);
    tick(&w);
    assert!(fake.started().is_empty(), "nothing installed that isn't on main");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("landing")), "asked after 15 minutes: {shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    assert!(shown["finish"]["question"].as_str().unwrap().contains("on side, not main"), "{shown}");

    // Main gets it: on its way into the app, then to check once Cue has it.
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--ff-only", "side"]);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("finishing"), Some("queued"), Some(side.as_str())), "{shown}");
    assert!(w.hub().desk_looks_right(&id).is_err(), "no Looks right before it's installed");
    fake.runs(&side);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    w.hub().desk_looks_right(&id).unwrap();
}

/// #337: you tell the lead an item that's still finishing is done: its commit and install go on, and it closes as
/// Looks right would once Cue has it, never waiting on your check.
#[test]
fn the_leads_done_on_a_finishing_item_closes_it_once_installed() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("queued")), "{shown}");
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(&w, &lead, &format!("The user says it's done.\nDONE: #{id}"));
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["close_on_check"].as_bool()), (Some("finishing"), Some(true)), "still finishing, to close once checked: {shown}");
    assert!(!w.hosted.sent().into_iter().any(|(s, t)| s == lead && t.contains("not closed")), "nothing to tell the lead");
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 1, "its install still made");
    fake.runs(&head);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "done", "closed once Cue has it: {shown}");
    assert_eq!(commits(&dir), "2", "no second commit");
}

/// #338: the lead's NOTE reaches whoever holds an item while it's finishing, shows on its Thread (a step with its
/// delivery), and leaves it finishing; a route back to its holder with a why, while its install waits, goes to it as a
/// note too (never dropped), and the lead is told so. An item nobody holds: the lead is told why.
#[test]
fn the_lead_can_send_a_note_to_the_holder_of_a_finishing_item() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (_, project, id) = done_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt")]));
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("queued")), "{shown}");
    let (holder, name) = (shown["holder"]["sid"].as_str().unwrap().to_string(), shown["holder"]["name"].as_str().unwrap().to_string());
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let to = |w: &World, sid: &str, has: &str| w.hosted.sent().into_iter().filter(|(s, t)| s == sid && t.contains(has)).map(|(_, t)| t).collect::<Vec<_>>();
    ends(&w, &lead, &format!("Telling it.\nNOTE: #{id} | check the header on a small window | too\nNOTE: #zz9 | hello"));
    w.settle();
    let got = to(&w, &holder, "Note from the Desk lead");
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0], format!("Note from the Desk lead on Desk item #{id} (Header jumps, finishing): check the header on a small window | too"));
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "finishing", "it stays as it is: {shown}");
    let decs = desk(&w, &project)["decisions"].clone();
    let step = decs.as_array().unwrap().iter().find(|d| d["why"] == format!("note to {name}")).cloned().unwrap_or_else(|| panic!("a step on its Thread: {decs}"));
    assert_eq!((step["item_id"].as_str(), step["text"].as_str()), (Some(id.as_str()), Some("check the header on a small window | too")));
    assert_eq!(step["delivery"]["session_id"], holder.as_str());
    let told = to(&w, &lead, "#zz9 note not sent");
    assert_eq!(told.len(), 1, "{told:?}");
    // A route back to it while it waits for its install: not refused, not dropped: its why goes to it as a note.
    ends(&w, &lead, &format!("ROUTE: #{id} -> {name} | why: also the footer"));
    w.settle();
    let got = to(&w, &holder, "also the footer");
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].starts_with(&format!("Note from the Desk lead on Desk item #{id}")), "{}", got[0]);
    let told = to(&w, &lead, &format!("Noted: #{id}"));
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("went to {name} as a note")), "{}", told[0]);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
}

/// A project that installs nothing: on main is the last step, and the item is to check as soon as it lands there.
#[test]
fn an_item_in_a_project_with_no_install_is_to_check_once_its_commit_is_on_main() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, dir| { git(dir, &["checkout", "-q", "-b", "side"]); }, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt")]));
    });
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["step"], "landing");
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--ff-only", "side"]);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

/// #343: main was what broke an item's build. A commit fixing main, and the item's landing run again: built from main
/// as it is now (never the commit that failed), and to check once the running Cue has that.
#[test]
fn rerun_after_main_is_fixed_builds_main_as_it_is_now_and_lands() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let old = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    install_goes(&w, &fake, &id, "failed");
    assert_eq!(step(&w, &project), "failed");
    std::fs::write(dir.join("fix.txt"), "main fixed\n").unwrap();
    git(&dir, &["add", "-f", "fix.txt"]);
    git(&dir, &["commit", "-qm", "Fix main's build"]);
    let fixed = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(old, fixed);
    w.hub().desk_item_act(&id, "rerun", "").unwrap();
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    let started = fake.started();
    assert_eq!((started.len(), started.last().map(|s| s.1.as_str())), (2, Some(fixed.as_str())), "built from main as it is now: {started:?}");
    fake.goes("installed", "");
    fake.runs(&fixed);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{shown}");
}
