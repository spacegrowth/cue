//! Relay reviews typed into a lead (stability plan §4, Relay): sent when it's free, queued while it works,
//! and the window's review rows built while a lead is busy.

use crate::leads::{install_index, reported_exec, Member};
use crate::testing::World;

/// Lead L (relay) and its executors, each reported.
fn crew(execs: &[&str]) {
    let lead = Member { role: "lead".into(), plugin: "relay".into(), name: "lead".into(), ..Default::default() };
    let mut index: std::collections::HashMap<String, Member> = [("L".to_string(), lead)].into();
    for e in execs {
        index.insert(e.to_string(), Member { name: format!("{e}-topic"), id: format!("relay-{e}"), ready_late: true, ..reported_exec() });
    }
    install_index(index);
}

#[test]
fn a_review_for_a_free_lead_is_typed_into_it() {
    let w = World::new();
    crew(&["E1"]);
    w.session("L").prompt("lead").stop("Waiting for reports.");
    let r = w.hub().review("L", "E1", "/relay:review E1").unwrap();
    w.settle();
    assert_eq!(r, "Sent /relay:review E1");
    assert_eq!(w.terminal.typed("L"), ["/relay:review E1"]);
    assert_eq!(w.snapshot().0["reviews"]["E1"]["state"], "reviewing");
}

#[test]
fn a_review_for_a_working_lead_is_queued_and_sent_once_its_turn_ends() {
    let w = World::new();
    crew(&["E1"]);
    let lead = w.session("L");
    lead.prompt("lead");
    let r = w.hub().review("L", "E1", "/relay:review E1").unwrap();
    assert!(r.starts_with("Queued: its lead is busy"), "{r}");
    assert_eq!(w.snapshot().0["reviews"]["E1"]["state"], "queued");
    lead.stop("Done with that.");
    assert!(w.terminal.typed("L").is_empty(), "a moment for its hooks to settle first");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["/relay:review E1"]);
    assert!(w.snapshot().0["review_queue"].as_object().unwrap().is_empty());
}

#[test]
fn a_queued_review_whose_executor_is_no_longer_done_is_dropped_not_sent() {
    let w = World::new();
    crew(&["E1"]);
    let lead = w.session("L");
    lead.prompt("lead");
    w.hub().review("L", "E1", "/relay:review E1").unwrap();
    install_index(Default::default());
    lead.stop("Done.");
    w.advance(1500);
    assert!(w.terminal.typed("L").is_empty());
}

/// The review rows ask whether each lead is free while the agent index is read: the index must be let go
/// first, or a busy lead's "reviewing <name>" (which reads the index again) waits on itself for good.
#[test]
fn the_window_state_builds_while_a_lead_that_missed_a_wake_is_reviewing() {
    let w = World::new();
    crew(&["E0", "E1"]);
    w.session("L").prompt("lead").stop("Waiting.");
    w.hub().review("L", "E0", "/relay:review E0").unwrap();
    w.settle();
    // L took the review (its hook came): working, with a review of E0 in hand; E1's wake was missed.
    let s = w.snapshot();
    assert_eq!(s.state("L"), "working");
    assert_eq!(s.0["reviews"]["E0"]["state"], "reviewing");
    assert_eq!((s.0["reviews"]["E1"]["state"].as_str(), s.0["reviews"]["E1"]["missed"].as_bool()), (Some("waiting"), Some(false)), "busy: not a missed wake yet");
}
