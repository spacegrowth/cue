//! Independent Desks sharing a workspace, including saved project-keyed Desks.

use super::desk::desk;
use crate::hosted::HostedEvent;
use crate::testing::World;

fn start(w: &World, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &w.home().to_string_lossy(), true, Some(name)).unwrap();
    (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into())
}

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

#[test]
fn separate_desks_share_a_folder_and_keep_their_data_after_relaunch() {
    let mut w = World::new();
    let project = crate::model::project_at(&w.home().to_string_lossy(), "");
    let legacy = w.hub().desk_add(&project, "task", "Existing work", "", None).unwrap();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    assert_ne!(a, b);
    assert_ne!(lead_a, lead_b);
    let one = w.hub().desk_add(&a, "bug", "Only A", "", None).unwrap();
    let two = w.hub().desk_add(&b, "task", "Only B", "", None).unwrap();
    w.hub().desk_rename(&a, "Alice's feature").unwrap();
    w.relaunch();
    let da = desk(&w, &a);
    let db = desk(&w, &b);
    assert_eq!(da["name"], "Alice's feature");
    assert_eq!(db["name"], "Feature B");
    assert_eq!(da["workspace_project"], project);
    assert_eq!(db["folder"], da["folder"]);
    assert_eq!(da["lead"]["sid"], lead_a);
    assert_eq!(db["lead"]["sid"], lead_b);
    assert_eq!(da["items"][0]["id"], one.id);
    assert_eq!(db["items"][0]["id"], two.id);
    assert_eq!(desk(&w, &project)["items"][0]["id"], legacy.id);
    assert!(crate::config::load()["project"]["names"].is_null());
}

#[test]
fn routes_and_new_items_stay_with_their_desk() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    ends(&w, &lead_a, "ITEM: task | Only A");
    ends(&w, &lead_b, "ITEM: task | Only B");
    let da = desk(&w, &a);
    let db = desk(&w, &b);
    let item_a = da["items"][0]["id"].as_str().unwrap();
    let item_b = db["items"][0]["id"].as_str().unwrap();
    let summary = w.hub().desk_summary(&a, &lead_a, false).0;
    assert!(summary.contains("Only A"));
    assert!(!summary.contains("Only B"));
    let starts = w.hosted.launched().len();
    ends(&w, &lead_a, &format!("ROUTE: #{item_b} -> new | why: wrong desk\nDONE: #{item_b}"));
    assert_eq!(w.hosted.launched().len(), starts);
    assert_eq!(desk(&w, &b)["items"][0]["state"], "new");
    assert!(w.hub().desk_give(item_a, &lead_b).is_err());
    let worker = super::team::lead(&w, "hello");
    ends(&w, &worker, "Hi.");
    w.hub().desk_give(item_a, &worker).unwrap();
    ends(&w, &worker, "ITEM: bug | Separate discovery | why: independent problem");
    assert_eq!(desk(&w, &a)["items"].as_array().unwrap().len(), 2);
    assert_eq!(desk(&w, &b)["items"].as_array().unwrap().len(), 1);
    w.hub().desk_set_member(&b, &worker, true).unwrap();
    ends(&w, &worker, "ITEM: bug | Ambiguous discovery | why: independent problem");
    assert_eq!(desk(&w, &a)["items"].as_array().unwrap().len(), 2);
    assert_eq!(desk(&w, &b)["items"].as_array().unwrap().len(), 1);
}

#[test]
fn a_new_folder_needs_no_prior_session_and_repeated_names_are_independent() {
    let w = World::new();
    let folder = w.home().join("new-folder");
    std::fs::create_dir(&folder).unwrap();
    let a = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some("Feature")).unwrap();
    let b = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some("Feature")).unwrap();
    assert_ne!(a["id"], b["id"]);
    assert_eq!(w.hosted.launched().last().unwrap().cwd, std::fs::canonicalize(folder).unwrap().to_string_lossy());
}

#[test]
fn handoff_keeps_the_named_desk_and_its_memberships() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let next = super::team::lead(&w, "hello");
    ends(&w, &next, "Hi.");
    w.hub().desk_carry(&lead_a, &next, "successor");
    assert_eq!(desk(&w, &a)["lead"]["sid"], next);
    assert_eq!(desk(&w, &b)["lead"]["sid"], lead_b);
    assert!(desk(&w, &a)["members"].as_array().unwrap().iter().any(|sid| sid == &next));
    assert!(!desk(&w, &b)["members"].as_array().unwrap().iter().any(|sid| sid == &next));
}

#[test]
fn one_session_cannot_be_selected_as_two_desks_lead_at_once() {
    let w = World::new();
    let (a, _) = start(&w, "Feature A");
    let (b, _) = start(&w, "Feature B");
    let worker = super::team::lead(&w, "hello");
    ends(&w, &worker, "Hi.");
    w.hub().desk_claim_start(&a).unwrap();
    let claim = crate::ipc::DeskStart::new(w.hub(), &a);
    w.hub().desk_claim_lead(&a, &worker).unwrap();
    assert!(crate::ipc::desk_set_lead_with(w.hub(), &b, Some(&worker), true).is_err());
    drop(claim);
    assert!(crate::ipc::desk_set_lead_with(w.hub(), &b, Some(&worker), true).is_ok());
    assert!(crate::ipc::desk_set_lead_with(w.hub(), &a, Some(&worker), true).is_err());
}
