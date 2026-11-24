//! When an item queued after another starts (#808, desk/lands.rs "waits"; desk/hub/turn/route.rs `release_landed`):
//! once the one before has landed (every Desk's default), or once you've said Looks right on it. A Send back of one that
//! already let others go tells those at work to take its fix in.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to};
use crate::testing::World;
use serde_json::json;

/// Header (#a) with cue-teams, Footer (#b) after it for cue-two, and Header's DONE said: (holder, lead, cue-two, a, b).
fn two_in_a_row(w: &World, project: &str, h: &str) -> (String, String, String, String) {
    let d = new_lead(w, project);
    let two = second(w, "cue-two");
    ends(w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(w, project), "Header")["id"].as_str().unwrap().to_string();
    ends(w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-two | after: #{a} | why: same file"));
    let b = item(&desk(w, project), "Footer")["id"].as_str().unwrap().to_string();
    ends(w, h, &format!("DONE: #{a}"));
    (d, two, a, b)
}

/// Item `a` lands: on main, its steps after landing done, to check (what finish.rs does at the end of its landing).
fn lands(w: &World, a: &str) {
    w.hub().write(|st| {
        let it = st.desk.item_mut(a).unwrap();
        it.state = "finishing".into();
        it.finish = Some(crate::desk::Finish { step: "landing".into(), at: 7, ..Default::default() });
    });
    w.hub().finish_checked(a);
    w.settle();
}

#[test]
fn a_new_desk_starts_a_waiting_item_once_the_one_before_lands() {
    let w = World::new();
    let (h, project) = holder(&w);
    assert_eq!(desk(&w, &project)["lands"]["set"]["waits"], "landed", "every Desk's default");
    let (d, two, a, b) = two_in_a_row(&w, &project, &h);
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&format!("after #{a} lands"))), "{:?}", sent_to(&w, &d));
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued", "DONE alone isn't landed");
    lands(&w, &a);
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(two.as_str())), "started without your Looks right: {it}");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check", "the one before still waits for your check");
    assert!(sent_to(&w, &two).iter().any(|t| t.contains(&format!("Desk item #{b}"))), "{:?}", sent_to(&w, &two));
    // Undo isn't possible yet; Looks right then lets nothing more go, and Undo keeps #b at work: it's still landed.
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.hub().desk_item_act(&a, "reopen", "").unwrap();
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "working");
}

#[test]
fn set_to_checked_a_waiting_item_starts_only_at_looks_right() {
    let w = World::new();
    let (h, project) = holder(&w);
    w.hub().desk_lands_set(&project, &json!({ "waits": "checked" })).unwrap();
    assert_eq!(desk(&w, &project)["lands"]["set"]["waits"], "checked");
    let (d, two, a, _) = two_in_a_row(&w, &project, &h);
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&format!("after #{a} gets Looks right"))), "{:?}", sent_to(&w, &d));
    lands(&w, &a);
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued", "landed isn't enough");
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(two.as_str())), "{it}");
}

#[test]
fn a_send_back_of_one_that_let_others_go_tells_those_at_work_to_rebase_on_its_fix() {
    let w = World::new();
    let (h, project) = holder(&w);
    let (_, two, a, b) = two_in_a_row(&w, &project, &h);
    lands(&w, &a);
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "working");
    let before = sent_to(&w, &two).len();
    w.hub().desk_item_act(&a, "send_back", "The header overlaps the logo").unwrap();
    w.settle();
    let note = sent_to(&w, &two)[before..].iter().find(|t| t.contains("sent back")).cloned().unwrap_or_default();
    assert!(note.contains(&format!("Note from Cue on Desk item #{b}")) && note.contains("The header overlaps the logo"), "{note}");
    assert!(note.contains(&format!("Once #{a}'s fix is on main, rebase #{b}'s work on main")), "{note}");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working", "back with its holder");
    // It lands again and is sent back again: told again.
    ends(&w, &h, &format!("DONE: #{a}"));
    lands(&w, &a);
    let before = sent_to(&w, &two).len();
    w.hub().desk_item_act(&a, "send_back", "Still overlaps").unwrap();
    w.settle();
    assert!(sent_to(&w, &two)[before..].iter().any(|t| t.contains("Still overlaps")), "{:?}", &sent_to(&w, &two)[before..]);
}

#[test]
fn a_send_back_holds_a_follower_that_hasnt_started_until_the_fix_lands() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let two = second(&w, "cue-two");
    // cue-two is busy with Logo, so Footer is let go but waits for it.
    ends(&w, &d, "ITEM: Logo\nROUTE: #new -> cue-two | why: first");
    ends(&w, &two, "On it.");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-two | after: #{a} | why: same file"));
    ends(&w, &h, &format!("DONE: #{a}"));
    lands(&w, &a);
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].is_null(), it["holder"].is_null()), (Some("queued"), true, true), "let go, waiting for cue-two: {it}");
    w.hub().desk_item_act(&a, "send_back", "Wrong colour").unwrap();
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["after"], a.as_str(), "after #{a} again");
}
