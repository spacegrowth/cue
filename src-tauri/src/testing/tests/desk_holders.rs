//! An item's holder at work (hub/desk_said.rs): its DONE, its asks, its crew, and the replies and steps kept on the item.
//! A turn that ends without DONE: desk_stops.rs.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use super::team::{says, writes};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

#[test]
fn a_decided_line_in_the_turn_that_says_done_is_the_finished_items() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    ends(&w, &h, &format!("Moved it.\nDECIDED: kept the class name — renaming it touches the tests\nDONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Footer")["state"], "working", "the next one started");
    let c = now["decisions"].as_array().unwrap().iter().find(|c| c["text"].as_str().unwrap().starts_with("kept the class name")).unwrap();
    assert_eq!(c["item_id"].as_str(), Some(a.as_str()), "{now}");
}

#[test]
fn a_queued_item_doesnt_need_you_for_its_holders_ask() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let origin = w.hub().session_origin(&h).unwrap();
    let input = json!({ "questions": [{ "question": "Sticky?", "header": "Header", "options": [], "multiSelect": false }] });
    w.hub().add_ask(origin, crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "needs_you");
    assert_eq!(item(&now, "Footer")["state"], "queued");
}

#[test]
fn an_item_needs_you_while_its_holder_asks() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
    let origin = w.hub().session_origin(&h).unwrap();
    let input = json!({ "questions": [{ "question": "Keep it sticky?", "header": "Header", "options": [{ "label": "Yes", "description": "" }], "multiSelect": false }] });
    w.hub().add_ask(origin, crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "needs_you");
    assert_eq!(item(&now, "Header")["ask"]["text"], "Keep it sticky?");
}

/// What a holder made, named in the turn it says DONE (a page's link, a mock or a build by its full path), is kept
/// on the item for the Desk to link to; a path that's only code (no kind made to be looked at) isn't.
#[test]
fn what_the_turn_that_says_done_made_is_kept_on_the_item() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: task | Mocks for the list | done when: three mocks\nROUTE: #new -> cue-teams | why: it knows the list");
    let id = item(&desk(&w, &project), "Mocks for the list")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three mocks: [the page](https://claude.ai/artifact/abc123), and `/tmp/mocks/list.html`.\nChanged `/src/ui/desk.js` too.\nDONE: #{id}"));
    let now = desk(&w, &project);
    let it = item(&now, "Mocks for the list");
    assert_eq!(it["state"], "to_check");
    assert_eq!(it["made"], serde_json::json!(["https://claude.ai/artifact/abc123", "/tmp/mocks/list.html"]), "{it}");
}

/// Not a stall: a holder whose crew is still at work as its turn ends, a crew member holding an item (its lead's
/// turns aren't its own), and an item the Desk's lead holds (its turns are routing). No ask, and none waits on you;
/// once the crew is done, the holder's next turn with no DONE is one.
#[test]
fn a_holder_with_crew_at_work_a_crew_member_or_the_desk_lead_isnt_asked_if_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let header = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let asks = |sid: &str| sent_to(&w, sid).iter().filter(|t| t.starts_with("Done? Write DONE")).count();
    let state = |title: &str| item(&desk(&w, &project), title)["state"].as_str().unwrap_or("").to_string();
    // The holder hands the header to its crew, still at work as its turn ends.
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    let mut started = writes("toolu_F", "agentF", "fixer", &[]);
    let finished = started.pop().unwrap();
    says(&w, &h, started);
    w.advance(1000);
    ends(&w, &h, "Handed the header to fixer.");
    assert_eq!((state("Header"), asks(&h)), ("working".to_string(), 0), "its crew is on it");
    // The Footer goes to that crew member: its lead's turns don't stall it.
    ends(&w, &l, "ITEM: Footer\nROUTE: #new -> fixer | why: it's in there");
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["holder"]["kind"], "crew");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Footer too".into() });
    w.advance(1000);
    ends(&w, &h, "Told fixer.");
    assert_eq!((state("Footer"), asks(&h)), ("working".to_string(), 0));
    // The crew is done: the holder's next turn with no DONE on the header is a stall, asked once.
    says(&w, &h, [finished]);
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "fixer finished".into() });
    w.advance(1000);
    ends(&w, &h, "fixer finished its part.");
    assert_eq!((state("Header"), item(&desk(&w, &project), "Header")["stopped"].as_bool()), ("working".to_string(), Some(true)));
    assert_eq!(sent_to(&w, &h).iter().filter(|t| **t == format!("Done? Write DONE: #{header}, or say what's left")).count(), 1);
    // An item the Desk's lead holds: its routing turns aren't stalls.
    let added = w.hub().desk_add(&project, "task", "Colours", "", None).unwrap();
    w.hub().desk_give(&added.id, &l).unwrap();
    w.settle();
    w.advance(1000);
    w.hosted_says(&l, HostedEvent::Taken { text: "route".into() });
    w.advance(1000);
    ends(&w, &l, "Nothing to route.");
    assert_eq!((state("Colours"), asks(&l)), ("needs_resume".to_string(), 0));
}

/// A DONE said while the holder's crew is still at work waits: the item stays at work (not to check, so not waiting
/// on you); once the crew has finished, the DONE it said sends it to check, without saying it again (#393).
#[test]
fn a_done_said_while_its_crew_still_works_keeps_the_item_at_work() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let header = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let state = || item(&desk(&w, &project), "Header")["state"].as_str().unwrap_or("").to_string();
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    let mut started = writes("toolu_R", "agentR", "reviewer", &[]);
    let finished = started.pop().unwrap();
    says(&w, &h, started);
    w.advance(1000);
    ends(&w, &h, &format!("Built it; the review is still running.\n\nDONE: #{header}"));
    assert_eq!(state(), "working", "its reviewer is still at work");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| t.starts_with("Done? Write DONE")).count(), 0);
    // The reviewer finishes: the DONE it said goes ahead (#393), with no need to say it again and no "Done?" ask.
    says(&w, &h, [finished]);
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "reviewer finished".into() });
    w.advance(1000);
    ends(&w, &h, "The review is clean.");
    assert_eq!(state(), "to_check");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| t.starts_with("Done? Write DONE")).count(), 0);
}

/// What you write on an item's page, to its lead or its holder, and what you send back, are in its Steps: your words,
/// by you, with whom they went to. Writing isn't news about the item (its `updated` stays).
#[test]
fn what_you_write_on_an_items_page_is_in_its_steps() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let it = item(&desk(&w, &project), "Header").clone();
    let id = it["id"].as_str().unwrap().to_string();
    let lead_name = desk(&w, &project)["lead"]["name"].as_str().unwrap().to_string();
    w.hub().desk_item_act(&id, "wrote_lead", "Make it sticky too").unwrap();
    w.hub().desk_item_act(&id, "wrote_holder", " Keep the old colour ").unwrap();
    assert!(w.hub().desk_item_act(&id, "wrote_lead", "  ").is_err());
    let now = desk(&w, &project);
    let mine: Vec<_> = now["decisions"].as_array().unwrap().iter().filter(|d| d["by"] == "You" && d["item_id"] == id.as_str()).map(|d| (d["text"].as_str().unwrap(), d["why"].as_str().unwrap())).collect();
    assert!(mine.contains(&("Make it sticky too", format!("to {lead_name}").as_str())), "{mine:?}");
    assert!(mine.contains(&("Keep the old colour", "to cue-teams")), "{mine:?}");
    assert_eq!(item(&now, "Header")["updated"], it["updated"]);
}

#[test]
fn a_session_holding_two_items_answers_on_the_one_it_is_on_now() {
    let w = World::new();
    let (h, project) = holder(&w);
    let take = |id: &str| {
        let words = sent_to(&w, &h).into_iter().rev().find(|t| t.contains(&format!("Desk item #{id}"))).unwrap();
        w.hosted_says(&h, HostedEvent::Taken { text: words });
    };
    let a = w.hub().desk_add(&project, "bug", "First thing", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    take(&a.id);
    ends(&w, &h, "Looking at the first thing.");
    let b = w.hub().desk_add(&project, "bug", "Second thing", "", None).unwrap();
    w.hub().desk_give(&b.id, &h).unwrap();
    w.settle();
    take(&b.id);
    ends(&w, &h, "On the second thing now.");
    let said = |d: &Value, t: &str| d["items"].as_array().unwrap().iter().find(|i| i["title"] == t).unwrap()["said"].as_array().map(|s| s.iter().map(|x| x["text"].as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    let d = desk(&w, &project);
    assert_eq!(said(&d, "First thing"), vec!["Looking at the first thing."], "{d}");
    assert_eq!(said(&d, "Second thing"), vec!["On the second thing now."], "{d}");
    // Its work shows only on the one it's on. It stopped there with no lead to settle it: that's what waits on you (#182).
    assert_eq!((item(&d, "Second thing")["now_on"].as_bool(), item(&d, "Second thing")["latest"].as_str()), (Some(true), Some("It stopped, and this Desk has no lead to settle it: say what's next")));
    assert_eq!((item(&d, "First thing")["now_on"].as_bool(), item(&d, "First thing")["latest"].as_str()), (None, Some("")));
    // Written from the first one's page: about the first one, so is its answer.
    let about = format!("About #{} (First thing): one more fix", a.id);
    w.send_to(&h, &about);
    w.hosted_says(&h, HostedEvent::Taken { text: about });
    ends(&w, &h, "Fixed it in the first thing.");
    let d = desk(&w, &project);
    assert_eq!(said(&d, "First thing").last().map(String::as_str), Some("Fixed it in the first thing."));
    assert_eq!(said(&d, "Second thing").len(), 1, "{d}");
    assert_eq!(item(&d, "First thing")["now_on"].as_bool(), Some(true));
    assert_eq!(item(&d, "Second thing")["now_on"].as_bool(), None);
    // Typed in its own chat, naming no item: untagged, so its answer is on no item's page.
    w.send_to(&h, "what's the time?");
    w.hosted_says(&h, HostedEvent::Taken { text: "what's the time?".into() });
    ends(&w, &h, "Half past.");
    let d = desk(&w, &project);
    assert_eq!((said(&d, "First thing").len(), said(&d, "Second thing").len()), (2, 1), "{d}");
    assert_eq!((item(&d, "First thing")["now_on"].as_bool(), item(&d, "Second thing")["now_on"].as_bool()), (None, None));
}

/// A session at work on just one item: what you type in its own chat, naming no item, is about that one, so its
/// work and its answer show on the item's page (#395). Naming another item, it isn't.
#[test]
fn a_session_holding_one_item_answers_there_untagged() {
    let w = World::new();
    let (h, project) = holder(&w);
    let a = w.hub().desk_add(&project, "bug", "First thing", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    let words = sent_to(&w, &h).into_iter().rev().find(|t| t.contains(&format!("Desk item #{}", a.id))).unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: words });
    ends(&w, &h, "Looking at it.");
    let said = |d: &Value| item(d, "First thing")["said"].as_array().map(|s| s.iter().map(|x| x["text"].as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    w.send_to(&h, "also check the footer");
    w.hosted_says(&h, HostedEvent::Taken { text: "also check the footer".into() });
    let d = desk(&w, &project);
    assert_eq!(item(&d, "First thing")["now_on"].as_bool(), Some(true), "{d}");
    ends(&w, &h, "The footer is fine too.");
    let d = desk(&w, &project);
    assert_eq!(said(&d).last().map(String::as_str), Some("The footer is fine too."), "{d}");
    let b = w.hub().desk_add(&project, "bug", "Other thing", "", None).unwrap();
    let other = format!("look at #{} instead", b.id);
    w.send_to(&h, &other);
    w.hosted_says(&h, HostedEvent::Taken { text: other });
    ends(&w, &h, "Looked at the other one.");
    let d = desk(&w, &project);
    assert_eq!(said(&d).len(), 2, "{d}");
}

/// A reply is kept whole on its item (no longer cut at 1,200 characters) and names the session it was said in, so the
/// item's page shows it in full with a link to that turn (#276).
#[test]
fn a_long_reply_is_kept_whole_with_its_session() {
    let w = World::new();
    let (h, project) = holder(&w);
    let a = w.hub().desk_add(&project, "bug", "First thing", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    let words = sent_to(&w, &h).into_iter().rev().find(|t| t.contains(&format!("Desk item #{}", a.id))).unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: words });
    let long = format!("Here is what I found.\n\n{}", "Every line of it matters. ".repeat(200));
    ends(&w, &h, &long);
    let d = desk(&w, &project);
    let said = &item(&d, "First thing")["said"][0];
    // Once, not twice: the turn's text is kept trimmed, and the last message (with its space) is that same text.
    assert_eq!(said["text"].as_str(), Some(long.trim_end()), "kept whole, once");
    assert_eq!(said["sid"].as_str(), Some(h.as_str()));
}

/// A turn's words told again with more after them (a turn that ended on a usage limit) are one reply on the item, the
/// longer; different words after them are a reply of their own.
#[test]
fn the_same_reply_told_again_with_more_is_one_reply() {
    let w = World::new();
    let (h, project) = holder(&w);
    let a = w.hub().desk_add(&project, "bug", "First thing", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    let words = sent_to(&w, &h).into_iter().rev().find(|t| t.contains(&format!("Desk item #{}", a.id))).unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: words });
    ends(&w, &h, "None of it is done yet.");
    ends(&w, &h, "None of it is done yet.\n\nYou've hit your limit");
    let said = |d: &Value| item(d, "First thing")["said"].as_array().map(|s| s.iter().map(|x| x["text"].as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    let d = desk(&w, &project);
    assert_eq!(said(&d), vec!["None of it is done yet.\n\nYou've hit your limit"], "{d}");
    ends(&w, &h, "Now it's done.");
    assert_eq!(said(&desk(&w, &project)).len(), 2);
}
