//! Desk spec (#498): who takes an item, from docs/desk-routing-rules.md and docs/desk-routing-merged.md (the merged
//! one supersedes the sizes). Routes made and refused in general: desk_routing.rs, desk_routes.rs; off the Desk:
//! desk_members.rs; queued items: desk_queue.rs.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::testing::World;

/// The id of `project`'s item titled `title`.
fn id_of(w: &World, project: &str, title: &str) -> String {
    item(&desk(w, project), title)["id"].as_str().unwrap().to_string()
}

/// One item per session (#576, replacing merged rule 6's two): a route to a session that holds an open item queues the
/// new one for it, with no holder yet, and its holder isn't sent it. The lead is told when it goes to a fresh session.
#[test]
fn a_route_to_a_session_with_an_open_item_queues_it_for_that_session() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = id_of(&w, &project, "Header jumps");
    let before = sent_to(&w, &d).len();
    ends(&w, &d, "ITEM: bug | Menu flickers | done when: it doesn't\nROUTE: #new -> cue-teams | why: it knows the header");
    let two = item(&desk(&w, &project), "Menu flickers").clone();
    assert_eq!((two["state"].as_str(), two["holder"].is_null(), two["next_for"]["sid"].as_str()), (Some("queued"), true, Some(h.as_str())), "{two}");
    assert!(two["fresh_at"].as_u64().is_some(), "{two}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains("Menu flickers")), "the holder isn't handed a second");
    let sent = sent_to(&w, &d);
    assert_eq!(sent.len(), before + 1, "one answer: {sent:?}");
    let id = two["id"].as_str().unwrap();
    assert!(sent.last().unwrap().starts_with(&format!("Queued: #{id} is next for cue-teams (busy with #{one}); it goes to a fresh session at ")), "{sent:?}");
}

/// One item per session (#576): an item waiting for your check keeps its session (it keeps the item until Looks
/// right), so a route to it queues; Looks right frees it and the queued item starts there.
#[test]
fn an_item_waiting_for_your_check_keeps_its_session_until_looks_right() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = id_of(&w, &project, "Header jumps");
    ends(&w, &h, &format!("Fixed.\nDONE: #{one}"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "to_check");
    ends(&w, &d, "ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | why: it knows the page");
    let two = id_of(&w, &project, "Footer is cut off");
    assert_eq!(item(&desk(&w, &project), "Footer is cut off")["state"], "queued");
    w.hub().desk_item_act(&one, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer is cut off").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(h.as_str())), "{it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{two}"))), "{:?}", sent_to(&w, &h));
}

/// Merged rule 6 counts open items, not routes: one the session holds already, sent back to it with what's left,
/// stays with it while it holds two (routing rules, "Open": "If you send it back, it is at work again").
#[test]
fn a_send_back_of_an_item_the_session_holds_isnt_a_third() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = id_of(&w, &project, "Header jumps");
    ends(&w, &d, &format!("ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | after: #{one} | why: same files"));
    let before = sent_to(&w, &d).len();
    ends(&w, &d, &format!("ROUTE: #{one} -> cue-teams | why: also the sticky part"));
    assert_eq!(sent_to(&w, &d).len(), before, "{:?}", sent_to(&w, &d));
    assert!(sent_to(&w, &h).last().unwrap().contains("also the sticky part"), "{:?}", sent_to(&w, &h));
}

/// Merged build order 1: the lead's instructions give the rule, so it routes by it (#576: one item per session).
#[test]
fn the_leads_instructions_say_one_item_per_session() {
    assert!(crate::desk::LEAD_SKILL.contains("One item per session, at work or waiting for the user's check"));
    assert!(crate::desk::LEAD_SKILL.contains("A route to a busy session queues the item for it"));
}
