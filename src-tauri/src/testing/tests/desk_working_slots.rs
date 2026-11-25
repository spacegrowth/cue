//! How many of a Desk's items work on a machine at once (#798): the one "at once" number per machine in Desk settings
//! limits this Mac too, even when items work in the shared folder. A route or a new start past it waits in Up next
//! ("waiting for a free slot") and starts by itself as one finishes; paused items and those to check don't count, and
//! your own Start now asks first (#712).

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to, summary};
use super::desk_remote::routed_there;
use crate::testing::World;
use serde_json::json;

/// A Desk whose items work in the shared folder, `n` at once on this Mac, its lead, and "Header jumps" at work with
/// cue-teams: (project, lead, its id).
fn one_at_work(w: &World, n: u32) -> (String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    let v = w.hub().desk_lands_set(&project, &json!({ "where": "shared", "at_once": n })).unwrap();
    assert_eq!((v["set"]["where"].as_str(), v["set"]["at_once"].as_u64()), (Some("shared"), Some(n as u64)), "{v}");
    let a = w.hub().desk_add(&project, "bug", "Header jumps", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    assert_eq!(item(&desk(w, &project), "Header jumps")["state"], "working");
    (project, l, a.id)
}

fn tick(w: &World) {
    w.hub().desk_queue_tick();
    w.advance(1000);
    w.settle();
}

#[test]
fn there_is_no_separate_working_setting_only_the_at_once_number() {
    let w = World::new();
    let (_, project) = holder(&w);
    new_lead(&w, &project);
    let v = w.hub().desk_lands_set(&project, &json!({ "where": "shared", "at_once": 2 })).unwrap();
    assert!(v["parts"].as_array().unwrap().iter().all(|p| p["key"] != "working"), "{}", v["parts"]);
    assert!(v["set"].get("working").is_none(), "{}", v["set"]);
    assert_eq!(v["set"]["at_once"], 2, "kept in the shared folder too");
}

#[test]
fn in_the_shared_folder_a_route_past_this_macs_number_waits_for_a_free_slot_then_starts() {
    let w = World::new();
    let (project, l, a) = one_at_work(&w, 1);
    let web = second(&w, "web");
    let b = w.hub().desk_add(&project, "bug", "Footer jumps", "", None).unwrap();
    let before = sent_to(&w, &web).len();
    ends(&w, &l, &format!("ROUTE: #{} -> web | why: it knows the footer", b.id));
    let it = item(&desk(&w, &project), "Footer jumps").clone();
    assert_eq!((it["state"].as_str(), it["slot_wait"].as_bool()), (Some("queued"), Some(true)), "{it}");
    assert!(!sent_to(&w, &web)[before..].iter().any(|t| t.contains(&format!("Desk item #{}", b.id))), "not sent while the slot is taken");
    let s = summary(&w, &project);
    assert!(s.contains("waiting for a free slot on this Mac, then to web"), "the lead is told why: {s}");
    // Header jumps is with you to check: its slot frees, and Footer jumps goes to web.
    w.hub().desk_put_in_state(&a, "to_check");
    tick(&w);
    let it = item(&desk(&w, &project), "Footer jumps").clone();
    assert_eq!(it["state"], "working", "{it}");
    assert!(it["slot_wait"].is_null() || it["slot_wait"] == false, "{it}");
    assert!(sent_to(&w, &web)[before..].iter().any(|t| t.contains(&format!("Desk item #{}", b.id))), "sent once a slot was free");
}

#[test]
fn in_the_shared_folder_new_sessions_past_the_number_wait_and_start_in_order() {
    let w = World::new();
    let (project, l, a) = one_at_work(&w, 1);
    ends(&w, &l, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: next\nITEM: bug | Sidebar jumps | done when: it stays put\nROUTE: #new -> new | why: next");
    w.advance(1000);
    for t in ["Footer jumps", "Sidebar jumps"] {
        let it = item(&desk(&w, &project), t).clone();
        assert!(it["holder"].is_null(), "no session started past the number: {it}");
        assert_eq!((it["state"].as_str(), it["slot_wait"].as_bool()), (Some("queued"), Some(true)), "{it}");
    }
    // Paused: its slot is free, and the first in line starts; the other still waits.
    w.hub().write(|st| st.desk.item_mut(&a).unwrap().paused = Some((0, "later".into())));
    tick(&w);
    let started = |t: &str| item(&desk(&w, &project), t)["holder"]["sid"].as_str().is_some();
    assert!(started("Footer jumps"), "the first waiting starts: {}", item(&desk(&w, &project), "Footer jumps"));
    assert!(!started("Sidebar jumps"), "only one at once: {}", item(&desk(&w, &project), "Sidebar jumps"));
    // Footer jumps is finishing: not counted, so Sidebar jumps starts.
    let f = item(&desk(&w, &project), "Footer jumps")["id"].as_str().unwrap().to_string();
    w.hub().desk_put_in_state(&f, "finishing");
    tick(&w);
    assert!(started("Sidebar jumps"), "{}", item(&desk(&w, &project), "Sidebar jumps"));
}

#[test]
fn start_now_on_an_item_waiting_for_a_slot_asks_before_going_past_the_number() {
    let w = World::new();
    let (project, l, _) = one_at_work(&w, 1);
    second(&w, "web");
    let b = w.hub().desk_add(&project, "bug", "Footer jumps", "", None).unwrap();
    ends(&w, &l, &format!("ROUTE: #{} -> web | why: it knows the footer", b.id));
    let it = item(&desk(&w, &project), "Footer jumps").clone();
    assert_eq!((it["start_full"]["on"].as_str(), it["start_full"]["n"].as_u64()), (Some("This Mac"), Some(1)), "the window warns first: {it}");
    assert!(w.hub().desk_item_act(&b.id, "start_now", "").is_err(), "not past the number without asking");
    assert_eq!(item(&desk(&w, &project), "Footer jumps")["state"], "queued");
    // Raise to 2 and start: it starts, and the number is 2.
    w.hub().desk_item_act(&b.id, "start_now_raise", "").unwrap();
    w.settle();
    let d = desk(&w, &project);
    assert_eq!(item(&d, "Footer jumps")["state"], "working", "{}", item(&d, "Footer jumps"));
    assert_eq!(d["lands"]["set"]["at_once"], 2, "{}", d["lands"]);
}

#[test]
fn on_a_machine_at_its_number_a_new_item_waits_for_a_free_slot() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    w.hub().desk_lands_set(&project, &json!({ "machines": ["box"], "machine_folders": { "box": "~/code/app" }, "machine_at_once": { "box": 1 } })).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: next");
    w.advance(1000);
    let it = item(&desk(&w, &project), "Footer jumps").clone();
    assert!(it["holder"].is_null(), "box is full: {it}");
    assert_eq!((it["state"].as_str(), it["slot_wait"].as_bool(), it["machine_wait"].as_str()), (Some("queued"), Some(true), Some("box")), "{it}");
}
