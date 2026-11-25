//! Which landing step each item of a Desk is in right now (#783, #814): its check before landing, a step after
//! landing, or Cue's full run of every test. Sent as `landing_now`; nothing draws it as a line of its own (that line
//! over the Desk is gone, #814): an item's row says the step it waits on instead (ui/desk/now_line.js).
//!
//! Built with the store held (core/view.rs), so nothing here reads a file or asks git: what the full run is doing is
//! kept here by its own look (full_run.rs `full_run_shown`), and the check and after-landing steps say here which step
//! they're on as they run (`Steps`).

use crate::desk::hub::landing::full_run_kit::{FullRunState, Running};
use crate::desk::hub::prelude::*;
use serde_json::{json, Value};

/// A step under way: `what` it is ("Testing main at 1e01e83", "Checking #12 before it lands", "After landing #12"),
/// the `step` it's at, and the items it carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Line {
    pub what: String,
    pub step: String,
    pub items: Vec<String>,
    /// `step` is one of your steps' commands (shown as typed), not words.
    pub cmd: bool,
}

impl Line {
    fn to_json(&self) -> Value {
        json!({ "state": "run", "what": self.what, "step": self.step, "items": self.items, "cmd": self.cmd })
    }
}

/// A check or after-landing step under way: Desk, item, "check" or "after", the step.
#[derive(Clone, Debug)]
struct Live {
    desk: String,
    item: String,
    kind: &'static str,
    step: String,
}

static LIVE: std::sync::Mutex<Vec<Live>> = std::sync::Mutex::new(Vec::new());
/// A Desk's key here: its name in Cue's folder now (a test's World has its own).
fn key(desk: &str) -> String {
    format!("{}\n{desk}", crate::server::cue_dir().display())
}
/// The full run going per Desk, as its last look found it.
static FULL: std::sync::Mutex<Vec<(String, Line)>> = std::sync::Mutex::new(Vec::new());

/// A check or after landing of item `item` of Desk `desk` under way, from `begin` till it's dropped.
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
        live.push(Live { desk: self.desk.clone(), item: self.item.clone(), kind: self.kind, step: step.trim().to_string() });
        drop(live);
        self.hub.redraw();
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

/// Desk `desk`'s full run going (None: none is) kept: whether it changed (the window is drawn again).
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

/// The full run going, from what's kept of it (`s`), the step it's at (`step`), where it runs ("" this machine, else
/// the machine's name) and the items it carries. None: no run is going.
pub(crate) fn full_line(s: &FullRunState, step: &str, on: &str, items: Vec<String>) -> Option<Line> {
    s.running.as_ref().map(|r| running_line(r, step, on, items))
}

fn running_line(r: &Running, step: &str, on: &str, items: Vec<String>) -> Line {
    let step = if on.is_empty() { step_words(step) } else { format!("on {on}") };
    Line { what: format!("Testing main at {}", short(&r.at)), step, items, cmd: false }
}

/// The step under way in Desk `desk`'s landing (null: none): a check before landing first, then a step after landing,
/// then Cue's full run.
pub(crate) fn view(desk: &str) -> Value {
    let k = key(desk);
    let live: Vec<Live> = {
        let all = crate::locking::lock(&LIVE, "landing now");
        all.iter().filter(|x| x.desk == k).cloned().collect()
    };
    let first = live.iter().find(|x| x.kind == "check").or_else(|| live.iter().find(|x| x.kind == "after"));
    if let Some(x) = first {
        let what = if x.kind == "check" { format!("Checking #{} before it lands", x.item) } else { format!("After landing #{}", x.item) };
        // The repository's own check says no step of its own: it's named.
        let (step, cmd) = if x.step.is_empty() && x.kind == "check" { ("The repository's check".into(), false) } else { step_shown(&x.step) };
        return Line { what, step, items: vec![x.item.clone()], cmd }.to_json();
    }
    let all = crate::locking::lock(&FULL, "landing full run");
    all.iter().find(|(d, _)| *d == k).map_or(Value::Null, |(_, l)| l.to_json())
}

/// A step of yours as the row shows it: its name, or its command as typed (`cmd`) when it has none.
fn step_shown(step: &str) -> (String, bool) {
    let name = crate::desk::lands::step_name(step);
    (name.to_string(), name == crate::desk::lands::step_run(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_going_says_its_commit_step_and_items() {
        let mut s = FullRunState { started: "1e01e83abc".into(), ..Default::default() };
        assert_eq!(full_line(&s, "", "", vec![]), None, "no run going: nothing");
        s.running = Some(Running { at: "1e01e83abc".into(), began: 5, ..Default::default() });
        let l = full_line(&s, "rust tests", "", vec!["779".into()]).unwrap();
        assert_eq!((l.what.as_str(), l.step.as_str()), ("Testing main at 1e01e83", "Rust tests"));
        assert_eq!(l.items, vec!["779".to_string()]);
        assert_eq!(full_line(&s, "", "studio", vec![]).unwrap().step, "on studio");
    }

    #[test]
    fn steps_read_plainly() {
        assert_eq!(step_words("rust tests"), "Rust tests");
        assert_eq!(step_words("looking for the commit that broke it"), "Finding the commit that broke it");
        assert_eq!(step_words("another full run is using /x; waiting for its end"), "Waiting for another run to end");
    }

    #[test]
    fn steps_show_by_their_names() {
        assert_eq!(step_shown("[Deploy] make deploy"), ("Deploy".to_string(), false));
        assert_eq!(step_shown("make deploy"), ("make deploy".to_string(), true));
    }
}
