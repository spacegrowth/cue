//! Desk spec (#498): what Cue sends a session (docs/cue-messages.md, "To a session"): who gets it, and that its full
//! text carries the item and what it means. The look of these lines is #489/#501's, not tested here.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to};
use crate::testing::World;

/// Two Desks on the same folder ("Feature A", "Feature B"), with their leads: (a, lead a, b, lead b).
fn two_desks(w: &World) -> (String, String, String, String) {
    let start = |name: &str| {
        let r = crate::ipc::desk_start_named(w.hub(), &w.home().to_string_lossy(), true, Some(name)).unwrap();
        (r["id"].as_str().unwrap().to_string(), r["sid"].as_str().unwrap().to_string())
    };
    let (a, la) = start("Feature A");
    let (b, lb) = start("Feature B");
    (a, la, b, lb)
}

/// cue-messages "Dropped": you drop an item its holder is at work on, and the holder is told to stop, naming it.
#[test]
fn dropping_an_item_at_work_tells_its_holder_to_stop_work_on_it() {
    for state in ["working"] {
        let w = World::new();
        let (sid, project) = holder(&w);
        let it = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
        w.hub().desk_set_state(&it.id, state);
        w.hub().desk_item_act(&it.id, "drop", "").unwrap();
        w.settle();
        let sent = sent_to(&w, &sid);
        assert!(sent.iter().any(|t| t.contains(&format!("Desk item #{} was dropped: stop work on it.", it.id))), "{state}: {sent:?}");
    }
}

/// cue-messages "Moved": the item went to another session; its old holder is told where, by number and title, and
/// to stop work on it. The new one is handed it; the old one isn't sent the hand-off.
#[test]
fn a_moved_item_tells_its_old_holder_where_it_went_and_to_stop() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let other = second(&w, "second");
    ends(&w, &l, &format!("ROUTE: #{id} -> second | why: it has the footer too"));
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t == &format!("Desk item #{id} (Header) has moved to second: stop work on it.")), "{:?}", sent_to(&w, &h));
    assert!(sent_to(&w, &other).iter().any(|t| t.starts_with(&format!("Desk item #{id}"))), "{:?}", sent_to(&w, &other));
}

/// cue-messages "Parked": PARK on a queued item tells the session it was queued for that it's off its list.
#[test]
fn parking_a_queued_item_tells_its_session_its_off_its_list() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "task", "Footer", "", Some(&h)).unwrap();
    w.hub().desk_set_state(&it.id, "queued");
    ends(&w, &l, &format!("PARK: #{} | why: the owner wants it later", it.id));
    w.advance(1000);
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{} (Footer) is parked: it's off your list.", it.id))), "{:?}", sent_to(&w, &h));
}

/// cue-messages "Not a new item" (desk_heard.rs): an ITEM: line from a session on an item, with no why:, is its own
/// fix: not added, and the session is told it stays in the item it's on, by number. With a why: it is a new item.
#[test]
fn an_item_line_from_a_holder_without_why_is_not_a_new_item_and_it_is_told_where_it_stays() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, "Working on it.\nITEM: bug | The header's left padding");
    let titles = |w: &World| desk(w, &project)["items"].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>();
    assert!(!titles(&w).iter().any(|t| t == "The header's left padding"), "{:?}", titles(&w));
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Not added as a new item: The header's left padding. Its fix or next step stays in #{id}"))), "{:?}", sent_to(&w, &h));
    ends(&w, &h, "ITEM: bug | The footer overlaps the dock | why: it's in another file and another screen");
    assert!(titles(&w).iter().any(|t| t == "The footer overlaps the dock"), "{:?}", titles(&w));
}

/// cue-messages "Several Desks" (desk_heard.rs): an ITEM: line from a session more than one Desk could take is not
/// added to any, and the session is told why.
#[test]
fn an_item_line_several_desks_could_take_is_added_to_none_and_its_session_is_told() {
    let w = World::new();
    let (a, _, b, _) = two_desks(&w);
    let worker = super::team::lead(&w, "hello");
    ends(&w, &worker, "Hi.");
    ends(&w, &worker, "ITEM: bug | The footer overlaps the dock");
    for d in [&a, &b] {
        assert!(!desk(&w, d)["items"].as_array().unwrap().iter().any(|i| i["title"] == "The footer overlaps the dock"), "{d}");
    }
    assert!(sent_to(&w, &worker).iter().any(|t| t.starts_with("Several Desks could receive this item")), "{:?}", sent_to(&w, &worker));
}

/// cue-messages "TO LEAD not sent" (desk_heard.rs): a TO LEAD: line from a session that holds no item and is on more
/// than one Desk reaches no lead, and the session is told how to name the item.
#[test]
fn to_lead_from_a_session_on_several_desks_with_no_item_comes_back_unsent() {
    let w = World::new();
    let (_, la, _, lb) = two_desks(&w);
    let worker = super::team::lead(&w, "hello");
    ends(&w, &worker, "Hi.");
    let (before_a, before_b) = (sent_to(&w, &la).len(), sent_to(&w, &lb).len());
    ends(&w, &worker, "TO LEAD: I'm free for more");
    w.settle();
    assert!(!sent_to(&w, &la)[before_a..].iter().chain(&sent_to(&w, &lb)[before_b..]).any(|t| t.contains("I'm free for more")));
    assert!(sent_to(&w, &worker).iter().any(|t| t.starts_with("TO LEAD: not sent:") && t.contains("#<id>")), "{:?}", sent_to(&w, &worker));
}

/// cue-messages "TO LEAD failed" (desk.rs), #474: a TO LEAD: line whose Desk lead can't be reached comes back to the
/// session, saying it didn't reach the lead and why, with its words (it may have written several).
#[test]
fn to_lead_that_cannot_reach_the_lead_tells_the_session_with_its_words() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &h).unwrap();
    w.settle();
    // The lead gone, and it won't start again.
    w.hosted.exits(&l);
    w.settle();
    w.hosted.refuse("it won't start");
    ends(&w, &h, "TO LEAD: Should it wrap on narrow windows?");
    w.advance(1000);
    w.settle();
    let back = sent_to(&w, &h).into_iter().find(|t| t.starts_with("Your TO LEAD: line didn't reach the Desk's lead")).unwrap_or_else(|| panic!("{:?}", sent_to(&w, &h)));
    assert!(back.contains("Should it wrap on narrow windows?"), "{back}");
    // Its first line says why, as the chat's short line quotes it (ui/desk/cue_said.js).
    assert_eq!(back.lines().next(), Some("Your TO LEAD: line didn't reach the Desk's lead: it won't start"), "{back}");
}
