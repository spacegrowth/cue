//! All the tests run once, after landing (desk/hub/landing/full_run.rs): on Cue's own project a landing starts one
//! full run at main's head, one at a time; its end is a line for the Desk's lead, a failure traced to an item's commit
//! opens that item again, and what's kept survives a relaunch. The runs here are a fake's: never the real script.

use super::desk_commit::{ends, item, routed_with};
use super::desk_finish::{as_this_cue, Fake};
use super::desk_spec_copies::{another, copy_of, head, lead_of, sent_to};
use crate::desk::hub::landing::full_run::last_passed;
use crate::desk::hub::landing::full_run_kit::{set_fake, Culprit, FullRunAsk, FullRunEnd, FullRunResult, FullRunner};
use crate::testing::World;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A runner that runs nothing: what it was asked, and the end the test gives its run.
#[derive(Default)]
pub(super) struct Runner {
    asks: Mutex<Vec<FullRunAsk>>,
    going: Mutex<Vec<String>>,
    ends: Mutex<Vec<(String, FullRunEnd)>>,
    /// Why a run on another machine can't start ("": it can).
    refuse: Mutex<String>,
    stopped: Mutex<Vec<String>>,
}

impl FullRunner for Runner {
    fn start(&self, ask: &FullRunAsk) -> Result<(), String> {
        let refuse = crate::locking::lock(&self.refuse, "fake runner refuse").clone();
        if !ask.machine.is_empty() && !refuse.is_empty() {
            return Err(refuse);
        }
        let mut going = crate::locking::lock(&self.going, "fake runner going");
        if going.contains(&ask.repo) {
            return Err("one is going".into());
        }
        going.push(ask.repo.clone());
        drop(going);
        crate::locking::lock(&self.asks, "fake runner asks").push(ask.clone());
        Ok(())
    }

    fn going(&self, repo: &str) -> bool {
        let going = crate::locking::lock(&self.going, "fake runner going");
        going.iter().any(|r| r == repo)
    }

    fn take(&self, repo: &str) -> Option<FullRunEnd> {
        let mut ends = crate::locking::lock(&self.ends, "fake runner ends");
        let at = ends.iter().position(|(r, _)| r == repo)?;
        let (_, end) = ends.remove(at);
        drop(ends);
        crate::locking::lock(&self.going, "fake runner going").retain(|r| r != repo);
        Some(end)
    }

    fn stop(&self, repo: &str) {
        crate::locking::lock(&self.going, "fake runner going").retain(|r| r != repo);
        crate::locking::lock(&self.stopped, "fake runner stopped").push(repo.to_string());
    }
}

impl Runner {
    pub(super) fn asks(&self) -> Vec<FullRunAsk> {
        crate::locking::lock(&self.asks, "fake runner asks").clone()
    }

    /// The run going ends with `end`.
    pub(super) fn ends_with(&self, end: FullRunEnd) {
        let repo = crate::locking::lock(&self.going, "fake runner going").first().cloned().expect("no run is going");
        crate::locking::lock(&self.ends, "fake runner ends").push((repo, end));
    }

    /// Cue quit: the new one knows of no run (the script it left may still be going: asked for the same commits
    /// again it makes no second run, tests/release/full_run.test.sh).
    fn quit(&self) {
        crate::locking::lock(&self.going, "fake runner going").clear();
        crate::locking::lock(&self.ends, "fake runner ends").clear();
    }
}

pub(super) fn passed() -> FullRunEnd {
    FullRunEnd::Result(FullRunResult { passed: true, ..Default::default() })
}

pub(super) fn failed(culprit: Option<Culprit>, note: Option<&str>) -> FullRunEnd {
    FullRunEnd::Result(FullRunResult { passed: false, failed: vec!["hub::tests::header_stays".into(), "tests/rail.test.js".into()], culprit, culprit_note: note.map(str::to_string), tail: Some("assertion failed: header moved".into()), ..Default::default() })
}

/// Cue's own project with the item "Header jumps" routed, a fake runner and a fake installer:
/// (runner, repo, project, the item's id, its holder).
fn in_cue(w: &World) -> (Arc<Runner>, PathBuf, String, String, String) {
    w.hub().desk_set_installer(Arc::new(Fake::default()));
    let (dir, project, id, h) = routed_with(w, |_, _, dir| as_this_cue(dir), |_, _, _| {});
    // Given once the item is routed: till then this World makes no full run (main as it stood isn't run here).
    let runner = Arc::new(Runner::default());
    set_fake(runner.clone());
    (runner, dir, project, id, h)
}

/// The same, main as it stood having passed its first full run (asked for at its head, nothing having passed
/// before): the runner's asks start empty again, and that commit is the last of the tuple.
pub(super) fn in_cue_passed(w: &World) -> (Arc<Runner>, PathBuf, String, String, String, String) {
    let (runner, dir, project, id, h) = in_cue(w);
    looks(w);
    let (asks, start) = (runner.asks(), head(&dir));
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert_eq!((asks[0].at.as_str(), asks[0].since.as_str(), asks[0].machine.as_str()), (start.as_str(), "", ""), "at head, nothing passed before, here");
    runner.ends_with(passed());
    looks(w);
    assert_eq!(last_passed(&asks[0].repo), Some(start.clone()));
    crate::locking::lock(&runner.asks, "fake runner asks").clear();
    (runner, dir, project, id, h, start)
}

/// Item `title` (id `id`, held by `h`) writes `file` in its copy and says DONE: it lands on main.
pub(super) fn lands(w: &World, dir: &std::path::Path, project: &str, title: &str, id: &str, h: &str, file: &str) -> String {
    std::fs::write(copy_of(w, project, title).join(file), "x\n").unwrap();
    ends(w, h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(super::desk_spec_copies::subject(dir, "HEAD"), format!("{title} (#{id})"), "it landed: {}", item(w, project, title));
    head(dir)
}

/// A second item, routed and landed: (its id, its holder, main's head with it).
pub(super) fn one_more(w: &World, dir: &std::path::Path, project: &str, title: &str, file: &str) -> (String, String, String) {
    let (id, h) = another(w, project, title);
    let at = lands(w, dir, project, title, &id, &h, file);
    (id, h, at)
}

/// The look the finishing tick takes.
pub(super) fn looks(w: &World) {
    w.hub().full_run_now();
    w.settle();
}

/// Everything the Desk's lead was told of full runs: its feed's rows and what reached it.
fn told(w: &World, project: &str) -> String {
    let rows = w.hub().read(|st| st.desk.feed.get(project).map(|f| f.rows.iter().map(|r| r.words.clone()).collect::<Vec<_>>()).unwrap_or_default());
    let all: Vec<String> = rows.into_iter().chain(sent_to(w, &lead_of(w, project))).filter(|t| t.contains("Full run")).collect();
    all.join("\n")
}

#[test]
fn a_landing_on_cues_own_project_starts_one_run_at_head_and_the_next_one_since_the_last_that_passed() {
    let w = World::new();
    let (runner, dir, project, id, h, start) = in_cue_passed(&w);
    looks(&w);
    assert!(runner.asks().is_empty(), "nothing landed since");
    let first = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let asks = runner.asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert_eq!((asks[0].at.as_str(), asks[0].since.as_str(), asks[0].machine.as_str()), (first.as_str(), start.as_str(), ""), "at head, since the last that passed, on this machine");
    looks(&w);
    assert_eq!(runner.asks().len(), 1, "one at a time, and never twice on one commit");
    assert_eq!(last_passed(&asks[0].repo), Some(start));
    runner.ends_with(passed());
    looks(&w);
    assert!(told(&w, &project).contains(&format!("Full run passed on main (#{id}).")), "{}", told(&w, &project));
    assert_eq!(last_passed(&asks[0].repo), Some(first.clone()));
    looks(&w);
    assert_eq!(runner.asks().len(), 1, "main hasn't moved");
    let (_, _, second) = one_more(&w, &dir, &project, "Footer", "b.txt");
    let asks = runner.asks();
    assert_eq!(asks.len(), 2, "{asks:?}");
    assert_eq!((asks[1].at.as_str(), asks[1].since.as_str()), (second.as_str(), first.as_str()));
}

#[test]
fn the_tick_starts_a_run_when_main_moved_with_no_landing_and_waits_while_an_item_is_installed() {
    let w = World::new();
    let (runner, dir, project, id, h) = in_cue(&w);
    std::fs::write(dir.join("yours.txt"), "yours\n").unwrap();
    super::team::git(&dir, &["add", "-f", "yours.txt"]);
    super::team::git(&dir, &["commit", "-qm", "Yours"]);
    looks(&w);
    let asks = runner.asks();
    assert_eq!((asks.len(), asks[0].at.as_str(), asks[0].since.as_str()), (1, head(&dir).as_str(), ""), "{asks:?}");
    runner.ends_with(passed());
    looks(&w);
    assert!(told(&w, &project).contains("Full run passed on main."), "no item in it: {}", told(&w, &project));
    // An item of the project at a build: the next run waits for it, but not past half an hour.
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    assert_eq!(runner.asks().len(), 2, "landed, nothing being built: {:?}", runner.asks());
    runner.ends_with(passed());
    looks(&w);
    let (_, _, next) = one_more(&w, &dir, &project, "Footer", "b.txt");
    runner.ends_with(passed());
    w.hub().write(|st| st.desk.item_mut(&id).unwrap().finish.get_or_insert_with(Default::default).step = "building".into());
    looks(&w);
    assert_eq!(runner.asks().len(), 3);
    std::fs::write(dir.join("more.txt"), "more\n").unwrap();
    super::team::git(&dir, &["add", "-f", "more.txt"]);
    super::team::git(&dir, &["commit", "-qm", "More"]);
    looks(&w);
    assert_eq!(runner.asks().len(), 3, "it waits for the build: {:?}", runner.asks());
    w.advance(31 * 60_000);
    w.hub().write(|st| st.desk.item_mut(&id).unwrap().finish.get_or_insert_with(Default::default).step = "building".into());
    looks(&w);
    let asks = runner.asks();
    assert_eq!((asks.len(), asks[3].at.as_str(), asks[3].since.as_str()), (4, head(&dir).as_str(), next.as_str()), "not longer than half an hour: {asks:?}");
    let _ = at;
}

#[test]
fn three_landings_during_a_run_give_one_following_run_that_names_all_three() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    let first = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let (b, _, _) = one_more(&w, &dir, &project, "Footer", "b.txt");
    let (c, _, _) = one_more(&w, &dir, &project, "Rail", "shared.txt");
    let (d, _, last) = one_more(&w, &dir, &project, "Notch", "by_builder.txt");
    looks(&w);
    assert_eq!(runner.asks().len(), 1, "the first run is still going: {:?}", runner.asks());
    runner.ends_with(passed());
    looks(&w);
    let asks = runner.asks();
    assert_eq!(asks.len(), 2, "{asks:?}");
    assert_eq!((asks[1].at.as_str(), asks[1].since.as_str()), (last.as_str(), first.as_str()));
    runner.ends_with(passed());
    looks(&w);
    assert!(told(&w, &project).contains(&format!("Full run passed on main (#{b}, #{c}, #{d}).")), "{}", told(&w, &project));
    assert_eq!(runner.asks().len(), 2);
}

#[test]
fn another_projects_landing_starts_none() {
    let w = World::new();
    let runner = Arc::new(Runner::default());
    set_fake(runner.clone());
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    looks(&w);
    assert!(runner.asks().is_empty(), "{:?}", runner.asks());
    assert_eq!(told(&w, &project), "");
}

#[test]
fn a_failure_traced_to_an_items_commit_opens_that_item_again_and_leaves_the_others_landed() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let (b, bh, at_b) = one_more(&w, &dir, &project, "Footer", "b.txt");
    let (_, _, _) = one_more(&w, &dir, &project, "Rail", "shared.txt");
    runner.ends_with(passed());
    looks(&w);
    assert_eq!(runner.asks().len(), 2);
    let states = |w: &World| ["Header jumps", "Footer", "Rail"].map(|t| item(w, &project, t)["state"].as_str().unwrap().to_string());
    let before = states(&w);
    let n = sent_to(&w, &bh).len();
    runner.ends_with(failed(Some(Culprit { commit: at_b.clone(), subject: format!("Footer (#{b})") }), None));
    looks(&w);
    let after = states(&w);
    assert_eq!(after[1], "working", "back with its holder: {}", item(&w, &project, "Footer"));
    assert_eq!((&after[0], &after[2]), (&before[0], &before[2]), "the others stay as they landed");
    let heard = sent_to(&w, &bh)[n..].join("\n");
    assert!(heard.contains(&format!("Desk item #{b} (Footer) is open again: the full run of main failed after it landed (hub::tests::header_stays, tests/rail.test.js). Fix it, then say DONE: #{b}.")), "{heard}");
    assert!(heard.contains("The end of its output:\nassertion failed: header moved"), "{heard}");
    let line = told(&w, &project);
    assert!(line.contains("Full run failed on main: hub::tests::header_stays, tests/rail.test.js.") && line.contains(&format!("#{b} is open again with its holder")), "{line}");
    assert!(line.contains("The app stays on the last version that passed, for 4 hours at most."), "{line}");
    assert_eq!(last_passed(&runner.asks()[0].repo).as_deref(), Some(runner.asks()[0].at.as_str()), "what passed before still did");
    looks(&w);
    assert_eq!(runner.asks().len(), 2, "not run again on the same commit");
}

#[test]
fn a_failure_with_no_commit_found_reopens_nothing_and_tells_the_lead() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let before = item(&w, &project, "Header jumps")["state"].clone();
    let n = sent_to(&w, &h).len();
    runner.ends_with(failed(None, Some("it fails at the last commit that passed too")));
    looks(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], before, "nothing reopened");
    assert!(!sent_to(&w, &h)[n..].iter().any(|t| t.contains("open again")), "{:?}", sent_to(&w, &h));
    let line = told(&w, &project);
    assert!(line.contains("Full run failed on main, on tests no item in this batch caused: hub::tests::header_stays, tests/rail.test.js. Why: it fails at the last commit that passed too. No item was reopened and the app still updates."), "{line}");
    assert!(line.contains("The end of its output:\nassertion failed: header moved"), "{line}");
    // A commit found that names no item: the same.
    let (_, fh, at) = one_more(&w, &dir, &project, "Footer", "b.txt");
    let n = sent_to(&w, &fh).len();
    runner.ends_with(failed(Some(Culprit { commit: at, subject: "Tidy the build".into() }), None));
    looks(&w);
    assert!(!sent_to(&w, &fh)[n..].iter().any(|t| t.contains("open again")));
    assert!(told(&w, &project).contains("(Tidy the build) broke it, which is no item's landed commit on this Desk. No item was reopened and the app still updates."), "{}", told(&w, &project));
}

#[test]
fn an_items_commit_that_changed_only_documents_is_never_reopened() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "plan.md");
    let before = item(&w, &project, "Header jumps")["state"].clone();
    let n = sent_to(&w, &h).len();
    // A search that ended on it (a test that fails only sometimes, or a script on another machine that doesn't know).
    runner.ends_with(failed(Some(Culprit { commit: at.clone(), subject: format!("Header jumps (#{id})") }), None));
    looks(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], before, "stays landed: {}", item(&w, &project, "Header jumps"));
    assert!(!sent_to(&w, &h)[n..].iter().any(|t| t.contains("full run")), "{:?}", sent_to(&w, &h));
    let line = told(&w, &project);
    assert!(line.contains(&format!("Why: the search ended on commit {} (Header jumps (#{id})), which changed only documents, so what failed fails only sometimes. No item was reopened and the app still updates.", &at[..7])), "{line}");
    // One that changed anything else is blamed as before.
    let (b, bh, at_b) = one_more(&w, &dir, &project, "Footer", "b.txt");
    runner.ends_with(failed(Some(Culprit { commit: at_b, subject: format!("Footer (#{b})") }), None));
    looks(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], "working", "{}", item(&w, &project, "Footer"));
    assert!(sent_to(&w, &bh).iter().any(|t| t.contains("is open again")), "{:?}", sent_to(&w, &bh));
}

#[test]
fn a_commit_that_names_an_item_whose_landed_commit_it_isnt_blames_no_item() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    let at_header = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let (b, bh, _) = one_more(&w, &dir, &project, "Footer", "b.txt");
    runner.ends_with(passed());
    looks(&w);
    let before = item(&w, &project, "Footer")["state"].clone();
    let n = sent_to(&w, &bh).len();
    // Header's commit, under a subject that names Footer (a commit of yours that copied the "(#id)").
    runner.ends_with(failed(Some(Culprit { commit: at_header, subject: format!("Footer again (#{b})") }), None));
    looks(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], before, "not its commit: {}", item(&w, &project, "Footer"));
    assert!(!sent_to(&w, &bh)[n..].iter().any(|t| t.contains("full run")), "{:?}", sent_to(&w, &bh));
    assert!(told(&w, &project).contains("which is no item's landed commit on this Desk. No item was reopened and the app still updates."), "{}", told(&w, &project));
}

#[test]
fn an_item_is_reopened_once_per_commit_and_never_while_it_is_at_work() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let (b, bh, at_b) = one_more(&w, &dir, &project, "Footer", "b.txt");
    let (c, ch, at_c) = one_more(&w, &dir, &project, "Rail", "shared.txt");
    runner.ends_with(passed());
    looks(&w);
    // Rail is at work again (sent back from a check) when the run names its commit: told, not reopened.
    w.hub().desk_open_again(&c, "you sent it back", "Fix it", "");
    w.settle();
    let (n, decisions) = (sent_to(&w, &ch).len(), w.hub().read(|st| st.desk.decisions.len()));
    runner.ends_with(failed(Some(Culprit { commit: at_c.clone(), subject: format!("Rail (#{c})") }), None));
    looks(&w);
    let heard = sent_to(&w, &ch)[n..].join("\n");
    assert!(heard.contains(&format!("Desk item #{c} (Rail): the full run of main failed on its commit")) && heard.contains("It is open with you already"), "{heard}");
    assert!(!heard.contains("is open again"), "{heard}");
    assert_eq!(w.hub().read(|st| st.desk.decisions.len()), decisions, "nothing done to the item");
    assert_eq!(item(&w, &project, "Rail")["state"], "working");
    assert!(told(&w, &project).contains(&format!("#{c} is at work already, so it wasn't reopened; its holder was told")), "{}", told(&w, &project));
    // Footer's commit is named: reopened. Named again by the next run, with Footer checked off since: not again.
    std::fs::write(dir.join("yours.txt"), "yours\n").unwrap();
    super::team::git(&dir, &["add", "-f", "yours.txt"]);
    super::team::git(&dir, &["commit", "-qm", "Yours"]);
    looks(&w);
    runner.ends_with(failed(Some(Culprit { commit: at_b.clone(), subject: format!("Footer (#{b})") }), None));
    looks(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], "working", "{}", item(&w, &project, "Footer"));
    w.hub().write(|st| st.desk.item_mut(&b).unwrap().state = "done".into());
    std::fs::write(dir.join("yours.txt"), "more\n").unwrap();
    super::team::git(&dir, &["commit", "-qam", "Yours again"]);
    looks(&w);
    let n = sent_to(&w, &bh).len();
    runner.ends_with(failed(Some(Culprit { commit: at_b, subject: format!("Footer (#{b})") }), None));
    looks(&w);
    assert_eq!(item(&w, &project, "Footer")["state"], "done", "never twice for one commit");
    assert!(!sent_to(&w, &bh)[n..].iter().any(|t| t.contains("open again")), "{:?}", &sent_to(&w, &bh)[n..]);
    assert!(told(&w, &project).contains(&format!("#{b} was already sent back for that commit: not reopened again")), "{}", told(&w, &project));
}

#[test]
fn the_tick_looks_once_a_minute_while_nothing_runs_and_a_landing_looks_at_once() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    std::fs::write(dir.join("yours.txt"), "yours\n").unwrap();
    super::team::git(&dir, &["add", "-f", "yours.txt"]);
    super::team::git(&dir, &["commit", "-qm", "Yours"]);
    w.hub().full_run_tick();
    w.settle();
    assert!(runner.asks().is_empty(), "looked at under a minute ago, nothing running: {:?}", runner.asks());
    w.advance(61_000);
    w.hub().full_run_tick();
    w.settle();
    assert_eq!(runner.asks().len(), 1, "the minute is up");
    runner.ends_with(passed());
    w.hub().full_run_tick();
    w.settle();
    assert!(told(&w, &project).contains("Full run passed on main."), "a run going is looked at on every tick: {}", told(&w, &project));
    // A landing doesn't wait for the minute.
    lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    assert_eq!(runner.asks().len(), 2, "{:?}", runner.asks());
}

/// The project's full run picked to go on machine "box", its clone there in ~/code/cue.
fn on_box(w: &World, project: &str) {
    let m = crate::machines::add("box", "").unwrap();
    crate::desk::machine_pref::save(project, &m.name, Some("~/code/cue")).unwrap();
    // Its folder there stays known; new sessions go on starting on this machine.
    crate::desk::machine_pref::save(project, "", None).unwrap();
    assert_eq!(crate::desk::machine_pref::of(project).folder(&m.name), Some("~/code/cue"));
    w.hub().set_setting("project.lands", json!({ project: { "full_run_on": m.name } })).unwrap();
}

#[test]
fn a_machine_that_cant_be_reached_falls_back_to_this_one_and_the_line_says_so() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    on_box(&w, &project);
    *crate::locking::lock(&runner.refuse, "fake runner refuse") = "no route to host".into();
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let asks = runner.asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert_eq!((asks[0].machine.as_str(), asks[0].at.as_str()), ("", at.as_str()), "here instead");
    runner.ends_with(passed());
    looks(&w);
    assert!(told(&w, &project).contains(&format!("Full run passed on main (#{id}). It ran on this machine instead: box couldn't start it (no route to host).")), "{}", told(&w, &project));
}

#[test]
fn a_run_on_a_machine_goes_to_its_clone_and_one_that_never_answers_is_made_here() {
    let w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    on_box(&w, &project);
    let installer = Arc::new(Fake::default());
    w.hub().desk_set_installer(installer.clone());
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    let asks = runner.asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert_eq!((asks[0].machine.as_str(), asks[0].host.as_str(), asks[0].folder.as_str(), asks[0].at.as_str()), ("box", "box", "~/code/cue", at.as_str()));
    w.advance(60 * 60_000);
    looks(&w);
    assert_eq!(runner.asks().len(), 1, "still within its time");
    // Its install, begun meanwhile, is through: nothing of the project is being built when the run comes here.
    installer.goes("installed", "");
    installer.runs(&at);
    super::desk_commit::tick(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "{}", item(&w, &project, "Header jumps"));
    w.advance(31 * 60_000);
    looks(&w);
    let asks = runner.asks();
    assert_eq!(asks.len(), 2, "{asks:?} {}", item(&w, &project, "Header jumps")["finish"]);
    assert_eq!((asks[1].machine.as_str(), asks[1].at.as_str()), ("", at.as_str()));
    assert_eq!(crate::locking::lock(&runner.stopped, "fake runner stopped").len(), 1, "the machine's run was given up on");
    runner.ends_with(passed());
    looks(&w);
    assert!(told(&w, &project).contains("It ran on this machine instead: box gave no answer."), "{}", told(&w, &project));
    // The next one tries the machine again; it answers that it couldn't make it: made here.
    let (_, _, next) = one_more(&w, &dir, &project, "Footer", "b.txt");
    assert_eq!(runner.asks().last().map(|a| (a.machine.clone(), a.at.clone())), Some(("box".to_string(), next.clone())));
    runner.ends_with(FullRunEnd::NotMade("the project's folder isn't there".into()));
    looks(&w);
    assert_eq!(runner.asks().last().map(|a| (a.machine.clone(), a.at.clone())), Some((String::new(), next)));
}

#[test]
fn a_relaunch_keeps_what_passed_starts_no_second_run_and_makes_a_run_cut_short_again() {
    let mut w = World::new();
    let (runner, dir, project, id, h, _) = in_cue_passed(&w);
    let first = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    runner.ends_with(passed());
    looks(&w);
    let repo = runner.asks()[0].repo.clone();
    w.relaunch();
    w.hub().desk_set_installer(Arc::new(Fake::default()));
    looks(&w);
    assert_eq!(last_passed(&repo), Some(first.clone()), "the mark is kept");
    assert_eq!(runner.asks().len(), 1, "nothing to run: main hasn't moved");
    let (_, _, second) = one_more(&w, &dir, &project, "Footer", "b.txt");
    assert_eq!(runner.asks().len(), 2);
    // Relaunched with its run still known to be going: no second one.
    w.relaunch();
    w.hub().desk_set_installer(Arc::new(Fake::default()));
    looks(&w);
    assert_eq!(runner.asks().len(), 2, "{:?}", runner.asks());
    // Main moves on while it runs. Cue quit under the run: the same run is asked for again, the commits it was
    // started with and not main's new head, so the script answers with the run left going instead of a second one.
    one_more(&w, &dir, &project, "Rail", "shared.txt");
    assert_eq!(runner.asks().len(), 2);
    runner.quit();
    w.relaunch();
    w.hub().desk_set_installer(Arc::new(Fake::default()));
    looks(&w);
    let asks = runner.asks();
    assert_eq!(asks.len(), 3, "{asks:?}");
    assert_eq!((asks[2].at.as_str(), asks[2].since.as_str()), (second.as_str(), first.as_str()));
    assert_eq!(last_passed(&repo), Some(first));
    looks(&w);
    assert_eq!(runner.asks().len(), 3, "it is the run going now: no other beside it");
}
