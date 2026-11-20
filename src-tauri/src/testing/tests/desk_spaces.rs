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
    let item = w.hub().desk_add(&a, "bug", "Fix the header", "Header fits", None).unwrap();
    w.hub().desk_carry(&lead_a, &next, "successor");
    assert_eq!(desk(&w, &a)["lead"]["sid"], next);
    assert_eq!(desk(&w, &b)["lead"]["sid"], lead_b);
    assert!(desk(&w, &a)["members"].as_array().unwrap().iter().any(|sid| sid == &next));
    assert!(!desk(&w, &b)["members"].as_array().unwrap().iter().any(|sid| sid == &next));

    let lead = desk(&w, &a)["lead"]["name"].as_str().unwrap().to_string();
    let (_, message) = w.hosted.sent().into_iter().find(|(sid, text)| {
        sid == &next && text.contains("now runs Feature A's Desk.")
    }).expect("the successor receives the handoff");
    assert_eq!(message.split_once("</desk>\n\n").unwrap().1, format!("{lead} now runs Feature A's Desk. Please route Fix the header."));
    let (_, summary) = message.split_once(">\n").unwrap();
    assert!(summary.starts_with(&format!("Desk lead: {lead} (you)\nHow work lands now (LANDS: to propose a change): ")) && summary.contains(" · at once: 3\n\nNeeds routing:\n"), "{message}");
    assert!(summary.lines().nth(4).unwrap().contains(&format!("#{} bug: Fix the header | new | done when: Header fits", item.id)), "{message}");
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

#[test]
fn codex_desk_starts_a_codex_lead_and_worker() {
    let w = World::new();
    let r = crate::ipc::desk_start_with_agent(w.hub(), &w.home().to_string_lossy(), true, Some("Codex desk"), crate::hosted::Agent::Codex).unwrap();
    let id = r["id"].as_str().unwrap();
    let lead = r["sid"].as_str().unwrap();
    assert_eq!(w.hosted.launched().last().unwrap().agent, crate::hosted::Agent::Codex);
    let started = desk(&w, id);
    assert_eq!(started["lead"]["sid"], lead);
    assert_eq!(started["lead"]["state"], "live");
    assert_eq!(started["members"].as_array().unwrap().len(), 1, "the provisional ID is removed");
    assert_eq!(started["members"][0], lead);
    assert!(w.hosted.sent().iter().any(|(sid, text)| sid == lead && text.contains(crate::desk::LEAD_SKILL)));
    ends(&w, lead, "ITEM: task | Work");
    let d = desk(&w, id);
    assert_eq!(d["items"][0]["from"], "Codex desk-lead");
    let item = d["items"][0]["id"].as_str().unwrap();
    ends(&w, lead, &format!("ROUTE: #{item} -> new | why: do the work"));
    assert_eq!(w.hosted.launched().last().unwrap().agent, crate::hosted::Agent::Codex);
}

#[test]
fn deleting_a_desk_keeps_sessions_and_other_desks_after_relaunch() {
    let mut w = World::new();
    let (a, lead_a) = start(&w, "Delete me");
    let (b, lead_b) = start(&w, "Keep me");
    w.hub().desk_add(&a, "task", "Discarded", "", None).unwrap();
    w.hub().desk_add(&b, "task", "Kept", "", None).unwrap();
    w.hub().desk_set_member(&b, &lead_a, true).unwrap();
    w.hub().desk_delete(&a).unwrap();
    assert!(w.hub().session_origin(&lead_a).is_some());
    assert!(w.hub().session_origin(&lead_b).is_some());
    assert!(w.hub().desk_delete(&a).is_err());
    w.relaunch();
    let b_desk = desk(&w, &b);
    assert_eq!(b_desk["items"][0]["title"], "Kept");
    assert!(b_desk["members"].as_array().unwrap().iter().any(|sid| sid == &lead_a));
    assert!(!crate::db::desk_spaces().iter().any(|d| d.id == a));
    assert!(!crate::db::desk_load(crate::clock::now_ms()).0.iter().any(|i| i.project == a));
}

#[test]
fn deleting_a_desk_only_selects_its_unshared_sessions_for_closing() {
    let w = World::new();
    let (a, lead_a) = start(&w, "First");
    let (b, lead_b) = start(&w, "Second");
    let worker = super::team::lead(&w, "hello");
    w.hub().desk_set_member(&a, &worker, true).unwrap();
    w.hub().desk_set_member(&b, &worker, true).unwrap();
    let selected = vec![lead_a.clone(), worker.clone(), lead_b.clone(), "unknown".into()];
    assert_eq!(w.hub().desk_delete_selected(&a, &selected).unwrap(), vec![lead_a]);
    assert!(w.hub().desk_session_shared(&worker));
    assert!(w.hub().session_origin(&worker).is_some());
}
