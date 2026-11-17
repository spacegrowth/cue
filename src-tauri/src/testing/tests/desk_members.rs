//! A Desk's own list of sessions (hub/desk.rs `desk_set_member`, `UseAsk`): its lead routes to those (and new); a
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
fn the_lead_sees_the_desks_sessions_as_routes_and_the_rest_as_ask_first() {
    let w = World::new();
    let (h, project, d) = off_desk(&w);
    let id: String = h.chars().take(8).collect();
    let block = w.hub().desk_summary(&project, &d, false).0;
    let on = block.lines().find(|l| l.starts_with("Sessions on the Desk, you can route to:")).unwrap();
    assert!(on.contains("none open but you") && !on.contains(&id), "{on}");
    let off = block.lines().find(|l| l.starts_with("Other sessions of this project, not on the Desk")).unwrap();
    assert!(off.contains(&format!("cue-teams (sid:{id})")) && off.contains("asks the user first"), "{off}");

    w.hub().desk_set_member(&project, &h, true).unwrap();
    let block = w.hub().desk_summary(&project, &d, false).0;
    let on = block.lines().find(|l| l.starts_with("Sessions on the Desk, you can route to:")).unwrap();
    assert!(on.contains(&format!("cue-teams (sid:{id}")), "{on}");
    assert!(!block.contains("Other sessions of this project"), "{block}");
}

#[test]
fn you_add_and_remove_sessions_and_one_you_give_an_item_joins() {
    let w = World::new();
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
}
