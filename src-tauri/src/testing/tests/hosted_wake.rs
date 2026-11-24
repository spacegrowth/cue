//! A paused hosted session woken for a message that doesn't wake (#857): its Claude ends before taking it, never takes
//! it within the limit, or won't start. The message is kept, marked as not woken for, with Try again and Send to a
//! fresh session; one that wakes in time is left alone.

use super::hosted::{running, turn_ends};
use crate::hosted::{HostedEvent, Start};
use crate::testing::World;

/// A hosted session whose Claude ended with its turn: paused, a message to it starts it again.
fn paused(w: &World) -> String {
    let sid = running(w, "go");
    turn_ends(w, &sid, "Done.");
    w.hosted.exits(&sid);
    w.hosted_says(&sid, HostedEvent::Exited { code: Some(0), during_turn: false, last_words: String::new() });
    assert_eq!(w.hub().session_origin(&sid).unwrap().agent_pid, None, "paused");
    sid
}

#[test]
fn a_message_whose_wake_exits_before_taking_it_says_it_didnt_wake_and_keeps_it() {
    let w = World::new();
    let sid = paused(&w);
    w.send_to(&sid, "and the docs?");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume(sid.clone())), "woken for it");
    assert_eq!(w.snapshot().last_yours(&sid)["pending"], true);
    // Its Claude dies resuming, before it takes anything.
    w.hosted.exits(&sid);
    w.hosted_says(&sid, HostedEvent::Exited { code: Some(1), during_turn: false, last_words: String::new() });
    let last = w.snapshot().last_yours(&sid).clone();
    assert_eq!((last["text"].as_str(), last["unsent"].as_bool(), last["no_wake"].as_bool()), (Some("and the docs?"), Some(true), Some(true)), "{last}");
    assert!(last["pending"].is_null(), "no longer on its way: {last}");
    assert!(last["delivery"].to_string().contains("didn't wake"), "{last}");
    // Try again wakes it once more, and this time it goes.
    w.send_to(&sid, "and the docs?");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "and the docs?".to_string())));
    w.hosted_says(&sid, HostedEvent::Taken { text: "and the docs?".into() });
    assert!(w.snapshot().last_yours(&sid)["no_wake"].is_null());
}

#[test]
fn a_wake_that_never_takes_the_message_is_ended_within_a_minute_and_says_so() {
    let w = World::new();
    let sid = paused(&w);
    w.send_to(&sid, "next");
    w.advance(crate::hub::hosted::WAKE_LIMIT_MS - 1000);
    assert_eq!(w.snapshot().last_yours(&sid)["pending"], true, "still on its way before the limit");
    assert!(w.hosted.closed().is_empty());
    w.advance(2000);
    let last = w.snapshot().last_yours(&sid).clone();
    assert_eq!((last["unsent"].as_bool(), last["no_wake"].as_bool()), (Some(true), Some(true)), "{last}");
    assert_eq!(w.hosted.closed(), vec![sid.clone()], "the stalled resume is ended, so a late take can't answer it");
    assert_eq!(w.hub().session_origin(&sid).unwrap().agent_pid, None, "paused again: Try again starts it afresh");
}

#[test]
fn a_wake_that_takes_the_message_in_time_is_left_alone() {
    let w = World::new();
    let sid = paused(&w);
    w.send_to(&sid, "next");
    w.advance(5000);
    w.hosted_says(&sid, HostedEvent::Taken { text: "next".into() });
    w.advance(crate::hub::hosted::WAKE_LIMIT_MS);
    assert!(w.hosted.closed().is_empty());
    assert!(w.snapshot().last_yours(&sid)["unsent"].is_null());
    assert_eq!(w.snapshot().state(&sid), "working");
}

#[test]
fn a_wake_that_wont_start_says_it_didnt_wake() {
    let w = World::new();
    let sid = paused(&w);
    w.hosted.refuse("claude: not found");
    let _ = w.hub().send_to_session(&sid, "next", &[], false);
    let last = w.snapshot().last_yours(&sid).clone();
    assert_eq!((last["unsent"].as_bool(), last["no_wake"].as_bool()), (Some(true), Some(true)), "{last}");
    assert!(last["delivery"].to_string().contains("claude: not found"), "{last}");
}

#[test]
fn send_to_a_fresh_session_starts_one_from_its_last_reply_with_the_message_and_closes_the_old() {
    let w = World::new();
    let sid = paused(&w);
    w.send_to(&sid, "and the docs?");
    w.hosted.exits(&sid);
    w.hosted_says(&sid, HostedEvent::Exited { code: Some(1), during_turn: false, last_words: String::new() });
    let r = crate::hub::handoff_fresh_with(w.hub(), &sid, "and the docs?").unwrap();
    let new_id = r["session_id"].as_str().unwrap().to_string();
    assert!(!new_id.is_empty() && new_id != sid, "{r}");
    let first = w.hosted.sent().into_iter().find(|(s, _)| *s == new_id).map(|(_, t)| t).unwrap_or_default();
    assert!(first.contains("and the docs?") && first.contains("Carry on from an earlier session"), "{first}");
    assert!(w.hub().session_origin(&sid).is_none(), "the old one is closed (Closed keeps it)");
}
