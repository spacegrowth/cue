//! A message sent as a turn ends: the sender read the session as working, and the turn's end (with its look for
//! a kept message) was handled before the message was kept. It must still go, not wait for a turn end that has
//! already passed (a lead's review handled by itself, sending fixes from the turn that filed it).

use crate::hosted::{HostedEvent, Start};
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::{Transcript, World};
use serde_json::json;

#[test]
fn a_message_kept_as_the_turn_ends_goes_instead_of_waiting_for_the_next_end() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("refactor the router");
    assert_eq!(w.snapshot().state("a"), "working");
    // It reads as working; before the message is kept, the turn ends and is handled.
    let r = w.hub().send_with_meanwhile("a", "now fix the review's findings", || {
        a.stop("Router done.");
    });
    w.settle();
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(w.terminal.typed("a"), ["now fix the review's findings"], "typed: its turn had ended ({r:?})");
    assert!(w.snapshot().session("a")["held"].is_null(), "nothing left kept: {}", w.snapshot().session("a"));
}

#[test]
fn a_message_kept_as_a_hosted_turn_ends_goes_too() {
    let w = World::new();
    crate::config::set("hosted.on", json!(true)).unwrap();
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "long job", "trial", &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    assert!(matches!(w.hosted.launched()[0].start, Start::New(_)));
    w.hosted_says(&sid, HostedEvent::Taken { text: "long job".into() });
    let log = w.hub().session_origin(&sid).unwrap().transcript_path;
    Transcript::at(&log).prompt("long job");
    assert_eq!(w.snapshot().state(&sid), "working");
    let r = w.hub().send_with_meanwhile(&sid, "send your teammate the fixes", || {
        w.hosted_says(&sid, HostedEvent::TurnEnded { text: "Reviewed.".into(), failed: false, cost_usd: 0.1, unprompted: false });
    });
    w.settle();
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "send your teammate the fixes".to_string())), "handed over: its turn had ended ({r:?})");
    assert!(w.snapshot().session(&sid)["held"].is_null(), "nothing left kept: {}", w.snapshot().session(&sid));
}
