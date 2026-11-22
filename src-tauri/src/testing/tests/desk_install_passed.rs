//! The install takes the last commit a full run passed (desk/hub/landing/install_pick.rs): items at or before that
//! commit go into the install, one landed after it stays queued and says it waits for the full run; a run that failed
//! on an item holds the install for four hours at most, one that failed on no item doesn't hold it; with no run finished two hours after main moved the tip is taken and the Desk's
//! lead is told; and nothing is installed when the commit that passed is the one running. The installs and the runs
//! here are fakes'.

use super::desk_commit::{item, tick};
use super::desk_finish::Fake;
use super::desk_full_run::{failed, in_cue_passed, lands, looks, one_more, passed, Runner};
use super::desk_spec_copies::{lead_of, sent_to};
use crate::desk::hub::landing::install_pick::FULL_RUN_WAIT;
use crate::desk::{InstallStatus, Installer};
use crate::testing::World;
use std::path::PathBuf;
use std::sync::Arc;

/// The fake installer, with git's own word on which commit has which (the fake's is "the same one").
struct ByGit(Arc<Fake>);

impl Installer for ByGit {
    fn has(&self, repo: &str, commit: &str) -> bool {
        let running = crate::locking::lock(&self.0.has, "fake installer has").clone();
        running.iter().any(|r| crate::install::self_install::is_ancestor(repo, commit, r))
    }

    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String> {
        self.0.start(repo, commit, tag)
    }

    fn status(&self) -> Option<InstallStatus> {
        self.0.status()
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::install::self_install::is_ancestor(repo, commit, of)
    }
}

/// What a test here starts from.
struct Start {
    runner: Arc<Runner>,
    fake: Arc<Fake>,
    dir: PathBuf,
    project: String,
    /// Main with "Header jumps", then "Footer", then "Rail" landed on it.
    header: String,
    footer: String,
    rail: String,
}

/// Cue's own project, three items landed one after another and queued for the install: the full run passed main as
/// it stood with "Footer"; the run of "Rail", landed after it, is still going.
fn footer_passed(w: &World) -> Start {
    let (runner, dir, project, id, h, _) = in_cue_passed(w);
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(Arc::new(ByGit(fake.clone())));
    let header = lands(w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let (_, _, footer) = one_more(w, &dir, &project, "Footer", "b.txt");
    runner.ends_with(passed());
    looks(w);
    runner.ends_with(passed());
    looks(w);
    let (_, _, rail) = one_more(w, &dir, &project, "Rail", "shared.txt");
    let asks = runner.asks();
    assert_eq!(asks.last().map(|a| (a.at.as_str(), a.since.as_str())), Some((rail.as_str(), footer.as_str())), "the run of Rail is going: {asks:?}");
    for title in ["Header jumps", "Footer", "Rail"] {
        assert_eq!(at(w, &project, title).0, "queued", "{}", item(w, &project, title));
    }
    Start { runner, fake, dir, project, header, footer, rail }
}

/// Item `title`'s finishing step, what it waits for and its commit.
fn at(w: &World, project: &str, title: &str) -> (String, String, String) {
    let it = item(w, project, title);
    let s = |k: &str| it["finish"][k].as_str().unwrap_or("").to_string();
    (s("step"), s("wait"), s("commit"))
}

/// The commits installs were started at.
fn built(fake: &Fake) -> Vec<String> {
    fake.started().into_iter().map(|(_, commit, _)| commit).collect()
}

/// How many times the Desk's lead was told of an install without a full run (its feed's rows, and what reached it).
fn told_unrun(w: &World, project: &str) -> usize {
    let rows = w.hub().read(|st| st.desk.feed.get(project).map(|f| f.rows.iter().map(|r| r.words.clone()).collect::<Vec<_>>()).unwrap_or_default());
    let rows = rows.iter().filter(|t| t.contains("without a full run")).count();
    rows.max(sent_to(w, &lead_of(w, project)).iter().filter(|t| t.contains("without a full run")).count())
}

/// The install under way ends, and the running Cue has `commit`.
fn installed(w: &World, fake: &Fake, commit: &str) {
    fake.goes("installed", "");
    fake.runs(commit);
    tick(w);
}

#[test]
fn the_install_builds_the_commit_that_passed_and_an_item_landed_after_it_waits_for_its_run() {
    let w = World::new();
    let s = footer_passed(&w);
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone()], "the last commit that passed, not the tip {}", s.rail);
    assert_eq!(at(&w, &s.project, "Header jumps"), ("building".into(), String::new(), s.footer.clone()), "before it: in the install");
    assert_eq!(at(&w, &s.project, "Footer"), ("building".into(), String::new(), s.footer.clone()), "at it: in the install");
    assert_eq!(at(&w, &s.project, "Rail"), ("queued".into(), FULL_RUN_WAIT.into(), s.rail.clone()), "after it: it waits and says why");
    assert_eq!(FULL_RUN_WAIT, "waiting for the full run");
    let why = w.hub().read(|st| crate::desk::hub::landing::finish::under_way(st, item(&w, &s.project, "Rail")["id"].as_str().unwrap()));
    assert_eq!(why.as_deref(), Some("is committed and waiting for the full run"));

    installed(&w, &s.fake, &s.footer);
    assert_eq!(item(&w, &s.project, "Header jumps")["state"], "to_check");
    assert_eq!(item(&w, &s.project, "Footer")["state"], "to_check");
    tick(&w);
    assert_eq!(at(&w, &s.project, "Rail").0, "queued", "its run hasn't ended");
    assert_eq!(built(&s.fake).len(), 1, "what passed is running: nothing more to install");

    // Its run passes: the next install takes it.
    s.runner.ends_with(passed());
    looks(&w);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone(), s.rail.clone()]);
    assert_eq!(at(&w, &s.project, "Rail"), ("building".into(), String::new(), s.rail.clone()));
    installed(&w, &s.fake, &s.rail);
    assert_eq!(item(&w, &s.project, "Rail")["state"], "to_check");
    assert_eq!(told_unrun(&w, &s.project), 0, "every install had a full run behind it");
    let _ = &s.header;
}

/// How many lines of the Desk's feed say `what`.
fn said(w: &World, project: &str, what: &str) -> usize {
    w.hub().read(|st| st.desk.feed.get(project).map(|f| f.rows.iter().filter(|r| r.words.contains(what)).count()).unwrap_or(0))
}

#[test]
fn a_run_that_failed_on_no_item_doesnt_hold_the_install() {
    let w = World::new();
    let s = footer_passed(&w);
    s.runner.ends_with(failed(None, Some("culprit unknown: it already fails at --since, before this batch")));
    looks(&w);
    assert_eq!(said(&w, &s.project, "on tests no item in this batch caused: hub::tests::header_stays, tests/rail.test.js"), 1);
    assert_eq!(said(&w, &s.project, "No item was reopened and the app still updates."), 1);
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.rail.clone()], "the tip: its failure is no item's");
    assert_eq!(at(&w, &s.project, "Rail"), ("building".into(), String::new(), s.rail.clone()));
    assert_eq!(told_unrun(&w, &s.project), 0);
    // What truly passed is kept apart: the next run still searches from there on.
    assert_eq!(crate::desk::hub::landing::full_run::last_passed(&s.runner.asks()[0].repo), Some(s.footer.clone()));
}

#[test]
fn a_run_that_failed_on_an_item_holds_the_install_for_four_hours_and_no_longer() {
    let w = World::new();
    let s = footer_passed(&w);
    let rail = item(&w, &s.project, "Rail")["id"].as_str().unwrap().to_string();
    let culprit = || Some(crate::desk::hub::landing::full_run_kit::Culprit { commit: s.rail.clone(), subject: format!("Rail (#{rail})") });
    // Notch lands after Rail, whose run then fails on Rail's own commit; so does the run of Notch after it.
    let (_, _, notch) = one_more(&w, &s.dir, &s.project, "Notch", "by_builder.txt");
    s.runner.ends_with(failed(culprit(), None));
    looks(&w);
    assert_eq!(item(&w, &s.project, "Rail")["state"], "working", "open again with its holder");
    assert_eq!(said(&w, &s.project, "The app stays on the last version that passed, for 4 hours at most."), 1);
    w.advance(30 * 60_000);
    s.runner.ends_with(failed(culprit(), None));
    looks(&w);
    assert_eq!(said(&w, &s.project, &format!("#{rail} was already sent back for that commit")), 1);
    w.advance(3 * 60 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone()], "held on what passed, three and a half hours on");
    assert_eq!(at(&w, &s.project, "Notch"), ("queued".into(), FULL_RUN_WAIT.into(), notch.clone()));
    installed(&w, &s.fake, &s.footer);
    assert_eq!((told_unrun(&w, &s.project), said(&w, &s.project, "although the full run still fails")), (0, 0));
    // Four hours from the first failure: the tip, and one line says so, naming the item.
    w.advance(31 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone(), notch.clone()], "{}", item(&w, &s.project, "Notch"));
    assert_eq!(at(&w, &s.project, "Notch").0, "building");
    assert_eq!(said(&w, &s.project, &format!("although the full run still fails: #{rail} broke it")), 1);
    tick(&w);
    assert_eq!((built(&s.fake).len(), said(&w, &s.project, "although the full run still fails")), (2, 1), "said once");
}

#[test]
fn with_no_run_finished_two_hours_after_main_moved_the_install_takes_the_tip_and_says_so() {
    let w = World::new();
    let s = footer_passed(&w);
    // The run of Rail never ends.
    w.advance(119 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone()], "not two hours yet: the commit that passed");
    installed(&w, &s.fake, &s.footer);
    assert_eq!(at(&w, &s.project, "Rail"), ("queued".into(), FULL_RUN_WAIT.into(), s.rail.clone()));
    assert_eq!(told_unrun(&w, &s.project), 0);
    w.advance(2 * 60_000);
    tick(&w);
    assert_eq!(built(&s.fake), vec![s.footer.clone(), s.rail.clone()], "the tip, as before there were full runs");
    assert_eq!(at(&w, &s.project, "Rail"), ("building".into(), String::new(), s.rail.clone()));
    assert_eq!(told_unrun(&w, &s.project), 1, "one line for the Desk's lead");
    tick(&w);
    assert_eq!((built(&s.fake).len(), told_unrun(&w, &s.project)), (2, 1), "said once");
}

#[test]
fn nothing_is_installed_when_the_commit_that_passed_is_the_one_running() {
    let w = World::new();
    let s = footer_passed(&w);
    s.fake.runs(&s.footer);
    tick(&w);
    assert_eq!(item(&w, &s.project, "Header jumps")["state"], "to_check", "the running Cue has it");
    assert_eq!(item(&w, &s.project, "Footer")["state"], "to_check");
    w.advance(31 * 60_000);
    tick(&w);
    assert!(built(&s.fake).is_empty(), "{:?}", built(&s.fake));
    assert_eq!(at(&w, &s.project, "Rail"), ("queued".into(), FULL_RUN_WAIT.into(), s.rail.clone()));
}
