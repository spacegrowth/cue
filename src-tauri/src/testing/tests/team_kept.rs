//! Send fixes while the lead works: Cue keeps the message for it (it goes when the lead's turn ends), so the member
//! waits on its lead, not at work, until it goes; taken back, it's ready again. As it went live: a member counted
//! "working" while its fixes sat kept, and the reviews behind them counted "waiting on the lead".

use super::team::{lead, mate, repo, says, writes};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::Value;

/// Lead `sid` with rows reviewed ("send fixes", left to you), then at work again on something else, and hub finished
/// meanwhile (its review queued behind it).
fn reviewed_then_busy(w: &World) -> String {
    let dir = repo(w);
    let sid = lead(w, "two teammates");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    ends(w, &sid, "started rows");
    w.hub().drain_reviews();
    w.settle();
    assert_eq!(mate(w, &sid, "rows")["state"], "reviewing");
    ends(w, &sid, "Review of rows:\n1. a.txt:1 — should-fix — say more\nRecommendation: send fixes.");
    assert_eq!(mate(w, &sid, "rows")["state"], "reviewed");
    w.hosted_says(&sid, HostedEvent::Taken { text: "carry on with the docs".into() });
    says(w, &sid, writes("toolu_B", "agentB", "hub", &[dir.join("b.txt").to_string_lossy().to_string()]));
    assert_eq!(w.snapshot().state(&sid), "working");
    assert_eq!(mate(w, &sid, "hub")["state"], "queued");
    sid
}

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

fn fixes_sent(w: &World, sid: &str) -> usize {
    w.hosted.sent().iter().filter(|(s, t)| s == sid && t.contains("SendMessage (to: \"rows\")")).count()
}

fn kept_ms(t: &Value) -> u64 {
    t["fixes_kept_ms"].as_u64().unwrap_or(0)
}

#[test]
fn fixes_kept_for_a_busy_lead_leave_the_member_waiting_on_it_and_at_work_once_they_go() {
    let w = World::new();
    let sid = reviewed_then_busy(&w);
    let r = w.hub().team_action(&sid, "agentA", "fixes").unwrap();
    w.settle();
    assert_eq!(r, "Cue sends rows's fixes to its lead when its turn ends");
    assert_eq!(fixes_sent(&w, &sid), 0, "kept, not sent");
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("reviewed"), Some("Fixes go to its lead when its turn ends")), "not at work: {t}");
    assert!(kept_ms(&t) > 0 && t["findings"].as_array().is_some_and(|f| f.len() == 1), "waits on its lead, its review kept: {t}");
    assert_eq!(mate(&w, &sid, "hub")["state"], "queued", "behind the kept message");
    // The lead's turn ends: the kept fixes go, and rows is at work.
    ends(&w, &sid, "Docs done.");
    assert_eq!(fixes_sent(&w, &sid), 1, "sent at the turn's end");
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["step"].as_str(), kept_ms(&t)), (Some("working"), Some("Fixes sent through its lead"), 0), "{t}");
    assert_eq!(t["fix_rounds"], 1);
}

#[test]
fn fixes_taken_back_from_the_box_leave_the_member_ready_again() {
    let w = World::new();
    let sid = reviewed_then_busy(&w);
    w.hub().team_action(&sid, "agentA", "fixes").unwrap();
    w.settle();
    assert!(w.hub().unhold(&sid).is_some_and(|h| h.text.contains("SendMessage (to: \"rows\")")));
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["step"].as_str(), kept_ms(&t)), (Some("reviewed"), Some("Its fixes were taken back: not sent"), 0), "{t}");
    ends(&w, &sid, "Docs done.");
    assert_eq!(fixes_sent(&w, &sid), 0, "taken back: never sent");
    assert_eq!(mate(&w, &sid, "rows")["state"], "reviewed");
}

#[test]
fn fixes_kept_for_a_lead_that_comes_back_on_a_new_terminal_never_went_and_the_member_is_ready_again() {
    let w = World::new();
    let sid = reviewed_then_busy(&w);
    w.hub().team_action(&sid, "agentA", "fixes").unwrap();
    w.settle();
    // Its record, kept message and all, parked and brought back: what it was holding ended with its agent.
    let mut s = w.hub().session_record(&sid).unwrap();
    assert!(s.held.is_some() && s.team.iter().any(|t| t.fixes_kept_ms > 0));
    s.state = "waiting".into();
    let origin = s.origin.clone();
    w.hub().add_parked(s);
    w.hub().unparked(w.hub().parked(&sid).unwrap(), origin, "");
    w.settle();
    assert!(w.snapshot().session(&sid)["held"].is_null(), "the kept message is gone");
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["step"].as_str(), kept_ms(&t)), (Some("reviewed"), Some("Its fixes weren't sent: its lead's session ended first"), 0), "ready again, Send fixes: {t}");
    assert_eq!(fixes_sent(&w, &sid), 0);
}

#[test]
fn fixes_kept_for_the_old_lead_dont_carry_over_a_handoff() {
    let rows = crate::sessions::team::Teammate { call: "c".into(), name: "rows".into(), state: "reviewed".into(), fixes_kept_ms: 5, findings: vec!["1. x".into()], ..Default::default() };
    let t = &crate::sessions::team::carried(&[rows], "old")[0];
    assert_eq!((t.state.as_str(), t.fixes_kept_ms, t.step.as_str()), ("reviewed", 0, "Its fixes weren't sent before the handoff"));
}

#[test]
fn fixes_sent_to_a_free_lead_put_the_member_at_work_at_once() {
    let w = World::new();
    let sid = reviewed_then_busy(&w);
    ends(&w, &sid, "Docs done.");
    let r = w.hub().team_action(&sid, "agentA", "fixes").unwrap();
    w.settle();
    assert_eq!(r, "Sent the fixes to rows");
    assert_eq!(fixes_sent(&w, &sid), 1);
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), kept_ms(&t)), (Some("working"), 0), "{t}");
}
