//! Queued items (desk/hub/turn/route.rs): a route to a new session that waits for the work before it, Put before, and the
//! idle routes the dispatcher sees first.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to, summary};
use crate::hosted::HostedEvent;
use crate::testing::World;

#[test]
fn the_dispatcher_sees_idle_routes_first_and_can_queue_overlapping_work_for_a_new_session() {
    let w = World::new();
    let (busy, project) = holder(&w);
    let lead = new_lead(&w, &project);
    let idle = second(&w, "idle-worker");
    ends(&w, &lead, "ITEM: Header\nROUTE: #new -> cue-teams | why: owns header.js");
    let head = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    w.hosted_says(&busy, HostedEvent::Taken { text: "Header".into() });
    let block = w.hub().desk_summary(&project, &lead, false).0;
    let (_, routes) = block.split_once("Desk routes (or use ROUTE: #<id> -> new):\n").unwrap();
    // idle-worker has no item: on the one Older line, after the routes with work, with its state (#363).
    assert!(routes.find("cue-teams (sid:").unwrap() < routes.find("idle-worker (sid:").unwrap(), "{routes}");
    assert!(routes.contains(&format!("Older, idle with no recent item: idle-worker (sid:{}, idle)", &idle[..8])), "{routes}");
    assert!(block.contains(crate::desk::ROUTING_GUIDANCE), "{block}");

    let launches = w.hosted.launched().len();
    ends(&w, &lead, &format!(
        "ITEM: Styles\nROUTE: #new -> idle-worker | why: independent stylesheet\nITEM: Header links\nROUTE: #new -> new | after: #{head} | why: also edits header.js"
    ));
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Styles")["holder"]["sid"], idle);
    let links = item(&view, "Header links");
    assert_eq!((links["state"].as_str(), links["after"].as_str()), (Some("queued"), Some(head.as_str())));
    assert_eq!(w.hosted.launched().len(), launches);
    ends(&w, &busy, &format!("DONE: #{head}"));
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Header links")["state"], "working");
    assert_eq!(w.hosted.launched().len(), launches + 1);
}

#[test]
fn a_queued_item_starts_when_the_one_before_it_is_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, "On it.");
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}"))));
    // An idle assignment with no finished reply (for example, a crash) needs to resume.
    let resumed = w.hub().desk_add(&project, "task", "Sidebar", "", Some(&h)).unwrap();
    w.settle();
    // Idle a while (one that wrote in the last minute is at work, #291), then cut off.
    w.advance(61_000);
    w.hub().mark_state(&w.hub().session_origin(&h).unwrap(), "stopped");
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Sidebar")["state"], "needs_resume");
    assert_eq!(item(&view, "Footer")["state"], "queued");
    let block = summary(&w, &project);
    assert!(block.contains(&format!("- #{} task: Sidebar | needs resume", resumed.id)), "{block}");
    assert!(block.lines().any(|line| line.starts_with(&format!("- #{b} task: Footer | queued")) && line.contains(&format!(" | after #{a}"))), "{block}");
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "to_check");
    assert_eq!(item(&now, "Footer")["state"], "working");
    let started = sent_to(&w, &h).into_iter().find(|t| t.contains(&format!("Desk item #{b}"))).unwrap();
    assert!(!started.contains("same file"), "why it was queued isn't why it starts: {started}");
}

#[test]
fn a_route_to_a_new_session_after_another_waits_for_it_to_land() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let launches = w.hosted.launched().len();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> new | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    let view = desk(&w, &project);
    assert_eq!((item(&view, "Footer")["state"].as_str(), item(&view, "Footer")["after"].as_str()), (Some("queued"), Some(a.as_str())), "{view}");
    assert!(item(&view, "Footer")["holder"].is_null(), "{view}");
    assert_eq!(w.hosted.launched().len(), launches, "no new session before #{a} lands");
    let decided = view["decisions"].as_array().cloned().unwrap_or_default();
    assert!(decided.iter().any(|x| x["text"] == format!("Queued #{b} after #{a} for a new session")), "{view}");
    // Said again by a later turn: still one queued item, nothing started.
    ends(&w, &d, &format!("ROUTE: #{b} -> new | after: #{a} | why: same file"));
    assert_eq!(w.hosted.launched().len(), launches);
    // #a lands: the new session starts now, and takes it.
    ends(&w, &h, &format!("DONE: #{a}"));
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Footer")["state"], "working", "{view}");
    assert!(item(&view, "Footer")["after"].is_null(), "{view}");
    assert_eq!(w.hosted.launched().len(), launches + 1);
    assert!(!item(&view, "Footer")["holder"]["sid"].as_str().unwrap_or_default().is_empty(), "{view}");
}

#[test]
fn a_route_to_a_new_session_cue_cant_queue_starts_now_and_the_lead_is_told() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    let launches = w.hosted.launched().len();
    ends(&w, &d, "ITEM: Footer\nROUTE: #new -> new | after: #zz99 | why: same file");
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Footer")["state"], "working", "{view}");
    assert_eq!(w.hosted.launched().len(), launches + 1);
    let s = summary(&w, &project);
    assert!(s.contains(&format!("Cue couldn't queue #{b} after #zz99, which isn't an item")), "{s}");
}

#[test]
fn an_item_isnt_queued_after_one_that_waits_on_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    // cue-teams holds two open items, the most a session holds: the third goes to another session.
    second(&w, "cue-two");
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-two | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    // Header after Logo: Logo waits on Footer, which waits on Header.
    ends(&w, &d, &format!("ROUTE: #{a} -> cue-teams | after: #{c} | why: loop"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "working", "{now}");
    assert!(item(&now, "Header")["after"].is_null(), "{now}");
    assert_eq!(item(&now, "Logo")["state"], "queued");
    // The lead hears of it in its next summary, once.
    let s = summary(&w, &project);
    assert!(s.contains(&format!("Cue couldn't queue #{a} after #{c}, which waits on it")), "{s}");
    crate::ipc::desk_send_with(w.hub(), &project, "and?").unwrap();
    let s = summary(&w, &project);
    assert!(!s.contains("couldn't queue"), "told once: {s}");
}

#[test]
fn put_before_takes_the_others_place_in_the_queue() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    // cue-teams holds two open items, the most a session holds: Logo and Menu go to another session.
    let t = second(&w, "cue-two");
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-two | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Menu\nROUTE: #new -> cue-two | after: #{c} | why: same file"));
    // Header at work <- Footer (cue-teams) <- Logo <- Menu (cue-two); Logo put before Footer: Header <- Logo <- Footer <- Menu.
    w.hub().desk_item_act(&c, "put_before", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    assert_eq!((item(&now, "Logo")["state"].as_str(), item(&now, "Logo")["after"].as_str()), (Some("queued"), Some(a.as_str())), "{now}");
    assert_eq!(item(&now, "Footer")["after"], c.as_str());
    assert_eq!(item(&now, "Menu")["after"], b.as_str());
    assert!(!sent_to(&w, &t).iter().any(|m| m.contains(&format!("Desk item #{c}"))), "Logo still waits on Header");
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Logo")["state"], "working");
    assert_eq!(item(&now, "Footer")["state"], "queued");
}
