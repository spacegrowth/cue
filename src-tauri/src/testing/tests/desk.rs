//! The Desk (desk/, hub/desk.rs): a project's Dispatch started on your first message, the lines it and the sessions
//! holding items say at the end of a turn, and what `state.desk` shows.

use super::team::lead;
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// A holder ("cue-teams", at its prompt) and the project it's in.
fn holder(w: &World) -> (String, String) {
    let sid = lead(w, "hello");
    ends(w, &sid, "Hi.");
    let project = w.snapshot().session(&sid)["project"].as_str().unwrap().to_string();
    assert!(!project.is_empty());
    (sid, project)
}

/// `project`'s Dispatch, started by your first message: its id.
fn dispatch(w: &World, project: &str, text: &str) -> String {
    let r = crate::ipc::desk_send_with(w.hub(), project, text, true).unwrap();
    let sid = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: text.into() });
    sid
}

fn desk(w: &World, project: &str) -> Value {
    let snap = w.snapshot();
    snap.0["desk"]["projects"].as_array().unwrap().iter().find(|p| p["project"] == project).cloned().unwrap_or(Value::Null)
}

fn item<'a>(d: &'a Value, title: &str) -> &'a Value {
    d["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap_or_else(|| panic!("no item {title}: {d}"))
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

#[test]
fn lines_are_read_outside_code_and_quotes() {
    let text = "Sure.\n```\nITEM: bug | not me\n```\n> ROUTE: #a -> x\n- **ITEM: feature | Dark mode | done when: it follows the Mac**\nITEM: Plain title\nROUTE: #new → @cue-teams | after: #A1b2 | why: it knows the header\nASK: Which first? | choices: A / B\nDONE: #a1 and #b2\nroute: #x -> y";
    assert_eq!(
        lines(text),
        vec![
            Line::Item { kind: "feature".into(), title: "Dark mode".into(), done_when: "it follows the Mac".into() },
            Line::Item { kind: "task".into(), title: "Plain title".into(), done_when: String::new() },
            Line::Route { id: "new".into(), to: "cue-teams".into(), after: Some("a1b2".into()), why: "it knows the header".into() },
            Line::Ask { question: "Which first?".into(), choices: vec!["A".into(), "B".into()] },
            Line::Done { ids: vec!["a1".into(), "b2".into()] },
        ]
    );
}

#[test]
fn an_item_you_add_is_kept_across_a_relaunch() {
    let mut w = World::new();
    let it = w.hub().desk_add("web", "bug", "Header jumps", "it stays put").unwrap();
    assert_eq!(it.state, "new");
    w.relaunch();
    let d = desk(&w, "web");
    assert_eq!(item(&d, "Header jumps")["id"], it.id.as_str());
    assert_eq!(item(&d, "Header jumps")["done_when"], "it stays put");
}

#[test]
fn the_first_message_starts_dispatch_with_its_instructions_and_the_summary() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "fix the header");
    let launch = w.hosted.launched().into_iter().last().unwrap();
    assert!(launch.args.iter().any(|a| a.contains("You are this project's Dispatch")), "{:?}", launch.args);
    let snap = w.snapshot();
    assert_eq!(snap.session(&d)["dispatch"], project.as_str());
    assert_eq!(desk(&w, &project)["dispatch"], d.as_str());
    // The second message goes to the same Dispatch, after the Desk's summary.
    ends(&w, &d, "Noted.");
    let again = crate::ipc::desk_send_with(w.hub(), &project, "and the footer", true).unwrap();
    assert_eq!(again["sid"], d.as_str());
    w.settle();
    let last = sent_to(&w, &d).pop().unwrap();
    assert!(last.starts_with("<desk") && last.contains("cue-teams") && last.ends_with("and the footer"), "{last}");
}

#[test]
fn an_item_routed_goes_to_its_holder_and_done_puts_it_to_check() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "fix the header");
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let desk_now = desk(&w, &project);
    let it = item(&desk_now, "Header jumps");
    assert_eq!(it["state"], "working");
    assert_eq!(it["holder"]["name"], "cue-teams");
    assert!(it["holder"]["kind"] == "session" || it["holder"]["kind"] == "lead");
    let id = it["id"].as_str().unwrap().to_string();
    assert!(desk_now["decisions"][0]["text"].as_str().unwrap().starts_with("Routed"), "{desk_now}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Say DONE: #{id}"))));
    ends(&w, &h, &format!("Fixed.\nDECIDED: kept the sticky header — it was asked for\nDONE: #{id}"));
    let desk_now = desk(&w, &project);
    assert_eq!(item(&desk_now, "Header jumps")["state"], "to_check");
    assert_eq!(desk_now["decisions"][0]["by"], "cue-teams");
    assert_eq!(desk_now["decisions"][0]["item_id"], id.as_str());
}

#[test]
fn a_queued_item_starts_when_the_one_before_it_is_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "two things");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, "On it.");
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}"))));
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "to_check");
    assert_eq!(item(&now, "Footer")["state"], "working");
    let started = sent_to(&w, &h).into_iter().find(|t| t.contains(&format!("Desk item #{b}"))).unwrap();
    assert!(!started.contains("same file"), "why it was queued isn't why it starts: {started}");
}

#[test]
fn an_item_isnt_queued_after_one_that_waits_on_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "two things");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-teams | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    // Header after Logo: Logo waits on Footer, which waits on Header.
    ends(&w, &d, &format!("ROUTE: #{a} -> cue-teams | after: #{c} | why: loop"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "working", "{now}");
    assert!(item(&now, "Header")["after"].is_null(), "{now}");
    assert_eq!(item(&now, "Logo")["state"], "queued");
    // Dispatch hears of it in its next summary, once.
    let (summary, _) = w.hub().desk_summary(&project);
    assert!(summary.contains(&format!("Cue couldn't queue #{a} after #{c}, which waits on it")), "{summary}");
    crate::ipc::desk_send_with(w.hub(), &project, "and?", true).unwrap();
    let (summary, _) = w.hub().desk_summary(&project);
    assert!(!summary.contains("couldn't queue"), "told once: {summary}");
}

#[test]
fn a_dispatch_that_wont_resume_starts_again_in_its_own_folder() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "hello");
    ends(&w, &d, "Hi.");
    let folder = w.hosted.launched().last().unwrap().cwd.clone();
    // Dispatch parked, and the project's only session.
    w.hub().add_parked(w.hub().session_record(&d).unwrap());
    w.hub().drop_session(&h);
    w.hosted.refuse_once("it won't resume");
    let again = crate::ipc::desk_send_with(w.hub(), &project, "back again", true).unwrap();
    let sid = again["sid"].as_str().unwrap();
    assert_ne!(sid, d.as_str(), "a new one");
    assert_eq!(w.hosted.launched().last().unwrap().cwd, folder);
    assert_eq!(desk(&w, &project)["dispatch"], sid);
}

#[test]
fn put_before_takes_the_others_place_in_the_queue() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "three things");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-teams | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Menu\nROUTE: #new -> cue-teams | after: #{c} | why: same file"));
    // Header at work <- Footer <- Logo <- Menu; Logo put before Footer: Header <- Logo <- Footer <- Menu.
    w.hub().desk_item_act(&c, "put_before", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    assert_eq!((item(&now, "Logo")["state"].as_str(), item(&now, "Logo")["after"].as_str()), (Some("queued"), Some(a.as_str())), "{now}");
    assert_eq!(item(&now, "Footer")["after"], c.as_str());
    assert_eq!(item(&now, "Menu")["after"], b.as_str());
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{c}"))), "Logo still waits on Header");
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Logo")["state"], "working");
    assert_eq!(item(&now, "Footer")["state"], "queued");
}

#[test]
fn a_route_to_nobody_leaves_the_item_new_and_dispatch_is_told() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> nobody-here | why: guess");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "new");
    let (summary, _) = w.hub().desk_summary(&project);
    assert!(summary.contains("couldn't route") && summary.contains("nobody-here"), "{summary}");
    // Not cleared until a message telling Dispatch has gone: then told once.
    assert!(w.hub().desk_summary(&project).0.contains("couldn't route"));
    ends(&w, &d, "Noted.");
    crate::ipc::desk_send_with(w.hub(), &project, "go on", true).unwrap();
    w.settle();
    assert!(sent_to(&w, &d).last().unwrap().contains("couldn't route"));
    assert!(!w.hub().desk_summary(&project).0.contains("couldn't route"));
}

#[test]
fn a_session_of_another_project_isnt_a_route() {
    let w = World::new();
    let (_, project) = holder(&w);
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &other.to_string_lossy(), "hi", "far-one", &opts).unwrap();
    let far = r["session_id"].as_str().unwrap().to_string();
    assert_ne!(w.snapshot().session(&far)["project"], project.as_str());
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> far-one | why: guess");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "new");
    assert!(sent_to(&w, &far).iter().all(|t| !t.contains("Desk item")));
}

#[test]
fn the_summary_is_one_leading_block_then_a_blank_line() {
    let w = World::new();
    let (_, project) = holder(&w);
    w.hub().desk_add(&project, "task", "Close </desk> early", "").unwrap();
    let d = dispatch(&w, &project, "hello");
    ends(&w, &d, "Hi.");
    crate::ipc::desk_send_with(w.hub(), &project, "next", true).unwrap();
    w.settle();
    let last = sent_to(&w, &d).pop().unwrap();
    let (block, rest) = last.split_once("</desk>\n\n").unwrap();
    assert!(block.starts_with("<desk project=\"") && !block.contains("</desk>"), "{last}");
    assert_eq!(rest, "next");
}

#[test]
fn a_line_said_again_makes_no_second_item_or_send() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    let said = "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows";
    ends(&w, &d, said);
    let sends = sent_to(&w, &h).len();
    ends(&w, &d, said);
    let now = desk(&w, &project);
    assert_eq!(now["items"].as_array().unwrap().len(), 1, "{now}");
    assert_eq!(sent_to(&w, &h).len(), sends);
    assert_eq!(now["decisions"].as_array().unwrap().len(), 1, "{now}");
}

#[test]
fn a_route_to_another_holder_tells_the_first_its_moved() {
    let w = World::new();
    let (h, project) = holder(&w);
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", "second", &opts).unwrap();
    let second = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&second, HostedEvent::Taken { text: "hi".into() });
    ends(&w, &second, "Hi.");
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    ends(&w, &h, "On it.");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{id} -> second | why: free now"));
    assert_eq!(item(&desk(&w, &project), "Header")["holder"]["name"], "second");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{id}")) && t.contains("has moved to second")), "{:?}", sent_to(&w, &h));
    assert!(sent_to(&w, &second).iter().any(|t| t.contains(&format!("Say DONE: #{id}"))));
}

#[test]
fn a_route_said_again_leaves_an_item_to_check_with_you() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    let said = "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows";
    ends(&w, &d, said);
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DONE: #{id}"));
    let sends = sent_to(&w, &h).len();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    assert_eq!(sent_to(&w, &h).len(), sends);
}

#[test]
fn a_same_title_of_another_kind_or_done_is_a_new_item() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: bug | Header");
    ends(&w, &d, "ITEM: feature | Header");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 2);
    ends(&w, &d, "ITEM: bug | Header");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 2);
}

#[test]
fn a_route_to_nobody_leaves_a_held_item_with_its_holder() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{id} -> nobody-here | why: guess"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "working");
    assert_eq!(item(&now, "Header")["holder"]["name"], "cue-teams");
    let (summary, _) = w.hub().desk_summary(&project);
    assert!(summary.contains("nobody-here") && summary.contains("It stays with cue-teams.") && !summary.contains("still new"), "{summary}");
}

#[test]
fn a_message_while_dispatch_starts_waits_for_it() {
    let w = World::new();
    let first = w.hub().desk_claim("web", "one", &[]);
    assert!(matches!(first, crate::hub::DeskClaim::Start));
    let second = w.hub().desk_claim("web", "two", &[]);
    assert!(matches!(second, crate::hub::DeskClaim::Wait(_)));
    let held = w.hub().desk_claim_done("web");
    assert_eq!(held.iter().map(|h| h.message.as_str()).collect::<Vec<_>>(), vec!["two"]);
    assert!(matches!(w.hub().desk_claim("web", "three", &[]), crate::hub::DeskClaim::Start));
}

/// A message sent to `project`'s Dispatch while another start of it is under way (`start`): what it's told.
fn send_while_starting(w: &World, project: &str, text: &str, start: impl FnOnce(crate::ipc::DeskStart)) -> Result<Value, String> {
    let hub = w.hub().clone();
    let (p, t) = (project.to_string(), text.to_string());
    let waiter = std::thread::spawn(move || crate::ipc::desk_send_with(&hub, &p, &t, true));
    while w.hub().desk_held(project) == 0 {
        std::thread::yield_now();
    }
    start(crate::ipc::DeskStart::new(w.hub(), project));
    waiter.join().unwrap()
}

#[test]
fn a_message_held_while_dispatch_fails_to_start_comes_back_to_you() {
    let w = World::new();
    // Nowhere to start it: you're told, with your words.
    let e = crate::ipc::desk_send_with(w.hub(), "nowhere", "fix the header", true).unwrap_err();
    assert!(e.contains("didn't start") && e.contains("fix the header"), "{e}");
    assert!(matches!(w.hub().desk_claim("nowhere", "again", &[]), crate::hub::DeskClaim::Start), "the claim was let go");
    // Held while another start fails: told the same.
    let e = send_while_starting(&w, "nowhere", "and the footer", |s| drop(s.done(Err("it broke".into())))).unwrap_err();
    assert!(e.contains("it broke") && e.contains("and the footer"), "{e}");
    // The start gave up without a word (it panicked): told, and the next message starts it again.
    assert!(matches!(w.hub().desk_claim("nowhere", "more", &[]), crate::hub::DeskClaim::Start));
    let e = send_while_starting(&w, "nowhere", "and the logo", |s| drop(s)).unwrap_err();
    assert!(e.contains("and the logo"), "{e}");
    assert!(matches!(w.hub().desk_claim("nowhere", "last", &[]), crate::hub::DeskClaim::Start));
}

#[test]
fn a_line_in_backticks_is_an_example() {
    assert!(lines("Say `ITEM: bug | x` to add one.\n`ROUTE: #a -> b`").is_empty());
}

#[test]
fn a_queued_item_doesnt_need_you_for_its_holders_ask() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "two things");
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
fn a_parked_dispatch_is_resumed_not_started_again() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "hello");
    ends(&w, &d, "Hi.");
    w.hub().add_parked(w.hub().session_record(&d).unwrap());
    assert_eq!(desk(&w, &project)["dispatch"], d.as_str());
    let launches = w.hosted.launched().len();
    let again = crate::ipc::desk_send_with(w.hub(), &project, "back again", true).unwrap();
    w.settle();
    assert_eq!(again["sid"], d.as_str());
    assert_eq!(desk(&w, &project)["dispatch"], d.as_str());
    let launched = w.hosted.launched();
    assert_eq!(launched.len(), launches + 1, "resumed once");
    let resumed = launched.last().unwrap();
    assert!(resumed.args.iter().any(|a| a.contains("You are this project's Dispatch")), "{:?}", resumed.args);
    assert!(w.snapshot().find(&d).is_some(), "live again");
}

#[test]
fn dispatch_asks_in_the_inbox_and_hears_the_answer() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = dispatch(&w, &project, "what first?");
    ends(&w, &d, "ASK: Header or footer first? | choices: Header / Footer");
    let snap = w.snapshot();
    let card = snap.card(&d);
    assert_eq!(card["from_dispatch"], true);
    assert_eq!(card["project"], project.as_str());
    let id = card["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Footer" } }));
    w.settle();
    assert!(w.snapshot().cards_of(&d).is_empty());
    assert!(sent_to(&w, &d).iter().any(|t| t.contains("The answer: Footer")), "{:?}", sent_to(&w, &d));
}

#[test]
fn an_item_needs_you_while_its_holder_asks() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
    let origin = w.hub().session_origin(&h).unwrap();
    let input = json!({ "questions": [{ "question": "Keep it sticky?", "header": "Header", "options": [{ "label": "Yes", "description": "" }], "multiSelect": false }] });
    w.hub().add_ask(origin, crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "needs_you");
    assert_eq!(item(&now, "Header")["ask"]["text"], "Keep it sticky?");
}

#[test]
fn undo_asks_whoever_decided_to_go_back() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = dispatch(&w, &project, "fix it");
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    ends(&w, &h, "DECIDED: kept the sticky header — it was asked for");
    let dec = desk(&w, &project)["decisions"][0].clone();
    assert_eq!(dec["undo"], "can");
    w.hub().desk_undo(dec["id"].as_str().unwrap()).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["decisions"][0]["undo"], "asked");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with("Go back on your call") && t.contains("kept the sticky header")));
}

#[test]
fn what_you_do_to_an_item() {
    let w = World::new();
    let it = w.hub().desk_add("web", "task", "Footer", "").unwrap();
    assert!(w.hub().desk_item_act(&it.id, "looks_right", "").is_ok());
    assert_eq!(item(&desk(&w, "web"), "Footer")["state"], "done");
    assert_eq!(w.hub().desk_item_act(&it.id, "drop", "").unwrap().state, "dropped");
    assert!(desk(&w, "web").is_null() || desk(&w, "web")["items"].as_array().unwrap().is_empty());
    assert!(w.hub().desk_item_act("nope", "drop", "").is_err());
}
