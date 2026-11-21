//! The Desk's change feed (desk/feed.rs, desk/hub/lead/feed.rs): numbered rows for the lead, sent in order by one
//! sender when the lead is free, heard once its next turn ends cleanly, sent again from "heard" after a restart or to
//! a new lead.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::desk::Finish;
use crate::hosted::{Ask, AskKind, HostedEvent};
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::Arc;

const MIN: u64 = 60_000;

/// The messages to `sid` that carry Desk changes.
fn changes(w: &World, sid: &str) -> Vec<String> {
    sent_to(w, sid).into_iter().filter(|t| t.contains("(Desk changes ")).collect()
}

/// Lead `l` at work on a turn of yours.
fn busy(w: &World, l: &str) {
    w.hosted_says(l, HostedEvent::Taken { text: "Look at the board".into() });
    w.settle();
}

/// Item "Header" routed to holder `h`, whose turns end without DONE until it stops on it: the stop's row is added.
fn stop_header(w: &World, h: &str, l: &str, project: &str) -> String {
    ends(w, l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(w, project), "Header")["id"].as_str().unwrap().to_string();
    for (prompt, reply) in [("Header", "Looked at the header."), (&format!("Done? Write DONE: #{id}, or say what's left")[..], "The footer still overlaps it.")] {
        w.advance(1000);
        w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
        w.settle();
        w.advance(1000);
        ends(w, h, reply);
    }
    id
}

#[test]
fn changes_reach_a_free_lead_in_order_numbered_with_no_summary() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    let told = changes(&w, &l);
    assert_eq!(told.len(), 1, "one message for both: {told:?}");
    let m = &told[0];
    let (ia, ib) = (m.find(&format!("New item #{} (bug): Header", a.id)).unwrap(), m.find(&format!("New item #{} (task): Footer", b.id)).unwrap());
    assert!(ia < ib && m.ends_with("(Desk changes 1–2)") && !m.contains("<desk"), "in order, rows only: {m}");
    assert!(m.matches("Route each now").count() == 1 && m.find("Route each now").unwrap() > ib, "said once, after them: {m}");
    // Its turn on them ends: heard. A later item is numbered on, and sent alone.
    ends(&w, &l, "Looking.");
    let c = w.hub().desk_add(&project, "task", "Menu", "", None).unwrap();
    w.settle();
    let last = changes(&w, &l).pop().unwrap();
    assert!(last.contains(&format!("New item #{}", c.id)) && !last.contains("Header") && last.ends_with("(Desk changes 3)"), "{last}");
}

#[test]
fn a_lead_mid_turn_hears_of_changes_once_its_turn_ends() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    busy(&w, &l);
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    w.hub().desk_tick_now();
    w.settle();
    assert!(changes(&w, &l).is_empty(), "nothing while it works: {:?}", sent_to(&w, &l));
    ends(&w, &l, "Done looking.");
    let told = changes(&w, &l);
    assert!(told.len() == 1 && told[0].contains(&format!("New item #{}", a.id)), "{told:?}");
}

#[test]
fn a_lead_asking_you_something_is_sent_nothing() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ASK: Which goes first? | choices: Header / Footer");
    assert!(w.snapshot().0["items"].as_array().unwrap().iter().any(|c| c["status"] == "pending"), "its question waits on you");
    w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    w.hub().desk_tick_now();
    w.settle();
    assert!(changes(&w, &l).is_empty(), "{:?}", sent_to(&w, &l));
}

#[test]
fn a_lead_a_usage_limit_stopped_is_sent_no_stop_and_hears_it_once_carried_on() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let limit = "You've hit your session limit · resets 4:10am (America/Los_Angeles)";
    let id = {
        ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
        let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
        w.hosted_says(&l, HostedEvent::Taken { text: "next".into() });
        w.hosted_says(&l, HostedEvent::ApiError { error: "rate_limit".into(), text: limit.into() });
        w.hosted_says(&l, HostedEvent::TurnEnded { text: String::new(), failed: true, cost_usd: 0.0, unprompted: false });
        w.settle();
        assert_eq!(w.snapshot().session(&l)["state"], "limited");
        for (prompt, reply) in [("Header", "Looked at the header."), (&format!("Done? Write DONE: #{id}, or say what's left")[..], "The footer still overlaps it.")] {
            w.advance(1000);
            w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
            w.settle();
            w.advance(1000);
            ends(&w, &h, reply);
        }
        id
    };
    let stop = format!("#{id} (Header) stopped:");
    assert_eq!(item(&desk(&w, &project), "Header")["stopped"], true);
    w.hub().desk_tick_now();
    w.settle();
    assert!(!sent_to(&w, &l).iter().any(|t| t.contains(&stop)), "no stop to a limited lead: {:?}", sent_to(&w, &l));
    assert_eq!(w.snapshot().session(&l)["send_at"]["at_ms"].as_u64(), Some(0), "its carry-on is kept");
    // Carried on after the reset: its turn ends, and the stop reaches it.
    w.hosted_says(&l, HostedEvent::Taken { text: "Continue".into() });
    ends(&w, &l, "Carrying on.");
    let told = changes(&w, &l);
    assert!(told.iter().any(|t| t.contains(&stop)), "{told:?}");
}

#[test]
fn a_new_lead_is_sent_again_what_the_old_one_never_heard() {
    let w = World::new();
    let (_, project) = holder(&w);
    let old = new_lead(&w, &project);
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    assert_eq!(changes(&w, &old).len(), 1);
    // It ends before its turn on it does: a new lead takes over and is sent it again, same number, after the summary.
    let new = new_lead(&w, &project);
    assert_ne!(new, old);
    let told = changes(&w, &new);
    assert!(told.len() == 1 && told[0].contains(&format!("New item #{}", a.id)) && told[0].ends_with("(Desk changes 1)") && told[0].starts_with("<desk"), "{told:?}");
}

#[test]
fn a_restart_between_send_and_heard_sends_it_again_with_the_same_number() {
    let mut w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    assert_eq!(changes(&w, &l).len(), 1);
    w.relaunch();
    w.hub().desk_tick_now();
    w.settle();
    let told: Vec<String> = changes(&w, &l).into_iter().filter(|t| t.contains(&format!("New item #{}", a.id))).collect();
    assert_eq!(told.len(), 2, "sent again from heard: {told:?}");
    assert!(told.iter().all(|t| t.ends_with("(Desk changes 1)")), "same number, so the lead skips it: {told:?}");
    // Heard this time: not sent a third time.
    ends(&w, &l, "Routing it.");
    w.hub().desk_tick_now();
    w.settle();
    assert_eq!(changes(&w, &l).len(), 2);
}

#[test]
fn a_stop_settled_before_the_lead_is_free_is_left_out() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = {
        ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
        let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
        busy(&w, &l);
        for (prompt, reply) in [("Header", "Looked at the header."), (&format!("Done? Write DONE: #{id}, or say what's left")[..], "The footer still overlaps it.")] {
            w.advance(1000);
            w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
            w.settle();
            w.advance(1000);
            ends(&w, &h, reply);
        }
        id
    };
    assert_eq!(item(&desk(&w, &project), "Header")["stopped"], true);
    // Its holder carries on and says DONE before the lead is free: the stop no longer applies.
    w.hosted_says(&h, HostedEvent::Taken { text: "Go on".into() });
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    ends(&w, &l, "Done looking.");
    w.hub().desk_tick_now();
    w.settle();
    assert!(!sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} (Header) stopped"))), "{:?}", sent_to(&w, &l));
    // The marks moved past it: a later change is numbered on, and the stop isn't in it. Its DONE's rows (said, then
    // to check: 2 and 3, folded into the latest) wait for that wake and go with it.
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    let last = changes(&w, &l).pop().unwrap();
    assert!(last.contains(&format!("New item #{}", b.id)) && last.contains(&format!("#{id} (Header) finished")) && !last.contains("stopped") && last.ends_with("(Desk changes 3–4)"), "{last}");
}

#[test]
fn a_stop_reaches_a_free_lead_once_and_is_marked_told() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = stop_header(&w, &h, &l, &project);
    let stops: Vec<String> = changes(&w, &l).into_iter().filter(|t| t.starts_with(&format!("#{id} (Header) stopped:"))).collect();
    assert_eq!(stops.len(), 1, "{:?}", sent_to(&w, &l));
    w.hub().desk_tick_now();
    w.settle();
    assert_eq!(changes(&w, &l).len(), 1, "sent once, not again by the tick");
    assert!(crate::db::desk_get_item(&id).is_some_and(|i| i.told.is_some()), "told kept on the item");
}

/// Item "Header" routed by lead `l` to holder `h`, which took it: its id.
fn at_work(w: &World, h: &str, l: &str, project: &str) -> String {
    ends(w, l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(w, project), "Header")["id"].as_str().unwrap().to_string();
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: format!("Desk item #{id}: Header") });
    w.settle();
    assert_eq!(item(&desk(w, project), "Header")["state"], "working");
    id
}

/// `ms` pass, with Cue's ticks (the finishing tick among them) on the way.
fn pass(w: &World, ms: u64) {
    for _ in 0..ms / MIN {
        w.advance(MIN);
        w.tick(true);
    }
}

/// The messages to `l` with a Desk change that has `words`.
fn told(w: &World, l: &str, words: &str) -> Vec<String> {
    changes(w, l).into_iter().filter(|t| t.contains(words)).collect()
}

#[test]
fn a_finishing_stuck_on_one_step_wakes_the_lead_once_even_after_it_heard_it() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    // Its check began and nothing moves it on (this item has no copy, so the landing line never takes it).
    let at = crate::clock::now_ms();
    w.hub().write(|st| {
        let it = st.desk.item_mut(&id).unwrap();
        (it.state, it.finish) = ("finishing".into(), Some(Finish { step: "checking".into(), at, ..Default::default() }));
    });
    let stuck = format!("#{id} (Header) is stuck finishing: its checking step");
    pass(&w, 14 * MIN);
    assert!(told(&w, &l, &stuck).is_empty(), "not 15 minutes yet: {:?}", sent_to(&w, &l));
    pass(&w, 2 * MIN);
    assert_eq!(told(&w, &l, &stuck).len(), 1, "{:?}", sent_to(&w, &l));
    // Heard, and still stuck: the tick sees the same change again and adds no row.
    ends(&w, &l, "Looking at it.");
    pass(&w, 10 * MIN);
    w.relaunch();
    pass(&w, 2 * MIN);
    assert_eq!(told(&w, &l, &stuck).len(), 1, "{:?}", sent_to(&w, &l));
}

#[test]
fn a_check_that_failed_twice_wakes_the_lead() {
    let w = World::new();
    let checker = Arc::new(super::desk_land::Checker::default());
    *crate::locking::lock(&checker.says, "checker says") = Some(Err("window: deskFoo is declared twice".into()));
    w.hub().desk_set_installer(checker.clone());
    let (_, project, id, h) = super::desk_commit::routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = w.hub().read(|st| st.desk.lead_sid(&project).map(str::to_string)).unwrap();
    let path = std::path::PathBuf::from(super::desk_commit::item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string());
    let twice = format!("#{id} (Header jumps) failed its check 2 times");
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(super::desk_commit::item(&w, &project, "Header jumps")["finish"]["step"], "check_failed");
    assert!(told(&w, &l, "failed its check").is_empty(), "once is its holder's to fix: {:?}", sent_to(&w, &l));
    std::fs::write(path.join("a.txt"), "a, again\n").unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: "Fix it".into() });
    ends(&w, &h, &format!("Fixed again.\nDONE: #{id}"));
    assert_eq!(told(&w, &l, &twice).len(), 1, "{:?}", sent_to(&w, &l));
}

#[test]
fn a_holder_on_a_permission_prompt_reaches_the_lead_as_a_desk_change() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    w.hosted_says(&h, HostedEvent::Ask(Ask {
        request_id: "r1".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "rm -rf build" }),
        suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: String::new(), message: String::new(), form: Value::Null,
    }));
    w.settle();
    for _ in 0..6 {
        w.advance(MIN);
        w.tick(false);
    }
    let t = told(&w, &l, "stuck on a permission prompt");
    assert!(t.len() == 1 && t[0].starts_with(&format!("#{id} (Header) ")) && t[0].contains("Bash: rm -rf build"), "{:?}", sent_to(&w, &l));
    assert!(crate::db::desk_get_item(&id).is_some_and(|i| i.fed.contains_key("permission")), "saved with the item");
}

#[test]
fn a_pick_reaches_its_holder_closes_the_item_and_tells_the_lead_which_option() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas\nPick one for working items: A, B or C."));
    ends(&w, &l, "Waiting on the pick.");
    let before = changes(&w, &l).len();
    w.hub().desk_item_act(&m, "pick", "option C").unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{m} (Mocks: working items): option C was picked"))), "its holder gets the pick");
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["state"], "done");
    let told = changes(&w, &l);
    assert_eq!(told.len(), before + 1, "{told:?}");
    let last = told.last().unwrap();
    let build = item(&desk(&w, &project), "Build: working items (option C)")["id"].as_str().unwrap().to_string();
    assert!(last.starts_with(&format!("You picked option C on #{m} (Mocks: working items)")) && last.contains(&format!("New item #{build}")) && last.contains("Route it now"), "{last}");
}

#[test]
fn a_route_to_no_such_session_wakes_the_lead() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> nobody-here | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let t = told(&w, &l, &format!("Your route of #{id} (Header) to \"nobody-here\" found no session by that name"));
    assert_eq!(t.len(), 1, "{:?}", sent_to(&w, &l));
}

#[test]
fn a_holder_that_ended_with_an_item_at_work_wakes_the_lead() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    w.hub().drop_session(&h);
    w.settle();
    pass(&w, MIN);
    let t = told(&w, &l, "ended with it at work");
    assert!(t.len() == 1 && t[0].starts_with(&format!("#{id} (Header)'s holder ")), "{:?}", sent_to(&w, &l));
}

#[test]
fn a_holder_asking_you_reaches_the_lead_alone_after_five_minutes() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    ends(&w, &h, "Looked at it.\nShould the header stay blue or turn green?");
    assert!(item(&desk(&w, &project), "Header")["asks"].is_array(), "it waits on you");
    let asks = format!("#{id} (Header)'s holder");
    w.hub().desk_tick_now();
    w.settle();
    pass(&w, 4 * MIN);
    assert!(told(&w, &l, &asks).is_empty(), "a grouped row waits for a wake: {:?}", sent_to(&w, &l));
    pass(&w, 2 * MIN);
    let t = told(&w, &l, &asks);
    assert!(t.len() == 1 && t[0].contains("asks the user: \"Should the header stay blue or turn green?\""), "then goes alone: {:?}", sent_to(&w, &l));
}

#[test]
fn a_holder_asking_you_wakes_the_lead_at_once_while_you_are_away() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    assert!(told(&w, &l, "Away mode:").is_empty(), "Away turning on waits for a wake: {:?}", sent_to(&w, &l));
    ends(&w, &h, "Looked at it.\nShould the header stay blue or turn green?");
    let t = told(&w, &l, &format!("#{id} (Header)'s holder"));
    assert!(t.len() == 1 && t[0].contains("Away mode:"), "at once, Away's row with it: {:?}", sent_to(&w, &l));
    assert!(t[0].find("Away mode:").unwrap() < t[0].find("asks the user").unwrap(), "in order: {}", t[0]);
}

#[test]
fn away_turning_on_or_off_reaches_the_lead_in_order_with_the_rest() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    let t = told(&w, &l, "Away mode:");
    assert!(t.len() == 1 && t[0].find("Away mode:").unwrap() < t[0].find(&format!("New item #{}", a.id)).unwrap(), "with the next wake, in order: {:?}", sent_to(&w, &l));
    ends(&w, &l, "Routing it.");
    w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    pass(&w, 6 * MIN);
    assert_eq!(told(&w, &l, "Away mode is off").len(), 1, "alone after five minutes: {:?}", sent_to(&w, &l));
}

#[test]
fn a_clash_handed_back_to_its_holder_reaches_the_lead() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    let at = crate::clock::now_ms();
    let it = w.hub().write(|st| {
        let it = st.desk.item_mut(&id).unwrap();
        (it.state, it.finish) = ("finishing".into(), Some(Finish { step: "landing".into(), at, ..Default::default() }));
        it.clone()
    });
    w.hub().finish_clash(&it, "a.txt clashed with the main line");
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working", "back with its holder");
    pass(&w, 6 * MIN);
    let t = told(&w, &l, &format!("#{id} (Header) clashed with the main line"));
    assert_eq!(t.len(), 1, "{:?}", sent_to(&w, &l));
    assert!(crate::db::desk_get_item(&id).is_some_and(|i| i.fed.contains_key("clash")), "saved with the item");
}

#[test]
fn a_done_goes_with_the_next_wake() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let done = format!("#{id} (Header) finished:");
    assert!(told(&w, &l, &done).is_empty(), "grouped: no wake of its own yet: {:?}", sent_to(&w, &l));
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    let t = told(&w, &l, &done);
    assert!(t.len() == 1 && t[0].contains(&format!("New item #{}", b.id)), "with the new item's wake: {:?}", sent_to(&w, &l));
}

#[test]
fn what_you_did_never_wakes_the_lead_but_goes_with_its_next_message() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = at_work(&w, &h, &l, &project);
    let before = changes(&w, &l).len();
    w.hub().desk_item_act(&id, "drop", "").unwrap();
    w.settle();
    pass(&w, 10 * MIN);
    assert_eq!(changes(&w, &l).len(), before, "silent: {:?}", sent_to(&w, &l));
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    let last = changes(&w, &l).pop().unwrap();
    assert!(last.starts_with(&format!("You dropped #{id} (Header).")) && last.contains(&format!("New item #{}", b.id)), "{last}");
}

/// Desk `project`'s feed marks: (sent, heard).
fn marks(w: &World, project: &str) -> (u64, u64) {
    w.hub().read(|st| st.desk.feed.get(project).map_or((0, 0), |f| (f.sent.n, f.heard.n)))
}

/// Wait (for real, a few seconds at most) until `done`.
fn wait_until(what: &str, done: impl Fn() -> bool) {
    let t0 = std::time::Instant::now();
    while !done() {
        assert!(t0.elapsed().as_secs() < 5, "{what}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_send_as_the_leads_turn_ends_is_heard_only_by_a_turn_after_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    busy(&w, &l);
    w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    assert!(changes(&w, &l).is_empty());
    // Its turn ends: it's marked at its prompt first, and the Desk hears of the end after. A send in between finds
    // it free, and goes.
    let origin = w.hub().session_origin(&l).unwrap();
    w.hub().write(|st| {
        st.sessions.mark(&origin, "waiting", None);
        st.sessions.turn_ended(&l);
    });
    w.hub().desk_feed_send();
    assert_eq!(changes(&w, &l).len(), 1);
    w.hub().desk_feed_turn_ended(&l);
    w.settle();
    assert_eq!(marks(&w, &project), (1, 0), "the turn that ended never saw it");
    w.hosted_says(&l, HostedEvent::Taken { text: "New item".into() });
    ends(&w, &l, "Routing it.");
    assert_eq!(marks(&w, &project), (1, 1), "the turn it started heard it");
}

#[test]
fn one_send_at_a_time_per_desk_and_rows_that_came_meanwhile_follow_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    busy(&w, &l);
    w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    w.hosted.hang_next_send(release.clone());
    // Its turn ends: its row goes, and is held up on its way.
    w.hosted_says(&l, HostedEvent::TurnEnded { text: "Done looking.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    let first = w.client().spawn(|c| c.hub.desk_feed_send());
    w.hosted.wait_hung();
    let p = project.clone();
    let second = w.client().spawn(move |c| {
        let b = c.hub.desk_add(&p, "task", "Footer", "", None).unwrap();
        c.hub.desk_feed_send();
        b
    });
    wait_until("a second send for the Desk leaves its row to the one under way", || second.is_finished());
    assert_eq!(marks(&w, &project).0, 1, "the marks wait for the send under way");
    release.store(true, std::sync::atomic::Ordering::SeqCst);
    first.join().unwrap();
    let b = second.join().unwrap();
    w.settle();
    let told = changes(&w, &l);
    assert!(told.len() == 2 && told[0].ends_with("(Desk changes 1)") && told[1].contains(&format!("New item #{}", b.id)) && told[1].ends_with("(Desk changes 2)"), "in order, one after the other: {told:?}");
    w.hosted_says(&l, HostedEvent::Taken { text: "New items".into() });
    ends(&w, &l, "Routing them.");
    assert_eq!(marks(&w, &project), (2, 2));
}

/// A new item's row sent to lead `l` as it went back to work (your message to it on its way): kept in Cue for its
/// turn's end. The item's id.
fn kept_for_turn_end(w: &World, l: &str, project: &str) -> String {
    let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    w.hosted.hang_next_send(release.clone());
    let to = l.to_string();
    let yours = w.client().spawn(move |c| c.hub.tell(&to, "Look at the board"));
    w.hosted.wait_hung();
    let p = project.to_string();
    let feed = w.client().spawn(move |c| {
        let a = c.hub.desk_add(&p, "bug", "Header", "", None).unwrap();
        c.hub.desk_feed_send();
        a
    });
    wait_until("its row is on its way", || marks(w, project).0 == 1);
    w.hosted_says(l, HostedEvent::Taken { text: "Look at the board".into() });
    release.store(true, std::sync::atomic::Ordering::SeqCst);
    yours.join().unwrap().unwrap();
    let a = feed.join().unwrap();
    w.settle();
    assert!(w.hub().read(|st| st.sessions.held(l)).is_some_and(|h| h.text.contains("(Desk changes 1)")), "kept for its turn's end");
    a.id
}

#[test]
fn rows_kept_for_the_leads_turn_end_are_heard_by_the_turn_they_start() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = kept_for_turn_end(&w, &l, &project);
    ends(&w, &l, "Looked.");
    assert_eq!(told(&w, &l, &format!("New item #{id}")).len(), 1, "it went as the turn ended: {:?}", sent_to(&w, &l));
    assert_eq!(marks(&w, &project), (1, 0), "the turn it was kept through didn't hear it");
    w.hosted_says(&l, HostedEvent::Taken { text: "New item".into() });
    ends(&w, &l, "Routing it.");
    assert_eq!(marks(&w, &project), (1, 1));
}

#[test]
fn rows_kept_for_the_leads_turn_end_and_taken_back_are_sent_again() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id = kept_for_turn_end(&w, &l, &project);
    // You take the kept message back into your box: it never went.
    assert!(w.hub().unhold(&l).is_some());
    ends(&w, &l, "Looked.");
    let t = told(&w, &l, &format!("New item #{id}"));
    assert!(t.len() == 1 && t[0].ends_with("(Desk changes 1)"), "sent again from heard, as it's free: {:?}", sent_to(&w, &l));
    assert_eq!(marks(&w, &project), (1, 0), "not heard: that turn never saw it");
}

#[test]
fn a_lead_you_stopped_is_not_woken_and_hears_its_changes_on_its_next_turn() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    w.hub().mark_state(&w.hub().session_origin(&l).unwrap(), "stopped");
    let launched = w.hosted.launched().len();
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    w.settle();
    w.hub().desk_tick_now();
    w.settle();
    assert!(changes(&w, &l).is_empty() && w.hosted.launched().len() == launched, "not woken: {:?}", sent_to(&w, &l));
    // You send it something: that turn's end, it hears the row.
    w.hosted_says(&l, HostedEvent::Taken { text: "Carry on".into() });
    ends(&w, &l, "Carrying on.");
    let t = told(&w, &l, &format!("New item #{}", a.id));
    assert_eq!(t.len(), 1, "{:?}", sent_to(&w, &l));
}

#[test]
fn a_pick_reaches_the_lead_after_its_mocks_item_left_memory() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas\nPick one for working items: A, B or C."));
    ends(&w, &l, "Waiting on the pick.");
    busy(&w, &l);
    w.hub().desk_item_act(&m, "pick", "option C").unwrap();
    w.settle();
    // Done, it leaves memory (as done items do over time) before the lead is free.
    w.hub().write(|st| st.desk.items.retain(|i| i.id != m));
    ends(&w, &l, "Done looking.");
    let last = changes(&w, &l).pop().unwrap_or_default();
    assert!(last.starts_with(&format!("You picked option C on #{m} (Mocks: working items)")), "{last}");
}

#[test]
fn rows_heard_already_never_stay_saved() {
    let _w = World::new();
    let row = |n: u64| crate::desk::feed::Change { desk: "d".into(), n, item: "a".into(), kind: crate::desk::feed::ChangeKind::NewItem, words: "w".into(), key: String::new(), created_ms: 0 };
    let heard = crate::desk::feed::Mark { n: 5, lead: "L".into() };
    let saved = |below: u64| crate::db::raw(|c| c.query_row("SELECT COUNT(*) FROM desk_feed WHERE project = 'd' AND n <= ?1", [below as i64], |r| r.get::<_, i64>(0))).unwrap();
    // "heard" moved past row 3 before the save of it ran (each is a write of its own).
    crate::db::desk_feed_marks("d", &heard, &heard).unwrap();
    crate::db::desk_save(&[], &[], &[row(3), row(6)]).unwrap();
    assert_eq!(saved(5), 0, "a row heard already isn't saved");
    // One left by a Cue before this fix: deleted as the feed loads.
    crate::db::raw(|c| c.execute("INSERT INTO desk_feed (project, n, item, kind, words, key, created_ms) VALUES ('d', 4, 'a', 'new', 'w', '', 0)", []));
    let f = crate::db::desk_feed_load();
    assert_eq!(f["d"].rows.iter().map(|r| r.n).collect::<Vec<_>>(), vec![6]);
    assert_eq!(saved(5), 0);
}
