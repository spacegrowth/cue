//! Desk spec (#498): queued items and waits, written from the spec (docs/desk-routing-rules.md rules 7-9,
//! docs/desk-routing-merged.md rules 9-11, closed items #250, #256, #340, #385, #387, #401, #476), not from the code.
//! An item queued after another starts by itself once nothing it waits on is still open, never waits on an item that
//! can't move, and stays queued across a relaunch.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to, summary};
use crate::testing::World;

/// cue-teams at work on "Header" (a), and "Footer" (b) queued for it after a: (holder, project, lead, a, b).
fn queued(w: &World) -> (String, String, String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    ends(w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows the header");
    let a = item(&desk(w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(w, &h, "On it.");
    ends(w, &l, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(w, &project), "Footer")["id"].as_str().unwrap().to_string();
    let it = item(&desk(w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    (h, project, l, a, b)
}

/// `queued`, plus "Logo" (c) queued after b for a second session, "cue-two" (t): a chain a <- b <- c. cue-teams
/// already holds two open items, the most a session holds (merged routing rule 6), so the third goes to another.
fn chain(w: &World) -> (String, String, String, String, String, String, String) {
    let (h, project, l, a, b) = queued(w);
    let t = second(w, "cue-two");
    ends(w, &l, &format!("ITEM: Logo\nROUTE: #new -> cue-two | after: #{b} | why: same file as #{b}"));
    let c = item(&desk(w, &project), "Logo")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(w, &project), "Logo")["after"], b.as_str());
    (h, project, l, a, b, c, t)
}

/// Whether session `h` was handed item `id` (its "Desk item #id" words).
fn handed(w: &World, h: &str, id: &str) -> bool {
    sent_to(w, h).iter().any(|t| t.starts_with(&format!("Desk item #{id}")) && t.contains(&format!("Say DONE: #{id}")))
}

/// Routing rules, "Open": an item waiting for your check doesn't count, and Cue won't queue anything behind it. The
/// item starts now, and the lead hears it wasn't queued.
#[test]
fn nothing_queues_behind_an_item_waiting_for_your_check() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows the header");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    ends(&w, &l, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let it = item(&desk(&w, &project), "Footer").clone();
    let b = it["id"].as_str().unwrap().to_string();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    assert!(handed(&w, &h, &b), "{:?}", sent_to(&w, &h));
    assert!(summary(&w, &project).contains(&format!("Cue couldn't queue #{b} after #{a}")), "the lead is told");
}

/// Queued items start by themselves when the item ahead is dropped (#340: no one has to nudge it).
#[test]
fn a_queued_item_starts_by_itself_when_the_item_ahead_is_dropped() {
    let w = World::new();
    let (h, project, _, a, b) = queued(&w);
    assert!(!handed(&w, &h, &b));
    w.hub().desk_item_act(&a, "drop", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    assert!(handed(&w, &h, &b), "{:?}", sent_to(&w, &h));
}

/// The queue keeps two items off the same files (routing rules 8, and the lead's guidance: "chain each one after the
/// previous item"): dropping the middle of a chain doesn't start the last while the first is still open. It waits on
/// the first in its place, as MERGE and Put before hand a wait on, and starts once the first is done.
#[test]
fn dropping_the_middle_of_a_chain_leaves_the_last_waiting_on_the_first() {
    let w = World::new();
    let (h, project, _, a, b, c, t) = chain(&w);
    w.hub().desk_item_act(&b, "drop", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Logo").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    assert!(!handed(&w, &t, &c), "not while #{a} is at work on the same files");
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &project), "Logo")["state"], "working");
    assert!(handed(&w, &t, &c));
}

/// Parked (#385): a parked item is never started by Cue, so nothing may be left waiting on it. The item queued after a
/// parked one waits on what that one waited on, and starts once that's done.
#[test]
fn parking_the_middle_of_a_chain_leaves_the_last_waiting_on_the_first() {
    let w = World::new();
    let (h, project, _, a, b, c, t) = chain(&w);
    w.hub().desk_item_act(&b, "park", "").unwrap();
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "held");
    let it = item(&desk(&w, &project), "Logo").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &project), "Logo")["state"], "working", "it doesn't wait on the parked one");
    assert!(handed(&w, &t, &c));
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "held", "the parked one stays parked");
}

/// Parked (#385) with nothing ahead of it: a parked item never starts, so what was queued after it starts now, as for
/// a dropped item.
#[test]
fn parking_the_item_ahead_starts_what_waited_on_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    // Menu: added, not routed yet (new). Footer queued for cue-teams after it.
    ends(&w, &l, "ITEM: Menu");
    let m = item(&desk(&w, &project), "Menu")["id"].as_str().unwrap().to_string();
    ends(&w, &l, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{m} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    w.hub().desk_item_act(&m, "park", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    assert!(handed(&w, &h, &b), "{:?}", sent_to(&w, &h));
}

/// Start now (#401, #476: "Start now where the item offers it"; desk_item_act: "not waiting on another any more"): an
/// item queued after another, for a session, starts now with that session: it's handed the item, as a release
/// would, not told to pick up work it never began.
#[test]
fn start_now_on_an_item_queued_after_another_hands_it_to_its_holder_now() {
    let w = World::new();
    let (h, project, _, a, b) = queued(&w);
    w.hub().desk_item_act(&b, "start_now", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    assert!(handed(&w, &h, &b), "handed the item: {:?}", sent_to(&w, &h));
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}")) && t.contains("Pick up where you left off")), "it never began");
    assert!(it["restart"].as_str().unwrap_or("").is_empty(), "not a Resume: {it}");
    // The one it waited on is untouched.
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
    let _ = a;
}

/// Put before (desk_queue.rs covers the move): it takes the place of an item still queued, not one already at work;
/// that's refused with a plain reason and nothing moves.
#[test]
fn put_before_an_item_already_at_work_is_refused_and_nothing_moves() {
    let w = World::new();
    let (h, project, _, a, b) = queued(&w);
    let e = w.hub().desk_item_act(&b, "put_before", "").unwrap_err();
    assert!(e.contains("already being worked on"), "{e}");
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    assert!(!handed(&w, &h, &b));
}

/// A queued item stays queued across a relaunch (with what it waits on), and still starts by itself once the item
/// ahead is done.
#[test]
fn a_queued_item_stays_queued_across_a_relaunch_and_starts_when_the_one_ahead_is_done() {
    let mut w = World::new();
    let (h, project, _, a, b) = queued(&w);
    w.relaunch();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    assert!(!handed(&w, &h, &b), "not started by the relaunch");
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "working");
    assert!(handed(&w, &h, &b));
}

/// The Waits list (#250): each queued item says what it waits on, and since when; once that's done it's off the list.
#[test]
fn the_waits_list_names_what_each_queued_item_waits_on_and_since_when() {
    let w = World::new();
    let (h, project, _, a, b) = queued(&w);
    let d = desk(&w, &project);
    let waits = d["waits"].as_array().unwrap();
    let wb: Vec<_> = waits.iter().filter(|x| x["item"] == b.as_str()).collect();
    assert_eq!(wb.len(), 1, "{waits:?}");
    assert_eq!((wb[0]["on"].as_str(), wb[0]["kind"].as_str()), (Some(a.as_str()), Some("queued")), "{waits:?}");
    assert!(wb[0]["since"].as_u64().unwrap_or(0) > 0, "{waits:?}");
    ends(&w, &h, &format!("DONE: #{a}"));
    let d = desk(&w, &project);
    assert!(!d["waits"].as_array().unwrap().iter().any(|x| x["item"] == b.as_str()), "{}", d["waits"]);
}

/// The Waits list (#250): you clear a wait by cutting its link ("Cut this link" on the item), and the queued item
/// starts now; a link that isn't there is refused.
#[test]
fn you_cut_a_queued_items_wait_and_it_starts_now() {
    let w = World::new();
    let (h, project, _, a, b) = queued(&w);
    assert!(w.hub().desk_item_act(&a, "cut", &b).is_err(), "#{a} doesn't wait on #{b}");
    w.hub().desk_item_act(&b, "cut", &a).unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    assert!(handed(&w, &h, &b));
}

/// A chain moves one step at a time: when the first is done the second starts, and the third still waits on the second.
#[test]
fn a_chain_moves_one_step_at_a_time() {
    let w = World::new();
    let (h, project, _, a, b, c, t) = chain(&w);
    ends(&w, &h, &format!("DONE: #{a}"));
    let d = desk(&w, &project);
    assert_eq!(item(&d, "Footer")["state"], "working");
    let it = item(&d, "Logo");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(b.as_str())), "{it}");
    assert!(!handed(&w, &t, &c));
    ends(&w, &h, &format!("DONE: #{b}"));
    assert_eq!(item(&desk(&w, &project), "Logo")["state"], "working");
    assert!(handed(&w, &t, &c));
}

/// Routing rules 9 / merged rule 11: with the item ahead open over 30 minutes the lead asks you (wait, or a smaller
/// piece now); with no answer the item waits "and the lead's line says how long the item ahead has been open". The
/// lead can only say so if its Desk block shows how long that item has been at work.
#[test]
#[ignore = "#498 → to file: the lead can't see how long the item ahead has been at work: its Desk block gives no age for an item at work"]
fn the_lead_sees_how_long_the_item_ahead_has_been_at_work() {
    let w = World::new();
    let (_, project, _, a, _) = queued(&w);
    w.advance(45 * 60_000);
    let s = summary(&w, &project);
    let line = s.lines().find(|l| l.starts_with(&format!("- #{a} "))).unwrap_or_default().to_string();
    assert!(line.contains("45m") || line.contains("45 min"), "{line}");
}
