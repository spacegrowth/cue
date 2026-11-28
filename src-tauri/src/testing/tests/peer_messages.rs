//! Messages between sessions in a session's chat, both ways: one it got says who sent it (the name Cue shows for that
//! session) and its project; one a hosted session sent with SendMessage says who to and how it did: sending, waiting,
//! answered when that session wrote back, or failed with why.

use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::{Transcript, World};
use serde_json::{json, Value};

/// A hosted session started with `message`, which Claude has taken (its turn under way, its log written): its id.
fn running(w: &World, message: &str) -> String {
    let opts = StartOpts { hosted: true, perm: "auto".into(), extra: "--model opus".into(), ..Default::default() };
    let sid = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), message, "trial", &opts).unwrap()["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: message.into() });
    Transcript::at(&log(w, &sid)).prompt(message);
    sid
}

fn log(w: &World, sid: &str) -> String {
    w.hub().session_origin(sid).unwrap().transcript_path
}

fn turn_ends(w: &World, sid: &str, reply: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: reply.into(), failed: false, cost_usd: 0.25, unprompted: false });
    w.settle();
}

fn sends(w: &World, sid: &str, id: &str, to: &str, message: &str) {
    w.hosted_says(sid, HostedEvent::ToolCall { id: id.into(), name: "SendMessage".into(), input: json!({ "to": to, "message": message, "summary": "a word" }), agent: None });
}

fn comes_back(w: &World, sid: &str, id: &str, result: Value) {
    let output = json!([{ "type": "text", "text": result.to_string() }]);
    w.hosted_says(sid, HostedEvent::ToolResult { id: id.into(), output, failed: false, agent: None });
}

/// The session with pid `pid` wrote `body` to `sid`: in `sid`'s log, as Claude Code puts another session's message.
fn writes_to(w: &World, sid: &str, pid: i32, body: &str) {
    w.advance(1000);
    let text = format!("Another Claude session sent a message:\n<cross-session-message from-name=\"cue-9e\">\n{body}\n</cross-session-message>");
    Transcript::at(&log(w, sid)).prompt_from(&text, "p2", json!({ "kind": "peer", "name": "cue-9e", "body": body, "verifiedPeerPid": pid }));
    w.advance(1000);
}

fn peers(w: &World, sid: &str) -> Vec<Value> {
    w.snapshot().session(sid)["thread"].as_array().unwrap().iter().filter(|e| e["role"] == "peer").cloned().collect()
}

#[test]
fn a_message_it_sends_another_session_shows_sending_then_waiting_then_answered_when_that_session_writes_back() {
    let w = World::new();
    w.session("b").in_project("loops").running_as(4242).prompt("go");
    w.hub().rename("b", "helper").unwrap();
    let a = running(&w, "ask the helper");
    sends(&w, &a, "s1", "helper [f7c5db]", "Can #452 wait about 20 min?");
    let sent = peers(&w, &a);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!((sent[0]["to"].as_str(), sent[0]["to_sid"].as_str(), sent[0]["sent"].as_str(), sent[0]["text"].as_str()), (Some("helper"), Some("b"), Some("sending"), Some("Can #452 wait about 20 min?")));
    assert!(sent[0].get("from").is_none(), "a message it sent has no sender: {}", sent[0]);
    comes_back(&w, &a, "s1", json!({ "success": true, "message": "Message sent to helper" }));
    assert_eq!(peers(&w, &a)[0]["sent"], "waiting");
    writes_to(&w, &a, 4242, "It can wait.");
    turn_ends(&w, &a, "The helper says it can wait.");
    let thread = w.snapshot().session(&a)["thread"].clone();
    let p = peers(&w, &a);
    assert_eq!(p.len(), 2, "{thread}");
    assert_eq!((p[1]["from"].as_str(), p[1]["from_sid"].as_str(), p[1]["text"].as_str()), (Some("helper"), Some("b"), Some("It can wait.")), "{thread}");
    assert_eq!(p[0]["sent"], "answered", "{thread}");
    assert_eq!(p[0]["answered_ms"], p[1]["at_ms"], "answered when the answer came");
    let roles: Vec<&str> = thread.as_array().unwrap().iter().map(|e| e["role"].as_str().unwrap()).collect();
    assert_eq!(roles.iter().rev().take(3).collect::<Vec<_>>(), [&"agent", &"peer", &"peer"], "sent, its answer, then the reply: {roles:?}");
}

#[test]
fn only_the_latest_message_still_waiting_on_a_session_is_answered_by_it() {
    let w = World::new();
    w.session("b").in_project("loops").running_as(4242).prompt("go");
    w.hub().rename("b", "helper").unwrap();
    let a = running(&w, "ask the helper twice");
    for (id, text) in [("s1", "First?"), ("s2", "Second?")] {
        sends(&w, &a, id, "helper", text);
        comes_back(&w, &a, id, json!({ "success": true }));
    }
    writes_to(&w, &a, 4242, "Yes to the second.");
    turn_ends(&w, &a, "Done.");
    let states: Vec<String> = peers(&w, &a).iter().map(|e| format!("{} {}", e["text"].as_str().unwrap(), e["sent"].as_str().unwrap_or("-"))).collect();
    assert_eq!(states, ["First? waiting", "Second? answered", "Yes to the second. -"]);
}

#[test]
fn a_message_that_didnt_go_says_failed_and_why() {
    let w = World::new();
    let a = running(&w, "ask x");
    sends(&w, &a, "s1", "x", "Are you there?");
    comes_back(&w, &a, "s1", json!({ "success": false, "message": "No agent named 'x' is reachable." }));
    let p = peers(&w, &a);
    assert_eq!((p[0]["to"].as_str(), p[0].get("to_sid"), p[0]["sent"].as_str(), p[0]["why"].as_str()), (Some("x"), None, Some("failed"), Some("No agent named 'x' is reachable.")), "{p:?}");
}

#[test]
fn a_message_it_got_says_the_senders_name_in_cue_and_its_project() {
    let w = World::new();
    w.session("b").in_project("loops").running_as(4242).prompt("go");
    let a = running(&w, "lead");
    writes_to(&w, &a, 4242, "The loops are green.");
    turn_ends(&w, &a, "Good.");
    let project = w.snapshot().session("b")["project"].as_str().unwrap().to_string();
    assert!(!project.is_empty());
    let p = peers(&w, &a);
    assert_eq!((p[0]["from"].as_str(), p[0]["project"].as_str(), p[0]["from_sid"].as_str()), (Some("loops"), Some(project.as_str()), Some("b")), "by its folder's name while it has none in Cue: {p:?}");
    w.hub().rename("b", "loop keeper").unwrap();
    writes_to(&w, &a, 4242, "Still green.");
    turn_ends(&w, &a, "Good.");
    let p = peers(&w, &a);
    assert_eq!((p[1]["from"].as_str(), p[1]["project"].as_str()), (Some("loop keeper"), Some(project.as_str())), "by the name you gave it: {p:?}");
}

#[test]
fn a_message_to_main_or_its_own_teammate_is_no_other_sessions_and_isnt_noted() {
    let w = World::new();
    let a = running(&w, "lead a crew");
    w.hosted_says(&a, HostedEvent::ToolCall { id: "t1".into(), name: "Agent".into(), input: json!({ "name": "keeper", "description": "keep it", "prompt": "Keep the list.", "run_in_background": true }), agent: None });
    sends(&w, &a, "s1", "keeper", "Carry on.");
    sends(&w, &a, "s2", "main", "Done here.");
    sends(&w, &a, "s3", "a74648c287d6e1f9c", "And you.");
    sends(&w, &a, "s4", "*", "Everyone stop.");
    assert!(peers(&w, &a).is_empty(), "{:?}", peers(&w, &a));
}

#[test]
fn another_session_by_the_same_name_writing_back_doesnt_answer_a_message_sent_to_the_first() {
    let w = World::new();
    w.session("b").in_project("loops").running_as(4242).prompt("go");
    w.hub().rename("b", "helper").unwrap();
    w.session("c").in_project("helper").running_as(4343).prompt("go");
    let a = running(&w, "ask the helper");
    sends(&w, &a, "s1", "helper", "Can it wait?");
    comes_back(&w, &a, "s1", json!({ "success": true }));
    writes_to(&w, &a, 4343, "Not me.");
    turn_ends(&w, &a, "Hm.");
    let p = peers(&w, &a);
    assert_eq!((p[0]["to_sid"].as_str(), p[0]["sent"].as_str()), (Some("b"), Some("waiting")), "{p:?}");
    assert_eq!((p[1]["from"].as_str(), p[1]["from_sid"].as_str()), (Some("helper"), Some("c")), "{p:?}");
}

#[test]
fn a_message_still_sending_when_its_session_stops_says_failed() {
    let w = World::new();
    let a = running(&w, "ask x");
    sends(&w, &a, "s1", "x", "Are you there?");
    w.hosted_says(&a, HostedEvent::Exited { code: Some(1), during_turn: true, last_words: String::new() });
    w.settle();
    let p = peers(&w, &a);
    assert_eq!((p[0]["sent"].as_str(), p[0]["why"].as_str()), (Some("failed"), Some(crate::sessions::peers::NO_RESULT)), "{p:?}");
    comes_back(&w, &a, "s1", json!({ "success": true }));
    let p = peers(&w, &a);
    assert_eq!((p[0]["sent"].as_str(), p[0].get("why")), (Some("waiting"), None), "a result that comes after all still settles it: {p:?}");
}

#[test]
fn a_quick_answer_in_the_same_words_as_the_message_it_sent_is_kept_and_answers_it() {
    let w = World::new();
    w.session("b").in_project("loops").running_as(4242).prompt("go");
    w.hub().rename("b", "helper").unwrap();
    let a = running(&w, "tell the helper ok");
    sends(&w, &a, "s1", "helper", "ok");
    comes_back(&w, &a, "s1", json!({ "success": true }));
    writes_to(&w, &a, 4242, "ok");
    turn_ends(&w, &a, "Done.");
    let p = peers(&w, &a);
    assert_eq!(p.len(), 2, "{p:?}");
    assert_eq!((p[0]["to"].as_str(), p[0]["sent"].as_str()), (Some("helper"), Some("answered")), "{p:?}");
    assert_eq!((p[1]["from"].as_str(), p[1]["text"].as_str()), (Some("helper"), Some("ok")), "{p:?}");
}
