//! Desk spec (#498): your notes and quick phrases from an item's page reach its holder, held while it's busy and
//! delivered after, and a quick phrase never sends an item to check back as not done.

use super::desk::{desk, holder, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::Value;

/// Item `id` as the window has it.
fn shown(w: &World, project: &str, id: &str) -> Value {
    desk(w, project)["items"].as_array().unwrap().iter().find(|i| i["id"] == id).cloned().unwrap_or_else(|| panic!("no item #{id}"))
}

/// Your latest step on item `id` (its why starting `why`).
fn step(w: &World, project: &str, id: &str, why: &str) -> Value {
    desk(w, project)["decisions"].as_array().unwrap().iter().rev()
        .find(|d| d["item_id"] == id && d["by"] == "You" && d["why"].as_str().unwrap_or("").starts_with(why))
        .cloned().unwrap_or_else(|| panic!("no step {why} on #{id}: {}", desk(w, project)["decisions"]))
}

/// Holder `sid` busy with something else (it took a prompt and hasn't ended its turn).
fn busy(w: &World, sid: &str) {
    w.advance(1000);
    w.hosted_says(sid, HostedEvent::Taken { text: "Something else".into() });
    w.settle();
}

/// Holder `sid` ends its turn.
fn ends_turn(w: &World, sid: &str) {
    w.advance(1000);
    w.hosted_says(sid, HostedEvent::TurnEnded { text: "Finished that.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.advance(1000);
    w.settle();
}

/// #437 done when, #419: a note sent while its holder is busy is held for it ("your note goes in next") and goes in
/// once its turn ends; never lost.
#[test]
fn a_note_held_while_its_holder_is_busy_goes_in_when_its_turn_ends() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.settle();
    busy(&w, &sid);
    w.hub().desk_item_act(&it.id, "note", "Use 8px, not 6").unwrap();
    w.settle();
    let words = format!("About #{} (Spacing): Use 8px, not 6", it.id);
    assert!(!sent_to(&w, &sid).iter().any(|t| t.starts_with(&words)), "not typed over its turn: {:?}", sent_to(&w, &sid));
    assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "kept");
    ends_turn(&w, &sid);
    assert!(sent_to(&w, &sid).iter().any(|t| t.starts_with(&words)), "goes in after its turn: {:?}", sent_to(&w, &sid));
    assert!(w.snapshot().session(&sid)["held"].is_null(), "nothing left held: {}", w.snapshot().session(&sid)["held"]);
}

/// #584 (in place of #437's "a to-check item that gets a note reopens"), desk_stall.rs ("a turn on its item"): a note
/// typed on an item to check while its holder is busy reaches the holder once its turn ends, and the item stays to
/// check; that turn, about something else, isn't the item's, so the holder isn't asked "Done?" nor the item stopped.
#[test]
fn a_typed_note_on_an_item_to_check_while_its_holder_is_busy_goes_in_after_and_it_stays_to_check() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "to_check");
    busy(&w, &sid);
    let after = w.hub().desk_item_act(&it.id, "note", "Still 6px on the left").unwrap();
    w.settle();
    assert_eq!(after.state, "to_check");
    ends_turn(&w, &sid);
    let sent = sent_to(&w, &sid);
    assert!(sent.iter().any(|t| t.starts_with(&format!("About #{} (Spacing): Still 6px on the left", it.id))), "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("isn't done") || t.starts_with("Done? Write DONE")), "{sent:?}");
    let now = shown(&w, &project, &it.id);
    assert_eq!((now["state"].as_str(), now["stopped"].as_bool().unwrap_or(false)), (Some("to_check"), false), "{now}");
}

/// #488 done when, #477 (its phrases): each quick phrase clicked on a to-check item reaches its holder as a note, as
/// its own message, and the item stays to check; none is sent as "isn't done".
#[test]
fn every_quick_phrase_on_an_item_to_check_reaches_its_holder_and_it_stays_to_check() {
    for phrase in ["Go ahead", "Commit", "Push+Install", "Release"] {
        let w = World::new();
        let (sid, project) = holder(&w);
        let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
        w.hub().desk_set_state(&it.id, "to_check");
        let after = w.hub().desk_item_act(&it.id, "phrase", phrase).unwrap();
        w.settle();
        assert_eq!(after.state, "to_check", "{phrase}");
        let sent = sent_to(&w, &sid);
        assert!(sent.iter().any(|t| t.starts_with(&format!("About #{} (Spacing): {phrase}", it.id))), "{phrase}: {sent:?}");
        assert!(!sent.iter().any(|t| t.contains("isn't done")), "{phrase}: {sent:?}");
        assert_eq!(shown(&w, &project, &it.id)["state"], "to_check", "{phrase}");
        assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "sent", "{phrase}");
    }
}

/// #488 with #419: a quick phrase on a to-check item whose holder is busy waits for it, goes in after its turn, and
/// the item stays to check meanwhile.
#[test]
fn a_quick_phrase_while_its_holder_is_busy_waits_and_the_item_stays_to_check() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "to_check");
    busy(&w, &sid);
    assert_eq!(w.hub().desk_item_act(&it.id, "phrase", "Commit").unwrap().state, "to_check");
    w.settle();
    assert_eq!(step(&w, &project, &it.id, "to ")["delivery"]["went"], "kept");
    assert_eq!(shown(&w, &project, &it.id)["state"], "to_check");
    ends_turn(&w, &sid);
    assert!(sent_to(&w, &sid).iter().any(|t| t.starts_with(&format!("About #{} (Spacing): Commit", it.id))), "{:?}", sent_to(&w, &sid));
    assert!(!sent_to(&w, &sid).iter().any(|t| t.contains("isn't done")), "{:?}", sent_to(&w, &sid));
}

/// #477 done when: a quick phrase on a working item's page is sent at once to its holder, as its own message.
#[test]
fn a_quick_phrase_on_a_working_item_goes_to_its_holder_at_once() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hub().desk_set_state(&it.id, "working");
    w.hub().desk_item_act(&it.id, "phrase", "Go ahead").unwrap();
    w.settle();
    let sent = sent_to(&w, &sid);
    assert!(sent.iter().any(|t| t.starts_with(&format!("About #{} (Spacing): Go ahead", it.id))), "{sent:?}");
    assert_eq!(shown(&w, &project, &it.id)["state"], "working");
}

/// #437 done when ("whatever the item's state"): two notes in a row while the holder is busy both reach it; the
/// second doesn't replace the first.
#[test]
fn two_notes_while_its_holder_is_busy_both_reach_it() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.settle();
    busy(&w, &sid);
    w.hub().desk_item_act(&it.id, "note", "Use 8px, not 6").unwrap();
    w.settle();
    w.hub().desk_item_act(&it.id, "note", "And the top margin too").unwrap();
    w.settle();
    ends_turn(&w, &sid);
    // Taken: it may come as one message or two, but both notes reach it.
    let all = sent_to(&w, &sid).join("\n");
    assert!(all.contains("Use 8px, not 6") && all.contains("And the top margin too"), "{all}");
}
