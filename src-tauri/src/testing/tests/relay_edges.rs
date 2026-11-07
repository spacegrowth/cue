//! Relay leads, executors and review cards at their edges (stability plan §4, Relay): reviews queued behind
//! every way a lead can be busy, one at a time per lead, set aside, a chain of hand-offs, an executor on a
//! machine, fifty executors, and the agent index never read twice on one thread (leads::missed).

use crate::testing::{Crew, Takes, World};
use serde_json::json;

/// Lead L, free (its turn over), with reported executors `execs`.
fn free_lead(w: &World, execs: &[&str]) {
    execs.iter().fold(Crew::default().lead("L"), |c, e| c.exec(e, "L")).install();
    w.session("L").prompt("lead").stop("Waiting for reports.");
}

fn review_state(w: &World, exec: &str) -> String {
    w.snapshot().0["reviews"][exec]["state"].as_str().unwrap_or("none").to_string()
}

#[test]
fn a_review_for_a_compacting_lead_waits_until_it_is_done() {
    let w = World::new();
    free_lead(&w, &["E"]);
    w.session("L").compacting();
    let r = w.hub().review("L", "E", "/relay:review E").unwrap();
    assert_eq!(r, "Queued: its lead is compacting, Cue sends it when it's free");
    assert_eq!(review_state(&w, "E"), "queued");
}

#[test]
fn a_review_for_a_lead_asking_you_something_goes_once_that_is_answered_and_its_turn_ends() {
    let w = World::new();
    free_lead(&w, &["E"]);
    let lead = w.session("L");
    let ask = lead.prompt("go on").ask_bash("git status");
    assert!(w.hub().review("L", "E", "/relay:review E").unwrap().starts_with("Queued: its lead is asking you something"));
    w.answer(&ask.id, json!({"behavior": "allow"}));
    lead.stop("Clean.");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["/relay:review E"]);
}

#[test]
fn two_reviews_for_one_lead_go_one_at_a_time() {
    let w = World::new();
    free_lead(&w, &["E1", "E2"]);
    assert_eq!(w.hub().review("L", "E1", "/relay:review E1").unwrap(), "Sent /relay:review E1");
    let second = w.hub().review("L", "E2", "/relay:review E2").unwrap();
    assert!(second.starts_with("Queued: its lead is reviewing E1-topic"), "{second}");
    w.session("L").stop("E1 reviewed.");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["/relay:review E1"], "a moment for the first review to show as done");
    w.advance(8000);
    w.tick(true);
    assert_eq!(w.terminal.typed("L"), ["/relay:review E1", "/relay:review E2"], "the watcher's next look sends it");
}

#[test]
fn reviews_for_two_leads_go_each_to_its_own_lead() {
    let w = World::new();
    Crew::default().lead("L").lead("M").exec("E", "L").exec("F", "M").install();
    w.session("L").prompt("lead").stop("Waiting.");
    w.session("M").prompt("lead").stop("Waiting.");
    w.hub().review("L", "E", "/relay:review E").unwrap();
    w.hub().review("M", "F", "/relay:review F").unwrap();
    assert_eq!((w.terminal.typed("L"), w.terminal.typed("M")), (vec!["/relay:review E".to_string()], vec!["/relay:review F".to_string()]));
}

#[test]
fn a_review_asked_for_twice_is_queued_once() {
    let w = World::new();
    free_lead(&w, &["E"]);
    w.session("L").prompt("more");
    w.hub().review("L", "E", "/relay:review E").unwrap();
    assert_eq!(w.hub().review("L", "E", "/relay:review E").unwrap(), "Already queued: it goes when its lead is free");
    assert_eq!(w.hub().queued_reviews(), ["E"]);
}

#[test]
fn a_review_already_typed_in_is_not_typed_again() {
    let w = World::new();
    free_lead(&w, &["E"]);
    w.hub().review("L", "E", "/relay:review E").unwrap();
    assert_eq!(w.hub().review("L", "E", "/relay:review E"), Err("its lead is already reviewing it".to_string()));
    assert_eq!(w.terminal.typed("L").len(), 1);
}

#[test]
fn a_packet_already_reviewed_is_not_reviewed_twice() {
    let w = World::new();
    Crew::default().lead("L").exec_as("E", "L", |m| m.reviewed = true).install();
    w.session("L").prompt("lead").stop("Waiting.");
    assert_eq!(w.hub().review("L", "E", "/relay:review E").unwrap(), "Its lead already reviewed this report");
    assert!(w.terminal.typed("L").is_empty());
    assert_eq!(review_state(&w, "E"), "reviewed");
}

#[test]
fn a_review_taken_back_before_its_lead_is_free_never_goes() {
    let w = World::new();
    free_lead(&w, &["E"]);
    let lead = w.session("L");
    lead.prompt("more");
    w.hub().review("L", "E", "/relay:review E").unwrap();
    w.hub().unqueue_review("E");
    lead.stop("Done.");
    w.advance(1500);
    assert!(w.terminal.typed("L").is_empty());
    assert_eq!(review_state(&w, "E"), "waiting");
}

#[test]
fn a_queued_review_reviewed_meanwhile_by_its_lead_on_auto_is_dropped() {
    let w = World::new();
    free_lead(&w, &["E"]);
    let lead = w.session("L");
    lead.prompt("more");
    w.hub().review("L", "E", "/relay:review E").unwrap();
    Crew::default().lead("L").exec_as("E", "L", |m| m.reviewed = true).install();
    lead.stop("Reviewed E on my own.");
    w.advance(1500);
    assert!(w.terminal.typed("L").is_empty(), "the same work twice");
    assert!(w.hub().queued_reviews().is_empty());
}

#[test]
fn a_message_of_yours_kept_for_the_lead_goes_before_a_queued_review() {
    let w = World::new();
    free_lead(&w, &["E"]);
    let lead = w.session("L");
    lead.prompt("more");
    w.send_to("L", "and update the plan");
    w.hub().review("L", "E", "/relay:review E").unwrap();
    lead.stop("Done.");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["and update the plan"], "yours first; the lead is at work on it");
    lead.stop("Plan updated.");
    w.advance(10_000);
    assert_eq!(w.terminal.typed("L"), ["and update the plan", "/relay:review E"]);
}

#[test]
fn a_review_its_lead_couldnt_take_is_not_left_showing_as_in_review() {
    let w = World::new();
    free_lead(&w, &["E"]);
    w.terminal.pane("L", |p| p.takes = Takes::Refuses("its pane is gone".into()));
    assert!(w.hub().review("L", "E", "/relay:review E").is_err());
    assert_eq!(review_state(&w, "E"), "waiting");
}

#[test]
fn set_aside_drops_the_packets_row_and_its_next_packet_shows_again() {
    let w = World::new();
    free_lead(&w, &["E"]);
    assert_eq!(w.hub().set_aside("E").unwrap(), "Set aside");
    assert_eq!(review_state(&w, "E"), "none");
    Crew::default().lead("L").exec_as("E", "L", |m| m.packet = Some(2)).install();
    w.hub().redraw();
    w.settle();
    assert_eq!(review_state(&w, "E"), "waiting", "packet 2 waits on you again");
}

#[test]
fn set_aside_for_an_executor_cue_doesnt_know_says_so() {
    let w = World::new();
    free_lead(&w, &[]);
    assert_eq!(w.hub().set_aside("nobody"), Err("Cue doesn't know this executor's packet".to_string()));
}

#[test]
fn a_chain_of_hand_offs_points_the_first_lead_at_the_last() {
    let w = World::new();
    Crew::default().successor("L2", "L1", 1_000).successor("L3", "L2", 2_000).exec("E", "L3").install();
    for sid in ["L1", "L2", "L3"] {
        w.session(sid).prompt("lead").stop("Handed off.");
    }
    let h = w.snapshot().0["handoffs"].clone();
    assert_eq!((h["L1"]["to"].as_str(), h["L1"]["hops"].as_u64(), h["L1"]["executors"].as_u64()), (Some("L3"), Some(2), Some(1)), "{h}");
    assert!(h["L3"].is_null(), "the one leading now hasn't handed off");
}

#[test]
fn a_hand_off_chain_that_loops_back_ends() {
    let w = World::new();
    Crew::default().successor("L2", "L1", 1_000).successor("L1", "L2", 2_000).install();
    w.session("L1").prompt("lead").stop("Handed off.");
    w.session("L2").prompt("lead").stop("Handed off.");
    let h = w.snapshot().0["handoffs"].clone();
    assert!(h.as_object().unwrap().is_empty(), "both lead again: neither handed off for good: {h}");
}

#[test]
fn an_executor_on_a_machine_is_reached_through_its_own_tmux_session_there() {
    let w = World::new();
    Crew::default().lead("L").exec_as("E", "L", |m| (m.machine, m.tmux_session) = ("box".into(), "relay-E".into())).install();
    w.session("L").prompt("lead").stop("Waiting.");
    assert_eq!(crate::leads::exec_tmux("E"), Some(("box".to_string(), "relay-E".to_string())));
    let crew = w.snapshot().0["crew"]["L"]["executors"].clone();
    assert_eq!((crew[0]["machine"].as_str(), crew[0]["tmux_session"].as_str()), (Some("box"), Some("relay-E")), "{crew}");
    assert_eq!(crate::leads::exec_tmux("L"), None, "a lead is no executor");
}

#[test]
fn fifty_executors_are_listed_under_their_lead_and_each_waits_on_you() {
    let w = World::new();
    let execs: Vec<String> = (0..50).map(|i| format!("E{i:02}")).collect();
    free_lead(&w, &execs.iter().map(String::as_str).collect::<Vec<_>>());
    let s = w.snapshot();
    assert_eq!(s.0["crew"]["L"]["executors"].as_array().unwrap().len(), 50);
    assert_eq!(s.0["reviews"].as_object().unwrap().len(), 50);
    w.hub().review("L", "E00", "/relay:review E00").unwrap();
    for e in &execs[1..] {
        w.hub().review("L", e, &format!("/relay:review {e}")).unwrap();
    }
    assert_eq!(w.hub().queued_reviews().len(), 49, "one typed in, the rest behind it in order");
    assert_eq!(w.hub().queued_reviews()[0], "E01");
}

#[test]
fn the_missed_wake_check_can_read_the_agent_index_from_its_callback() {
    let _w = World::new();
    Crew::default().lead("L").exec_as("E", "L", |m| m.ready_late = true).install();
    // The callback reads the index itself (a lead's name, its role): taken twice on one thread, the lock
    // would wait on itself for good (locking panics on it in tests).
    let missed = crate::leads::missed(|lead| crate::leads::name_of(lead) == "L" && crate::leads::is_relay_lead(lead) && !crate::leads::is_executor(lead));
    assert_eq!(missed, ["E"]);
    assert!(crate::leads::missed(|_| false).is_empty(), "a busy lead hasn't missed it yet");
}
