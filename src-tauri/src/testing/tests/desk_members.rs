//! A Desk's own list of sessions (desk/hub/core/desk.rs `desk_set_member`, `UseAsk`): its lead routes to those (and new); a
//! ROUTE to another session of the project asks you first, and a yes puts it on the Desk and makes the route.

use super::desk::desk;
use super::team::lead;
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// "cue-teams" (at its prompt, not on the Desk), its project, and the project's new lead (on it, as a lead is).
fn off_desk(w: &World) -> (String, String, String) {
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    (h, project, d)
}

fn members(w: &World, project: &str) -> Vec<String> {
    desk(w, project)["members"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect()
}

fn header(w: &World, project: &str) -> Value {
    desk(w, project)["items"].as_array().unwrap().iter().find(|i| i["title"] == "Header jumps").cloned().unwrap()
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

/// The lead's open cards: (id, question).
fn asks(w: &World, d: &str) -> Vec<(String, String)> {
    w.snapshot().cards_of(d).iter().map(|c| (c["id"].as_str().unwrap().to_string(), c["tool_input"]["questions"][0]["question"].as_str().unwrap_or("").to_string())).collect()
}

fn answer(w: &World, id: &str, question: &str, choice: &str) {
    w.answer(id, json!({ "behavior": "allow", "answers": { question: choice } }));
    w.settle();
}

#[test]
fn a_route_to_a_session_off_the_desk_asks_first_and_yes_adds_it_and_routes() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    assert_eq!(members(&w, &project), vec![d.clone()], "a lead is on its Desk; cue-teams isn't");
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let it = header(&w, &project);
    assert_eq!(it["state"], "new", "not routed yet: {it}");
    assert!(it["holder"].is_null());
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains("Header jumps")), "cue-teams isn't sent it before you say yes");
    let a = asks(&w, &d);
    assert_eq!(a.len(), 1, "{a:?}");
    let (card, q) = &a[0];
    assert!(q.starts_with("Use cue-teams for #") && q.contains("Header jumps") && q.contains("isn't on the Desk"), "{q}");

    // Said again in a later turn: still one card.
    ends(&w, &d, "ROUTE: #1 -> cue-teams | why: it made the header");
    assert_eq!(asks(&w, &d).len(), 1);

    answer(&w, card, q, "Yes, add it to the Desk");
    assert!(asks(&w, &d).is_empty());
    assert_eq!(members(&w, &project), vec![d.clone(), h.clone()]);
    let it = header(&w, &project);
    assert_eq!(it["state"], "working", "{it}");
    assert_eq!(it["holder"]["name"], "cue-teams");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("Header jumps")), "routed: {:?}", sent_to(&w, &h));
    assert!(sent_to(&w, &d).iter().any(|t| t.contains("cue-teams is on the Desk now")), "the lead is told: {:?}", sent_to(&w, &d));
    assert!(crate::db::desk_members().contains(&(project.clone(), h.clone())), "saved");
}

/// A yes to a route that can't be made any more (here the item was dropped meanwhile): the lead is told why, not
/// that it was routed (#248).
#[test]
fn yes_to_a_route_that_cant_be_made_now_tells_the_lead_why() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    assert!(!sent_to(&w, &d).iter().any(|t| t.starts_with("Asked:")), "the card asks; the lead isn't told so too");
    let id = header(&w, &project)["id"].as_str().unwrap().to_string();
    let (card, q) = asks(&w, &d).remove(0);
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    answer(&w, &card, &q, "Yes, add it to the Desk");
    let told = sent_to(&w, &d);
    assert!(told.iter().any(|t| t.contains(&format!("The user said yes, but Cue couldn't route #{id} to cue-teams. Refused: #{id}"))), "{told:?}");
    assert!(!told.iter().any(|t| t.contains("Cue routed")), "{told:?}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains("Header jumps")), "nothing sent");
}

#[test]
fn a_refused_route_said_while_the_lead_asks_you_something_comes_in_its_next_desk_block() {
    let w = World::new();
    let (_h, project, d) = off_desk(&w);
    let before = sent_to(&w, &d).len();
    // One route opens a card in the lead's name (cue-teams isn't on the Desk); the other is refused.
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header\nROUTE: #99 -> cue-teams");
    assert_eq!(asks(&w, &d).len(), 1);
    assert_eq!(sent_to(&w, &d).len(), before, "it can't be sent while the card is up: {:?}", sent_to(&w, &d));
    let (block, failed) = w.hub().desk_summary(&project, &d, false);
    assert!(block.contains("Refused: #99 isn't an open item on this Desk."), "{block}");
    // Told once it reaches the lead: gone from the next block.
    w.hub().desk_told(&failed);
    assert!(!w.hub().desk_summary(&project, &d, false).0.contains("Refused: #99"));
}

#[test]
fn no_leaves_the_item_and_the_desk_as_they_were() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    ends(&w, &d, "ITEM: bug | Header jumps\nROUTE: #new -> cue-teams | why: it made the header");
    let (card, q) = asks(&w, &d).remove(0);
    answer(&w, &card, &q, "No");
    assert_eq!(header(&w, &project)["state"], "new");
    assert!(!members(&w, &project).contains(&h));
    assert!(sent_to(&w, &d).iter().any(|t| t.contains("The answer: No") && t.contains("stays as it was")), "{:?}", sent_to(&w, &d));
}

#[test]
fn the_lead_sees_only_the_desks_sessions_as_routes() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    let id: String = h.chars().take(8).collect();
    let block = w.hub().desk_summary(&project, &d, false).0;
    assert!(block.contains("No Desk sessions available."), "{block}");
    assert!(!block.contains(&format!("sid:{id}")), "{block}");

    w.hub().desk_set_member(&project, &h, true).unwrap();
    let block = w.hub().desk_summary(&project, &d, false).0;
    // On the Desk with no item: on the one Older line (#363).
    let (_, on) = block.split_once("Desk routes (or use ROUTE: #<id> -> new):\n").unwrap();
    assert!(on.contains(&format!("Older, idle with no recent item:\n- cue-teams (sid:{id}, idle)")), "{on}");
    assert!(!block.contains("Off the Desk"), "{block}");
}

#[test]
fn you_add_and_remove_sessions_and_one_you_give_an_item_joins() {
    let mut w = World::new();
    let (h, project, d) = off_desk(&w);
    assert_eq!(w.hub().desk_set_member("elsewhere", &h, true).unwrap_err(), "that session isn't of elsewhere");
    w.hub().desk_set_member(&project, &h, true).unwrap();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    assert_eq!(members(&w, &project), vec![d.clone(), h.clone()], "once");
    w.hub().desk_set_member(&project, &h, false).unwrap();
    assert_eq!(members(&w, &project), vec![d.clone()]);
    assert!(!crate::db::desk_members().contains(&(project.clone(), h.clone())), "dropped from the saved list too");

    // Given an item ("Give to…"), it's on the Desk from then.
    let it = w.hub().desk_add(&project, "bug", "Header jumps", "", None).unwrap();
    w.hub().desk_give(&it.id, &h).unwrap();
    assert_eq!(members(&w, &project), vec![d.clone(), h.clone()]);
    assert!(w.hub().desk_set_member(&project, &h, false).unwrap_err().contains("work history"));
    assert!(desk(&w, &project)["work_history"].as_array().unwrap().iter().any(|sid| sid == h.as_str()));
    let reassigned = w.hub().desk_give(&it.id, &d).unwrap();
    assert_eq!(reassigned.holder.unwrap().to(), d.as_str());
    w.relaunch();
    assert!(desk(&w, &project)["work_history"].as_array().unwrap().iter().any(|sid| sid == h.as_str()));
    assert!(w.hub().desk_set_member(&project, &h, false).unwrap_err().contains("work history"));
}

#[test]
fn removing_the_dispatcher_keeps_its_session_and_persists() {
    let mut w = World::new();
    let (_, project, lead) = off_desk(&w);
    w.hub().desk_add(&project, "task", "Keep this item", "", None).unwrap();
    w.hub().desk_set_member(&project, &lead, false).unwrap();
    let d = desk(&w, &project);
    assert!(d["lead"].is_null());
    assert!(!d["members"].as_array().unwrap().iter().any(|sid| sid == lead.as_str()));
    assert!(w.hub().session_origin(&lead).is_some());
    w.relaunch();
    assert!(desk(&w, &project)["lead"].is_null());
}

fn with_work(w: &World, project: &str) -> Vec<String> {
    desk(w, project)["with_work"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect()
}

/// Only the sessions with work show (#363): an idle one with no item is "older" (the lead's routes say it on one
/// line), one holding an item shows. One idle a day with no item stays while it's live (#485: the Desk offers to close
/// it); closed, it comes off the Desk a day later without asking.
#[test]
fn a_desk_shows_sessions_with_work_and_takes_off_one_closed_a_day() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    w.hub().desk_set_member(&project, &h, true).unwrap();
    assert_eq!(with_work(&w, &project), vec![d.clone()], "cue-teams has no item: older");
    let routes = w.hub().desk_summary(&project, &d, false).0;
    assert!(routes.contains("Older, idle with no recent item:\n- cue-teams (sid:"), "{routes}");
    assert!(!routes.split("Older, idle").next().unwrap().contains("- cue-teams (sid:"), "only under Older: {routes}");

    // Idle under a day: it stays on the Desk.
    w.advance(23 * 60 * 60_000);
    w.hub().desk_retire_idle();
    assert_eq!(members(&w, &project), vec![d.clone(), h.clone()]);

    // Given an item, it has work: a line of its own, and it stays however long it holds it.
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    assert!(with_work(&w, &project).contains(&h));
    let routes = w.hub().desk_summary(&project, &d, false).0;
    assert!(routes.contains("- cue-teams (sid:") && !routes.contains("Older,"), "{routes}");
    ends(&w, &h, "Still on it.");
    w.advance(2 * 24 * 60 * 60_000);
    w.hub().desk_retire_idle();
    assert!(members(&w, &project).contains(&h), "it holds an open item");

    // Its item gone and a day idle: off the Desk, saved, with no card asking you.
    let id = header(&w, &project)["id"].as_str().unwrap().to_string();
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    w.hub().desk_retire_idle();
    assert!(members(&w, &project).contains(&h), "live, idle over a day, no item: it stays, offered to be closed");
    // Closed: a day later it comes off.
    w.hub().mark_state(&w.hub().session_origin(&h).unwrap(), "stopped");
    w.settle();
    w.hub().desk_retire_idle();
    assert!(members(&w, &project).contains(&h), "closed under a day ago");
    w.advance(24 * 60 * 60_000);
    w.hub().desk_retire_idle();
    assert!(!members(&w, &project).contains(&h), "closed over a day, no item");
    assert!(!crate::db::desk_members().contains(&(project.clone(), h.clone())), "saved");
    assert!(members(&w, &project).contains(&d), "the lead stays");
    assert!(!asks(&w, &d).iter().any(|(_, q)| q.contains("cue-teams")), "no card asks about it: {:?}", asks(&w, &d));
}
