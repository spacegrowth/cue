//! Pausing a whole Desk (desk/hub/away/pause.rs, #366): its sessions at work are told to stop at a safe point, nothing is
//! sent them nor started meanwhile (queued items stay queued), and Resume sends what was held and starts what waited.
//! The owner presses it; the lead writes PAUSE: / RESUME:.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hub::desk_pause_words;
use crate::testing::World;
use serde_json::json;

fn summary(w: &World, project: &str) -> String {
    w.hub().desk_summary(project, "", false).0
}

#[test]
fn a_paused_desk_holds_its_sessions_and_queue_and_resume_picks_them_up() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    // The holder is mid-turn on #a.
    w.hub().mark_state(&w.hub().session_origin(&h).unwrap(), "working");
    assert_eq!(desk(&w, &project)["paused"], json!({ "on": false }));

    assert!(w.hub().desk_pause_set(&project, true, ("You", ""), "").unwrap());
    w.settle();
    assert!(!w.hub().desk_pause_set(&project, true, ("You", ""), "").unwrap(), "paused already");
    let p = desk(&w, &project)["paused"].clone();
    assert_eq!((p["on"].clone(), p["by"].clone()), (json!(true), json!("You")), "{p}");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| t.as_str() == desk_pause_words()).count(), 1, "{:?}", sent_to(&w, &h));
    assert!(sent_to(&w, &d).iter().any(|t| t.starts_with("The Desk is paused")), "{:?}", sent_to(&w, &d));
    assert!(summary(&w, &project).contains("The Desk is paused (by You"));

    // It stops where it is: no "Done?" ask, no stop to the lead.
    let (to_h, to_d) = (sent_to(&w, &h).len(), sent_to(&w, &d).len());
    ends(&w, &h, "Stopped after the header markup; styles are next.");
    assert_eq!((sent_to(&w, &h).len(), sent_to(&w, &d).len()), (to_h, to_d), "{:?} / {:?}", sent_to(&w, &h), sent_to(&w, &d));

    // #a done and checked meanwhile doesn't start #b; a route the lead makes meanwhile is held. (To another session:
    // cue-teams takes #b next, one item per session, #576.)
    ends(&w, &h, &format!("DONE: #{a}"));
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let t = super::desk::second(&w, "cue-two");
    ends(&w, &d, "ITEM: Sidebar\nROUTE: #new -> cue-two | why: near it");
    let c = item(&desk(&w, &project), "Sidebar")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}"))), "{:?}", sent_to(&w, &h));
    assert!(!sent_to(&w, &t).iter().any(|m| m.contains(&format!("#{c}"))), "{:?}", sent_to(&w, &t));
    assert!(desk(&w, &project)["paused"]["held"].as_u64().unwrap() >= 1);

    // The lead resumes it: what was held goes, and #b starts.
    ends(&w, &d, "RESUME: why: the owner said go");
    let view = desk(&w, &project);
    assert_eq!(view["paused"], json!({ "on": false }));
    assert_eq!(item(&view, "Footer")["state"], "working", "{view}");
    assert!(sent_to(&w, &t).iter().any(|m| m.contains(&format!("#{c}"))), "{:?}", sent_to(&w, &t));
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{b}"))), "{:?}", sent_to(&w, &h));
    assert!(!summary(&w, &project).contains("The Desk is paused"));
    let decided: Vec<String> = view["decisions"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap_or("").to_string()).collect();
    assert!(decided.contains(&"Paused the Desk".to_string()) && decided.contains(&"Resumed the Desk".to_string()), "{decided:?}");

    // The lead pauses it too.
    ends(&w, &d, "PAUSE: why: the owner is stepping out");
    assert_eq!(desk(&w, &project)["paused"]["on"], true);
}
