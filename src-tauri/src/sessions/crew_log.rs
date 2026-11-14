//! What a hosted lead's crew decided on its own (Cue, or the lead, without asking you): its job in one line, and a
//! short log of each decision, kept on the lead's record (the crew tray's "Decided on its own"). Pure: no clock, no
//! process. One member's review that goes on to fixes and a commit is one line that grows, not three.

use super::team::Teammate;
use super::Sessions;
use serde::{Deserialize, Serialize};

/// The most decisions a lead keeps: older ones drop off.
const KEEP: usize = 50;
/// The most a decision's `what` and `why` say, and the crew's job.
const WHAT_MAX: usize = 110;
const WHY_MAX: usize = 90;
const JOB_MAX: usize = 140;

/// One thing the crew decided on its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrewDecision {
    /// "d<at_ms>-<n>": unique within its lead.
    pub id: String,
    /// When it was decided, or last grew.
    pub at_ms: u64,
    /// The member it's about, or "lead".
    pub who: String,
    /// review · fixes · commit · hold · split · merge · aside: what it is now (a review that went on to a commit is one).
    pub kind: String,
    /// What was decided, in plain words: its `head`, then what came of it (`steps`).
    pub what: String,
    #[serde(default)]
    pub why: String,
    /// Others see it, or it can't be undone (a commit to main, a push).
    #[serde(default)]
    pub one_way: bool,
    /// The commit it made, short; "" when it made none.
    #[serde(default)]
    pub sha: String,
    /// "asked": you went back on it, and Cue told the lead.
    #[serde(default)]
    pub back: String,
    /// The member's Agent call it's about ("" for the lead's own): its later steps are found by it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub call: String,
    /// What it said first ("Reviewed: 2 should-fix"), and what came of it since ("sent back", "fixed").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

/// A decision as it's noted: a new line saying `head`, or, when one about the same member is still open (not yet
/// committed or set aside, not gone back on), that line grown by `step`.
#[derive(Default)]
pub struct Decided<'a> {
    pub call: &'a str,
    pub who: &'a str,
    pub kind: &'a str,
    pub head: String,
    pub step: String,
    pub why: String,
    pub sha: String,
    pub one_way: bool,
}

impl<'a> Decided<'a> {
    /// About member `t`: `step` grows its open line; with none open, a new line says it.
    pub fn about(t: &'a Teammate, kind: &'a str, step: &str) -> Self {
        Decided { call: &t.call, who: &t.name, kind, head: capital(step), step: step.to_string(), ..Default::default() }
    }
}

/// `s` with its first letter a capital.
fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// `s`, at most `max` characters, cut with "…".
fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

impl CrewDecision {
    /// Still going: a step can grow it (not committed or set aside, not gone back on).
    fn open(&self) -> bool {
        !self.call.is_empty() && self.back.is_empty() && !matches!(self.kind.as_str(), "commit" | "aside" | "merge")
    }

    /// `what` again from its head and steps: "Reviewed: 2 should-fix. Sent back, fixed, committed to `crew/auth`".
    fn render(&mut self) {
        let mut what = self.head.clone();
        for (i, step) in self.steps.iter().enumerate() {
            what.push_str(if i == 0 { ". " } else { ", " });
            what.push_str(&if i == 0 { capital(step) } else { step.clone() });
        }
        self.what = clip(&what, WHAT_MAX);
    }
}

/// The open line about the member of Agent call `call`, if any.
fn open_of<'l>(log: &'l mut [CrewDecision], call: &str) -> Option<&'l mut CrewDecision> {
    log.iter_mut().rev().find(|d| d.call == call && d.open())
}

/// `d` noted in `log` at `now`: its member's open line grown by its step, else a new line (the oldest dropped past
/// KEEP). The line's id.
pub fn decided(log: &mut Vec<CrewDecision>, now: u64, d: Decided) -> String {
    if let Some(e) = open_of(log, d.call).filter(|_| !d.call.is_empty()) {
        if !d.step.is_empty() && e.steps.last() != Some(&d.step) {
            e.steps.push(d.step);
        }
        e.kind = d.kind.to_string();
        e.at_ms = now;
        if !d.sha.is_empty() {
            e.sha = d.sha;
        }
        e.one_way |= d.one_way;
        if !d.why.is_empty() {
            e.why = clip(&d.why, WHY_MAX);
        }
        e.render();
        return e.id.clone();
    }
    // One past the highest of this ms's ids (some may have dropped off): never one in use.
    let same = format!("d{now}-");
    let n = log.iter().filter_map(|e| e.id.strip_prefix(&same)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    let mut e = CrewDecision {
        id: format!("d{now}-{n}"),
        at_ms: now,
        who: d.who.to_string(),
        kind: d.kind.to_string(),
        why: clip(&d.why, WHY_MAX),
        one_way: d.one_way,
        sha: d.sha,
        call: d.call.to_string(),
        head: d.head,
        ..Default::default()
    };
    e.render();
    let id = e.id.clone();
    log.push(e);
    if log.len() > KEEP {
        let extra = log.len() - KEEP;
        log.drain(..extra);
    }
    id
}

/// What the lead is told when you go back on `d`.
pub fn go_back_words(d: &CrewDecision) -> String {
    let then = if d.one_way { "It can't be undone; make a revert instead." } else { "Undo it, then carry on: send it back as a fix if a teammate should redo it." };
    format!("Go back on this: {}: {}. {then}", d.who, d.what.trim_end_matches('.'))
}

/// The lead's first line of `text` (its prompt), as the crew's job.
fn job_of(text: &str) -> String {
    clip(text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(""), JOB_MAX)
}

impl Sessions {
    /// `d` noted on lead `sid`'s crew log (`decided`); its id, None when there's no such session.
    pub fn crew_decided(&mut self, sid: &str, now: u64, d: Decided) -> Option<String> {
        let s = self.0.get_mut(sid)?;
        Some(decided(&mut s.crew_log, now, d))
    }

    /// Whether lead `sid` has an open line (`CrewDecision::open`) about its member of Agent call `call`.
    pub fn crew_open(&self, sid: &str, call: &str) -> bool {
        self.0.get(sid).is_some_and(|s| s.crew_log.iter().any(|d| d.call == call && d.open()))
    }

    /// Lead `sid`'s decision `id`, marked gone back on ("asked"): it, as it was. Err when Cue doesn't know it or you
    /// went back on it already.
    pub fn crew_go_back(&mut self, sid: &str, id: &str) -> Result<CrewDecision, String> {
        let s = self.0.get_mut(sid).ok_or("that session is gone")?;
        let d = s.crew_log.iter_mut().find(|d| d.id == id).ok_or("Cue doesn't know that decision")?;
        if !d.back.is_empty() {
            return Err("You went back on this already".into());
        }
        let was = d.clone();
        d.back = "asked".into();
        Ok(was)
    }

    /// Going back on `id` didn't reach the lead: it can be asked again.
    pub fn crew_go_back_failed(&mut self, sid: &str, id: &str) {
        if let Some(d) = self.0.get_mut(sid).and_then(|s| s.crew_log.iter_mut().find(|d| d.id == id)) {
            d.back.clear();
        }
    }

    /// Teammates that just joined lead `sid`'s crew (`before`: whether each of its members until now is still in the
    /// crew; a new one is pushed after them, one started again after a handoff keeps its row): the crew's job set from
    /// the lead's prompt with its first, and one started while others are still in the crew noted as a split.
    pub(super) fn crew_joined(&mut self, sid: &str, before: &[bool], now: u64) {
        let Some(s) = self.0.get_mut(sid) else { return };
        let running = before.iter().any(|in_team| *in_team);
        let new: Vec<Teammate> = s.team.iter().skip(before.len()).cloned().collect();
        for t in new {
            if s.crew_job.is_empty() {
                s.crew_job = job_of(if s.prompt.trim().is_empty() { &t.asked } else { &s.prompt });
            }
            if running {
                let head = format!("Started a new teammate `{}`", t.name);
                decided(&mut s.crew_log, now, Decided { who: "lead", kind: "split", head, why: t.what.clone(), ..Default::default() });
            }
        }
    }

    /// Members of `sid` that finished again (`was_working`: the calls at work until now) while their open line says
    /// fixes went back: that line grows "fixed".
    pub(super) fn crew_fixed(&mut self, sid: &str, was_working: &[String], now: u64) {
        let Some(s) = self.0.get_mut(sid) else { return };
        let done: Vec<Teammate> = s.team.iter().filter(|t| was_working.contains(&t.call) && t.state == "done").cloned().collect();
        for t in done {
            if open_of(&mut s.crew_log, &t.call).is_some_and(|e| e.kind == "fixes") {
                decided(&mut s.crew_log, now, Decided::about(&t, "fixes", "fixed"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Teammate {
        Teammate { call: "c1".into(), name: "rows".into(), ..Default::default() }
    }

    #[test]
    fn a_review_then_its_fixes_and_commit_are_one_line_that_grows() {
        let mut log = vec![];
        let t = rows();
        let id = decided(&mut log, 10, Decided { head: "Reviewed: 2 should-fix".into(), step: "reviewed again: 2 should-fix".into(), ..Decided::about(&t, "review", "") });
        assert_eq!(id, "d10-1");
        decided(&mut log, 20, Decided::about(&t, "fixes", "sent back"));
        decided(&mut log, 30, Decided::about(&t, "fixes", "fixed"));
        decided(&mut log, 40, Decided { sha: "3f2a1c".into(), ..Decided::about(&t, "commit", "committed to `crew/rows`") });
        assert_eq!(log.len(), 1);
        let e = &log[0];
        assert_eq!((e.what.as_str(), e.kind.as_str(), e.sha.as_str(), e.at_ms, e.id.as_str()), ("Reviewed: 2 should-fix. Sent back, fixed, committed to `crew/rows`", "commit", "3f2a1c", 40, "d10-1"));
        // Committed: closed, so its next review is a new line.
        decided(&mut log, 50, Decided::about(&t, "review", "reviewed: clean"));
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].what, "Reviewed: clean");
    }

    #[test]
    fn the_log_keeps_fifty_and_ids_stay_unique_within_one_ms() {
        let mut log = vec![];
        for _ in 0..60 {
            decided(&mut log, 7, Decided { who: "lead", kind: "split", head: "Started".into(), ..Default::default() });
        }
        assert_eq!(log.len(), KEEP);
        let mut ids: Vec<&str> = log.iter().map(|d| d.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KEEP);
        let long = "x".repeat(300);
        decided(&mut log, 8, Decided { who: "lead", kind: "split", head: long, ..Default::default() });
        assert_eq!(log.last().unwrap().what.chars().count(), WHAT_MAX);
    }

    #[test]
    fn going_back_says_whether_it_can_be_undone() {
        let d = CrewDecision { who: "auth".into(), what: "Committed to `crew/auth`".into(), ..Default::default() };
        assert_eq!(go_back_words(&d), "Go back on this: auth: Committed to `crew/auth`. Undo it, then carry on: send it back as a fix if a teammate should redo it.");
        let d = CrewDecision { one_way: true, ..d };
        assert!(go_back_words(&d).ends_with("It can't be undone; make a revert instead."));
        assert_eq!(job_of("\n  Build the auth flow  \nand more"), "Build the auth flow");
    }
}
