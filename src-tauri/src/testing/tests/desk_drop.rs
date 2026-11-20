//! DROP (#544, desk/hub/stall/unstick.rs): the Desk lead's `DROP: #<id> | why: …` takes an item off the list in any
//! state, as its Drop in the window does: its holder stops (or never starts it), its copy goes, what waited on it goes
//! ahead, and a step says it was dropped and why, never done.

use super::desk::desk;
use super::desk_commit::{done_with, ends, item, routed_with};
use super::desk_finish::done_in_cue;
use crate::desk::parse::{lines, Line};
use crate::testing::World;

fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

fn listed(w: &World, project: &str, id: &str) -> bool {
    desk(w, project)["items"].as_array().unwrap().iter().any(|i| i["id"] == id)
}

/// The lead drops item `id` of `project` with a why; it's gone, and its step says it was dropped and why.
fn drops(w: &World, project: &str, id: &str, why: &str) {
    ends(w, &lead_of(w, project), &format!("DROP: #{id} | why: {why}"));
    w.advance(1000);
    w.settle();
    assert!(!listed(w, project, id), "#{id} is gone: {}", desk(w, project)["items"]);
    let steps = desk(w, project)["decisions"].as_array().unwrap().clone();
    assert!(steps.iter().any(|d| d["item_id"] == id && d["act"] == "drop" && d["why"] == why && d["text"].as_str().unwrap().starts_with(&format!("Dropped #{id}"))), "{steps:?}");
    assert!(!steps.iter().any(|d| d["item_id"] == id && d["act"] == "close"), "dropped, not closed as done");
}

/// A second item, "Footer", for the same session, after item #1.
fn footer(w: &World, project: &str) -> serde_json::Value {
    ends(w, &lead_of(w, project), "ITEM: bug | Footer | done when: it shows\nROUTE: #new -> cue-teams | after: #1 | why: it made the footer");
    w.advance(1000);
    w.settle();
    let footer = item(w, project, "Footer");
    assert_eq!(footer["state"], "queued", "{footer}");
    footer
}

#[test]
fn drop_is_read_with_a_bar_or_a_dash() {
    for l in ["DROP: #12 | why: you asked", "DROP: #12 — why: you asked", "- **DROP:** #12 - you asked"] {
        assert_eq!(lines(l), [Line::Unstick { what: "drop".into(), id: "12".into(), commit: String::new(), why: "you asked".into() }], "{l}");
    }
    assert!(lines("DROP: the old plan").is_empty(), "prose, not an item");
}

#[test]
fn a_new_item_is_dropped() {
    let w = World::new();
    let (_, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let id = w.hub().desk_add(&project, "feature", "Dark sidebar", "it's dark", None).unwrap().id;
    assert_eq!(item(&w, &project, "Dark sidebar")["state"], "new");
    drops(&w, &project, &id, "the user doesn't want it");
}

/// At work: its holder is told to stop, its copy goes, and the item queued after it for the same session starts.
#[test]
fn an_item_at_work_is_dropped_its_holder_stops_and_the_one_queued_after_it_starts() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    assert!(!item(&w, &project, "Header jumps")["tree"].is_null(), "it has a copy");
    footer(&w, &project);
    drops(&w, &project, &id, "the user doesn't need it");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id} was dropped: stop work on it."))), "{:?}", sent_to(&w, &h));
    assert!(desk(&w, &project)["kept"].as_array().is_none_or(|k| k.is_empty()), "no work in its copy, so nothing kept");
    assert_eq!(item(&w, &project, "Footer")["state"], "working", "what waited on it goes ahead");
}

/// Queued for a session that hasn't started it: that session is told it's off its list.
#[test]
fn a_queued_item_is_dropped_and_its_session_never_starts_it() {
    let w = World::new();
    let (_, project, _, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let id = footer(&w, &project)["id"].as_str().unwrap().to_string();
    drops(&w, &project, &id, "folded into the header fix");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id} (Footer) was dropped: it's off your list. Don't start it."))), "{:?}", sent_to(&w, &h));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working", "the other item is untouched");
}

#[test]
fn a_paused_item_is_dropped() {
    let w = World::new();
    let (_, project, id, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    ends(&w, &lead_of(&w, &project), &format!("PAUSE: #{id} | why: #9 is urgent"));
    w.advance(1000);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "paused");
    drops(&w, &project, &id, "no longer wanted");
}

#[test]
fn a_parked_item_is_dropped() {
    let w = World::new();
    let (_, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let id = w.hub().desk_add(&project, "feature", "Dark sidebar", "it's dark", None).unwrap().id;
    w.hub().desk_item_act(&id, "park", "").unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Dark sidebar")["state"], "held");
    drops(&w, &project, &id, "the user said drop it");
}

#[test]
fn a_finishing_item_is_dropped() {
    let w = World::new();
    let (_, _, project, id) = done_in_cue(&w);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "finishing");
    drops(&w, &project, &id, "the user doesn't want it installed");
}

#[test]
fn an_item_to_check_is_dropped() {
    let w = World::new();
    let (_, project, id) = done_with(&w, |_, _, _| {}, |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    drops(&w, &project, &id, "not needed after all");
}

/// Done: off the list too, shown as dropped, not left as done.
#[test]
fn a_done_item_is_dropped() {
    let w = World::new();
    let (_, project, id) = done_with(&w, |_, _, _| {}, |_, _, _| {});
    w.hub().desk_item_act(&id, "looks_right", "").unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    drops(&w, &project, &id, "a repeat of #7");
}

/// A DROP with no why is refused, said plainly; the item stays.
#[test]
fn a_drop_with_no_why_is_refused() {
    let w = World::new();
    let (_, project, id, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = lead_of(&w, &project);
    ends(&w, &l, &format!("DROP: #{id}"));
    w.advance(1000);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    assert!(sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} not dropped: say why"))), "{:?}", sent_to(&w, &l));
}
