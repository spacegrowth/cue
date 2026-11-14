//! A crew member's review (hub/team.rs has the rest of the crew): asked of its lead when the member finishes (through
//! Review's queue, hub/reviews.rs), read off the lead's turn that answers the ask, filed to the member the ask named,
//! and what Cue does next by itself (commit a clean one, send findings back, or leave it to you). A member whose
//! review never comes stops waiting, so none stays "under review" (counted at work) for good.

use super::team::{changes, first, KEY};
use super::team_tidy::commit_reply;
use super::*;
use crate::sessions::team::{fix_findings, read_review, review_ask_name, Teammate};

/// How many times Cue sends a teammate its findings by itself before the next review waits for you.
pub(crate) const AUTO_FIX_ROUNDS: u32 = 1;

/// Why a member under review is done again with no review (`didnt_come_back`); a review that names it still counts.
const NOT_BACK: &str = "Its review didn't come back";

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
    /// A turn nothing said to it started (a teammate finishing, or a background review reporting).
    pub unprompted: bool,
    /// A review that names its member (`review_heading`): the member's name and the text, the turn's last such text.
    pub named: Option<(String, &'a str)>,
}

impl<'a> LeadTurn<'a> {
    /// The turn that just ended with `last`, its texts `parts`, read with its transcript.
    pub fn read(transcript: &str, parts: &'a [crate::model::TurnPart], last: &'a str, unprompted: bool) -> Self {
        let answer = crate::sessions::team::answer_in(parts.iter().map(|p| p.text.as_str()), last);
        let mut all: Vec<&str> = parts.iter().map(|p| p.text.as_str()).collect();
        if !all.contains(&last) {
            all.push(last);
        }
        let named = all.iter().rev().find_map(|t| review_heading(t).map(|n| (n, *t)));
        let (asks, said) = crate::hook::said_to(transcript);
        LeadTurn { answer, all: all.join("\n"), asks, said, unprompted, named }
    }
}

/// The member a review names, when `text` is one: a `Review of <name>:` line (the heading Cue's ask gives it) and a
/// Recommendation line. A stop hook's audit after it, or a teammate's news, has neither, so it's never taken for one.
pub(super) fn review_heading(text: &str) -> Option<String> {
    if read_review(text).1.is_empty() {
        return None;
    }
    text.lines().find_map(|l| {
        let rest = l.trim().trim_matches(['*', '#', '`', ' ']).strip_prefix("Review of ")?;
        let (name, _) = rest.split_once(':')?;
        Some(name.trim_matches(['*', '`', ' ']).to_string()).filter(|n| !n.is_empty())
    })
}

impl Hub {
    /// The lead's turn ended: a review that names its member (`Review of <name>:`, as Cue's ask has it) is that
    /// member's, in any turn, the one a background review's report started too; a prompted reply with a Recommendation
    /// line and no name is the review its ask asked for (`reviewed_member`). `Committed <name>:` lines settle the members
    /// they name (`commit_reply`); and a member whose review ask this turn took, with neither its review nor word that
    /// it runs in the background, stops waiting (`unanswered`). Otherwise a turn nothing of yours started (a teammate
    /// finishing) is no review, and no answer to an ask; only its `Committed <name>: <sha>` lines count.
    pub(super) fn team_reply(&self, lead: &str, turn: &LeadTurn) {
        let answered = !turn.unprompted && !read_review(turn.answer).1.is_empty();
        // A review that names a member no longer waiting for one is nobody else's.
        let filed = match &turn.named {
            Some((name, text)) => self.file_named(lead, name, text),
            None if answered => self.file_review(lead, turn),
            None => None,
        };
        let review = filed.is_some() || answered;
        let settled = self.write(|st| commit_reply(st, lead, turn, review));
        let stale = !turn.unprompted && self.write(|st| unanswered(st, lead, turn));
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
    fn file_review(&self, lead: &str, turn: &LeadTurn) -> Option<(String, AutoNext)> {
        self.write(|st| {
            let (t, stale) = reviewed_member(&st.sessions.team(lead), &turn.asks)?;
            for call in stale {
                st.sessions.with_teammate(lead, &call, didnt_come_back);
            }
            Some((t.call.clone(), file(st, lead, &t, turn.answer)))
        })
    }

    /// Review `text`, which names member `name`, filed to it: one under review, or one that stopped waiting for it
    /// (it came late). What Cue does next with it; None when no such member waits for one.
    fn file_named(&self, lead: &str, name: &str, text: &str) -> Option<(String, AutoNext)> {
        self.write(|st| {
            let late = |t: &Teammate| t.state == "done" && t.review_ms == 0 && t.step == NOT_BACK;
            let t = st.sessions.team(lead).into_iter().find(|t| t.name == name && (t.state == "reviewing" || late(t)))?;
            Some((t.call.clone(), file(st, lead, &t, text)))
        })
    }

    /// Why member `t`, just finished, isn't reviewed by itself under its lead's Review setting; None when it is.
    /// Git is asked only for "only code in a repo" (the default).
    fn why_no_review(&self, lead: &str, origin: &Origin, t: &Teammate) -> Option<&'static str> {
        let setting = self.read(|st| st.sessions.crew_review(lead));
        let in_repo = || changes(&origin.machine, &origin.cwd, t).ok().map(|c| !c.files().is_empty());
        no_review(&setting, in_repo)
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
        if let Some(why) = self.why_no_review(lead, origin, &t) {
            self.set_teammate(lead, call, |m| {
                m.step = why.into();
                m.step_ms = now_ms();
            });
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

    /// Review: the lead reviews the teammate's work through a fork in the background, the ask sent when the lead is free
    /// (Review's queue; queued, the member is marked so in the same step, hub/reviews.rs `take_review`). The lead says
    /// it started it and is free again at once; the fork's report, in the turn it starts, comes back as the teammate's
    /// review, named by its `Review of <name>:` line (`team_reply`).
    pub(super) fn review_teammate(&self, lead: &str, origin: &Origin, t: &Teammate) -> Result<String, String> {
        if t.agent_id.is_empty() || t.working() {
            return Err(format!("{} is still working", t.name));
        }
        let files = changes(&origin.machine, &origin.cwd, t).map(|c| c.files()).unwrap_or_else(|_| t.files.clone());
        let what = if t.what.is_empty() { first(&t.asked) } else { t.what.clone() };
        self.review(lead, &format!("{KEY}{lead}:{}", t.agent_id), &review_words(&t.name, &t.worktree, &what, &files))
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

/// The ask for member `name`'s review: a fork in the background, so the lead is free again at once, and a reply that
/// names the member when the fork reports. Files Cue can't see in a git repository: the fork reads what it says it
/// changed.
fn review_words(name: &str, worktree: &str, what: &str, files: &[String]) -> String {
    let place = if worktree.is_empty() { String::new() } else { format!(" in its worktree {worktree}") };
    let look = if files.is_empty() { "reads the files it says it changed".to_string() } else { format!("reads the diff of its files{place}") };
    let files = if files.is_empty() { "none Cue can see in a git repository".to_string() } else { files.join(", ") };
    format!(
        "Review your teammate {name}'s work with a fork in the background: one Agent call with `run_in_background: true`, then reply only `Reviewing {name} in the background.` and end your turn; don't wait for it. The fork {look} against what it was asked and runs its tests if it has any; it changes, commits and messages nothing. When it reports, reply with a first line `Review of {name}:`, then only findings that need a change, numbered (`1. file:line — should-fix — what`; no notes that something is fine), and a last line `Recommendation: commit` or `Recommendation: send fixes`.\nWhat it was asked: {what}\nIts files: {files}"
    )
}

/// Review `text` filed to member `t` (reviewed), and what Cue does next with it.
fn file(st: &mut Store, lead: &str, t: &Teammate, text: &str) -> AutoNext {
    let (findings, recommendation) = read_review(text);
    let next = auto_next(&st.sessions.crew_commit(lead), &recommendation, &findings, t.fix_rounds);
    st.sessions.with_teammate(lead, &t.call, |m| {
        m.state = "reviewed".into();
        m.findings = findings;
        m.recommendation = recommendation;
        m.review = text.to_string();
    });
    next
}

/// Why a finished member isn't reviewed by itself under the Review `setting` ("always", "never", else only code in a
/// git repository: `in_repo`, whether git shows it changed files in one, None when git couldn't say), as its row says
/// it; None when it is reviewed.
fn no_review(setting: &str, in_repo: impl FnOnce() -> Option<bool>) -> Option<&'static str> {
    match setting {
        "always" => None,
        "never" => Some("Not reviewed: reviews are off for this crew"),
        _ if in_repo() == Some(false) => Some("Not reviewed: no changes Cue can see in a git repo"),
        _ => None,
    }
}

/// A member under review whose review isn't coming: done again, with Review in its menu, its row saying why.
pub(super) fn didnt_come_back(m: &mut Teammate) {
    m.state = "done".into();
    m.review_ms = 0;
    m.step = NOT_BACK.into();
    m.step_ms = now_ms();
}

/// After a prompted turn of lead `lead` ended: each member still under review whose current ask this turn took (one of
/// its `asks`, in its transcript, `said`, written since that ask went) got neither its review nor word that the lead
/// runs it in the background (`Reviewing <name>`), and stops waiting. One whose ask isn't there yet (sent, not
/// taken), or one an earlier turn took (its fork still out), waits on. Whether any changed.
/// SHORTCUT: needs the lead's transcript on this Mac (a lead on another machine has none here); the tidy's
/// REVIEW_SHOWN_MS fallback (team_tidy.rs) covers that, and a background review that never reports, slower. Upgrade:
/// the turn's asks, and the Agent calls it started, from the stream itself.
fn unanswered(st: &mut Store, lead: &str, turn: &LeadTurn) -> bool {
    let all = turn.all.to_lowercase();
    let took = |t: &Teammate| {
        let mine = |text: &str| review_ask_name(text).as_deref() == Some(t.name.as_str());
        turn.asks.iter().any(|a| mine(a)) && turn.said.iter().any(|(text, at)| mine(text) && at + ASK_SLACK_MS >= t.review_ms)
    };
    let started = |t: &Teammate| all.contains(&format!("reviewing {}", t.name.to_lowercase()));
    let stale: Vec<String> = st.sessions.team(lead).into_iter().filter(|t| t.state == "reviewing" && took(t) && !started(t)).map(|t| t.call).collect();
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
    use super::{auto_next, no_review, review_heading, review_words, reviewed_member, unanswered, AutoNext, LeadTurn};
    use crate::model::TurnPart;
    use crate::sessions::team::{review_ask_name, Teammate};

    fn mate(name: &str, state: &str, review_ms: u64) -> Teammate {
        Teammate { call: format!("call-{name}"), name: name.into(), state: state.into(), review_ms, ..Default::default() }
    }

    /// A prompted turn: what was said to the lead in it (`asks`), its transcript's tail (`said`), and what it wrote.
    fn turn<'a>(asks: &[String], said: &[(String, u64)], all: &str) -> LeadTurn<'a> {
        LeadTurn { answer: "", all: all.into(), asks: asks.to_vec(), said: said.to_vec(), unprompted: false, named: None }
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
        assert_eq!(auto_next("", "send fixes", &[], 1), AutoNext::Ask, "after one round: yours");
        assert_eq!(auto_next("wait", "commit", &[], 0), AutoNext::Ask);
        assert_eq!(auto_next("", "looks good", &[], 0), AutoNext::Ask, "never Commit on a guess");
    }

    #[test]
    fn a_member_stops_waiting_once_the_turn_that_took_its_current_ask_neither_reviews_it_nor_starts_it() {
        let mut st = crate::hub::Store::default();
        let lead = crate::model::Origin { session_id: "L".into(), harness: "claude".into(), hosted: true, ..Default::default() };
        st.sessions.mark(&lead, "stopped", None);
        for t in [mate("rows", "reviewing", 100_000), mate("docs", "reviewing", 200_000)] {
            st.sessions.add_teammate("L", t);
        }
        let ask = |n: &str| format!("Review your teammate {n}'s work with a fork in the background");
        // rows's ask from an earlier round, long before its current one went: no answer to that one yet.
        assert!(!unanswered(&mut st, "L", &turn(&[ask("rows")], &[(ask("rows"), 10_000)], "Done.")));
        let said = [(ask("rows"), 99_000), ("Fix it".into(), 150_000)];
        // A turn after the one that took it (you talking to the lead while its fork runs): rows waits on.
        assert!(!unanswered(&mut st, "L", &turn(&["Fix it".into()], &said, "Fixed it.")));
        // The turn that took it says the review runs in the background: it waits on.
        assert!(!unanswered(&mut st, "L", &turn(&[ask("rows")], &said, "Reviewing rows in the background.")));
        assert!(unanswered(&mut st, "L", &turn(&[ask("rows")], &said, "Sure.")), "its ask, a moment after Cue noted it, and nothing started");
        let team = st.sessions.team("L");
        assert_eq!(team.iter().map(|t| t.state.as_str()).collect::<Vec<_>>(), ["done", "reviewing"], "docs's ask isn't the lead's yet");
    }

    #[test]
    fn the_review_ask_runs_the_fork_in_the_background_and_asks_for_a_reply_that_names_the_member() {
        let ask = review_words("rows", "", "Fix the rows", &["ui/rows.js".into()]);
        assert_eq!(review_ask_name(&ask).as_deref(), Some("rows"), "the window and the turn still read whose it is");
        assert!(ask.starts_with("Review your teammate rows's work with a fork"), "the chat folds it as Cue's ask (ui/team.js)");
        assert!(ask.contains("`run_in_background: true`") && ask.contains("`Review of rows:`") && ask.contains("Its files: ui/rows.js"));
        assert!(!ask.contains("not in the background"));
        let outside = review_words("mocks", "", "Draw 10 mocks", &[]);
        assert!(outside.contains("reads the files it says it changed") && !outside.contains("diff") && !outside.contains("(none yet)"), "{outside}");
    }

    #[test]
    fn a_review_names_its_member_by_its_heading_and_an_audit_after_it_is_no_review() {
        assert_eq!(review_heading("Review of rows:\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes").as_deref(), Some("rows"));
        assert_eq!(review_heading("**Review of docs:**\nRecommendation: commit").as_deref(), Some("docs"));
        assert_eq!(review_heading("Review of rows: nothing to fix.\n\nRecommendation: commit").as_deref(), Some("rows"), "words after the heading");
        assert!(review_heading("Review of rows:\n1. a.rs:1 — note — fine").is_none(), "no Recommendation line: not a review yet");
        assert!(review_heading("1. a.rs:1 — note — fine\nRecommendation: commit").is_none(), "names no member");
        // The notification turn: the fork's report, then the lead's review, then a stop hook's audit.
        let parts = [("rows reported back.", false), ("Review of rows:\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes", false), ("Audit: 1. checked the rules. Nothing else to do.", true)]
            .map(|(t, hooked)| TurnPart { text: t.into(), hooked });
        let last = parts[2].text.clone();
        let t = LeadTurn::read("", &parts, &last, true);
        assert_eq!(t.named.as_ref().map(|(n, text)| (n.as_str(), text.starts_with("Review of rows:"))), Some(("rows", true)), "the review, not the audit");
        let audit_only = [TurnPart { text: "Audit: 1. a.rs:1 — note — fine. Review of rows: done.".into(), hooked: true }];
        assert!(LeadTurn::read("", &audit_only, &audit_only[0].text, true).named.is_none());
    }

    #[test]
    fn the_review_setting_says_which_finished_members_are_reviewed_by_themselves() {
        let never_asked = || -> Option<bool> { panic!("git isn't asked") };
        assert_eq!(no_review("always", never_asked), None);
        assert_eq!(no_review("never", never_asked), Some("Not reviewed: reviews are off for this crew"));
        assert_eq!(no_review("", || Some(true)), None, "only code in a repo, the default: files changed in one");
        assert_eq!(no_review("", || Some(false)), Some("Not reviewed: no changes Cue can see in a git repo"));
        assert_eq!(no_review("", || None), None, "git couldn't say: reviewed, as before");
    }
}
