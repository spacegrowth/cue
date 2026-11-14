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
    /// Its texts in order: each `Review of <name>` section in them is that member's review (`named_reviews`).
    pub texts: Vec<&'a str>,
    /// Unprompted: the member whose review ask started the background Agent call whose report started this turn
    /// (`background_of`), when it was one.
    pub background: Option<String>,
}

impl<'a> LeadTurn<'a> {
    /// The turn that just ended with `last`, its texts `parts`, read with its transcript.
    pub fn read(transcript: &str, parts: &'a [crate::model::TurnPart], last: &'a str, unprompted: bool) -> Self {
        let answer = crate::sessions::team::answer_in(parts.iter().map(|p| p.text.as_str()), last);
        let mut texts: Vec<&str> = parts.iter().map(|p| p.text.as_str()).collect();
        if !texts.contains(&last) {
            texts.push(last);
        }
        let (asks, said) = crate::hook::said_to(transcript);
        let background = if unprompted && !transcript.is_empty() { background_of(&crate::hook::read_tail(transcript)) } else { None };
        LeadTurn { answer, all: texts.join("\n"), asks, said, unprompted, texts, background }
    }
}

/// The member a `Review of <name>` heading line names, of `names` (the lead's members): any case, after markdown marks
/// (`## `, `**`, backticks), the name followed by anything but more of a name (`:`, `'s work:`, nothing at all). Matched
/// against the names, longest first, so a name with a `:` in it, or one another name starts with, is read whole.
fn heading_name(line: &str, names: &[&str]) -> Option<String> {
    let l = line.trim().trim_start_matches(['#', '*', '_', '`', '>', ' ']);
    let rest = l.get(..10).filter(|p| p.eq_ignore_ascii_case("review of ")).map(|_| &l[10..])?;
    let rest = rest.trim_start_matches(['*', '_', '`', '"', ' ']);
    let mut names: Vec<&str> = names.iter().copied().filter(|n| !n.trim().is_empty()).collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let whole = |after: &str| after.chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '-' || c == '_'));
    names.into_iter().find(|n| rest.get(..n.len()).is_some_and(|p| p.eq_ignore_ascii_case(n)) && whole(&rest[n.len()..])).map(String::from)
}

/// The reviews in a turn's `texts` that name a member of `names`: each text split at its `Review of <name>` heading
/// lines (`heading_name`), each section, from its heading to the next (or the text's end), its member's review when it
/// has a Recommendation line (so a stop hook's audit, or a teammate's news, is never one). A member named twice: its
/// last.
pub(super) fn named_reviews(texts: &[&str], names: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let mut keep = |section: Option<(String, Vec<&str>)>| {
        let Some((name, lines)) = section else { return };
        let text = lines.join("\n").trim_end().to_string();
        if !read_review(&text).1.is_empty() {
            out.retain(|(n, _)| *n != name);
            out.push((name, text));
        }
    };
    for text in texts {
        let mut section: Option<(String, Vec<&str>)> = None;
        for line in text.lines() {
            if let Some(name) = heading_name(line, names) {
                keep(section.take());
                section = Some((name, vec![line]));
            } else if let Some((_, lines)) = section.as_mut() {
                lines.push(line);
            }
        }
        keep(section);
    }
    out
}

/// The text of a transcript's "user" entry; None when it carries tool results.
fn user_text(e: &serde_json::Value) -> Option<String> {
    match e.pointer("/message/content") {
        Some(serde_json::Value::String(t)) => Some(t.clone()),
        Some(serde_json::Value::Array(b)) if !b.iter().any(|b| b["type"] == "tool_result") => Some(b.iter().filter_map(|b| b["text"].as_str()).collect()),
        _ => None,
    }
}

/// The member whose background review the transcript's last task notification reports: the notification names the
/// tool call it ends (`<tool-use-id>`), and that call is an Agent call the lead made after a member's review ask (its
/// prompt, or one taken mid-turn) and before anything else said to it. None when it reports anything else (a teammate
/// finishing, a command).
fn background_of(entries: &[serde_json::Value]) -> Option<String> {
    let mut asked: Option<String> = None;
    let mut forks: Vec<(String, String)> = vec![];
    let mut notified: Option<String> = None;
    for e in entries.iter().filter(|e| e["isSidechain"] != true) {
        match e["type"].as_str().unwrap_or("") {
            "user" => {
                let Some(t) = user_text(e) else { continue };
                if let Some(rest) = t.trim_start().strip_prefix("<task-notification>") {
                    notified = rest.split_once("<tool-use-id>").and_then(|(_, r)| r.split_once("</tool-use-id>")).map(|(id, _)| id.trim().to_string());
                } else if e["isMeta"] != true && !t.trim_start().starts_with('<') {
                    asked = review_ask_name(&t);
                }
            }
            "attachment" => {
                if let Some(name) = e.pointer("/attachment/prompt").and_then(|p| p.as_str()).and_then(review_ask_name) {
                    asked = Some(name);
                }
            }
            "assistant" => {
                let calls = e.pointer("/message/content").and_then(|c| c.as_array()).into_iter().flatten();
                for c in calls.filter(|c| c["type"] == "tool_use" && matches!(c["name"].as_str(), Some("Agent" | "Task"))) {
                    if let (Some(name), Some(id)) = (&asked, c["id"].as_str()) {
                        forks.push((id.to_string(), name.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    let id = notified?;
    forks.into_iter().rev().find(|(call, _)| *call == id).map(|(_, name)| name)
}

/// The reviews in lead `lead`'s `turn` filed to their members, and what Cue does next with each. Each `Review of
/// <name>` section is that member's (`file_named`); an answer with a Recommendation line that names nobody is the
/// review of the member whose ask this (prompted) turn took (`reviewed_member`), or, unprompted, of the only member under
/// review when its background fork's report started the turn (`background_member`). Anything else is nobody's review.
fn file_turn(st: &mut Store, lead: &str, turn: &LeadTurn) -> Vec<(String, AutoNext)> {
    let team = st.sessions.team(lead);
    let names: Vec<&str> = team.iter().map(|t| t.name.as_str()).collect();
    let mut filed: Vec<(String, AutoNext)> = named_reviews(&turn.texts, &names).iter().filter_map(|(name, text)| file_named(st, lead, &team, name, text)).collect();
    // An answer that names a member is no other member's review, even when that one no longer waits for it. (The
    // turn's texts reach back to your last prompt: an earlier report's named review doesn't make this answer named.)
    if read_review(turn.answer).1.is_empty() || !named_reviews(&[turn.answer], &names).is_empty() {
        return filed;
    }
    let team = st.sessions.team(lead);
    let found = if turn.unprompted { background_member(&team, turn.background.as_deref()) } else { reviewed_member(&team, &turn.asks) };
    let Some((t, stale)) = found else { return filed };
    for call in stale {
        st.sessions.with_teammate(lead, &call, didnt_come_back);
    }
    filed.push((t.call.clone(), file(st, lead, &t, turn.answer)));
    filed
}

/// Review `text`, which names member `name`, filed to it: one under review, or one that stopped waiting for it (it came
/// late). What Cue does next with it; None when no such member waits for one.
fn file_named(st: &mut Store, lead: &str, team: &[Teammate], name: &str, text: &str) -> Option<(String, AutoNext)> {
    let late = |t: &Teammate| t.state == "done" && t.review_ms == 0 && t.step == NOT_BACK;
    let t = team.iter().find(|t| t.name == name && (t.state == "reviewing" || late(t)))?;
    Some((t.call.clone(), file(st, lead, t, text)))
}

/// The member an unnamed review in a turn a task's report started is about: the one whose background fork the report
/// is (`background`, the member whose ask started the call reporting), while it's under review, others under review or
/// not. Else None.
fn background_member(team: &[Teammate], background: Option<&str>) -> Option<(Teammate, Vec<String>)> {
    let name = background?;
    team.iter().find(|t| t.state == "reviewing" && t.name == name).map(|t| (t.clone(), vec![]))
}

/// Whether git shows a member's `c`hanges in a git repository; None when git couldn't read one of its repositories
/// (it may have changes there: reviewed).
pub(super) fn in_repo(c: &super::team::Changes) -> Option<bool> {
    if !c.unread.is_empty() {
        return None;
    }
    Some(!c.files().is_empty())
}

impl Hub {
    /// The lead's turn ended: its reviews filed to their members (`file_turn`); `Committed <name>:` lines settle the
    /// members they name (`commit_reply`); and a member whose review ask this turn took, with neither its review nor
    /// word that it runs in the background, stops waiting (`unanswered`). A turn nothing of yours started (a teammate
    /// finishing) is no answer to an ask; only its reviews and `Committed <name>: <sha>` lines count.
    pub(super) fn team_reply(&self, lead: &str, turn: &LeadTurn) {
        let answered = !turn.unprompted && !read_review(turn.answer).1.is_empty();
        let filed = self.write(|st| file_turn(st, lead, turn));
        let review = !filed.is_empty() || answered;
        let settled = self.write(|st| commit_reply(st, lead, turn, review));
        let stale = !turn.unprompted && self.write(|st| unanswered(st, lead, turn));
        if !filed.is_empty() || settled || stale {
            self.changed();
        }
        for (call, next) in filed {
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
    }

    /// Why member `t`, just finished, isn't reviewed by itself under its lead's Review setting; None when it is.
    /// Git is asked only for "only code in a repo" (the default).
    fn why_no_review(&self, lead: &str, origin: &Origin, t: &Teammate) -> Option<&'static str> {
        let setting = self.read(|st| st.sessions.crew_review(lead));
        no_review(&setting, || changes(&origin.machine, &origin.cwd, t).ok().as_ref().and_then(in_repo))
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
        "Review your teammate {name}'s work with a fork in the background: one Agent call with `run_in_background: true` and no `name` (it isn't a teammate), its prompt starting `Review your teammate {name}'s work`, then reply only `Reviewing {name} in the background.` and end your turn; don't wait for it. The fork {look} against what it was asked and runs its tests if it has any; it changes, commits and messages nothing. When it reports, reply with a first line `Review of {name}:`, then only findings that need a change, numbered (`1. file:line — should-fix — what`; no notes that something is fine), and a last line `Recommendation: commit` or `Recommendation: send fixes`.\nWhat it was asked: {what}\nIts files: {files}"
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
/// else waits for you, and so does "send fixes" with no findings at all: there is nothing to send.
fn auto_next(crew_commit: &str, recommendation: &str, findings: &[String], fix_rounds: u32) -> AutoNext {
    if crew_commit == "wait" || (recommendation.contains("fix") && findings.is_empty()) {
        return AutoNext::Ask;
    }
    let commit = recommendation.starts_with("commit");
    if recommendation.contains("fix") || (commit && fix_findings(findings) > 0) {
        return if fix_rounds < AUTO_FIX_ROUNDS { AutoNext::Fixes } else { AutoNext::Ask };
    }
    if commit { AutoNext::Commit } else { AutoNext::Ask }
}

/// The crew member an unnamed review reply is about, of `team`'s members under review: the one this turn's own ask
/// named ("Review your teammate <name>'s work", the last of the turn's `asks`); the members asked before it whose
/// answer never came (lost to a reply after it, say) are `stale`: they wait no more. No ask this turn (you talking to
/// the lead while a review runs in the background: its "Recommendation" is about something else), or its member no
/// longer under review: None.
pub(super) fn reviewed_member(team: &[Teammate], asks: &[String]) -> Option<(Teammate, Vec<String>)> {
    let reviewing: Vec<&Teammate> = team.iter().filter(|t| t.state == "reviewing").collect();
    let name = asks.iter().rev().find_map(|a| review_ask_name(a))?;
    // Named, but no longer under review (its work committed meanwhile, say): nobody else's review.
    let t = reviewing.iter().find(|t| t.name == name)?;
    let stale = reviewing.iter().filter(|o| o.call != t.call && o.review_ms <= t.review_ms).map(|o| o.call.clone()).collect();
    Some(((*t).clone(), stale))
}

#[cfg(test)]
mod tests {
    use super::{auto_next, background_member, background_of, in_repo, named_reviews, no_review, read_review, review_words, reviewed_member, unanswered, AutoNext, LeadTurn};
    use crate::model::TurnPart;
    use crate::sessions::team::{review_ask_name, Teammate};

    fn mate(name: &str, state: &str, review_ms: u64) -> Teammate {
        Teammate { call: format!("call-{name}"), name: name.into(), state: state.into(), review_ms, ..Default::default() }
    }

    /// A prompted turn: what was said to the lead in it (`asks`), its transcript's tail (`said`), and what it wrote.
    fn turn<'a>(asks: &[String], said: &[(String, u64)], all: &str) -> LeadTurn<'a> {
        LeadTurn { answer: "", all: all.into(), asks: asks.to_vec(), said: said.to_vec(), unprompted: false, texts: vec![], background: None }
    }

    #[test]
    fn a_review_is_the_named_members_and_the_ones_asked_before_it_whose_answer_never_came_wait_no_more() {
        let team = [mate("loops-board", "reviewing", 10), mate("rows", "reviewing", 20), mate("docs", "reviewing", 30), mate("hub", "done", 0)];
        let asks = ["Fix the tray.".to_string(), "Review your teammate rows's work with a fork (one Agent call…)".to_string()];
        let (t, stale) = reviewed_member(&team, &asks).unwrap();
        assert_eq!((t.name.as_str(), stale), ("rows", vec!["call-loops-board".to_string()]), "docs, asked after it, still waits");
        // You talking to the lead while reviews run in the background: its "Recommendation" is about something else.
        assert!(reviewed_member(&team, &["How should the tokenizer go?".to_string()]).is_none(), "no ask this turn: nobody's review");
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
        assert_eq!(auto_next("", "send fixes", &[], 0), AutoNext::Ask, "fixes asked with none listed: nothing to send, yours");
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
        assert!(ask.contains("no `name`") && ask.contains("its prompt starting `Review your teammate rows's work`"), "the fork is no crew member (sessions/team.rs `started`): {ask}");
        let outside = review_words("mocks", "", "Draw 10 mocks", &[]);
        assert!(outside.contains("reads the files it says it changed") && !outside.contains("diff") && !outside.contains("(none yet)"), "{outside}");
    }

    /// The member each of `texts`' reviews names, of `names`.
    fn named<'a>(texts: &[&str], names: &[&'a str]) -> Vec<String> {
        named_reviews(texts, names).into_iter().map(|(n, _)| n).collect()
    }

    #[test]
    fn a_review_names_its_member_by_its_heading_and_an_audit_after_it_is_no_review() {
        let names = ["rows", "docs"];
        assert_eq!(named(&["Review of rows:\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes"], &names), ["rows"]);
        assert_eq!(named(&["**Review of docs:**\nRecommendation: commit"], &names), ["docs"]);
        assert_eq!(named(&["Review of rows: nothing to fix.\n\nRecommendation: commit"], &names), ["rows"], "words after the heading");
        assert!(named(&["Review of rows:\n1. a.rs:1 — note — fine"], &names).is_empty(), "no Recommendation line: not a review yet");
        assert!(named(&["1. a.rs:1 — note — fine\nRecommendation: commit"], &names).is_empty(), "names no member");
        assert!(named(&["Review of hub:\nRecommendation: commit"], &names).is_empty(), "names no member of this crew");
        // The notification turn: the fork's report, then the lead's review, then a stop hook's audit.
        let parts = [("rows reported back.", false), ("Review of rows:\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes", false), ("Audit: 1. checked the rules. Nothing else to do.", true)]
            .map(|(t, hooked)| TurnPart { text: t.into(), hooked });
        let last = parts[2].text.clone();
        let t = LeadTurn::read("", &parts, &last, true);
        let reviews = named_reviews(&t.texts, &names);
        assert_eq!(reviews, [("rows".to_string(), parts[1].text.clone())], "the review, not the audit");
        let audit_only = [TurnPart { text: "Audit: 1. a.rs:1 — note — fine. Review of rows: done.".into(), hooked: true }];
        assert!(named_reviews(&LeadTurn::read("", &audit_only, &audit_only[0].text, true).texts, &names).is_empty());
    }

    #[test]
    fn two_reviews_in_one_turn_are_each_filed_to_their_own_member_with_their_own_findings() {
        let names = ["rows", "docs"];
        let one = "Both forks reported.\n\nReview of rows:\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes\n\nReview of docs:\nRecommendation: commit";
        let reviews = named_reviews(&[one], &names);
        assert_eq!(reviews.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["rows", "docs"]);
        assert_eq!(read_review(&reviews[0].1), (vec!["1. a.rs:1 — should-fix — x".to_string()], "send fixes".to_string()));
        assert_eq!(read_review(&reviews[1].1), (vec![], "commit".to_string()), "docs's own Recommendation, none of rows's findings");
        // In two texts of the turn (two reports): both, not only the last.
        let two = ["Review of rows:\nRecommendation: commit", "Review of docs:\n1. b.rs:2 — should-fix — y\nRecommendation: send fixes"];
        assert_eq!(named(&two, &names), ["rows", "docs"]);
    }

    #[test]
    fn a_heading_that_strays_from_the_asked_one_still_names_its_member() {
        let names = ["rows", "rows-2", "api: v2"];
        let r = "\n1. a.rs:1 — should-fix — x\nRecommendation: send fixes";
        for heading in ["Review of Rows:", "Review of rows's work:", "Review of rows’s work", "## Review of rows", "**Review of `rows`**", "REVIEW OF ROWS — two findings"] {
            assert_eq!(named(&[&format!("{heading}{r}")], &names), ["rows"], "{heading}");
        }
        assert_eq!(named(&[&format!("Review of rows-2:{r}")], &names), ["rows-2"], "not rows");
        assert_eq!(named(&[&format!("Review of api: v2:{r}")], &names), ["api: v2"], "a name with a colon, whole");
        assert!(named(&[&format!("Review of rowsy:{r}")], &names).is_empty(), "another name that starts like it");
    }

    #[test]
    fn an_unnamed_review_in_a_report_turn_is_the_only_member_under_reviews_when_its_own_fork_reported() {
        let line = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        let ask = review_words("rows", "", "Fix the rows", &["ui/rows.js".into()]);
        let fork = |id: &str| line(&format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Agent","input":{{"prompt":"Review your teammate rows's work","run_in_background":true}}}}]}}}}"#));
        let note = |id: &str| line(&format!(r#"{{"type":"user","message":{{"content":"<task-notification>\n<task-id>x</task-id>\n<tool-use-id>{id}</tool-use-id>\n<status>completed</status>\n</task-notification>"}}}}"#));
        let prompt = |t: &str| serde_json::json!({ "type": "user", "message": { "content": t } });
        let entries = [prompt(&ask), fork("toolu_fork"), prompt("Fix the tray."), fork("toolu_other"), note("toolu_fork")];
        assert_eq!(background_of(&entries).as_deref(), Some("rows"), "its fork's report");
        assert_eq!(background_of(&[prompt(&ask), fork("toolu_fork"), prompt("Fix the tray."), fork("toolu_other"), note("toolu_other")]), None, "an Agent call for your ask");
        assert_eq!(background_of(&[prompt(&ask), fork("toolu_fork"), note("toolu_mate")]), None, "a teammate's finish");
        let team = [mate("rows", "reviewing", 10), mate("hub", "done", 0)];
        assert_eq!(background_member(&team, Some("rows")).map(|(t, _)| t.name), Some("rows".into()));
        assert!(background_member(&team, None).is_none(), "not its fork's report");
        let two = [mate("rows", "reviewing", 10), mate("docs", "reviewing", 20)];
        assert_eq!(background_member(&two, Some("docs")).map(|(t, _)| t.name), Some("docs".into()), "two under review: the one whose fork reported");
        assert!(background_member(&[mate("docs", "reviewed", 20)], Some("docs")).is_none(), "no longer under review");
    }

    #[test]
    fn a_member_with_files_in_a_repo_git_couldnt_read_is_reviewed_even_with_one_outside_any_repo() {
        let mut c = crate::hub::team::Changes::default();
        assert_eq!(in_repo(&c), Some(false), "nothing in a repo");
        c.unread.push("/elsewhere/repo/a.rs".into());
        assert_eq!(in_repo(&c), None, "git couldn't say for one of its repos: reviewed");
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
