//! What you write on an item's page reaches its holder, at every stage, or its Thread says it didn't and offers
//! Retry (#437).

use super::desk::{desk, holder, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::Value;

/// Your step on item `id` (why starting `why`), as the window has it.
fn step(w: &World, project: &str, id: &str, why: &str) -> Value {
    desk(w, project)["decisions"].as_array().unwrap().iter()
        .find(|d| d["item_id"] == id && d["by"] == "You" && d["why"].as_str().unwrap_or("").starts_with(why))
        .cloned().unwrap_or_else(|| panic!("no step {why} on #{id}: {}", desk(w, project)["decisions"]))
}

/// An item held by a session at its prompt, put in `state`, with a note written on its page: what the session was
/// sent, and the note's step.
fn note_in(state: &str) -> (World, String, String, Value) {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, state);
    w.hub().desk_item_act(&it.id, "note", "Use 8px, not 6").unwrap();
    w.settle();
    let s = step(&w, &project, &it.id, "to ");
    (w, sid, it.id, s)
}

#[test]
fn a_note_reaches_its_holder_at_every_stage() {
    for state in ["working", "finishing", "queued", "needs_you"] {
        let (w, sid, id, s) = note_in(state);
        let sent = sent_to(&w, &sid);
        assert!(sent.iter().any(|t| t.starts_with(&format!("About #{id} (Spacing): Use 8px, not 6"))), "{state}: {sent:?}");
        assert_eq!((s["text"].as_str(), s["delivery"]["session_id"].as_str(), s["delivery"]["went"].as_str()), (Some("Use 8px, not 6"), Some(sid.as_str()), Some("sent")), "{state}: {s}");
    }
}

/// A note on an item to check (#584: "ok testing now"): to its holder as a note, the item still to check. Only Send
/// back reopens it.
#[test]
fn a_note_on_an_item_to_check_reaches_its_holder_and_it_stays_to_check() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "to_check");
    let after = w.hub().desk_item_act(&it.id, "note", "Still 6px on the left").unwrap();
    w.settle();
    assert_eq!(after.state, "to_check");
    let sent = sent_to(&w, &sid);
    assert!(sent.iter().any(|t| t.starts_with(&format!("About #{} (Spacing): Still 6px on the left", it.id))), "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("isn't done")), "{sent:?}");
    assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "sent");
}

/// A quick phrase clicked on the page of an item to check (#488): to its holder as a note, the item still to check.
#[test]
fn a_quick_phrase_on_an_item_to_check_is_a_note_and_it_stays_to_check() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "to_check");
    let after = w.hub().desk_item_act(&it.id, "phrase", "Go ahead").unwrap();
    w.settle();
    assert_eq!(after.state, "to_check");
    let sent = sent_to(&w, &sid);
    assert!(sent.iter().any(|t| t.starts_with(&format!("About #{} (Spacing): Go ahead", it.id))), "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("isn't done")), "{sent:?}");
    assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "sent");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().iter().find(|i| i["id"] == it.id.as_str()).unwrap()["state"], "to_check");
}

#[test]
fn a_note_to_a_busy_holder_is_queued_and_says_so() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hosted_says(&sid, HostedEvent::Taken { text: "Keep working".into() });
    w.hub().desk_item_act(&it.id, "note", "Use 8px").unwrap();
    w.settle();
    assert!(w.snapshot().session(&sid)["held"]["text"].as_str().unwrap_or("").contains("Use 8px"));
    assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "kept");
}

#[test]
fn a_note_that_cannot_go_says_why_and_retry_sends_it() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    // Its session gone, and it won't start again.
    w.hosted.exits(&sid);
    w.settle();
    w.hosted.refuse("it won't start");
    w.hub().desk_set_state(&it.id, "working");
    w.hub().desk_item_act(&it.id, "note", "Use 8px").unwrap();
    w.settle();
    let s = step(&w, &project, &it.id, "to ");
    assert!(s["delivery"]["went"].as_str().unwrap().starts_with("failed: "), "{s}");
    // Back again: Retry sends the same message.
    w.hosted.accept();
    let before = sent_to(&w, &sid).len();
    w.hub().desk_item_act(&it.id, "retry", s["id"].as_str().unwrap()).unwrap();
    w.settle();
    let after = step(&w, &project, &it.id, "to ");
    assert_ne!(after["delivery"]["went"], s["delivery"]["went"], "{after}");
    assert!(!after["delivery"]["went"].as_str().unwrap().starts_with("failed"), "{after}");
    assert!(sent_to(&w, &sid).len() > before || w.snapshot().session(&sid)["held"]["text"].as_str().unwrap_or("").contains("Use 8px"));
    // One that went isn't sent twice.
    assert!(w.hub().desk_item_act(&it.id, "retry", s["id"].as_str().unwrap()).is_err());
}

#[test]
fn a_note_still_going_when_cue_stopped_offers_retry() {
    let (mut w, _, id, s) = note_in("working");
    // As if Cue stopped between keeping the note and sending it.
    crate::db::desk_put_decision(&{
        let mut d: crate::desk::DeskDecision = serde_json::from_value(s.clone()).unwrap();
        d.delivery.as_mut().unwrap().went = "sending".into();
        d
    });
    w.relaunch();
    let project = desk_project(&w, &id);
    let after = step(&w, &project, &id, "to ");
    assert_eq!(after["delivery"]["went"], crate::desk::hub::core::desk::STOPPED, "{after}");
}

fn desk_project(w: &World, id: &str) -> String {
    let snap = w.snapshot();
    snap.0["desk"]["projects"].as_array().unwrap().iter()
        .find(|p| p["items"].as_array().unwrap().iter().any(|i| i["id"] == id)).unwrap()["project"].as_str().unwrap().to_string()
}

#[test]
fn a_note_with_no_holder_is_refused_for_the_lead() {
    let w = World::new();
    let (_, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", None).unwrap();
    assert!(w.hub().desk_item_act(&it.id, "note", "Use 8px").is_err());
}

/// #861: ⌘↵ in an item's box. With its holder at work, a plain note is kept in Cue for its turn's end; one sent now
/// ("note_now") goes at once and stops the turn it's on, so it reads it next.
#[test]
fn a_note_sent_now_to_a_holder_at_work_goes_at_once() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "working");
    // It takes your message and is at work on it.
    w.send_to(&sid, "go on");
    w.hosted_says(&sid, HostedEvent::Taken { text: "go on".into() });
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    w.settle();
    assert_eq!(w.snapshot().state(&sid), "working");
    let before = sent_to(&w, &sid).len();
    w.hub().desk_item_act(&it.id, "note", "Use 8px, not 6").unwrap();
    w.settle();
    assert_eq!(sent_to(&w, &sid).len(), before, "kept for its turn's end");
    assert!(w.hosted.interrupted().is_empty());
    w.hub().unhold(&sid);
    w.hub().desk_item_act(&it.id, "note_now", "Stop, 6px is right").unwrap();
    w.settle();
    let sent = sent_to(&w, &sid);
    assert_eq!(sent.len(), before + 1, "handed over now: {sent:?}");
    assert!(sent.last().unwrap().starts_with(&format!("About #{} (Spacing): Stop, 6px is right", it.id)), "{sent:?}");
    assert_eq!(w.hosted.interrupted(), [sid.clone()], "the turn it was on is stopped");
    // On its Thread as a note of yours, as any other.
    assert_eq!(step(&w, &project, &it.id, "to ")["text"], "Stop, 6px is right");
}
