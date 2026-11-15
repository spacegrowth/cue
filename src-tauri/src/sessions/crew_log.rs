//! What a hosted lead's crew did and decided: its job in one line, and a short log, kept on the lead's record (the
//! crew tray's list, the crew page's chat). Two sorts of line:
//! - activity, Cue's own steps (`split`: started a teammate, `review`, `fixes`, `hold`): shown as quiet steps, with
//!   nothing to undo. One member's review that goes on to fixes and a commit is one line that grows, not three.
//! - what can be undone (`UNDO_KINDS`): a commit (`commit`, an old log's `merge`; undone with a revert), a member set
//!   aside (`aside`; brought back), and the lead's own judgment calls (`decided`): each `DECIDED: what — why` line of
//!   its replies (`calls_in`), which Undo this asks it to go back on.
//!
//! Pure: no clock, no process.

use super::team::Teammate;
use super::Sessions;
use serde::{Deserialize, Serialize};

/// The most lines a lead keeps: older ones drop off.
const KEEP: usize = 50;
/// The most a step's `what` and `why` say, and the crew's job.
const WHAT_MAX: usize = 110;
const WHY_MAX: usize = 90;
const JOB_MAX: usize = 140;
/// The lead's judgment calls (`DECIDED:` lines): at most this many from one reply (more lines are passed over, so a
/// reply that lists every step can't flood the log), each its choice cut to CALL_WHAT_MAX characters and its reason
/// to CALL_WHY_MAX (longer than a step's: the reason is the point of one). The log's KEEP holds for them too.
pub const CALLS_PER_REPLY: usize = 3;
pub const CALL_WHAT_MAX: usize = 140;
pub const CALL_WHY_MAX: usize = 160;
/// The kinds a line can be undone as (Undo this, or Make a revert for a one-way commit); the window's team.js
/// `CREW_UNDO` says the same. Every other kind is activity: a step, with nothing to undo.
pub const UNDO_KINDS: &[&str] = &["commit", "merge", "aside", "decided"];
/// The word that starts one of the lead's judgment calls.
const CALL_MARK: &str = "decided";

/// One thing the crew decided on its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrewDecision {
    /// "d<at_ms>-<n>": unique within its lead.
    pub id: String,
    /// When it was decided, or last grew.
    pub at_ms: u64,
    /// The member it's about, or "lead".
    pub who: String,
    /// What it is now (a review that went on to a commit is one). Activity: split · review · fixes · hold. Undoable
    /// (UNDO_KINDS): commit · merge (old logs) · aside · decided (the lead's judgment call).
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
    /// Whether Undo this can go back on it (UNDO_KINDS); activity can't.
    pub fn undoable(&self) -> bool {
        UNDO_KINDS.contains(&self.kind.as_str())
    }

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
    let mut e = CrewDecision {
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
    push(log, e)
}

/// `e` added to `log` under a new id, the oldest dropped past KEEP. Its id.
fn push(log: &mut Vec<CrewDecision>, mut e: CrewDecision) -> String {
    // One past the highest of this ms's ids (some may have dropped off): never one in use.
    let same = format!("d{}-", e.at_ms);
    let n = log.iter().filter_map(|x| x.id.strip_prefix(&same)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    e.id = format!("d{}-{n}", e.at_ms);
    let id = e.id.clone();
    log.push(e);
    if log.len() > KEEP {
        let extra = log.len() - KEEP;
        log.drain(..extra);
    }
    id
}

/// The lead's judgment calls in `text`: each line `DECIDED: what it chose — why` (any case; a list mark or bold around
/// it is fine; the reason after an em or en dash, or " -- "; none is fine), as (what, why), uncut. The instructions'
/// own placeholder, said back, isn't one; nor is a line in a code block (``` or ~~~ fences) or a `>` quote: those
/// show or cite one, they don't make it.
pub fn calls_in(text: &str) -> Vec<(String, String)> {
    let mut fenced = false;
    text.lines().filter_map(|line| {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            return None;
        }
        if fenced || t.starts_with('>') {
            return None;
        }
        let l = t.trim_start_matches(['-', '*', '`', ' ']).trim();
        let head = l.get(..CALL_MARK.len())?;
        if !head.eq_ignore_ascii_case(CALL_MARK) {
            return None;
        }
        let body = l[CALL_MARK.len()..].trim_start().strip_prefix(':')?.trim().trim_start_matches("**").trim().trim_end_matches(['*', '`']).trim();
        let (what, why) = ["—", "–", " -- "].iter().find_map(|sep| body.split_once(sep)).unwrap_or((body, ""));
        let (what, why) = (what.trim().trim_end_matches(['.', ',', ';']).trim(), why.trim());
        let placeholder = what.starts_with('<') || ["what you chose", "what it chose"].iter().any(|p| what.eq_ignore_ascii_case(p));
        (!what.is_empty() && !placeholder).then(|| (what.to_string(), why.to_string()))
    }).collect()
}

/// The lead's judgment calls in its reply `text`, said at `at` (the reply's time), noted in `log`: at most
/// CALLS_PER_REPLY of them, each cut (CALL_WHAT_MAX, CALL_WHY_MAX); one it noted already (the same choice, any case)
/// isn't noted twice. How many it noted.
pub fn calls_noted(log: &mut Vec<CrewDecision>, at: u64, text: &str) -> usize {
    let same = |a: &str, b: &str| a.trim_end_matches('.').eq_ignore_ascii_case(b.trim_end_matches('.'));
    let mut noted = 0;
    for (what, why) in calls_in(text).into_iter().take(CALLS_PER_REPLY) {
        let what = clip(&what, CALL_WHAT_MAX);
        if log.iter().any(|d| d.kind == "decided" && same(&d.what, &what)) {
            continue;
        }
        let e = CrewDecision { at_ms: at, who: "lead".into(), kind: "decided".into(), head: what.clone(), what, why: clip(&why, CALL_WHY_MAX), ..Default::default() };
        push(log, e);
        noted += 1;
    }
    noted
}

/// What the lead is told when you go back on `d` (one of UNDO_KINDS): a commit reverted (one-way: with a revert
/// commit, never by rewriting what may be shared), a member set aside brought back, a judgment call taken back.
pub fn go_back_words(d: &CrewDecision) -> String {
    let what = d.what.trim_end_matches('.');
    let sha = if d.sha.is_empty() { String::new() } else { format!(" (`{}`)", d.sha) };
    match d.kind.as_str() {
        "decided" => {
            let why = if d.why.is_empty() { String::new() } else { format!(" (your reason: {})", d.why.trim_end_matches('.')) };
            format!("Go back on your call: you decided {what}{why}. Undo it and take the other way, then carry on; say what that changes.")
        }
        "aside" => format!("Bring {} back: it was set aside ({what}). Pick its part up again: start a teammate for it, from its worktree if its work is still there.", d.who),
        _ if d.one_way => format!("Make a revert: {}: {what}{sha}. It can't be undone in place: make a revert commit, don't rewrite history.", d.who),
        _ => format!("Undo this: {}: {what}{sha}. Revert that commit, then carry on: send it back as a fix if a teammate should redo it.", d.who),
    }
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

    /// Lead `sid`'s judgment calls in its reply `text`, said at `at` (`calls_noted`). How many it noted.
    pub fn crew_calls(&mut self, sid: &str, at: u64, text: &str) -> usize {
        self.0.get_mut(sid).map_or(0, |s| calls_noted(&mut s.crew_log, at, text))
    }

    /// Lead `sid`'s decision `id`, marked gone back on ("asked"): it, as it was. Err when Cue doesn't know it, it's
    /// activity (nothing to undo), or you went back on it already.
    pub fn crew_go_back(&mut self, sid: &str, id: &str) -> Result<CrewDecision, String> {
        let s = self.0.get_mut(sid).ok_or("that session is gone")?;
        let d = s.crew_log.iter_mut().find(|d| d.id == id).ok_or("Cue doesn't know that decision")?;
        if !d.undoable() {
            return Err("That's a step it took: there's nothing to undo".into());
        }
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
    fn going_back_says_what_undoing_it_means() {
        let d = CrewDecision { who: "auth".into(), kind: "commit".into(), what: "Committed to `crew/auth`".into(), sha: "3f2a1c".into(), ..Default::default() };
        assert_eq!(go_back_words(&d), "Undo this: auth: Committed to `crew/auth` (`3f2a1c`). Revert that commit, then carry on: send it back as a fix if a teammate should redo it.");
        let one = CrewDecision { one_way: true, ..d.clone() };
        assert!(go_back_words(&one).starts_with("Make a revert: auth:") && go_back_words(&one).ends_with("make a revert commit, don't rewrite history."));
        let aside = CrewDecision { kind: "aside".into(), what: "Set aside: it stopped having written nothing".into(), ..d.clone() };
        assert!(go_back_words(&aside).starts_with("Bring auth back: it was set aside (Set aside: it stopped having written nothing)."));
        let call = CrewDecision { who: "lead".into(), kind: "decided".into(), what: "Kept the old cache".into(), why: "the new one needs a migration".into(), ..Default::default() };
        assert_eq!(go_back_words(&call), "Go back on your call: you decided Kept the old cache (your reason: the new one needs a migration). Undo it and take the other way, then carry on; say what that changes.");
        assert_eq!(job_of("\n  Build the auth flow  \nand more"), "Build the auth flow");
    }

    #[test]
    fn decided_lines_are_read_off_the_leads_reply() {
        let text = "All three are in.\nDECIDED: Kept per-key limits over per-IP — keys are what the docs promise\n- **Decided: skipped the flaky date test -- it needs a fixed clock first**\nDecided: cut the CSV export\nWe decided: not at the line's start\nDECIDED: <what it chose> — <why>\nDECIDED:";
        assert_eq!(calls_in(text), [
            ("Kept per-key limits over per-IP".to_string(), "keys are what the docs promise".to_string()),
            ("skipped the flaky date test".to_string(), "it needs a fixed clock first".to_string()),
            ("cut the CSV export".to_string(), String::new()),
        ]);
        assert!(calls_in("DECISION: one-way | why: the add-on's").is_empty(), "the add-on's own DECISION lines aren't these");
        // Shown in a code block or quoted: not made here. After the block, lines count again.
        let shown = "The form is:\n```\nDECIDED: kept the cache — it's warm\n```\n~~~text\nDECIDED: in tildes — too\n~~~\n> DECIDED: quoted from the user — not mine\n>DECIDED: quoted tight — not mine either\nDECIDED: after the block — it counts";
        assert_eq!(calls_in(shown), [("after the block".to_string(), "it counts".to_string())]);
    }

    #[test]
    fn a_reply_notes_at_most_three_calls_each_cut_and_never_one_twice() {
        let mut log = vec![];
        let reply = (1..=5).map(|i| format!("DECIDED: choice {i} — reason {i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(calls_noted(&mut log, 100, &reply), CALLS_PER_REPLY);
        assert_eq!(log.iter().map(|d| d.what.as_str()).collect::<Vec<_>>(), ["choice 1", "choice 2", "choice 3"]);
        assert!(log.iter().all(|d| d.kind == "decided" && d.who == "lead" && d.at_ms == 100 && d.call.is_empty() && d.undoable()));
        // Said again in a later reply: not twice; a new one is.
        assert_eq!(calls_noted(&mut log, 200, "DECIDED: Choice 1. — still\nDECIDED: choice 6 — new"), 1);
        assert_eq!((log.len(), log[3].what.as_str(), log[3].at_ms), (4, "choice 6", 200));
        let long = format!("DECIDED: {} — {}", "w".repeat(400), "y".repeat(400));
        calls_noted(&mut log, 300, &long);
        let e = log.last().unwrap();
        assert_eq!((e.what.chars().count(), e.why.chars().count()), (CALL_WHAT_MAX, CALL_WHY_MAX));
    }

    #[test]
    fn only_commits_set_asides_and_judgment_calls_can_be_undone() {
        let kind = |k: &str| CrewDecision { kind: k.into(), ..Default::default() }.undoable();
        for k in ["commit", "merge", "aside", "decided"] {
            assert!(kind(k), "{k} can be undone");
        }
        for k in ["split", "review", "fixes", "hold", ""] {
            assert!(!kind(k), "{k} is activity");
        }
        // A review that grew into a commit is a commit now: it can be.
        let mut log = vec![];
        let t = rows();
        decided(&mut log, 10, Decided::about(&t, "review", "reviewed: clean"));
        assert!(!log[0].undoable());
        decided(&mut log, 20, Decided::about(&t, "commit", "committed"));
        assert!(log[0].undoable());
    }
}
