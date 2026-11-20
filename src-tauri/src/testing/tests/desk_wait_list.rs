//! The Desk's one list of waits (#250, desk/hub/core/waits.rs): a queued item is in it, saying what it waits on; a circle
//! in it is found, shown and told to the lead once; and a wait is cleared by cutting its link, in place of a plain
//! "Try again".

use super::desk::{desk, sent_to};
use super::desk_commit::{done_while_tidier_edits, ends, item, routed_with, tick, tidier_editing};
use super::team::says;
use crate::hosted::HostedEvent;
use crate::testing::World;

fn id_of(w: &World, project: &str, title: &str) -> String {
    item(w, project, title)["id"].as_str().unwrap().to_string()
}

/// Two items queued after each other: "Header jumps" (a) at work with cue-teams, "Tidy the footer" (b) routed to
/// "tidier" after it, then a put back in the queue after b (as a Desk saved before the guard against it could
/// load: a lead's ROUTE can't make one). (repo, project, a, b, tidier, lead)
fn circle(w: &World) -> (std::path::PathBuf, String, String, String, String, String) {
    let o = std::cell::RefCell::new(String::new());
    let (dir, project, a, _) = routed_with(w, |_, _, _| {}, |w, _, dir| *o.borrow_mut() = tidier_editing(w, dir));
    let o = o.into_inner();
    let lead = desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &o, true).unwrap();
    ends(w, &lead, &format!("ITEM: task | Tidy the footer | done when: it's tidy\nROUTE: #new -> tidier | after: #{a} | why: it knows the files"));
    let b = id_of(w, &project, "Tidy the footer");
    let it = item(w, &project, "Tidy the footer");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    w.hub().desk_queue_after_unchecked(&a, &b);
    tick(w);
    (dir, project, a, b, o, lead)
}

#[test]
fn a_circle_is_found_shown_and_told_to_the_lead_once() {
    let w = World::new();
    let (_, project, a, b, _, lead) = circle(&w);
    let d = desk(&w, &project);
    let circles = d["circles"].as_array().unwrap();
    assert_eq!(circles.len(), 1, "{d}");
    let mut items: Vec<String> = circles[0]["items"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string()).collect();
    items.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(items, want);
    assert_eq!(circles[0]["told"], true);
    assert!(circles[0]["at"].as_u64().unwrap() > 0);
    let links = circles[0]["links"].as_array().unwrap();
    assert!(links.iter().any(|l| l["item"] == a.as_str() && l["on"] == b.as_str() && l["why"] == "queued after it"), "{links:?}");
    assert!(links.iter().any(|l| l["item"] == b.as_str() && l["on"] == a.as_str() && l["why"] == "queued after it"), "{links:?}");
    let told = |w: &World| sent_to(w, &lead).into_iter().filter(|t| t.starts_with("Circle on the Desk")).collect::<Vec<_>>();
    let notes = told(&w);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains(&format!("#{a}")) && notes[0].contains(&format!("#{b}")) && notes[0].contains("CUT: #"), "{}", notes[0]);
    tick(&w);
    tick(&w);
    assert_eq!(told(&w).len(), 1, "told once");
}

#[test]
fn the_lead_cuts_a_queue_link_and_the_queued_item_starts() {
    let w = World::new();
    let (_, project, a, b, o, lead) = circle(&w);
    says(&w, &lead, [HostedEvent::TurnStarted]);
    ends(&w, &lead, &format!("CUT: #{b} -> #{a} | why: tidier can start it now"));
    let it = item(&w, &project, "Tidy the footer");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    // Its holder is mid-turn: Cue keeps the hand-off for it and it goes as that turn ends.
    ends(&w, &o, "Still on b.");
    assert!(sent_to(&w, &o).iter().any(|t| t.contains(&format!("#{b}"))), "its holder handed it");
    assert!(desk(&w, &project)["decisions"].as_array().unwrap().iter().any(|d| d["text"] == format!("Cut its wait on #{a}").as_str() && d["why"] == "tidier can start it now"));
    tick(&w);
    assert!(desk(&w, &project)["circles"].as_array().unwrap().is_empty(), "the circle is gone");
    // A link that isn't there: the lead is told so.
    says(&w, &lead, [HostedEvent::TurnStarted]);
    ends(&w, &lead, &format!("CUT: #{b} -> #{a} | why: again"));
    assert!(sent_to(&w, &lead).iter().any(|t| t.starts_with("Not cut:")), "{:?}", sent_to(&w, &lead));
}

#[test]
fn try_again_is_gone_and_a_plain_rerun_is_its_own_act() {
    let w = World::new();
    let (_, project, _) = done_while_tidier_edits(&w);
    let id = id_of(&w, &project, "Header jumps");
    assert!(w.hub().desk_item_act(&id, "retry_finish", "").is_err(), "no Try again act");
    assert!(w.hub().desk_item_act(&id, "rerun", "").is_err(), "not stopped on a question: nothing to rerun");
}

#[test]
fn a_cut_line_is_read() {
    use crate::desk::parse::{lines, Line};
    assert_eq!(lines("CUT: #231 -> #228 | why: the commit can go without it"), vec![Line::Cut { id: "231".into(), on: "228".into(), why: "the commit can go without it".into() }]);
    assert_eq!(lines("- **CUT: #a1 → #b2**"), vec![Line::Cut { id: "a1".into(), on: "b2".into(), why: String::new() }]);
    assert_eq!(lines("CUT: 231 -> 228"), vec![]);
}
