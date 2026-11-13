//! A crew member's review (hub/team.rs has the rest of the crew): asked of its lead when the member finishes (through
//! Review's queue, hub/reviews.rs), read off the lead's turn that answers the ask, filed to the member the ask named,
//! and what Cue does next by itself (commit a clean one, send findings back, or leave it to you). A member whose
//! review never comes stops waiting, so none stays "under review" (counted at work) for good.

use super::team::{changes, first, KEY};
use super::team_tidy::commit_reply;
use super::*;
use crate::sessions::team::{fix_findings, read_review, review_ask_name, Teammate};

/// How many times Cue sends a teammate its findings by itself before the next review waits for you.
pub(crate) const AUTO_FIX_ROUNDS: u32 = 2;

/// An ask in the transcript counts as the one that went at `review_ms` when it was written no earlier than this
/// before it (Cue notes the time just after the ask goes; Claude Code writes it a moment later, on the same Mac).
const ASK_SLACK_MS: u64 = 5000;

/// A lead's turn as Cue reads it for its crew.
pub(super) struct LeadTurn<'a> {
    /// The reply that answers Cue's asks (`answer_in`: not a stop hook's audit after it).
    pub answer: &'a str,
    /// Everything it wrote this turn: a `Committed <name>:` line anywhere in it counts.
    pub all: String,
    /// What was said to it this turn (its prompt, and what came mid-turn).
    pub asks: Vec<String>,
    /// Everything said to it in its transcript's tail, this turn's too, with when it was written.
    pub said: Vec<(String, u64)>,
    /// A turn nothing said to it started (a teammate finishing).
    pub unprompted: bool,
}

impl<'a> LeadTurn<'a> {
    /// The turn that just ended with `last`, its texts `parts`, read with its transcript.
    pub fn read(transcript: &str, parts: &'a [crate::model::TurnPart], last: &'a str, unprompted: bool) -> Self {
        let answer = crate::sessions::team::answer_in(parts.iter().map(|p| p.text.as_str()), last);
        let mut all: Vec<&str> = parts.iter().map(|p| p.text.as_str()).collect();
        if !all.contains(&last) {
            all.push(last);
        }
        let (asks, said) = crate::hook::said_to(transcript);
        LeadTurn { answer, all: all.join("\n"), asks, said, unprompted }
    }
}

impl Hub {
    /// The lead's turn ended: a prompted reply with a Recommendation line is a crew member's review (`reviewed_member`);
    /// `Committed <name>:` lines settle the members they name (`commit_reply`); and a member whose review ask the lead
    /// has had, with no review back, stops waiting (`unanswered`). A turn nothing of yours started (a teammate
    /// finishing) is no review, and no answer to an ask; only its `Committed <name>: <sha>` lines count.
    pub(super) fn team_reply(&self, lead: &str, turn: &LeadTurn) {
        let (findings, recommendation) = read_review(turn.answer);
        let review = !turn.unprompted && !recommendation.is_empty();
        let filed = if review { self.file_review(lead, turn, findings, recommendation) } else { None };
        let settled = self.write(|st| commit_reply(st, lead, turn, review));
        let stale = !turn.unprompted && self.write(|st| unanswered(st, lead, &turn.said));
        if filed.is_some() || settled || stale {
            self.changed();
        }
        let Some((call, next)) = filed else { return };
        let lead = lead.to_string();
        match next {
            AutoNext::Commit => self.later(0, move |h| h.commit_when_clean(&lead, &call)),
            AutoNext::Fixes => self.later(0, move |h| {
                if let Err(e) = h.team_action(&lead, &call, "fixes") {
                    h.set_teammate(&lead, &call, |m| m.step = format!("Fixes not sent by themselves: {e}"));
                }
            }),
            AutoNext::Ask => {}
        }
    }

    /// The review in `turn` filed to its member, and what Cue does next with it; None when it's nobody's.
    fn file_review(&self, lead: &str, turn: &LeadTurn, findings: Vec<String>, recommendation: String) -> Option<(String, AutoNext)> {
        self.write(|st| {
            let (t, stale) = reviewed_member(&st.sessions.team(lead), &turn.asks)?;
            for call in stale {
                st.sessions.with_teammate(lead, &call, didnt_come_back);
            }
            let next = auto_next(&st.sessions.crew_commit(lead), &recommendation, &findings, t.fix_rounds);
            st.sessions.with_teammate(lead, &t.call, |m| {
                m.state = "reviewed".into();
                m.findings = findings;
                m.recommendation = recommendation;
                m.review = turn.answer.to_string();
            });
            Some((t.call, next))
        })
    }

    /// A crew member that just finished, reviewed by itself through its lead's review queue: only one that finished its
    /// work (not stopped), and not a loop's (a loop reviews its own crew).
    pub(super) fn review_on_finish(&self, lead: &str, origin: &Origin, call: &str) {
        #[cfg(feature = "ext")]
        if self.ext_leads_loop(lead) {
            return;
        }
        let Some(t) = self.read(|st| st.sessions.team(lead)).into_iter().find(|t| t.call == call) else { return };
        if t.state != "done" || t.agent_id.is_empty() || !t.from.is_empty() {
            return;
        }
        if let Err(e) = self.review_teammate(lead, origin, &t) {
            eprintln!("cue: couldn't queue {}'s review: {e}", t.name);
        }
    }

    /// The lead's "commit when clean": a crew member whose review recommends commit goes on a branch of its own (never
    /// main); held or refused, it stays reviewed with Commit for you, and why is on its row.
    fn commit_when_clean(&self, lead: &str, call: &str) {
        let (origin, t) = self.read(|st| (st.sessions.origin(lead), st.sessions.team(lead).into_iter().find(|t| t.call == call)));
        let (Some(origin), Some(t)) = (origin, t) else { return };
        if t.state != "reviewed" {
            return;
        }
        if let Err(e) = self.commit_teammate(lead, &origin, &t, true) {
            self.set_teammate(lead, call, |m| m.step = format!("Not committed by itself: {e}"));
        }
    }

    /// Review: the lead reviews the teammate's work through a fork, sent when the lead is free (Review's queue, one
    /// review at a time per lead; queued, the member is marked so in the same step, hub/reviews.rs `take_review`). Its
    /// reply comes back as the teammate's review (`team_reply`).
    pub(super) fn review_teammate(&self, lead: &str, origin: &Origin, t: &Teammate) -> Result<String, String> {
        if t.agent_id.is_empty() || t.working() {
            return Err(format!("{} is still working", t.name));
        }
        let files = changes(&origin.machine, &origin.cwd, t).map(|c| c.files()).unwrap_or_else(|_| t.files.clone());
        let place = if t.worktree.is_empty() { String::new() } else { format!(" in its worktree {}", t.worktree) };
        let text = format!(
            "Review your teammate {name}'s work with a fork (one Agent call, not in the background): read the diff of its files{place} against what it was asked, run its tests, and reply with only findings that need a change, numbered (`1. file:line — should-fix — what`; no notes that something is fine), and a last line `Recommendation: commit` or `Recommendation: send fixes`. Don't change, commit or message anything.\nWhat it was asked: {what}\nIts files: {files}",
            name = t.name,
            what = if t.what.is_empty() { first(&t.asked) } else { t.what.clone() },
            files = if files.is_empty() { "(none yet)".to_string() } else { files.join(", ") },
        );
        self.review(lead, &format!("{KEY}{lead}:{}", t.agent_id), &text)
    }

    /// A teammate's review typed into its lead (now, or from the queue): it's under review from here.
    pub(super) fn team_review_sent(&self, key: &str, sent: bool) {
        let Some((lead, agent)) = super::team::teammate_key(key) else { return };
        self.set_teammate(lead, agent, |m| {
            m.state = if sent { "reviewing" } else { "done" }.into();
            m.review_ms = if sent { now_ms() } else { 0 };
        });
    }
}

/// A member under review whose review isn't coming: done again, with Review in its menu, its row saying why.
pub(super) fn didnt_come_back(m: &mut Teammate) {
    m.state = "done".into();
    m.review_ms = 0;
    m.step = "Its review didn't come back".into();
    m.step_ms = now_ms();
}

/// After a prompted turn of lead `lead` ended: each member still under review whose ask the lead has had (in its
/// transcript, `said`, written since that ask went) didn't get its review from that turn, and stops waiting. One
/// whose ask isn't there yet (sent, not taken) waits on. Whether any changed.
/// SHORTCUT: needs the lead's transcript on this Mac (a lead on another machine has none here); the tidy's
/// REVIEW_SHOWN_MS fallback (team_tidy.rs) covers that, slower. Upgrade: the turn's asks from the stream itself.
fn unanswered(st: &mut Store, lead: &str, said: &[(String, u64)]) -> bool {
    let had = |t: &Teammate| said.iter().any(|(text, at)| review_ask_name(text).as_deref() == Some(t.name.as_str()) && at + ASK_SLACK_MS >= t.review_ms);
    let stale: Vec<String> = st.sessions.team(lead).into_iter().filter(|t| t.state == "reviewing" && had(t)).map(|t| t.call).collect();
    for call in &stale {
        st.sessions.with_teammate(lead, call, didnt_come_back);
    }
    !stale.is_empty()
}

/// What Cue does by itself once a teammate's review is in.
#[derive(Debug, PartialEq)]
enum AutoNext {
    Commit,
    Fixes,
    Ask,
}

/// With the lead handling reviews (`crew_commit` "", the default; "wait": ask you every time): a review that asks for
/// fixes, or says commit but lists a finding that needs a change (`fix_findings`), goes back to the teammate, up to
/// `AUTO_FIX_ROUNDS` times; a commit whose findings are only notes (or none) is committed to its branch. Anything
/// else waits for you.
fn auto_next(crew_commit: &str, recommendation: &str, findings: &[String], fix_rounds: u32) -> AutoNext {
    if crew_commit == "wait" {
        return AutoNext::Ask;
    }
    let commit = recommendation.starts_with("commit");
    if recommendation.contains("fix") || (commit && fix_findings(findings) > 0) {
        return if fix_rounds < AUTO_FIX_ROUNDS { AutoNext::Fixes } else { AutoNext::Ask };
    }
    if commit { AutoNext::Commit } else { AutoNext::Ask }
}

/// The crew member a review reply is about, of `team`'s members under review: the one the turn's ask named ("Review
/// your teammate <name>'s work", the last of the turn's `asks`), else the one asked longest ago. Named, the members
/// asked before it whose answer never came (lost to a reply after it, say) are `stale`: they wait no more. Named but
/// no longer under review: None.
pub(super) fn reviewed_member(team: &[Teammate], asks: &[String]) -> Option<(Teammate, Vec<String>)> {
    let reviewing: Vec<&Teammate> = team.iter().filter(|t| t.state == "reviewing").collect();
    if let Some(name) = asks.iter().rev().find_map(|a| review_ask_name(a)) {
        // Named, but no longer under review (its work committed meanwhile, say): nobody else's review.
        let t = reviewing.iter().find(|t| t.name == name)?;
        let stale = reviewing.iter().filter(|o| o.call != t.call && o.review_ms <= t.review_ms).map(|o| o.call.clone()).collect();
        return Some(((*t).clone(), stale));
    }
    reviewing.into_iter().min_by_key(|t| t.review_ms).map(|t| (t.clone(), vec![]))
}

#[cfg(test)]
mod tests {
    use super::{auto_next, reviewed_member, unanswered, AutoNext};
    use crate::sessions::team::Teammate;

    fn mate(name: &str, state: &str, review_ms: u64) -> Teammate {
        Teammate { call: format!("call-{name}"), name: name.into(), state: state.into(), review_ms, ..Default::default() }
    }

    #[test]
    fn a_review_is_the_named_members_and_the_ones_asked_before_it_whose_answer_never_came_wait_no_more() {
        let team = [mate("loops-board", "reviewing", 10), mate("rows", "reviewing", 20), mate("docs", "reviewing", 30), mate("hub", "done", 0)];
        let asks = ["Fix the tray.".to_string(), "Review your teammate rows's work with a fork (one Agent call…)".to_string()];
        let (t, stale) = reviewed_member(&team, &asks).unwrap();
        assert_eq!((t.name.as_str(), stale), ("rows", vec!["call-loops-board".to_string()]), "docs, asked after it, still waits");
        let (t, stale) = reviewed_member(&team, &["Go ahead".to_string()]).unwrap();
        assert_eq!((t.name.as_str(), stale.len()), ("loops-board", 0), "no ask named one: the one asked longest ago, nothing let go");
        assert!(reviewed_member(&[mate("hub", "done", 0)], &asks).is_none());
        assert!(reviewed_member(&[mate("docs", "reviewing", 30)], &asks).is_none(), "rows's review, rows no longer under review: not docs's");
    }

    #[test]
    fn handled_by_cue_notes_alone_dont_go_back_but_a_should_fix_does() {
        let f = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let notes = f(&["1. a.rs:1 — note — verified fine", "2. a.rs:2 — nit — fine", "3. b.rs — question — why?", "4. no kind at all"]);
        assert_eq!(auto_next("", "commit", &notes, 0), AutoNext::Commit, "notes only: committed");
        assert_eq!(auto_next("", "commit", &f(&["1. a.rs:1 — should-fix — x", "2. a.rs:2 — note — y"]), 0), AutoNext::Fixes);
        assert_eq!(auto_next("", "commit", &f(&["1. a.rs:1 — Blocker — x"]), 0), AutoNext::Fixes, "any case");
        assert_eq!(auto_next("", "commit", &f(&["1. a.rs — should-fix now — x"]), 0), AutoNext::Commit, "not an exact kind: a note, as the window counts it");
        assert_eq!(auto_next("", "send fixes", &notes, 0), AutoNext::Fixes, "asked for fixes: they go back");
        assert_eq!(auto_next("", "send fixes", &[], 2), AutoNext::Ask, "after two rounds: yours");
        assert_eq!(auto_next("wait", "commit", &[], 0), AutoNext::Ask);
        assert_eq!(auto_next("", "looks good", &[], 0), AutoNext::Ask, "never Commit on a guess");
    }

    #[test]
    fn a_member_stops_waiting_once_the_lead_had_its_current_ask_not_an_older_one() {
        let mut st = crate::hub::Store::default();
        let lead = crate::model::Origin { session_id: "L".into(), harness: "claude".into(), hosted: true, ..Default::default() };
        st.sessions.mark(&lead, "stopped", None);
        for t in [mate("rows", "reviewing", 100_000), mate("docs", "reviewing", 200_000)] {
            st.sessions.add_teammate("L", t);
        }
        let ask = |n: &str| format!("Review your teammate {n}'s work with a fork");
        // rows's ask from an earlier round, long before its current one went: no answer to that one yet.
        assert!(!unanswered(&mut st, "L", &[(ask("rows"), 10_000)]));
        assert!(unanswered(&mut st, "L", &[(ask("rows"), 99_000), ("Fix it".into(), 150_000)]), "its ask, a moment after Cue noted it");
        let team = st.sessions.team("L");
        assert_eq!(team.iter().map(|t| t.state.as_str()).collect::<Vec<_>>(), ["done", "reviewing"], "docs's ask isn't the lead's yet");
    }
}
