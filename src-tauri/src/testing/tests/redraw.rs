//! Drawing the window after a change is coalesced (hub/redraw.rs): a burst of changes is drawn once or
//! twice, not once each, and what's drawn is the latest state.

use crate::testing::World;
use serde_json::json;

#[test]
fn a_thousand_changes_are_drawn_once_and_the_last_one_shows() {
    let w = World::new();
    w.session("p").pi().prompt("build it");
    let before = w.hub().draws();
    let c = w.client();
    for i in 0..1000 {
        c.drive(json!({"type": "event", "event": "doing", "session_id": "p", "harness": "pi", "cwd": "/x/p", "message": format!("Running: step {i}")}));
    }
    w.settle();
    let draws = w.hub().draws() - before;
    assert!(draws <= 2, "1,000 changes drawn {draws} times");
    assert_eq!(w.snapshot().session("p")["doing"], "Running: step 999", "the latest state drawn");
}

#[test]
fn a_change_after_a_draw_is_drawn_again() {
    let w = World::new();
    let p = w.session("p").pi();
    p.prompt("one");
    let before = w.hub().draws();
    p.prompt("two");
    p.stop("done");
    assert_eq!(w.hub().draws() - before, 2, "each settled change drawn once");
    assert_eq!(w.snapshot().state("p"), "waiting");
}

/// A notification about something that isn't a card (a usage limit, a session gone quiet) stays up through
/// the draw that follows it; a card's goes once the card does.
#[test]
fn a_draw_takes_back_only_answered_cards_notifications() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all");
    a.failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    assert_eq!(w.notes.shown(), ["usage"]);
    let ask = w.session("b").prompt("clean").ask_permission("Bash", json!({"command": "rm -rf build"}));
    w.answer(&ask.id, json!({"behavior": "allow"}));
    assert_eq!(w.notes.taken_back(), [ask.id.clone()], "the answered card's, and nothing else");
}
