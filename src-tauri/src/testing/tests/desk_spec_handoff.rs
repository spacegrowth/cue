//! Desk spec (#498): a session handed off to a fresh one, and its items moving with it (desk/hub/lead/handoff.rs,
//! docs/desk-routing-rules.md "Fit with the context bands", docs/desk-routing-merged.md rules 1 and 17). The size marks
//! and the lead's own hand-off: desk_context.rs; the lead changing: desk_lead.rs, desk_spaces.rs.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to};
use crate::testing::World;

/// Routing rules, "Fit with the context bands": "an item routed to a session marked 'will hand off at expiry' must
/// move with the hand-off"; merged rule 17: "a fresh session takes over with its record". Every item it holds that
/// isn't done, at work, waiting behind it, or with you to check, is the fresh session's, and that one is on the Desk.
#[test]
fn every_item_a_handed_off_session_holds_moves_to_the_session_that_took_over() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let one = item(&desk(&w, &project), "Header jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Fixed.\nDONE: #{one}"));
    ends(&w, &d, "ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | why: it knows the page");
    let two = item(&desk(&w, &project), "Footer is cut off")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: bug | Menu flickers | done when: it doesn't\nROUTE: #new -> cue-teams | after: #{two} | why: same files"));
    let next = second(&w, "fresh");
    w.hub().desk_set_member(&project, &next, false).unwrap();
    w.hub().desk_carry(&h, &next, "fresh");
    w.settle();
    let dk = desk(&w, &project);
    for (title, state) in [("Header jumps", "to_check"), ("Footer is cut off", "working"), ("Menu flickers", "queued")] {
        let it = item(&dk, title);
        assert_eq!((it["holder"]["sid"].as_str(), it["state"].as_str()), (Some(next.as_str()), Some(state)), "{title}: {it}");
    }
    let members: Vec<String> = dk["members"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect();
    assert!(members.contains(&next), "the fresh session is on the Desk: {members:?}");
    // A send-back now goes to the session that took over (merged rule 17), not to the one handed off.
    let before = sent_to(&w, &h).len();
    ends(&w, &d, &format!("ROUTE: #{one} -> fresh | why: the sticky part too"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "working");
    assert!(sent_to(&w, &next).iter().any(|t| t.contains("the sticky part too")), "{:?}", sent_to(&w, &next));
    assert_eq!(sent_to(&w, &h).len(), before);
}
