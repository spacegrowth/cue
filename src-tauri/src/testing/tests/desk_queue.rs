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
    // Done isn't enough: it waits for the user's Looks right (#576).
    assert_eq!(item(&desk(&w, &project), "Header links")["state"], "queued");
    assert_eq!(w.hosted.launched().len(), launches);
    w.hub().desk_item_act(&head, "looks_right", "").unwrap();
    w.settle();
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
    // Done isn't enough: it waits for the user's Looks right (#576).
    assert_eq!(item(&now, "Footer")["state"], "queued");
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    // cue-teams still holds Sidebar: one item per session, so Footer waits for it, marked for cue-teams.
    let now = desk(&w, &project);
    let footer = item(&now, "Footer");
    assert_eq!(footer["state"], "queued", "{now}");
    assert!(footer["after"].is_null() && footer["holder"].is_null(), "{now}");
    assert_eq!(footer["next_for"]["sid"], h.as_str(), "{now}");
    ends(&w, &h, &format!("DONE: #{}", resumed.id));
    w.hub().desk_item_act(&resumed.id, "looks_right", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Footer")["state"], "working", "{now}");
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
    // #a is done, then gets Looks right (#576): the new session starts now, and takes it.
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert_eq!(w.hosted.launched().len(), launches);
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
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
    // (Another session for the third, so the chain spans two.)
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
    // (Logo and Menu to another session, so the chain spans two.)
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
    assert_eq!(item(&desk(&w, &project), "Logo")["state"], "queued");
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Logo")["state"], "working");
    assert_eq!(item(&now, "Footer")["state"], "queued");
}

/// Header at work with cue-teams, then Footer routed to it too: (holder, project, lead, Header's id, Footer's id).
fn busy_route(w: &World) -> (String, String, String, String, String) {
    let (h, project) = holder(w);
    let d = new_lead(w, &project);
    ends(w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(w, &h, "On it.");
    ends(w, &d, "ITEM: Footer\nROUTE: #new -> cue-teams | why: knows the page");
    let b = item(&desk(w, &project), "Footer")["id"].as_str().unwrap().to_string();
    (h, project, d, a, b)
}

/// #576: a route to a session busy with another item queues it for that session, with no holder.
#[test]
fn a_route_to_a_busy_session_queues_the_item_for_it_with_no_holder() {
    let w = World::new();
    let (h, project, d, a, b) = busy_route(&w);
    let view = desk(&w, &project);
    let it = item(&view, "Footer");
    assert_eq!(it["state"], "queued", "{it}");
    assert!(it["holder"].is_null(), "{it}");
    assert_eq!(it["next_for"]["sid"], h.as_str(), "{it}");
    assert_eq!(it["next_for"]["name"], "cue-teams", "{it}");
    let at = it["queued_at"].as_u64().unwrap();
    assert_eq!(it["fresh_at"].as_u64(), Some(at + 20 * 60_000), "{it}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}"))), "not sent to the busy session");
    let told = format!("Queued: #{b} is next for cue-teams (busy with #{a}); it goes to a fresh session at ");
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&told)), "{:?}", sent_to(&w, &d));
    assert!(summary(&w, &project).lines().any(|l| l.starts_with(&format!("- #{b} ")) && l.contains("| next for cue-teams")), "{}", summary(&w, &project));
}

/// #576: a session keeps its item through DONE until Looks right; then its next item starts in it at once.
#[test]
fn looks_right_on_the_sessions_item_starts_the_one_queued_for_it() {
    let w = World::new();
    let (h, project, _, a, b) = busy_route(&w);
    ends(&w, &h, &format!("DONE: #{a}"));
    let view = desk(&w, &project);
    assert_eq!(item(&view, "Header")["state"], "to_check");
    assert_eq!(item(&view, "Footer")["state"], "queued", "still busy with #{a} to check: {view}");
    assert!(item(&view, "Footer")["holder"].is_null());
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let view = desk(&w, &project);
    let it = item(&view, "Footer");
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(h.as_str())), "{it}");
    assert!(it["next_for"].is_null() && it["fresh_at"].is_null(), "{it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{b}"))), "{:?}", sent_to(&w, &h));
}

/// #576: an item after another starts when that one gets Looks right, not at its DONE.
#[test]
fn an_item_after_another_waits_for_its_looks_right_not_its_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let two = second(&w, "cue-two");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-two | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str(), it["next_for"]["sid"].as_str()), (Some("queued"), Some(a.as_str()), Some(two.as_str())), "{it}");
    assert!(it["holder"].is_null() && it["fresh_at"].is_null(), "no clock while #{a} is ahead: {it}");
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&format!("Queued: #{b} is next for cue-two, after #{a} gets Looks right"))), "{:?}", sent_to(&w, &d));
    ends(&w, &h, &format!("DONE: #{a}"));
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!(it["state"], "queued", "DONE isn't enough: {it}");
    assert!(!sent_to(&w, &two).iter().any(|t| t.contains(&format!("#{b}"))));
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(two.as_str())), "{it}");
    assert!(it["after"].is_null(), "{it}");
}

/// #576: an item waits 20 minutes for the busy session it's next for, then goes to a fresh session with a hand-off
/// that names the files that session touched.
#[test]
fn after_twenty_minutes_a_queued_item_goes_to_a_fresh_session_with_a_hand_off() {
    let w = World::new();
    let (h, project, _, a, b) = busy_route(&w);
    w.hub().desk_put_files(&a, &["ui/header.js", "ui/page.css"]);
    let launches = w.hosted.launched().len();
    w.advance(19 * 60_000);
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert_eq!(w.hosted.launched().len(), launches, "still waiting");
    // Nothing else happens: the wait running out starts it.
    w.advance(60_000 + 1_000);
    let view = desk(&w, &project);
    let it = item(&view, "Footer");
    assert_eq!(w.hosted.launched().len(), launches + 1, "{view}");
    let to = it["holder"]["sid"].as_str().unwrap_or_default().to_string();
    assert!(!to.is_empty() && to != h, "{it}");
    assert_eq!(it["state"], "working", "{it}");
    assert!(it["next_for"].is_null(), "{it}");
    let sent = sent_to(&w, &to).into_iter().find(|t| t.contains(&format!("Desk item #{b}"))).unwrap_or_default();
    assert!(sent.contains(&format!("Hand-off: #{b} was next for cue-teams, which knows this area, but it was busy for 20 minutes")), "{sent}");
    assert!(sent.contains("ui/header.js, ui/page.css"), "{sent}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{b}"))), "not to the busy one");
}

/// #576: a paused Desk's queue waits: nothing starts, and its row shows no time to go fresh.
#[test]
fn a_paused_desk_starts_nothing_from_its_queue() {
    let w = World::new();
    let (h, project, _, a, b) = busy_route(&w);
    assert!(w.hub().desk_pause_set(&project, true, ("You", ""), "").unwrap());
    w.settle();
    let launches = w.hosted.launched().len();
    assert!(item(&desk(&w, &project), "Footer")["fresh_at"].is_null());
    w.advance(25 * 60_000);
    w.hub().desk_tick_now();
    w.settle();
    assert_eq!(w.hosted.launched().len(), launches, "no fresh session while paused");
    ends(&w, &h, &format!("DONE: #{a}"));
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!(it["state"], "queued", "{it}");
    assert!(it["holder"].is_null(), "{it}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{b}"))));
    assert_eq!(w.hosted.launched().len(), launches);
}

/// #576: a pause doesn't count toward the 20 minutes: the clock moves on by the pause when the Desk resumes.
#[test]
fn a_pause_doesnt_count_toward_the_twenty_minutes() {
    let w = World::new();
    let (_, project, _, _, _) = busy_route(&w);
    let at = item(&desk(&w, &project), "Footer")["queued_at"].as_u64().unwrap();
    w.advance(10 * 60_000);
    assert!(w.hub().desk_pause_set(&project, true, ("You", ""), "").unwrap());
    w.settle();
    w.advance(30 * 60_000);
    assert!(w.hub().desk_pause_set(&project, false, ("You", ""), "").unwrap());
    w.settle();
    let launches = w.hosted.launched().len();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!(it["state"], "queued", "{it}");
    let moved = it["queued_at"].as_u64().unwrap();
    assert!(moved >= at + 30 * 60_000, "moved on by the pause: {at} -> {moved}");
    w.advance(9 * 60_000);
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued", "10 of its 20 minutes left after the pause");
    assert_eq!(w.hosted.launched().len(), launches);
    w.advance(2 * 60_000);
    assert_eq!(w.hosted.launched().len(), launches + 1, "{}", item(&desk(&w, &project), "Footer"));
}

/// #576: the session an item waits for closes: a fresh session takes it on Cue's next tick with the hand-off, not 20
/// minutes on.
#[test]
fn a_queued_item_goes_to_a_fresh_session_at_once_when_its_session_closes() {
    let w = World::new();
    let (h, project, _, _, b) = busy_route(&w);
    let launches = w.hosted.launched().len();
    w.hub().forget(&h);
    w.settle();
    // Cue's next tick (every ~10 s).
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!(w.hosted.launched().len(), launches + 1, "{it}");
    let to = it["holder"]["sid"].as_str().unwrap_or_default().to_string();
    assert!(!to.is_empty() && to != h, "{it}");
    let sent = sent_to(&w, &to).into_iter().find(|t| t.contains(&format!("Desk item #{b}"))).unwrap_or_default();
    assert!(sent.contains(&format!("Hand-off: #{b} was next for cue-teams, which knows this area, but it closed")), "{sent}");
}

/// #576: Undo on an item's Looks right puts the items it let go back after it, while they haven't started.
#[test]
fn undo_of_looks_right_puts_items_that_havent_started_back_after_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let two = second(&w, "cue-two");
    ends(&w, &d, "ITEM: Logo\nROUTE: #new -> cue-two | why: first");
    ends(&w, &two, "On it.");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-two | after: #{a} | why: same file"));
    ends(&w, &h, &format!("DONE: #{a}"));
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].is_null(), it["holder"].is_null()), (Some("queued"), true, true), "let go, waiting for cue-two: {it}");
    w.hub().desk_item_act(&a, "reopen", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "after #{a} again: {it}");
    assert!(it["fresh_at"].is_null(), "{it}");
}

/// #576: once one it let go has started, Undo leaves it be.
#[test]
fn undo_of_looks_right_leaves_an_item_that_started() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let two = second(&w, "cue-two");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-two | after: #{a} | why: same file"));
    ends(&w, &h, &format!("DONE: #{a}"));
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["holder"]["sid"], two.as_str());
    w.hub().desk_item_act(&a, "reopen", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].is_null()), (Some("working"), true), "{it}");
}
