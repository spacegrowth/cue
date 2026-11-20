//! Desk spec (#498): who takes an item, from docs/desk-routing-rules.md and docs/desk-routing-merged.md (the merged
//! one supersedes the sizes). Routes made and refused in general: desk_routing.rs, desk_routes.rs; off the Desk:
//! desk_members.rs; queued items: desk_queue.rs.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::testing::World;

/// The id of `project`'s item titled `title`.
fn id_of(w: &World, project: &str, title: &str) -> String {
    item(&desk(w, project), title)["id"].as_str().unwrap().to_string()
}

/// Merged rule 6: "A session holds at most two open items: one at work, one waiting. Cue refuses a third and tells
/// the lead to route it again." The third stays new, nobody holds it, and its holder isn't sent it.
#[test]
#[ignore = "#498 → to file: Cue refuses a third open item: both routing docs are proposals not picked yet, and today one session can hold a chain of any length"]
fn a_session_with_two_open_items_is_refused_a_third_and_the_lead_is_told_to_route_it_again() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = id_of(&w, &project, "Header jumps");
    ends(&w, &d, &format!("ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | after: #{one} | why: same files"));
    let two = id_of(&w, &project, "Footer is cut off");
    assert_eq!(item(&desk(&w, &project), "Footer is cut off")["state"], "queued");
    let before = sent_to(&w, &d).len();
    ends(&w, &d, "ITEM: bug | Menu flickers | done when: it doesn't\nROUTE: #new -> cue-teams | why: it knows the header");
    let three = item(&desk(&w, &project), "Menu flickers").clone();
    assert_eq!((three["state"].as_str(), three["holder"].is_null()), (Some("new"), true), "{three}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains("Menu flickers")), "the holder isn't handed a third");
    let sent = sent_to(&w, &d);
    assert_eq!(sent.len(), before + 1, "one answer: {sent:?}");
    let told = sent.last().unwrap();
    let id = three["id"].as_str().unwrap();
    assert!(told.starts_with(&format!("Refused: #{id}: cue-teams already holds two open items (#{one}, #{two})")) && told.contains("You can route it again"), "{told}");
    // Routed again, to new, it's made.
    ends(&w, &d, &format!("ROUTE: #{id} -> new | why: cue-teams is full"));
    assert_ne!(item(&desk(&w, &project), "Menu flickers")["state"], "new");
}

/// Routing rules, "Open": "An item waiting for your check does not count toward the cap of two." A session with
/// one to check and one at work takes one more to wait behind it.
#[test]
fn an_item_waiting_for_your_check_doesnt_count_toward_the_two() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = id_of(&w, &project, "Header jumps");
    ends(&w, &h, &format!("Fixed.\nDONE: #{one}"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "to_check");
    ends(&w, &d, "ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | why: it knows the page");
    let two = id_of(&w, &project, "Footer is cut off");
    assert_eq!(item(&desk(&w, &project), "Footer is cut off")["state"], "working");
    let before = sent_to(&w, &d).len();
    ends(&w, &d, &format!("ITEM: bug | Menu flickers | done when: it doesn't\nROUTE: #new -> cue-teams | after: #{two} | why: same files"));
    assert_eq!(item(&desk(&w, &project), "Menu flickers")["state"], "queued");
    assert_eq!(sent_to(&w, &d).len(), before, "made, so not answered: {:?}", sent_to(&w, &d));
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

/// Merged build order 1: the lead's instructions give the cap, so it routes by it and isn't only refused.
#[test]
#[ignore = "#498 → to file: Cue refuses a third open item: both routing docs are proposals not picked yet, and today one session can hold a chain of any length"]
fn the_leads_instructions_say_a_session_holds_at_most_two_open_items() {
    assert!(crate::desk::LEAD_SKILL.contains("A session holds at most two open items: one at work, one waiting behind it. Cue refuses a third"));
}
