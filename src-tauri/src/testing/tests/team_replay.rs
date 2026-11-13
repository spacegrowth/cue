//! A crew's reviews and commits end to end, replayed as they went for real: the lead's turns as Claude Code writes them
//! into its transcript (hosted/fixtures/crew_*.transcript.jsonl: a stop hook's audit after the review, a review asked
//! mid-turn, a review that never came, a commit the lead made by itself), each turn's end through the hub as its
//! stream says it, so Cue reads the transcript as it does live (hook::said_to, team::answer_in).

use super::team::{git, lead, main_repo, mate, repo, says, writes};
use crate::hosted::HostedEvent;
use crate::testing::{Transcript, World};
use serde_json::{json, Value};

const AUDIT: &str = include_str!("../../hosted/fixtures/crew_review_then_audit.transcript.jsonl");
const MID_TURN: &str = include_str!("../../hosted/fixtures/crew_review_mid_turn.transcript.jsonl");
const NEVER_CAME: &str = include_str!("../../hosted/fixtures/crew_review_never_came.transcript.jsonl");
const COMMITTED: &str = include_str!("../../hosted/fixtures/crew_committed_unasked.transcript.jsonl");

/// `fixture`'s turn written into lead `sid`'s transcript, its `{name}`, `{ask}` and `{review}` filled in.
fn turn_in_log(w: &World, sid: &str, fixture: &str, name: &str, ask: &str, review: &str) {
    let log = w.hub().session_origin(sid).unwrap().transcript_path;
    let inside = |s: &str| {
        let j = serde_json::to_string(s).unwrap();
        j[1..j.len() - 1].to_string()
    };
    for line in fixture.lines().filter(|l| !l.trim().is_empty()) {
        let line = line.replace("{name}", name).replace("{ask}", &inside(ask)).replace("{review}", &inside(review));
        Transcript::at(&log).entry(serde_json::from_str(&line).unwrap());
    }
}

/// The lead's turn ends with `text`: a reply to something said to it, or (`unprompted`) one a teammate's finish started.
fn ends(w: &World, sid: &str, text: &str, unprompted: bool) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted });
    w.settle();
}

/// What lead `sid` was sent that starts with `start`.
fn sent(w: &World, sid: &str, start: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, t)| s == sid && t.starts_with(start)).map(|(_, t)| t).collect()
}

/// The lead's next queued review goes (the lead free, past the last one's settling): the ask it was sent.
fn next_review(w: &World, sid: &str) -> String {
    let before = sent(w, sid, "Review your teammate ").len();
    // The lead's last turn end drains the queue by itself a moment later; else the watcher's tick does.
    w.advance(60_000);
    if sent(w, sid, "Review your teammate ").len() == before {
        w.hub().drain_reviews();
        w.settle();
    }
    let asks = sent(w, sid, "Review your teammate ");
    assert_eq!(asks.len(), before + 1, "one review went");
    asks.last().unwrap().clone()
}

/// Member `name`'s review out to its lead (sent as it finished, the lead free; else once it is): the ask it was sent.
fn review_of(w: &World, sid: &str, name: &str) -> String {
    if state(w, sid, name) == "queued" {
        next_review(w, sid);
    }
    assert_eq!(state(w, sid, name), "reviewing");
    sent(w, sid, &format!("Review your teammate {name}'s")).pop().unwrap()
}

fn path(dir: &std::path::Path, f: &str) -> String {
    dir.join(f).to_string_lossy().to_string()
}

fn state(w: &World, sid: &str, name: &str) -> String {
    mate(w, sid, name)["state"].as_str().unwrap_or("").to_string()
}

/// Item 1: the lead's stop hook made it answer an audit after its review, in the same turn. The review is still the
/// member's, read from the turn's reply that answers the ask.
#[test]
fn a_review_followed_by_a_stop_hooks_audit_in_the_same_turn_is_still_the_members_review() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "one part");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[path(&dir, "a.txt")]));
    ends(&w, &sid, "started rows", false);
    let ask = next_review(&w, &sid);
    let review = "1. a.txt:1 — should-fix — the line says nothing about why\nRecommendation: send fixes";
    turn_in_log(&w, &sid, AUDIT, "rows", &ask, review);
    ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["recommendation"].as_str(), t["review"].as_str()), (Some("reviewed"), Some("send fixes"), Some(review)), "{t}");
    assert_eq!(t["findings"].as_array().map(Vec::len), Some(1));
}

/// Item 1: the ask reached a lead mid-turn (Claude Code hands it over as a queued_command attachment); the lead
/// reviewed, then went back to what it was doing. The review is the member's, not the turn's last words.
#[test]
fn a_review_asked_mid_turn_is_read_from_the_reply_that_answers_it_not_the_turns_last_words() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "one part");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[path(&dir, "a.txt")]));
    ends(&w, &sid, "started rows", false);
    let ask = next_review(&w, &sid);
    let review = "1. a.txt:1 — note — verified the sort is stable\nRecommendation: commit";
    turn_in_log(&w, &sid, MID_TURN, "rows", &ask, review);
    ends(&w, &sid, "Back to the tray: its spacing is fixed in ui/styles/team.css.", false);
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["review"].as_str()), (Some("reviewed"), Some(review)), "{t}");
}

/// Item 2: three members reviewed one after another, each review followed by a stop hook's audit. Each review is its
/// own member's, and what Cue does next by itself (fixes, commit) goes to that member, never to an earlier one.
#[test]
fn three_reviews_each_followed_by_an_audit_each_go_to_their_own_member_and_fixes_go_to_the_right_one() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "three parts");
    for (call, id, name, f) in [("toolu_A", "agentA", "loops-board", "a.txt"), ("toolu_B", "agentB", "rows", "b.txt"), ("toolu_C", "agentC", "docs", "shared.txt")] {
        says(&w, &sid, writes(call, id, name, &[path(&dir, f)]));
    }
    ends(&w, &sid, "started the crew", false);
    let reviews = [
        ("loops-board", "1. a.txt:1 — should-fix — the board drops its last column\nRecommendation: send fixes"),
        ("rows", "Recommendation: commit"),
        ("docs", "1. shared.txt:1 — should-fix — say what the flag does\nRecommendation: send fixes"),
    ];
    for (name, review) in reviews {
        let ask = next_review(&w, &sid);
        assert!(ask.starts_with(&format!("Review your teammate {name}'s work")), "in the order they finished: {ask}");
        turn_in_log(&w, &sid, AUDIT, name, &ask, review);
        ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
    }
    let fixes = sent(&w, &sid, "Send your teammate ");
    assert_eq!(fixes.len(), 2, "{fixes:?}");
    assert!(fixes[0].starts_with("Send your teammate loops-board its review's findings") && fixes[0].contains("drops its last column"), "{}", fixes[0]);
    assert!(fixes[1].starts_with("Send your teammate docs its review's findings") && fixes[1].contains("say what the flag does"), "{}", fixes[1]);
    assert_eq!((state(&w, &sid, "loops-board"), state(&w, &sid, "rows"), state(&w, &sid, "docs")), ("working".into(), "committed".into(), "working".into()));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "b.txt", "rows's file, committed by itself");
}

/// Item 2: the turns that answered two earlier asks never reached Cue (its process restarted meanwhile, say); the
/// third's review names its member. It's that member's, and the two asked before it stop waiting, back to done.
#[test]
fn a_review_after_two_asks_whose_turns_cue_never_saw_is_the_named_members_and_the_two_stop_waiting() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "three parts");
    for (call, id, name, f) in [("toolu_A", "agentA", "loops-board", "a.txt"), ("toolu_B", "agentB", "rows", "b.txt"), ("toolu_C", "agentC", "docs", "shared.txt")] {
        says(&w, &sid, writes(call, id, name, &[path(&dir, f)]));
    }
    ends(&w, &sid, "started the crew", false);
    for name in ["loops-board", "rows"] {
        let ask = next_review(&w, &sid);
        turn_in_log(&w, &sid, NEVER_CAME, name, &ask, "");
    }
    let ask = next_review(&w, &sid);
    turn_in_log(&w, &sid, AUDIT, "docs", &ask, "1. shared.txt:1 — should-fix — say what the flag does\nRecommendation: send fixes");
    ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
    let fixes = sent(&w, &sid, "Send your teammate ");
    assert_eq!(fixes.len(), 1, "{fixes:?}");
    assert!(fixes[0].starts_with("Send your teammate docs its review's findings"), "{}", fixes[0]);
    for name in ["loops-board", "rows"] {
        let t = mate(&w, &sid, name);
        assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Its review didn't come back")), "{t}");
    }
}

/// Item 3: a member whose files Cue never saw is under review, and the turn its ask started ends without its review.
/// It stops waiting (back to done, its row says why) and isn't counted at work; Review in its menu asks again.
#[test]
fn a_review_ask_whose_turn_ended_without_its_review_puts_the_member_back_to_done() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part, done with shell commands elsewhere");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    ends(&w, &sid, "started board", false);
    let ask = next_review(&w, &sid);
    assert_eq!(state(&w, &sid, "board"), "reviewing");
    turn_in_log(&w, &sid, NEVER_CAME, "board", &ask, "");
    ends(&w, &sid, "board wrote its files outside this folder with shell commands; I'll look at them once the tray fix lands.", false);
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Its review didn't come back")), "{t}");
    assert!(sent(&w, &sid, "Commit your teammate ").is_empty(), "no review, no commit");
    // Review from its menu: asked again (the lead is free: at once), and this time it comes.
    w.advance(60_000);
    w.hub().team_action(&sid, "agentA", "review").unwrap();
    let asks = sent(&w, &sid, "Review your teammate board's");
    assert_eq!((asks.len(), state(&w, &sid, "board")), (2, "reviewing".to_string()));
    let ask = asks[1].clone();
    turn_in_log(&w, &sid, AUDIT, "board", &ask, "Recommendation: commit");
    w.advance(30_000); // the review takes a while
    ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
    assert_eq!(sent(&w, &sid, "Commit your teammate board's work now").len(), 1, "a clean review: its lead commits it");
}

/// Item 3: the turn the ask started ended without Cue seeing it end (its stream cut off); your next message's turn
/// ends. The ask is earlier in the transcript than that turn: its turn is over, so the member stops waiting. A member
/// whose ask hasn't reached the lead yet keeps waiting.
#[test]
fn a_review_ask_answered_by_nothing_before_your_next_turn_ended_stops_waiting_but_one_not_yet_taken_waits() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two parts");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    says(&w, &sid, writes("toolu_B", "agentB", "rows", &[path(&dir, "a.txt")]));
    ends(&w, &sid, "started both", false);
    let ask = next_review(&w, &sid);
    assert!(ask.starts_with("Review your teammate board's"), "{ask}");
    turn_in_log(&w, &sid, NEVER_CAME, "board", &ask, "");
    next_review(&w, &sid); // rows's ask goes, but the lead hasn't taken it yet: it's not in its transcript
    Transcript::at(w.hub().session_origin(&sid).unwrap().transcript_path).prompt("Fix the tray's spacing.").assistant("Fixed.");
    ends(&w, &sid, "Fixed.", false);
    assert_eq!(mate(&w, &sid, "board")["step"], "Its review didn't come back");
    assert_eq!(state(&w, &sid, "board"), "done");
    assert_eq!(state(&w, &sid, "rows"), "reviewing", "its ask isn't the lead's yet: still waiting");
}

/// Item 3, without a transcript to read (or its ask scrolled out of it): a member under review whose lead has been
/// free for REVIEW_SHOWN_MS since its ask went stops waiting at the next tidy.
#[test]
fn a_member_under_review_for_long_while_its_lead_is_free_stops_waiting() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    ends(&w, &sid, "started board", false);
    next_review(&w, &sid);
    w.advance(5 * 60_000);
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "reviewing", "a review can take a while");
    w.hosted_says(&sid, HostedEvent::Taken { text: "something".into() });
    w.advance(20 * 60_000);
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "reviewing", "its lead still works: maybe on the review");
    ends(&w, &sid, "Done with that.", true);
    w.hub().team_tidy();
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Its review didn't come back")), "{t}");
}

/// Item 3's fallback, not too soon: a lead paused on an ask of yours, or compacting, past REVIEW_SHOWN_MS isn't free,
/// so its member under review keeps waiting, and the review that then comes is still its.
#[test]
fn a_member_under_review_keeps_waiting_while_its_lead_asks_you_something_or_compacts() {
    use crate::hosted::{Ask, AskKind};
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    ends(&w, &sid, "started board", false);
    next_review(&w, &sid);
    let ask = Ask { request_id: "r1".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "cargo test" }), suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: String::new(), message: String::new(), form: Value::Null };
    w.hosted_says(&sid, HostedEvent::Ask(ask));
    w.settle();
    w.advance(20 * 60_000);
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "reviewing", "its lead asks you something: not free");
    let id = w.snapshot().card(&sid)["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({ "behavior": "allow" }));
    w.settle();
    w.hosted_says(&sid, HostedEvent::Compacting { done: false });
    w.settle();
    w.advance(5 * 60_000);
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "reviewing", "its lead compacts: not free");
    w.hosted_says(&sid, HostedEvent::Compacting { done: true });
    ends(&w, &sid, "Recommendation: commit", false);
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["recommendation"].as_str()), (Some("reviewed"), Some("commit")), "the review that came is its: {t}");
}

/// Skip that comes too late (clicked on a row drawn before its review went, or before the member was resumed) changes
/// nothing: the member stays under review (or at work), and the review that comes is still its.
#[test]
fn a_skip_after_the_review_went_or_the_member_was_resumed_changes_nothing() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two parts");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[path(&dir, "a.txt")]));
    says(&w, &sid, writes("toolu_B", "agentB", "docs", &[path(&dir, "b.txt")]));
    ends(&w, &sid, "started both", false);
    next_review(&w, &sid);
    assert_eq!(state(&w, &sid, "rows"), "reviewing");
    let said = w.hub().unqueue_review(&format!("team:{sid}:agentA"));
    assert_eq!(said, Err("Its review isn't waiting any more".to_string()));
    assert_eq!(state(&w, &sid, "rows"), "reviewing", "still under review");
    ends(&w, &sid, "Recommendation: commit", false);
    assert_eq!(state(&w, &sid, "rows"), "reviewed", "its review isn't lost");
    // docs is resumed by its lead while its review waits: the review is dropped, and a late Skip leaves it at work.
    assert_eq!(state(&w, &sid, "docs"), "queued");
    says(&w, &sid, [HostedEvent::Task { what: "task_started".into(), detail: json!({ "task_type": "local_agent", "task_id": "agentB", "tool_use_id": "toolu_send" }) }]);
    w.advance(60_000);
    w.hub().drain_reviews();
    w.settle();
    assert!(w.hub().unqueue_review(&format!("team:{sid}:agentB")).is_err());
    assert_eq!(state(&w, &sid, "docs"), "working");
}

/// A member left "queued" with no review in Review's queue (an older Cue's queue lost, say) stops being counted at work.
#[test]
fn a_member_queued_with_no_review_in_the_queue_is_done_again_at_the_next_tidy() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    assert_eq!(state(&w, &sid, "board"), "queued");
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "queued", "its review is in the queue: it waits");
    w.hub().unqueue_review_only(&format!("team:{sid}:agentA"));
    w.hub().team_tidy();
    assert_eq!(state(&w, &sid, "board"), "done");
}

/// Item 4: the review ask asks only for what needs a change, and handled by Cue a commit whose findings are all notes
/// is committed (should-fix still goes back). A member the lead resumes again and again is committed each time.
#[test]
fn a_commit_whose_findings_are_only_notes_is_committed_by_itself_each_time_the_member_finishes() {
    let w = World::new();
    let dir = main_repo(&w);
    let sid = lead(&w, "one part");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[path(&dir, "a.txt")]));
    ends(&w, &sid, "started rows", false);
    for round in 0..3 {
        let ask = review_of(&w, &sid, "rows");
        if round == 0 {
            assert!(ask.contains("only findings that need a change") && ask.contains("no notes that something is fine"), "{ask}");
        }
        turn_in_log(&w, &sid, AUDIT, "rows", &ask, "1. a.txt:1 — note — verified the sort is stable\n2. a.txt:2 — nit — fine as is\nRecommendation: commit");
        ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
        assert_eq!(state(&w, &sid, "rows"), "committed", "round {round}: {}", mate(&w, &sid, "rows"));
        // The lead sends it something more with SendMessage: it's at work again, then finishes.
        says(&w, &sid, [
            HostedEvent::Task { what: "task_started".into(), detail: json!({ "task_type": "local_agent", "task_id": "agentA", "tool_use_id": "toolu_send" }) },
            HostedEvent::ToolCall { id: format!("w{round}"), name: "Write".into(), input: json!({ "file_path": path(&dir, "a.txt"), "content": "…" }), agent: Some("toolu_A".into()) },
        ]);
        std::fs::write(dir.join("a.txt"), format!("round {round}\n")).unwrap();
        says(&w, &sid, [
            HostedEvent::ToolResult { id: format!("w{round}"), output: json!("ok"), failed: false, agent: Some("toolu_A".into()) },
            HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": "agentA", "status": "completed", "summary": "Done again." }) },
        ]);
        // Its finish wakes the lead: a turn nothing of yours started.
        ends(&w, &sid, "rows is done again.", true);
        let t = mate(&w, &sid, "rows");
        assert!(matches!(t["state"].as_str(), Some("queued" | "reviewing")), "round {round}: finished again, reviewed again: {t}");
    }
    assert!(sent(&w, &sid, "Send your teammate ").is_empty(), "notes never go back as fixes");
    // A commit with a should-fix finding still goes back.
    let ask = review_of(&w, &sid, "rows");
    turn_in_log(&w, &sid, AUDIT, "rows", &ask, "1. a.txt:1 — should-fix — the round is off by one\n2. a.txt:2 — note — fine\nRecommendation: commit");
    ends(&w, &sid, "1–12: N/A, no edits of mine this turn.", false);
    assert_eq!(sent(&w, &sid, "Send your teammate rows").len(), 1);
    assert_eq!(state(&w, &sid, "rows"), "working");
}

/// Item 5: the lead committed a member's work by itself (files Cue never saw) and said so with a `Committed <name>:`
/// line, no ask out. The member is committed; its review and Cue's ask to commit it never go.
#[test]
fn a_committed_line_from_the_lead_with_no_ask_out_commits_the_member_and_no_ask_goes() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "two parts, done with shell commands elsewhere");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    assert_eq!(state(&w, &sid, "board"), "queued");
    turn_in_log(&w, &sid, COMMITTED, "board", "", "");
    ends(&w, &sid, "Now the tray: done, its spacing is fixed.", false);
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["commit"].as_str(), t["step"].as_str()), (Some("committed"), Some("0a1b2c3"), Some("Committed by its lead")), "{t}");
    w.advance(60_000);
    w.hub().drain_reviews();
    w.settle();
    assert!(sent(&w, &sid, "Review your teammate board").is_empty(), "its queued review dropped");
    assert!(sent(&w, &sid, "Commit your teammate board").is_empty());
    // In a turn a teammate's finish started, too; and once Commit waits for the lead to be free, the line settles it.
    says(&w, &sid, writes("toolu_B", "agentB", "notes", &[]));
    w.hub().unqueue_review(&format!("team:{sid}:agentB")).unwrap();
    w.hosted_says(&sid, HostedEvent::Taken { text: "go on".into() });
    let said = w.hub().team_action(&sid, "agentB", "commit").unwrap();
    assert_eq!(said, "Cue asks its lead to commit notes's work once it's free");
    assert!(mate(&w, &sid, "notes")["commit_wait_ms"].as_u64().is_some_and(|ms| ms > 0));
    ends(&w, &sid, "notes is done.\nCommitted notes: 1234abc", true);
    let t = mate(&w, &sid, "notes");
    assert_eq!((t["state"].as_str(), t["commit"].as_str()), (Some("committed"), Some("1234abc")), "{t}");
    w.advance(60_000);
    w.hub().drain_reviews();
    w.settle();
    assert!(sent(&w, &sid, "Commit your teammate notes").is_empty(), "its wait was settled before it went");
}

/// Item 6: Cue's own asks that wait for the lead to be free are kept on the member's row, never as a message in the
/// lead's chat (kept or queued); the chat gets only what was really sent, once the lead is free.
#[test]
fn a_commit_ask_waits_on_the_members_row_not_in_the_chat_and_goes_once_the_lead_is_free() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part, done with shell commands elsewhere");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    w.hub().unqueue_review(&format!("team:{sid}:agentA")).unwrap();
    w.hub().team_action(&sid, "agentA", "commit").unwrap();
    let t = mate(&w, &sid, "board");
    assert_eq!(t["step"], "Cue asks its lead to commit it when it's free", "{t}");
    assert!(t["commit_asked_ms"].is_null());
    assert!(w.hub().team_action(&sid, "agentA", "commit").is_err(), "asked once");
    let s = w.snapshot();
    assert!(s.session(&sid)["held"].is_null() && s.session(&sid)["queued"].is_null(), "nothing in the chat: {}", s.session(&sid));
    assert!(sent(&w, &sid, "Commit your teammate").is_empty());
    // You write to it while it works: yours goes first, at the turn's end; Cue's ask after that turn.
    w.send_to(&sid, "and check the tray");
    ends(&w, &sid, "Started on it.", false);
    let all: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == sid).map(|(_, t)| t).collect();
    assert_eq!(all.last().map(String::as_str), Some("and check the tray"), "{all:?}");
    w.hosted_says(&sid, HostedEvent::Taken { text: "and check the tray".into() });
    ends(&w, &sid, "Checked.", false);
    w.advance(2000);
    let asks = sent(&w, &sid, "Commit your teammate board's work now");
    assert_eq!(asks.len(), 1, "{:?}", w.hosted.sent());
    let t = mate(&w, &sid, "board");
    assert!(t["commit_wait_ms"].is_null() && t["commit_asked_ms"].as_u64().is_some_and(|ms| ms > 0), "{t}");
    assert_eq!(t["step"], "Asked its lead to commit it");
    let _: Value = t;
}
