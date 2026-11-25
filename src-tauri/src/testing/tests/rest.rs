//! Idle sessions at rest (hub/rest.rs, #773): a session Cue runs that has sat idle for Settings' time has its agent
//! ended (FakeHosted records the close) and stays, paused, with its chat; the next message starts it again on its
//! conversation. Never one at work, asking, with something still running, in a terminal, or with the setting at Never.

use crate::hosted::{Ask, AskKind, HostedEvent, Start};
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::{Transcript, World};
use serde_json::json;

const MIN: u64 = 60_000;

/// A session Cue runs, at the end of a turn that said `reply`: its id.
fn idle(w: &World, reply: &str) -> String {
    let sid = at_work(w);
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: reply.into(), failed: false, cost_usd: 0.1, unprompted: false });
    w.settle();
    sid
}

/// A session Cue runs, mid-turn on its first message (its conversation written, as Claude does on taking it): its id.
fn at_work(w: &World) -> String {
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "go", "trial", &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: "go".into() });
    Transcript::at(&w.hub().session_origin(&sid).unwrap().transcript_path).prompt("go");
    sid
}

#[test]
fn an_idle_session_has_its_agent_ended_and_the_next_message_resumes_its_conversation() {
    let w = World::new();
    let sid = idle(&w, "Done.");
    // Not yet: it has only just stopped.
    w.advance(29 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "idle 29 minutes: left running");
    assert!(w.hub().session_origin(&sid).unwrap().agent_pid.is_some());
    // Half an hour idle: its agent is ended, by the port that started it, and only that.
    w.advance(2 * MIN);
    w.tick(true);
    assert_eq!(w.hosted.closed(), vec![sid.clone()]);
    let o = w.hub().session_origin(&sid).expect("the session stays");
    assert!(o.hosted && o.agent_pid.is_none(), "paused, no process");
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "waiting", "still waiting on you, as before");
    assert_eq!(s.card(&sid)["message"], "Done.", "its reply is still there");
    // Looked at again, a paused one is left alone.
    w.tick(true);
    assert_eq!(w.hosted.closed().len(), 1);
    // A message starts it again on its conversation, and reaches it.
    let starts = w.hosted.launched().len();
    w.send_to(&sid, "one more thing");
    let l = w.hosted.launched();
    assert_eq!(l.len(), starts + 1);
    assert_eq!(l.last().map(|l| l.start.clone()), Some(Start::Resume(sid.clone())), "resumed with its history, not started new");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "one more thing".to_string())));
    assert!(w.hub().session_origin(&sid).unwrap().agent_pid.is_some(), "running again");
    assert_eq!(w.snapshot().session(&sid)["thread"].as_array().map(|t| t.iter().any(|x| x["text"] == "go")), Some(true), "its chat is whole");
}

#[test]
fn the_idle_time_is_a_setting_and_never_means_never() {
    let w = World::new();
    let sid = idle(&w, "Done.");
    assert_eq!(crate::config::rest_minutes(), 30, "half an hour unless you say");
    crate::config::set("hosted.rest_min", json!(0)).unwrap();
    w.advance(5 * 60 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "Never: left running however long");
    crate::config::set("hosted.rest_min", json!(15)).unwrap();
    assert_eq!(crate::config::effective()["hosted"]["rest_min"], 15);
    w.tick(true);
    assert_eq!(w.hosted.closed(), vec![sid]);
}

#[test]
fn one_at_work_or_asking_you_something_is_never_ended() {
    let w = World::new();
    let working = at_work(&w);
    w.advance(3 * 60 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "three hours into a turn: at work");
    // It asks a permission and waits on you for hours: its agent holds the question.
    w.hosted_says(&working, HostedEvent::Ask(Ask { request_id: "r1".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "rm -rf build" }), suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: String::new(), message: String::new(), form: json!(null) }));
    w.settle();
    w.advance(3 * 60 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "waiting on your answer");
    assert!(w.hub().session_origin(&working).unwrap().agent_pid.is_some());
}

#[test]
fn one_with_a_teammate_still_at_work_is_never_ended() {
    let w = World::new();
    let sid = at_work(&w);
    // The lead's own Agent call names its teammate, then Claude says it started (as its stream has them).
    w.hosted_says(&sid, HostedEvent::ToolCall { id: "call-1".into(), name: "Agent".into(), input: json!({ "name": "parser-tests", "prompt": "Write the parser tests", "description": "Parser tests" }), agent: None });
    w.hosted_says(&sid, HostedEvent::Task { what: "task_started".into(), detail: json!({ "task_type": "local_agent", "tool_use_id": "call-1", "task_id": "a1" }) });
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "A teammate is on it.".into(), failed: false, cost_usd: 0.1, unprompted: false });
    w.settle();
    w.advance(2 * 60 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "its teammate would end with it");
    assert_eq!(w.snapshot().session(&sid)["team"][0]["state"], "working", "and is still at work");
}

#[test]
fn one_whose_cache_is_still_warm_waits_until_it_runs_out() {
    let w = World::new();
    let sid = idle(&w, "Done.");
    w.advance(31 * MIN);
    let warm_until = crate::clock::now_ms() + 20 * MIN;
    w.hub().write(|st| st.insight.insert(sid.clone(), json!({ "until_ms": warm_until })));
    w.hub().rest_idle();
    assert!(w.hosted.closed().is_empty(), "coming back within its cache costs what it would have");
    w.hub().write(|st| st.insight.insert(sid.clone(), json!({ "until_ms": crate::clock::now_ms() - 1 })));
    w.hub().rest_idle();
    assert_eq!(w.hosted.closed(), vec![sid]);
}

#[test]
fn a_session_in_a_terminal_is_never_ended() {
    let w = World::new();
    w.session("a").running_as(4242).prompt("one").stop("Done.");
    w.advance(5 * 60 * MIN);
    w.tick(true);
    assert!(w.hosted.closed().is_empty(), "its process is its terminal's: nothing of Cue's to end");
}
