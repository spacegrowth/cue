//! The line at the top of each Desk (#783): what Cue is doing with landed work, in that Desk's own landing steps
//! (desk/lands.rs): its check before landing, its steps after landing, Cue's full run of every test and its build and
//! install. One line, the step under way first; under it the items that step carries. A Desk with no check and
//! nothing after landing has none.
//!
//! Built with the store held (core/view.rs), so nothing here reads a file or asks git: what the full run is doing is
//! kept here by its own look (full_run.rs `full_run_step`), and the check and after-landing steps say here which step
//! they're on as they run (`Steps`).

use crate::desk::hub::landing::full_run_kit::{FullRunState, Running};
use crate::desk::hub::prelude::*;
use crate::desk::lands::Lands;
use serde_json::{json, Value};

/// One line: "run" (a step under way), "wait" (one due, not begun), "fail" (the last one failed), "ok" (it passed),
/// "idle" (nothing yet: what the steps are). `what` first, then `step`, when it began (`since`, ms), and `note`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Line {
    pub state: &'static str,
    pub what: String,
    pub step: String,
    pub since: u64,
    pub note: String,
    pub items: Vec<String>,
    /// `step` is one of your steps' commands (shown as typed), not words.
    pub cmd: bool,
}

impl Line {
    fn new(state: &'static str, what: impl Into<String>) -> Line {
        Line { state, what: what.into(), ..Default::default() }
    }

    fn to_json(&self, also: &str) -> Value {
        json!({ "state": self.state, "what": self.what, "step": self.step, "since": self.since, "note": self.note, "items": self.items, "cmd": self.cmd, "also": also })
    }
}

/// A check or after-landing step under way: Desk, item, "check" or "after", the step, since when.
#[derive(Clone, Debug)]
struct Live {
    desk: String,
    item: String,
    kind: &'static str,
    step: String,
    since: u64,
}

/// How a Desk's last check or after landing ended: Desk, "check" or "after", item, the step that failed ("" when all
/// passed), when.
#[derive(Clone, Debug)]
struct Last {
    desk: String,
    kind: &'static str,
    item: String,
    failed: String,
    at: u64,
}

static LIVE: std::sync::Mutex<Vec<Live>> = std::sync::Mutex::new(Vec::new());
/// A Desk's key here: its name in Cue's folder now (a test's World has its own).
fn key(desk: &str) -> String {
    format!("{}\n{desk}", crate::server::cue_dir().display())
}
static LAST: std::sync::Mutex<Vec<Last>> = std::sync::Mutex::new(Vec::new());
/// The full run's line per Desk, as its last look found it.
static FULL: std::sync::Mutex<Vec<(String, Line)>> = std::sync::Mutex::new(Vec::new());

/// A check or after landing of item `item` of Desk `desk` under way, from `begin` till it's dropped (or `end`ed).
pub(crate) struct Steps<'a> {
    hub: &'a Hub,
    desk: String,
    item: String,
    kind: &'static str,
}

impl<'a> Steps<'a> {
    /// `kind` ("check" or "after") begun on `item`, at `step`.
    pub(crate) fn begin(hub: &'a Hub, desk: &str, item: &str, kind: &'static str, step: &str) -> Steps<'a> {
        let s = Steps { hub, desk: key(desk), item: item.to_string(), kind };
        s.at(step);
        s
    }

    /// Now at `step`.
    pub(crate) fn at(&self, step: &str) {
        let mut live = crate::locking::lock(&LIVE, "landing now");
        live.retain(|l| !(l.desk == self.desk && l.item == self.item && l.kind == self.kind));
        live.push(Live { desk: self.desk.clone(), item: self.item.clone(), kind: self.kind, step: step.trim().to_string(), since: now_ms() });
        drop(live);
        self.hub.redraw();
    }

    /// Ended: passed, or failed on `failed`.
    pub(crate) fn end(self, failed: Option<&str>) {
        let mut last = crate::locking::lock(&LAST, "landing last");
        last.retain(|l| !(l.desk == self.desk && l.kind == self.kind));
        last.push(Last { desk: self.desk.clone(), kind: self.kind, item: self.item.clone(), failed: failed.unwrap_or("").trim().chars().take(120).collect(), at: now_ms() });
    }
}

impl Drop for Steps<'_> {
    fn drop(&mut self) {
        let mut live = crate::locking::lock(&LIVE, "landing now");
        live.retain(|l| !(l.desk == self.desk && l.item == self.item && l.kind == self.kind));
        drop(live);
        self.hub.redraw();
    }
}

/// Desk `desk`'s full run as its look found it kept: whether it changed (the window is drawn again).
pub(crate) fn set_full(desk: &str, line: Option<Line>) -> bool {
    let desk = key(desk);
    let mut full = crate::locking::lock(&FULL, "landing full run");
    let was = full.iter().position(|(d, _)| *d == desk);
    let same = match (was, &line) {
        (Some(k), Some(l)) => full[k].1 == *l,
        (None, None) => true,
        _ => false,
    };
    if let Some(k) = was {
        full.remove(k);
    }
    if let Some(l) = line {
        full.push((desk, l));
    }
    !same
}

/// A commit's name, short.
fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// The full run's step in plain words ("rust tests" → "Rust tests").
fn step_words(step: &str) -> String {
    let step = step.trim();
    if step.starts_with("another full run") {
        return "Waiting for another run to end".into();
    }
    match step {
        "" => String::new(),
        "build" => "Build".into(),
        "running what failed once more" => "Running what failed once more".into(),
        "looking for the commit that broke it" => "Finding the commit that broke it".into(),
        s => {
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

/// The full run's line, from what's kept of it (`s`), main's head (`head`), the step the run going is at (`step`), where
/// it runs ("" this machine, else the machine's name) and the items `items` carries (those of the run going, or of the
/// commits waiting for one). None: no run has ever been made or asked for.
pub(crate) fn full_line(s: &FullRunState, head: &str, step: &str, on: &str, items: Vec<String>) -> Option<Line> {
    if let Some(r) = &s.running {
        return Some(running_line(r, step, on, items));
    }
    // Main moved past every run: one is due.
    let unrun = !head.is_empty() && head != s.started && head != s.passed && head != s.judged;
    if unrun {
        let mut l = Line::new("wait", format!("Main at {} waits for its full run", short(head)));
        l.note = if s.wanted_since > 0 { "it starts once nothing is being built or installed".into() } else { String::new() };
        l.since = s.wanted_since;
        l.items = items;
        return Some(l);
    }
    if !s.judged.is_empty() && s.judged != s.passed {
        let first = s.failed_names.first().map(String::as_str).unwrap_or("a test");
        let more = s.failed_names.len().saturating_sub(1);
        let mut l = Line::new("fail", format!("Failed: {first}{}", if more > 0 { format!(" and {more} more") } else { String::new() }));
        let mut note = vec![format!("on {}", short(&s.judged))];
        if !head.is_empty() && head != s.judged {
            note.push(format!("an older commit than main ({}), so it's ignored till main's own run", short(head)));
        }
        if s.failing == s.judged {
            note.push("no item caused it, so it's ignored and the app still updates".into());
        } else if !s.held_item.is_empty() {
            note.push(format!("#{} went back to be fixed; the app stays on the last version that passed", s.held_item));
        }
        l.note = note.join(" · ");
        l.since = s.judged_ms;
        l.items = s.judged_items.clone();
        return Some(l);
    }
    if !s.passed.is_empty() {
        let mut l = Line::new("ok", format!("Tests passed on main at {}", short(&s.passed)));
        l.since = s.judged_ms;
        l.items = s.judged_items.clone();
        return Some(l);
    }
    None
}

fn running_line(r: &Running, step: &str, on: &str, items: Vec<String>) -> Line {
    let mut l = Line::new("run", format!("Testing main at {}", short(&r.at)));
    l.step = if on.is_empty() { step_words(step) } else { format!("on {on}") };
    l.since = r.began;
    if !r.instead.is_empty() {
        l.note = format!("on this Mac instead: {}", r.instead);
    }
    l.items = items;
    l
}

/// The line of Desk `desk` with landing settings `l`: null when it has no check, nothing after landing and no full run.
pub(crate) fn view(st: &Store, desk: &str, l: &Lands) -> Value {
    // Cue's full run is made only for its own project (full_run.rs): a Desk that has one has it among its steps.
    let k = key(desk);
    let full = {
        let all = crate::locking::lock(&FULL, "landing full run");
        all.iter().find(|(d, _)| *d == k).map(|(_, l)| l.clone())
    };
    if l.check == "none" && l.after == "nothing" && full.is_none() {
        return Value::Null;
    }
    let lines = lines_of(st, desk, &k, l, full.clone());
    let Some(first) = lines.first() else { return Value::Null };
    // The full run going behind something else under way: said after it, in a few words.
    let also = full.filter(|f| f.state == "run" && first.state == "run" && first.what != f.what).map(|f| if f.step.is_empty() { f.what.clone() } else { format!("{} · {}", f.what, f.step) }).unwrap_or_default();
    first.to_json(&also)
}

/// The finishing items of Desk `desk`: (those at any of `steps`).
fn at<'a>(st: &'a Store, desk: &str, steps: &[&str]) -> Vec<&'a crate::desk::DeskItem> {
    st.desk.items.iter().filter(|i| i.project == desk && i.state == "finishing" && i.finish.as_ref().is_some_and(|f| steps.contains(&f.step.as_str()))).collect()
}

/// One line for `on` (items at one step): what, when the first began, and the items.
fn line_for(state: &'static str, what: &str, on: &[&crate::desk::DeskItem]) -> Line {
    let mut line = Line::new(state, what);
    line.since = on.iter().filter_map(|i| i.finish.as_ref().map(|f| f.at)).filter(|a| *a > 0).min().unwrap_or(0);
    line.items = on.iter().map(|i| i.id.clone()).collect();
    line
}

/// Desk `desk`'s lines, the one to show first: what's under way, what failed, what waits to install, how the last
/// passed, and what its steps are.
fn lines_of(st: &Store, desk: &str, k: &str, l: &Lands, full: Option<Line>) -> Vec<Line> {
    let live: Vec<Live> = {
        let all = crate::locking::lock(&LIVE, "landing now");
        all.iter().filter(|x| x.desk == k).cloned().collect()
    };
    let installs = l.after == "install" || full.is_some();
    let mut out = Vec::new();
    // Cue's build and install, after its full run.
    if installs {
        let now = [("starting", "Cue is restarting"), ("installing", "Installing Cue"), ("building", "Building Cue")].into_iter().map(|(s, w)| (at(st, desk, &[s]), w)).find(|(on, _)| !on.is_empty());
        out.extend(now.map(|(on, what)| line_for("run", what, &on)));
    }
    out.extend(checking(st, desk, l, &live));
    out.extend(live.iter().filter(|x| x.kind == "after").map(|x| {
        let mut line = Line::new("run", format!("After landing #{}", x.item));
        (line.step, line.since, line.items, line.cmd) = (x.step.clone(), x.since, vec![x.item.clone()], true);
        line
    }));
    out.extend(full.iter().filter(|f| f.state == "run").cloned());
    out.extend(full.iter().filter(|f| f.state == "wait").cloned());
    // How the last of each ended, a failure first and the newest first; between failures and passes, what waits to install.
    let mut ended: Vec<Line> = {
        let all = crate::locking::lock(&LAST, "landing last");
        all.iter().filter(|x| x.desk == k).map(last_line).collect()
    };
    ended.extend(full.filter(|f| matches!(f.state, "fail" | "ok")));
    ended.sort_by_key(|x| (x.state != "fail", std::cmp::Reverse(x.since)));
    let passed = ended.split_off(ended.iter().position(|x| x.state != "fail").unwrap_or(ended.len()));
    out.extend(ended);
    let queued = at(st, desk, &["queued"]);
    if installs && !queued.is_empty() {
        let mut line = line_for("wait", "Waiting to install", &queued);
        line.note = queued.iter().filter_map(|i| i.finish.as_ref()).map(|f| f.wait.trim()).find(|w| !w.is_empty()).unwrap_or("").to_string();
        out.push(line);
    }
    out.extend(passed);
    out.push(idle_line(l));
    out
}

/// The checks before landing under way: the step each is on when they're your own, else Cue's own check.
fn checking(st: &Store, desk: &str, l: &Lands, live: &[Live]) -> Vec<Line> {
    at(st, desk, &["checking"])
        .into_iter()
        .map(|c| {
            let mut line = Line::new("run", format!("Checking #{} before it lands", c.id));
            let x = live.iter().find(|x| x.kind == "check" && x.item == c.id);
            line.since = x.map_or(0, |x| x.since);
            match x.filter(|x| !x.step.is_empty()) {
                Some(x) => (line.step, line.cmd) = (x.step.clone(), l.check == "command"),
                None => line.step = if l.check == "command" { "its check".into() } else { "Cue's own check".into() },
            }
            line.items = vec![c.id.clone()];
            line
        })
        .collect()
}

fn last_line(x: &Last) -> Line {
    let kind = if x.kind == "check" { "Check" } else { "After landing" };
    let mut line = if x.failed.is_empty() { Line::new("ok", format!("{kind} passed on #{}", x.item)) } else { Line::new("fail", format!("{kind} failed on #{}", x.item)) };
    (line.step, line.since, line.items) = (x.failed.clone(), x.at, vec![x.item.clone()]);
    // After landing names the step that failed as typed; a check says it in its own words ("\"npm test\" failed").
    line.cmd = x.kind == "after";
    line
}

/// What the Desk's steps are, before any has run: "Check before landing: npm test, then lint · After landing: make deploy".
fn idle_line(l: &Lands) -> Line {
    let steps = |s: &[String]| s.join(", then ");
    let check = match l.check {
        "command" => format!("Check before landing: {}", steps(&l.check_steps)),
        "cue" => "Cue's own check before landing, every test after".into(),
        _ => String::new(),
    };
    let after = match l.after {
        "command" => format!("After landing: {}", steps(&l.after_steps)),
        "install" => "Installed after landing".into(),
        _ => String::new(),
    };
    let mut line = Line::new("idle", [check, after].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "));
    line.note = match l.push {
        "after" => "sent to GitHub as each lands".into(),
        _ => String::new(),
    };
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(at: &str) -> FullRunState {
        FullRunState { started: at.into(), ..Default::default() }
    }

    #[test]
    fn a_run_going_says_its_commit_step_and_start() {
        let mut s = st("1e01e83abc");
        s.running = Some(Running { at: "1e01e83abc".into(), began: 5, ..Default::default() });
        let l = full_line(&s, "1e01e83abc", "rust tests", "", vec!["779".into()]).unwrap();
        assert_eq!((l.state, l.what.as_str(), l.step.as_str(), l.since), ("run", "Testing main at 1e01e83", "Rust tests", 5));
        assert_eq!(l.items, vec!["779".to_string()]);
        let there = full_line(&s, "1e01e83abc", "", "studio", vec![]).unwrap();
        assert_eq!(there.step, "on studio");
    }

    #[test]
    fn a_failed_run_names_its_first_test_and_whether_main_moved_past_it() {
        let mut s = FullRunState { judged: "aaaaaaa1".into(), passed: "bbbbbbb2".into(), started: "aaaaaaa1".into(), failed_names: vec!["a::b".into(), "x.test.js".into()], judged_ms: 9, ..Default::default() };
        let l = full_line(&s, "aaaaaaa1", "", "", vec![]).unwrap();
        assert_eq!((l.state, l.what.as_str(), l.note.as_str()), ("fail", "Failed: a::b and 1 more", "on aaaaaaa"));
        // Main moved on and the next run hasn't begun: the old one's failure says so.
        s.started = "ccccccc3".into();
        let old = full_line(&s, "ccccccc3", "", "", vec![]).unwrap();
        assert!(old.note.contains("an older commit than main (ccccccc)"), "{}", old.note);
        // No item caused it.
        s.failing = s.judged.clone();
        assert!(full_line(&s, "ccccccc3", "", "", vec![]).unwrap().note.contains("no item caused it"));
    }

    #[test]
    fn main_ahead_of_every_run_waits_for_one_and_a_pass_says_so() {
        let s = FullRunState { started: "a1a1a1a1".into(), passed: "a1a1a1a1".into(), judged: "a1a1a1a1".into(), ..Default::default() };
        assert_eq!(full_line(&s, "a1a1a1a1", "", "", vec![]).unwrap().what, "Tests passed on main at a1a1a1a");
        let l = full_line(&s, "b2b2b2b2", "", "", vec!["9".into()]).unwrap();
        assert_eq!((l.state, l.what.as_str()), ("wait", "Main at b2b2b2b waits for its full run"));
        assert_eq!(full_line(&FullRunState::default(), "b2b2b2b2", "", "", vec![]).unwrap().state, "wait");
        assert_eq!(full_line(&FullRunState::default(), "", "", "", vec![]), None);
    }

    #[test]
    fn steps_read_plainly() {
        assert_eq!(step_words("rust tests"), "Rust tests");
        assert_eq!(step_words("looking for the commit that broke it"), "Finding the commit that broke it");
        assert_eq!(step_words("another full run is using /x; waiting for its end"), "Waiting for another run to end");
    }

    #[test]
    fn the_idle_line_is_the_desks_own_steps() {
        let mut l = crate::desk::lands::defaults(crate::desk::lands::Place::Repo);
        (l.check, l.check_steps, l.after, l.after_steps, l.push) = ("command", vec!["npm test".into(), "npm run lint".into()], "command", vec!["make deploy".into()], "after");
        let i = idle_line(&l);
        assert_eq!(i.what, "Check before landing: npm test, then npm run lint · After landing: make deploy");
        assert_eq!(i.note, "sent to GitHub as each lands");
    }
}
