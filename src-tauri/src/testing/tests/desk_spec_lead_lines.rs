//! Desk spec (#498): the lead's lines and Cue's answers to them (desk/mod.rs LEAD_SKILL, #248, docs/cue-messages.md
//! "Replies to its commands"). Route answers in general: desk_routes.rs; unstick lines: desk_unstick.rs.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::testing::World;

/// #248 and LEAD_SKILL ("each answer costs it a turn"; one it can't make is answered "in one message after your
/// turn"); docs/cue-messages.md shows a reply to its commands as one line with "N more lines". Every line of one turn
/// that Cue can't make, a ROUTE, a SPLIT, a MERGE and a CUT, comes back in one message, none dropped.
#[test]
fn every_line_of_one_turn_cue_cant_make_comes_back_in_one_message() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let before = sent_to(&w, &d).len();
    ends(&w, &d, "On it.\nROUTE: #99 -> cue-teams | why: old one\nSPLIT: #98 | Header | Footer\nMERGE: #97 into #96 | why: same\nCUT: #95 -> #94 | why: circle");
    let sent = sent_to(&w, &d);
    assert_eq!(sent.len(), before + 1, "one message: {:?}", &sent[before..]);
    let told = sent.last().unwrap();
    for said in ["Refused: #99 isn't an open item", "#98 not split: this Desk has no item #98", "#97 not merged: this Desk has no item #97", "#95 not cut: this Desk has no item #95"] {
        assert!(told.contains(said), "{said}: {told}");
    }
}

/// LEAD_SKILL, PARK: "Neither Cue nor you start a parked item: no ROUTE, no START; it waits until the user presses
/// Start." A route to it is refused with why and what the lead can do instead (#248), and it stays parked.
#[test]
fn a_route_to_a_parked_item_is_refused_and_it_stays_parked() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: task | Clean old copies | done when: none left\nPARK: #new | why: the user asked");
    let it = item(&desk(&w, &project), "Clean old copies").clone();
    assert_eq!(it["state"], "held", "{it}");
    let id = it["id"].as_str().unwrap();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: it's free now"));
    let told = sent_to(&w, &d).last().cloned().unwrap();
    assert!(told.starts_with(&format!("Refused: #{id} is parked: the user starts it themselves")) && told.contains("You can"), "{told}");
    assert_eq!(item(&desk(&w, &project), "Clean old copies")["state"], "held");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains("Clean old copies")));
}

/// LEAD_SKILL: "In a ROUTE, #new means the ITEM you added in the same reply." Two ITEM+ROUTE pairs in one reply route
/// each item, not the last one twice.
#[test]
fn route_new_names_the_item_just_above_it_in_the_reply() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header\nITEM: task | Write the notes | done when: in docs\nROUTE: #new -> new | why: fresh eyes");
    let dk = desk(&w, &project);
    assert_eq!(item(&dk, "Header jumps")["holder"]["name"], "cue-teams", "{dk}");
    let notes = item(&dk, "Write the notes");
    assert!(notes["holder"]["name"] != "cue-teams", "{notes}");
}
