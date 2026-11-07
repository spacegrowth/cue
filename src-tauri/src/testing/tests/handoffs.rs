//! Hand-offs (stability plan §4, Hand-offs): Cue's own hand-off sent to a waiting or a working session, one
//! asked ahead of a cache running out, Continue here, Continue in a new session refused or retried, a hand-off
//! that outlives a restart, and the watcher's tick that asks ahead.

use crate::ipc::{handoff_continue_now, handoff_cue_ahead, handoff_cue_now, handoff_fresh_now, handoff_keep_now, AHEAD_SESSION, CUE_HANDOFF};
use crate::testing::World;

const SID: &str = "3081f22f-1c2d";

fn insight_events(sid: &str, rule: &str) -> Vec<String> {
    crate::db::insight_events_of(sid).into_iter().filter(|(r, ..)| r == rule).map(|(_, a, d)| format!("{a}:{d}")).collect()
}

#[test]
fn cues_own_hand_off_is_typed_into_a_waiting_session_naming_a_note_of_its_own() {
    let w = World::new();
    w.session(SID).prompt("go").stop("Done.");
    handoff_cue_now(w.hub(), SID).unwrap();
    w.settle();
    let typed = w.terminal.typed(SID);
    assert!(typed.len() == 1 && typed[0].starts_with("Hand off: write a short note") && typed[0].contains("HANDOFF-3081f22f.md"), "{typed:?}");
    let kept = crate::db::handoff(SID).unwrap();
    assert_eq!((kept.skill.as_str(), kept.resume, kept.next.as_str(), kept.ahead), (CUE_HANDOFF, false, "", 0));
    assert_eq!(w.snapshot().0["skill_handoffs"][SID]["skill"], CUE_HANDOFF);
}

#[test]
fn a_hand_off_for_a_session_at_work_goes_when_its_turn_ends() {
    let w = World::new();
    let a = w.session(SID);
    a.prompt("go");
    assert!(handoff_cue_now(w.hub(), SID).unwrap().starts_with("kept in Cue"));
    assert!(w.terminal.typed(SID).is_empty());
    a.stop("Done.");
    w.advance(2000);
    assert!(w.terminal.typed(SID).first().is_some_and(|t| t.starts_with("Hand off:")), "{:?}", w.terminal.typed(SID));
}

#[test]
fn a_note_asked_ahead_of_a_session_that_got_busy_is_taken_back_not_kept_for_later() {
    let w = World::new();
    let a = w.session(SID);
    a.prompt("go");
    assert_eq!(handoff_cue_ahead(w.hub(), SID, AHEAD_SESSION), Err("busy".to_string()));
    a.stop("Done.");
    w.advance(2000);
    assert!(w.terminal.typed(SID).is_empty(), "you weren't there to see it go");
    assert!(crate::db::handoff(SID).is_none());
}

#[test]
fn continue_here_drops_a_note_written_ahead_but_not_one_you_asked_for() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    crate::db::handoff_sent("a", CUE_HANDOFF, false, AHEAD_SESSION);
    handoff_keep_now(w.hub(), "a");
    assert!(crate::db::handoff("a").is_none());
    assert_eq!(insight_events("a", "ahead"), ["acted:continue"]);
    crate::db::handoff_sent("a", CUE_HANDOFF, false, 0);
    handoff_keep_now(w.hub(), "a");
    assert!(crate::db::handoff("a").is_some(), "yours stays until you continue it");
}

#[test]
fn continue_here_after_it_already_continued_keeps_where_it_went() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    crate::db::handoff_sent("a", CUE_HANDOFF, false, AHEAD_SESSION);
    crate::db::handoff_continued("a", "b");
    handoff_keep_now(w.hub(), "a");
    assert_eq!(crate::db::handoff("a").map(|k| k.next), Some("b".to_string()));
}

#[test]
fn continuing_a_session_that_never_handed_off_or_already_continued_is_refused() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    assert_eq!(handoff_continue_now(w.hub(), "a"), Err("it hasn't handed off".to_string()));
    crate::db::handoff_sent("a", "handoff", true, 0);
    crate::db::handoff_continued("a", "b");
    assert_eq!(handoff_continue_now(w.hub(), "a"), Err("it already continued in a new session".to_string()));
}

#[test]
fn a_continue_that_couldnt_start_its_new_session_can_be_tried_again() {
    let w = World::new();
    crate::db::handoff_sent("gone", "handoff", true, 0); // its session isn't one Cue has now
    for _ in 0..2 {
        assert_eq!(handoff_continue_now(w.hub(), "gone"), Err("Cue has no record of that session".to_string()), "not \"already starting\"");
    }
    assert_eq!(crate::db::handoff("gone").map(|k| k.next), Some(String::new()), "still offered");
}

#[test]
fn starting_fresh_from_a_session_cue_has_no_record_of_is_refused() {
    let w = World::new();
    assert_eq!(handoff_fresh_now(w.hub(), "nobody"), Err("Cue has no record of that session".to_string()));
}

#[test]
fn a_hand_off_sent_is_still_offered_to_continue_after_a_restart() {
    let mut w = World::new();
    w.session("a").prompt("/handoff").stop("Note in HANDOFF.md.");
    crate::db::handoff_sent("a", "handoff", true, 0);
    w.relaunch();
    let h = w.snapshot().0["skill_handoffs"]["a"].clone();
    assert_eq!((h["skill"].as_str(), h["next"].as_str(), h["resume"].as_bool()), (Some("handoff"), Some(""), Some(true)), "{h}");
}

#[test]
fn the_watchers_tick_asks_ahead_only_where_you_turned_it_on_and_holds_where_it_cant_see_the_box() {
    let w = World::new();
    for sid in ["on", "off"] {
        let a = w.session(sid);
        a.transcript().cached_reply(3_600_000 - 60_000, 150_000, 3600);
        a.stop("Done.");
    }
    crate::db::ahead_watch("on", true);
    assert_eq!(w.snapshot().0["ahead_watch"], serde_json::json!(["on"]));
    w.tick(true);
    assert_eq!(insight_events("on", "ahead"), ["held:unseen"], "no tmux pane: Cue can't see what's typed in its box");
    assert!(insight_events("off", "ahead").is_empty());
    assert!(w.terminal.typed("on").is_empty() && crate::db::handoff("on").is_none());
    w.tick(true);
    assert_eq!(insight_events("on", "ahead"), ["held:unseen"], "once per idle stretch");
}
