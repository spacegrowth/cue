//! A finishing item's install (desk/hub/landing/install.rs; split from desk_finish.rs): half an hour after the first DONE
//! waiting (or at once on Install now), never while a session that ends with Cue is mid-turn, built from the main
//! line's HEAD; once the running Cue has its commit it's to check. A failed build, or a new Cue that didn't start,
//! asks you and sends its holder the log.

use super::desk_finish::*;
use super::desk_commit::{commits, done_with, ends, item, tick};
use super::team::{git, says, writes};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use std::sync::Arc;

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
pub(super) fn mid_turn(w: &World) -> String {
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

/// Install now while Cue's tick is mid-run (a landing's check, a pull request read: #641): the install starts at
/// once, not when that tick ends, and only once.
#[test]
fn install_now_starts_at_once_while_cues_tick_is_mid_run() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    let tick_held = crate::desk::hub::landing::finish::Claim::take(w.hub(), "tick").expect("no tick runs");
    let it = w.hub().desk_item_act(&id, "install_now", "").unwrap();
    assert_eq!(it.finish.as_ref().map(|f| f.step.as_str()), Some("building"), "started while the tick ran");
    assert_eq!(fake.started().len(), 1);
    drop(tick_held);
    tick(&w);
    assert_eq!(fake.started().len(), 1, "the tick doesn't start it again");
    assert_eq!(step(&w, &project), "building");
}

/// An item waiting while an install runs that already has its commit (one a session began itself) goes with that
/// install: no second one is started.
#[test]
fn a_waiting_item_goes_with_a_running_install_that_has_its_commit() {
    let mut w = World::new();
    let (fake, dir, project, _) = done_in_cue(&w);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    fake.begun_elsewhere("desk-283", &head);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["step"].as_str(), shown["finish"]["tag"].as_str()), (Some("building"), Some("desk-283")), "{shown}");
    assert!(fake.started().is_empty(), "no second install");
    fake.goes("installed", "");
    tick(&w);
    assert_eq!(step(&w, &project), "starting");
    w.relaunch();
    fake.runs(&head);
    w.hub().desk_set_installer(fake.clone());
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert!(fake.started().is_empty(), "it went in the other install");
}

/// Install now while an install runs that doesn't have its commit: one at a time, its own starts the moment that
/// one ends.
#[test]
fn install_now_while_an_install_without_its_commit_runs_starts_the_moment_that_one_ends() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    fake.begun_elsewhere("desk-older", "0123456789abcdef");
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    assert_eq!(step(&w, &project), "queued", "that install doesn't have it");
    assert!(fake.started().is_empty(), "one at a time");
    // It says why it hasn't started (#641), rather than looking as it did before you clicked.
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["now"].as_bool(), shown["finish"]["wait"].as_str()), (Some(true), Some(crate::desk::hub::landing::install::AFTER_THIS_INSTALL)), "{shown}");
    fake.goes("installed", "");
    tick(&w);
    let started = fake.started();
    assert_eq!((started.len(), started.first().map(|s| s.1.as_str())), (1, Some(head.as_str())), "started the moment the other ended");
    assert_eq!(step(&w, &project), "building");
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["wait"], serde_json::Value::Null, "its wait said, cleared");
}

#[test]
fn a_failed_build_goes_to_the_lead_keeps_its_log_and_sends_the_holder_back_to_it() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    fake.goes("failed", "Compiling cue\nerror[E0425]: cannot find value `x`\n");
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["latest"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("The build failed"), Some("failed")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
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
fn a_new_cue_that_didnt_start_goes_to_the_lead_the_old_one_is_back() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    fake.goes("rolled_back", "launch timed out");
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["latest"].as_str()), (Some("finishing"), Some("The new Cue didn't start; the old one is back")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
}

#[test]
fn everything_waiting_goes_in_one_install_half_an_hour_after_the_first() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let h = item(&w, &project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string();
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(20 * 60_000);
    // To a new session: cue-teams keeps Header until its Looks right (one item per session, #576).
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: it made the footer");
    w.settle();
    let id2 = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    let h2 = item(&w, &project, "Footer jumps")["holder"]["sid"].as_str().unwrap().to_string();
    std::fs::write(dir.join("shared.txt"), "footer\n").unwrap();
    super::desk_commit::log_edit(&w, &h2, &dir.join("shared.txt"));
    ends(&w, &h2, &format!("Fixed.\nDONE: #{id2}"));
    let _ = h;
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
fn an_item_queued_after_another_waits_for_its_looks_right_not_its_commit() {
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
    assert_eq!(item(&w, &project, "Footer jumps")["state"], "queued", "not started at the commit: it waits for the first's Looks right (#576)");
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

/// `id`'s install, begun at once, gone `step` (failed or rolled_back).
pub(super) fn install_goes(w: &World, fake: &Fake, id: &str, step: &str) {
    w.hub().desk_item_act(id, "install_now", "").unwrap();
    w.settle();
    fake.goes(step, "it went wrong");
    tick(w);
}

#[test]
fn a_finishing_item_stopped_on_a_failed_install_goes_back_to_its_holder_with_a_why_and_needs_one() {
    let w = World::new();
    let (fake, _dir, project, id) = done_in_cue(&w);
    install_goes(&w, &fake, &id, "failed");
    let (h, d) = (holder(&w, &project), lead_of(&w, &project));
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams"));
    let told = sent_to(&w, &d).last().cloned().unwrap();
    assert!(told.starts_with(&format!("Refused: #{id} stopped while finishing (")) && told.contains("needs a why"), "{told}");
    assert_eq!(step(&w, &project), "failed", "left as it was");

    let n = sent_to(&w, &d).len();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: fix the build error"));
    assert_eq!(sent_to(&w, &d).len(), n, "a route made isn't answered");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["holder"]["sid"].as_str()), (Some("working"), Some(h.as_str())), "{shown}");
    assert!(shown["finish"].is_null(), "off its finish: {shown}");
    let sent = sent_to(&w, &h);
    assert!(sent.last().unwrap().contains("Why you: fix the build error"), "{sent:?}");
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 1, "out of the install line");
}

/// To new, or to the lead itself ("me", which only hands work out): off its finish, nobody holds it, and Cue starts
/// a session for it (#248).
#[test]
fn a_finishing_item_stopped_on_a_failed_install_can_go_to_a_new_session_or_me() {
    for to in ["new", "me"] {
        let w = World::new();
        let (fake, _dir, project, id) = done_in_cue(&w);
        install_goes(&w, &fake, &id, "failed");
        let (h, d) = (holder(&w, &project), lead_of(&w, &project));
        let n = sent_to(&w, &d).len();
        ends(&w, &d, &format!("ROUTE: #{id} -> {to} | why: fresh eyes on the build"));
        assert_eq!(sent_to(&w, &d).len(), n, "{to}: a route made isn't answered: {:?}", sent_to(&w, &d));
        let shown = item(&w, &project, "Header jumps");
        assert_eq!(shown["state"], "working", "{to}: {shown}");
        assert!(shown["finish"].is_null(), "{to}: {shown}");
        let s = shown["holder"]["sid"].as_str().unwrap_or_else(|| panic!("{to}: a new session holds it: {shown}")).to_string();
        assert!(s != h && s != d, "{to}: not its old holder, nor the lead: {shown}");
        assert!(sent_to(&w, &s).iter().any(|t| t.contains(&format!("#{id}")) && t.contains("fresh eyes on the build")), "{to}: {:?}", sent_to(&w, &s));
    }
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
fn rerun_on_a_failed_install_queues_it_again_with_no_holder_needed() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    assert!(w.hub().desk_item_act(&id, "rerun", "").is_err(), "not stopped on a question");
    install_goes(&w, &fake, &id, "rolled_back");
    let it = w.hub().desk_item_act(&id, "rerun", "").unwrap();
    w.settle();
    assert_eq!(it.finish.as_ref().map(|f| (f.step.as_str(), f.question.as_str())), Some(("queued", "")), "{it:?}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(fake.started().len(), 2, "its install begun again");
}

#[test]
fn an_install_that_refuses_to_start_goes_to_the_lead_with_why() {
    let w = World::new();
    let (fake, _, project, id) = done_in_cue(&w);
    *crate::locking::lock(&fake.refuse, "fake installer refuse") = "Another install is running".into();
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["log"].as_str()), (Some("finishing"), Some("failed"), Some("Another install is running")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    assert_eq!(shown["latest"], "The install didn't start: Another install is running", "not a failed build: {shown}");
    assert!(fake.started().is_empty());
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

/// #343: an item stopped on a failed install whose work a later install brought into the running Cue (main fixed and
/// installed by another item, or by hand) has nothing left to install: to check, never stuck on the old failure.
#[test]
fn a_failed_install_whose_work_a_later_install_has_goes_to_check() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    install_goes(&w, &fake, &id, "failed");
    tick(&w);
    assert_eq!(step(&w, &project), "failed", "stays stopped while the running Cue hasn't its work");
    fake.runs(&head);
    tick(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{shown}");
    assert_eq!(fake.started().len(), 1, "nothing built again");
}

/// #343: the lead's DONE on an item stopped on a failed install runs its landing again on main as it is now, and
/// closes it once the running Cue has that.
#[test]
fn the_leads_done_on_a_failed_install_runs_it_again_and_closes_it_once_installed() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    install_goes(&w, &fake, &id, "failed");
    std::fs::write(dir.join("fix.txt"), "main fixed\n").unwrap();
    git(&dir, &["add", "-f", "fix.txt"]);
    git(&dir, &["commit", "-qm", "Fix main's build"]);
    let fixed = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let lead = lead_of(&w, &project);
    ends(&w, &lead, &format!("Its work is on main.\nDONE: #{id}"));
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some("queued")), "{shown}");
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    assert_eq!(fake.started().last().map(|s| s.1.clone()), Some(fixed.clone()));
    fake.goes("installed", "");
    fake.runs(&fixed);
    tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
}

/// Skip install (#490): an item landed on main and waiting for its install is done at once, its holder told no
/// install is needed, and no install starts for it; the Desk's after-landing setting stays as it was.
#[test]
fn an_item_waiting_to_install_closes_with_done_skip_install_and_no_install_runs() {
    let w = World::new();
    let (fake, dir, project, id) = done_in_cue(&w);
    assert_eq!(step(&w, &project), "queued");
    let lands = super::desk::desk(&w, &project)["lands"].clone();
    let h = holder(&w, &project);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();

    w.hub().desk_item_act(&id, "skip_install", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "done", "{shown}");
    let told = sent_to(&w, &h);
    assert!(told.last().is_some_and(|t| t.contains(&format!("#{id}")) && t.contains("is done") && t.contains("none is needed")), "{told:?}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]).trim(), head, "its work stays on main as it is");
    let calls = super::desk::desk(&w, &project)["decisions"].as_array().cloned().unwrap_or_default();
    assert!(calls.iter().any(|d| d["text"] == "Skip install"), "its chat notes the button by its name (#621): {calls:?}");

    w.advance(31 * 60_000);
    tick(&w);
    assert!(fake.started().is_empty(), "no install for it: {:?}", fake.started());
    assert_eq!(super::desk::desk(&w, &project)["lands"], lands, "the after-landing setting is unchanged");
    assert!(w.hub().desk_item_act(&id, "skip_install", "").is_err(), "only while it waits for an install");
}

/// An item done in this Cue, its repository given a local bare "shared copy" holding main as it was before the item,
/// `set` run, then installed and in the running Cue: (repo, shared copy, project).
fn installed_with_shared_copy(w: &mut World, set: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, std::path::PathBuf, String) {
    let (fake, dir, project, id) = done_in_cue(w);
    let bare = dir.join("shared.git");
    git(&dir, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
    git(&dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(&dir, &["push", "-q", "origin", "HEAD~1:refs/heads/main"]);
    set(&*w, &project, &dir);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    fake.goes("installed", "");
    tick(w);
    w.relaunch();
    fake.runs(&head);
    w.hub().desk_set_installer(fake.clone());
    tick(w);
    w.settle();
    assert_eq!(step(w, &project), "checked", "installed");
    (dir, bare, project)
}

fn same_main(dir: &std::path::Path, bare: &std::path::Path) -> bool {
    git(bare, &["rev-parse", "main"]).trim() == git(dir, &["rev-parse", "main"]).trim()
}

/// After an install, main ahead of the shared copy is offered for a push (#599): nothing pushed before your yes.
#[test]
fn after_an_install_main_ahead_of_the_shared_copy_is_offered_and_a_yes_pushes_it() {
    let mut w = World::new();
    let (dir, bare, project) = installed_with_shared_copy(&mut w, |_, _, _| {});
    let card = super::desk_remote::push_card(&w, &project).expect("offered");
    assert!(card.to_string().contains("Main is 1 landing ahead of the shared copy. Push it?"), "{card}");
    assert!(!same_main(&dir, &bare), "nothing pushed before your yes");
    super::desk_remote::answer(&w, card["id"].as_str().unwrap(), "Yes");
    assert!(same_main(&dir, &bare), "pushed");
    assert!(super::desk_remote::push_card(&w, &project).is_none());
}

/// A yes still goes through the check before a push: a word that must not go public refuses it, naming the find.
#[test]
fn a_yes_to_the_offer_is_refused_when_the_leak_check_finds_a_word() {
    let mut w = World::new();
    let (dir, bare, project) = installed_with_shared_copy(&mut w, |_, _, _| {});
    let card = super::desk_remote::push_card(&w, &project).expect("offered");
    w.hub().set_setting("project.never", serde_json::json!({ project.clone(): ["zorblax"] })).unwrap();
    std::fs::write(dir.join("shared.txt"), "the zorblax plan\n").unwrap();
    git(&dir, &["add", "-f", "shared.txt"]);
    git(&dir, &["commit", "-q", "-m", "plan"]);
    super::desk_remote::answer(&w, card["id"].as_str().unwrap(), "Yes");
    assert!(!same_main(&dir, &bare), "not pushed");
    let said: Vec<String> = w.hub().read(|st| st.desk.decisions.iter().map(|d| d.text.clone()).collect());
    assert!(said.iter().any(|t| t.starts_with("Not pushed") && t.contains("zorblax") && t.contains("shared.txt line 1")), "{said:?}");
}

#[test]
fn after_an_install_with_main_even_with_the_shared_copy_nothing_is_offered() {
    let mut w = World::new();
    let (dir, bare, project) = installed_with_shared_copy(&mut w, |_, _, dir| {
        git(dir, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    });
    assert!(same_main(&dir, &bare));
    assert!(super::desk_remote::push_card(&w, &project).is_none());
}

#[test]
fn a_project_set_to_never_push_is_offered_nothing_after_an_install() {
    let mut w = World::new();
    let (dir, bare, project) = installed_with_shared_copy(&mut w, |w, project, _| {
        w.hub().set_setting("project.lands", serde_json::json!({ project: { "push": "never" } })).unwrap();
    });
    assert!(super::desk_remote::push_card(&w, &project).is_none());
    assert!(!same_main(&dir, &bare), "nothing pushed");
}

/// "After each landing": main ahead is pushed after the install (checked), nothing asked.
#[test]
fn a_project_pushed_after_each_landing_is_pushed_after_an_install_without_asking() {
    let mut w = World::new();
    let (dir, bare, project) = installed_with_shared_copy(&mut w, |w, project, _| {
        w.hub().set_setting("project.lands", serde_json::json!({ project: { "push": "after" } })).unwrap();
    });
    w.settle();
    assert!(super::desk_remote::push_card(&w, &project).is_none(), "nothing asked");
    assert!(same_main(&dir, &bare), "pushed");
}

/// A shared copy that doesn't answer isn't taken as even: main is still offered, saying Cue couldn't compare.
#[test]
fn a_shared_copy_that_cant_be_reached_still_gets_the_offer_after_an_install() {
    let mut w = World::new();
    let (_, _, project) = installed_with_shared_copy(&mut w, |_, _, dir| {
        let gone = dir.join("no-such-remote.git");
        git(dir, &["remote", "set-url", "origin", &gone.to_string_lossy()]);
    });
    let card = super::desk_remote::push_card(&w, &project).expect("offered although it couldn't compare");
    assert!(card.to_string().contains("couldn't compare main with the shared copy"), "{card}");
}
