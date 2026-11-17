//! Done means it's in your app (hub/desk_finish.rs): an item that changed the Cue this app is built from, once its
//! commit is made, is in the next install: half an hour after the first DONE waiting (or at once on Install now), never
//! while a session that ends with Cue is mid-turn, built from the main line's HEAD. Once the running Cue has its commit
//! it's to check; a failed build, or a new Cue that didn't start, asks you and sends its holder the log. A crew's work
//! on a branch of its own is merged into the main checkout's branch first, never forced.

use super::desk_commit::{commits, done_with, ends, item, tick};
use super::team::{git, runs, says, starts, writes};
use crate::desk::{InstallStatus, Installer};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use serde_json::json;
use std::sync::{Arc, Mutex};

/// An installer that installs nothing: what it was asked to start, how its install goes, and the commits the "running
/// Cue" has, as each test sets them.
#[derive(Default)]
struct Fake {
    starts: Mutex<Vec<(String, String, String)>>,
    status: Mutex<Option<InstallStatus>>,
    has: Mutex<Vec<String>>,
    refuse: Mutex<String>,
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
}

impl Fake {
    fn started(&self) -> Vec<(String, String, String)> {
        crate::locking::lock(&self.starts, "fake installer starts").clone()
    }

    /// Its latest install now at `step`, with `log`.
    fn goes(&self, step: &str, log: &str) {
        let mut s = crate::locking::lock(&self.status, "fake installer status");
        if let Some(s) = s.as_mut() {
            (s.step, s.log) = (step.into(), log.into());
        }
    }

    /// The running Cue has `commit` now.
    fn runs(&self, commit: &str) {
        crate::locking::lock(&self.has, "fake installer has").push(commit.into());
    }
}

/// `dir` made to hold this Cue's own source, as builds_this_cue reads it.
fn as_this_cue(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
    std::fs::write(dir.join("src-tauri/tauri.conf.json"), "{ \"identifier\": \"dev.spacegrowth.cue\" }\n").unwrap();
}

/// An item done in this Cue's source (its crew wrote a.txt and b.txt), with a fake installer: (fake, repo, project, id).
fn done_in_cue(w: &World) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    (fake, dir, project, id)
}

fn step(w: &World, project: &str) -> String {
    item(w, project, "Header jumps")["finish"]["step"].as_str().unwrap_or("").to_string()
}

#[test]
fn an_item_in_this_cue_is_committed_in_the_next_install_built_from_head_after_half_an_hour_and_to_check_once_cue_has_it() {
    let mut w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let shown = item(&w, &project, "Header jumps");
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("finishing"), Some("queued"), Some(head.as_str())), "{shown}");
    assert_eq!(commits(&dir), "2", "committed at DONE");
    let due = w.snapshot().0["desk"]["install_at"].as_u64().unwrap();
    assert!(due > 0, "the Desk says when the next install is");
    tick(&w);
    w.advance(29 * 60_000);
    assert!(fake.started().is_empty(), "the next install is half an hour after the first item waiting");
    w.advance(60_000);
    let started = fake.started();
    assert_eq!(started.len(), 1, "{started:?}");
    let canon = |p: &str| std::fs::canonicalize(p).unwrap();
    assert_eq!((canon(&started[0].0), started[0].1.as_str()), (canon(&dir.to_string_lossy()), head.as_str()), "built from the main line's HEAD");
    assert_eq!(step(&w, &project), "building");

    fake.goes("installing", "");
    tick(&w);
    assert_eq!(step(&w, &project), "installing");
    fake.goes("installed", "");
    tick(&w);
    assert_eq!(step(&w, &project), "starting", "installed; this Cue doesn't have it yet");

    // Cue restarts on the install: the new one has the commit.
    w.relaunch();
    fake.runs(&head);
    w.hub().desk_set_installer(fake.clone());
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{shown}");
    assert_eq!(fake.started().len(), 1, "one install");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(commits(&dir), "2", "Looks right only closes it");
}

/// Session "tidier", at work now (mid-turn).
fn mid_turn(w: &World) -> String {
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy", "tidier", &opts).unwrap();
    let o = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&o, HostedEvent::Taken { text: "tidy".into() });
    w.settle();
    o
}

#[test]
fn a_session_mid_turn_holds_the_install_back_and_install_now_starts_it_anyway() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    let o = mid_turn(&w);
    w.advance(31 * 60_000);
    tick(&w);
    assert!(fake.started().is_empty(), "not while tidier is mid-turn");
    assert_eq!(step(&w, &project), "queued");
    let it = w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    assert_eq!(fake.started().len(), 1);
    assert_eq!(it.finish.as_ref().map(|f| f.step.as_str()), Some("building"));
    assert!(w.hub().desk_item_act(&id, "install_now", "").is_err(), "not queued now");
    // Its turn ending would have let it start too; nothing starts twice.
    ends(&w, &o, "Tidied.");
    tick(&w);
    assert_eq!(fake.started().len(), 1);
}

#[test]
fn install_now_skips_the_half_hour_wait() {
    let w = World::new();
    let (fake, _, _, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    assert_eq!(fake.started().len(), 1);
}

#[test]
fn a_failed_build_asks_you_keeps_its_log_and_sends_the_holder_back_to_it() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    fake.goes("failed", "Compiling cue\nerror[E0425]: cannot find value `x`\n");
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["latest"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("The build failed"), Some("failed")), "{shown}");
    assert!(shown["finish"]["log"].as_str().unwrap().contains("E0425"), "{shown}");
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    let sent: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == h).map(|(_, t)| t).collect();
    assert!(sent.iter().any(|t| t.contains("isn't done: The build failed") && t.contains("E0425") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");

    // Its fix and DONE: committed, and queued again.
    std::fs::write(w.home().join("a.txt"), "fixed\n").unwrap();
    ends(&w, &h, &format!("Fixed the build.\nDONE: #{id}"));
    assert_eq!(step(&w, &project), "queued");
}

#[test]
fn a_new_cue_that_didnt_start_asks_you_the_old_one_is_back() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    fake.goes("rolled_back", "launch timed out");
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["latest"].as_str()), (Some("needs_you"), Some("The new Cue didn't start; the old one is back")), "{shown}");
}

#[test]
fn everything_waiting_goes_in_one_install_half_an_hour_after_the_first() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let h = item(&w, &project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string();
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(20 * 60_000);
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the footer");
    w.settle();
    let id2 = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    std::fs::write(dir.join("shared.txt"), "footer\n").unwrap();
    super::desk_commit::log_edit(&w, &h, &dir.join("shared.txt"));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id2}"));
    assert_eq!(item(&w, &project, "Footer jumps")["finish"]["step"], "queued");
    let due = w.snapshot().0["desk"]["install_at"].as_u64().unwrap();
    w.advance(9 * 60_000);
    assert!(fake.started().is_empty(), "half an hour after the first, not the latest");
    w.advance(60_000);
    tick(&w);
    assert!(w.snapshot().0["desk"]["install_at"].as_u64() == Some(0) && due > 0, "nothing waits now");
    let started = fake.started();
    assert_eq!(started.len(), 1, "{started:?}");
    assert_eq!(started[0].1, git(&dir, &["rev-parse", "HEAD"]).trim(), "the latest HEAD");
    assert_eq!((step(&w, &project), item(&w, &project, "Footer jumps")["finish"]["step"].as_str().unwrap().to_string()), ("building".into(), "building".into()));
    let _ = id;
}

#[test]
fn an_item_queued_after_another_starts_once_its_commit_is_made() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let queued = std::cell::RefCell::new(String::new());
    let (_, project, _) = done_with(&w, |_, _, dir| as_this_cue(dir), |w, h, dir| {
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        let lead = super::desk::desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
        let first = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
        ends(w, &lead, &format!("ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> cue-teams | after: #{first} | why: same file"));
        w.settle();
        *queued.borrow_mut() = item(w, &project, "Footer jumps")["state"].as_str().unwrap_or("").to_string();
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt")]));
    });
    assert_eq!(queued.into_inner(), "queued", "waits on the first");
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["step"], "queued", "the first waits for its install");
    assert_eq!(item(&w, &project, "Footer jumps")["state"], "working", "started at the commit, not the install");
}

#[test]
fn a_finishing_item_survives_a_restart_where_it_was() {
    let mut w = World::new();
    let (fake, _, project, _) = done_in_cue(&w);
    w.relaunch();
    w.hub().desk_set_installer(fake.clone());
    assert_eq!(step(&w, &project), "queued");
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 1, "queued across the restart");
}

/// The World's repository with a crew teammate in a worktree of its own (branch worktree-agent-agentW), which writes
/// `file` there: the lead, the worktree.
fn crew_in_worktree(w: &World, h: &str, dir: &std::path::Path, file: &str) -> std::path::PathBuf {
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
    assert_eq!(git(&dir, &["branch", "--show-current"]).trim(), "team-work");
    assert!(git(&dir, &["log", "-1", "--format=%s"]).trim().starts_with("Merge worktree-agent-agentW"), "{}", git(&dir, &["log", "-3", "--format=%s"]));
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "builder's\n", "in the main checkout");
    assert!(git(&dir, &["remote"]).trim().is_empty(), "never pushed");
}

#[test]
fn a_merge_that_conflicts_asks_you_and_leaves_the_main_checkout_as_it_was() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        std::fs::write(dir.join("a.txt"), "yours\n").unwrap();
        git(dir, &["add", "a.txt"]);
        git(dir, &["commit", "-q", "-m", "yours"]);
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("conflict")), "{shown}");
    assert!(shown["latest"].as_str().unwrap().contains("a.txt"), "{shown}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), "yours", "nothing merged");
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty(), "nothing left half-merged");
}

/// The holder of the item "Header jumps".
fn holder(w: &World, project: &str) -> String {
    item(w, project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string()
}

/// `id`'s install, begun at once, gone `step` (failed or rolled_back).
fn install_goes(w: &World, fake: &Fake, id: &str, step: &str) {
    w.hub().desk_item_act(id, "install_now", "").unwrap();
    w.settle();
    fake.goes(step, "it went wrong");
    tick(w);
}

#[test]
fn a_done_with_nothing_new_after_a_failed_install_installs_the_earlier_commit() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    install_goes(&w, &fake, &id, "failed");
    let h = holder(&w, &project);
    ends(&w, &h, &format!("It was a flaky build; nothing to change.\nDONE: #{id}"));
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("queued"), Some(head.as_str())), "not to check uninstalled: {shown}");
    assert_eq!(commits(&dir), "2", "nothing new committed");
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    assert_eq!(fake.started().len(), 2, "installed again");
}

#[test]
fn try_again_on_a_failed_install_queues_it_again_with_no_holder_needed() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    assert!(w.hub().desk_item_act(&id, "retry_finish", "").is_err(), "not stopped on a question");
    install_goes(&w, &fake, &id, "rolled_back");
    let it = w.hub().desk_item_act(&id, "retry_finish", "").unwrap();
    w.settle();
    assert_eq!(it.finish.as_ref().map(|f| (f.step.as_str(), f.question.as_str())), Some(("queued", "")), "{it:?}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 2, "its install begun again");
}

#[test]
fn try_again_on_a_word_you_took_out_commits_it() {
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
    w.hub().desk_item_act(&id, "retry_finish", "").unwrap();
    w.settle();
    assert_eq!((step(&w, &project), commits(&dir)), ("queued".to_string(), "2".to_string()));
}

#[test]
fn an_install_that_refuses_to_start_asks_you_with_why() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    *crate::locking::lock(&fake.refuse, "fake installer refuse") = "Another install is running".into();
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["log"].as_str()), (Some("needs_you"), Some("failed"), Some("Another install is running")), "{shown}");
    assert_eq!(shown["latest"], "The install didn't start: Another install is running", "not a failed build: {shown}");
    assert!(fake.started().is_empty());
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
fn a_restart_while_building_follows_the_install_or_queues_it_again_when_its_lost() {
    let mut w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.relaunch();
    w.hub().desk_set_installer(fake.clone());
    tick(&w);
    assert_eq!(step(&w, &project), "building", "the installer still has it");
    fake.goes("installed", "");
    tick(&w);
    assert_eq!(step(&w, &project), "starting");

    // The installer lost it: queued again, and begun again once due.
    *crate::locking::lock(&fake.status, "fake installer status") = None;
    w.relaunch();
    w.hub().desk_set_installer(fake.clone());
    tick(&w);
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 2, "{:?}", fake.started());
    assert_eq!(step(&w, &project), "building");
    fake.runs(git(&dir, &["rev-parse", "HEAD"]).trim());
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
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
        assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("word")), "{shown}");
        assert!(shown["latest"].as_str().unwrap().contains(at), "{shown}");
        assert!(!git(&dir, &["log", "-1", "--format=%s"]).starts_with("Merge"), "nothing merged");
    }
}

#[test]
fn a_main_checkout_mid_merge_is_left_alone_and_asks_you() {
    let w = World::new();
    let (dir, project, _) = done_with(&w, |_, _, _| {}, |w, h, dir| {
        crew_in_worktree(w, h, dir, "a.txt");
        // Your own merge, stopped on a conflict in shared.txt.
        git(dir, &["switch", "-q", "-c", "yours"]);
        std::fs::write(dir.join("shared.txt"), "yours\n").unwrap();
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "-m", "yours"]);
        git(dir, &["switch", "-q", "team-work"]);
        std::fs::write(dir.join("shared.txt"), "mine\n").unwrap();
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "-m", "mine"]);
        let o = std::process::Command::new("git").arg("-C").arg(dir).args(["merge", "-q", "yours"]).output().unwrap();
        assert!(!o.status.success(), "a conflict");
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("conflict")), "{shown}");
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
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("conflict")), "{shown}");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "yours, not committed\n");
    assert!(!dir.join(".git/MERGE_HEAD").exists());
    assert!(!git(&dir, &["log", "-1", "--format=%s"]).starts_with("Merge"), "nothing merged");
}

/// An item done in this Cue's source, the running Cue built from its commit before the item's work, which writes
/// `files` (paths in the repo): (fake, repo, project, id).
fn done_since_running(w: &World, files: &'static [&'static str]) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = done_with(
        w,
        |_, _, dir| {
            as_this_cue(dir);
            // The test repo ignores everything but a few files: let in the window's, the tests' and the app's.
            let mut ignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
            ignore.push_str("!ui/\n!ui/**\n!tests/\n!tests/**\n!src-tauri/\n!src-tauri/desk.rs\n");
            std::fs::write(dir.join(".gitignore"), ignore).unwrap();
            std::fs::create_dir_all(dir.join("ui")).unwrap();
            std::fs::write(dir.join("ui/index.html"), "<p>built in</p>\n").unwrap();
            git(dir, &["add", "-A"]);
            git(dir, &["commit", "-q", "-m", "the Cue that runs"]);
            let running = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
            let install = crate::server::cue_dir().join("install");
            std::fs::create_dir_all(&install).unwrap();
            std::fs::write(install.join("running.json"), json!({ "commit": running }).to_string()).unwrap();
        },
        |w, h, dir| {
            let paths: Vec<String> = files.iter().map(|f| dir.join(f).to_string_lossy().to_string()).collect();
            for p in &paths {
                std::fs::create_dir_all(std::path::Path::new(p).parent().unwrap()).unwrap();
            }
            says(w, h, writes("toolu_A", "agentA", "fixer", &paths));
        },
    );
    (fake, dir, project, id)
}

#[test]
fn an_item_that_changed_only_the_windows_files_is_served_in_place_and_to_check_with_no_install() {
    let w = World::new();
    let (fake, dir, project, _) = done_since_running(&w, &["ui/desk.js", "tests/ui/specs/desk.spec.mjs"]);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("to_check"), Some("checked"), Some(head.as_str())), "{shown}");
    assert_eq!(w.window.heard("ui-reload"), vec![json!({ "commit": head })], "the window reloads");
    let served = crate::server::cue_dir().join("live").join(&head);
    assert_eq!(std::fs::read_to_string(served.join("desk.js")).unwrap(), "fixer wrote this\n", "from the item's commit");
    assert_eq!(std::fs::read_to_string(served.join("index.html")).unwrap(), "<p>built in</p>\n");
    w.advance(3 * 60_000);
    assert!(fake.started().is_empty(), "no build, no restart");
}

#[test]
fn an_item_that_changed_the_app_side_too_still_waits_for_an_install() {
    let w = World::new();
    let (fake, _, project, _) = done_since_running(&w, &["ui/desk.js", "src-tauri/desk.rs"]);
    assert_eq!(step(&w, &project), "queued");
    assert!(w.window.heard("ui-reload").is_empty());
    w.advance(30 * 60_000);
    assert_eq!(fake.started().len(), 1, "installed");
    assert!(!crate::server::cue_dir().join("live").exists(), "nothing served in place");
}
