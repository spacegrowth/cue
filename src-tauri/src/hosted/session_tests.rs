//! A session against the stand-in Claude (testing::FakeClaude): what Cue writes to it, what it reports, and how
//! it ends, including the ways a real one goes wrong (dies mid-turn, says something that isn't JSON, asks
//! something Cue doesn't know, won't end when asked, leaves a child behind).

use super::{Answer, AskKind, HostedEvent, InputAction, Program, Session, Spec, Start};
use crate::model::Decision;
use crate::testing::{until, FakeClaude};
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

fn start(fake: &FakeClaude) -> (Session, Receiver<HostedEvent>) {
    start_as(fake, Start::fresh())
}

fn start_as(fake: &FakeClaude, start: Start) -> (Session, Receiver<HostedEvent>) {
    let spec = Spec { cwd: std::env::temp_dir(), start, permissions: String::new(), model: None, args: vec![], program: Program::At(fake.program()) };
    let (tx, rx) = channel();
    let s = Session::start(&spec, move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    (s, rx)
}

/// The events up to and including the first `stop` accepts (failing after 10 s).
fn until_event(rx: &Receiver<HostedEvent>, stop: impl Fn(&HostedEvent) -> bool) -> Vec<HostedEvent> {
    let end = Instant::now() + Duration::from_secs(10);
    let mut seen = vec![];
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let e = rx.recv_timeout(left).unwrap_or_else(|_| panic!("no such event in 10 s; saw {seen:?}"));
        let done = stop(&e);
        seen.push(e);
        if done {
            return seen;
        }
    }
}

fn exited(rx: &Receiver<HostedEvent>) -> HostedEvent {
    until_event(rx, |e| matches!(e, HostedEvent::Exited { .. })).pop().unwrap()
}

/// A line Claude writes asking permission for Bash.
fn bash_ask(id: &str) -> Value {
    json!({ "type": "control_request", "request_id": id, "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "t1" } })
}

fn heard_with(fake: &FakeClaude, what: &str, pick: impl Fn(&Value) -> bool) -> Value {
    until(what, || fake.heard().iter().any(&pick));
    fake.heard().into_iter().find(|v| pick(v)).unwrap()
}

#[test]
fn it_starts_headless_marked_hosted_outside_any_tmux_and_sets_up_first() {
    let fake = FakeClaude::new();
    let start = Start::fresh();
    let id = start.session_id().to_string();
    let (_s, _rx) = start_as(&fake, start);
    heard_with(&fake, "the setup request", |v| v["request"]["subtype"] == "initialize");
    assert_eq!(fake.heard()[0]["request"]["subtype"], "initialize", "set up before anything else");
    let args = fake.args();
    assert!(args.windows(2).any(|w| w[0] == "--session-id" && w[1] == id), "{args:?}");
    assert_eq!(fake.env(), ("1".to_string(), String::new()), "CUE_HOSTED set, TMUX not");
}

#[test]
fn an_ask_is_answered_once_with_the_input_it_runs_with() {
    let fake = FakeClaude::new().wait_for("initialize").say(bash_ask("r1"));
    let (s, rx) = start(&fake);
    let ask = until_event(&rx, |e| matches!(e, HostedEvent::Ask(_))).pop().unwrap();
    let HostedEvent::Ask(a) = ask else { unreachable!() };
    assert_eq!((a.kind, a.request_id.as_str()), (AskKind::Permission, "r1"));
    s.answer("r1", &Answer::Decision(Decision { behavior: "allow".into(), ..Default::default() })).unwrap();
    let r = heard_with(&fake, "the answer", |v| v["type"] == "control_response");
    assert_eq!(r["response"]["response"], json!({ "behavior": "allow", "updatedInput": { "command": "ls" } }));
    assert!(s.answer("r1", &Answer::Decision(Decision::default())).is_err(), "a second answer has nothing to go to");
}

#[test]
fn messages_forms_and_interrupts_reach_it_as_written() {
    let fake = FakeClaude::new();
    let (s, _rx) = start(&fake);
    s.send("hello there").unwrap();
    heard_with(&fake, "the message", |v| v["type"] == "user" && v["message"]["content"] == "hello there");
    s.interrupt().unwrap();
    heard_with(&fake, "the interrupt", |v| v["request"]["subtype"] == "interrupt");
    assert!(s.answer("nobody", &Answer::Input { action: InputAction::Cancel, content: Value::Null }).is_err());
}

#[test]
fn a_request_cue_cant_answer_is_refused_and_lines_that_arent_json_are_skipped() {
    let fake = FakeClaude::new()
        .say_text("Last login: a shell's greeting")
        .say(json!({ "type": "control_request", "request_id": "r7", "request": { "subtype": "brand_new" } }))
        .say(json!({ "type": "result", "subtype": "success", "result": "done", "total_cost_usd": 0.0 }));
    let (_s, rx) = start(&fake);
    let seen = until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { .. }));
    assert!(seen.contains(&HostedEvent::Refused { request_id: "r7".into(), subtype: "brand_new".into() }), "{seen:?}");
    let r = heard_with(&fake, "the refusal", |v| v["response"]["request_id"] == "r7");
    assert_eq!(r["response"]["subtype"], "error");
}

#[test]
fn dying_mid_turn_says_so_with_its_last_words() {
    let fake = FakeClaude::new()
        .say(json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "working on it" }] } }))
        .complain("fatal: out of memory")
        .exit(3);
    let (s, rx) = start(&fake);
    let HostedEvent::Exited { code, during_turn, last_words } = exited(&rx) else { unreachable!() };
    assert_eq!((code, during_turn), (Some(3), true));
    assert!(last_words.contains("out of memory"), "{last_words}");
    assert!(s.gone());
    assert!(s.send("anyone?").is_err(), "nothing is left to read it");
}

#[test]
fn an_ask_left_unanswered_when_it_exits_cant_be_answered_after() {
    let fake = FakeClaude::new().wait_for("initialize").say(bash_ask("r2")).exit(0);
    let (s, rx) = start(&fake);
    let _ = exited(&rx);
    assert!(s.answer("r2", &Answer::Decision(Decision::default())).is_err());
}

#[test]
fn closing_ends_it_when_its_input_closes() {
    let fake = FakeClaude::new();
    let (s, rx) = start(&fake);
    let t = Instant::now();
    s.close();
    assert!(s.gone() && t.elapsed() < Duration::from_secs(2), "it ended on its own as its input closed");
    assert!(matches!(exited(&rx), HostedEvent::Exited { code: Some(0), during_turn: false, .. }));
    s.close();
}

#[test]
fn closing_one_that_wont_end_ends_its_whole_group() {
    let fake = FakeClaude::new().ignore_term().start_child().pause_ms(60_000);
    let (s, rx) = start(&fake);
    until("its child started", || fake.child_pid().is_some());
    let child = fake.child_pid().unwrap();
    s.close();
    assert!(s.gone());
    assert!(matches!(exited(&rx), HostedEvent::Exited { .. }));
    until("its child ended with it", || !alive(child));
}

#[test]
fn a_child_holding_its_output_open_doesnt_keep_the_session_going_after_it_exits() {
    let fake = FakeClaude::new().start_child().exit(0);
    let (s, rx) = start(&fake);
    assert!(matches!(exited(&rx), HostedEvent::Exited { code: Some(0), .. }));
    assert!(s.gone());
    until("the leftover child ended", || !fake.child_pid().is_some_and(alive));
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn two_asks_waiting_at_once_are_each_answered_in_any_order() {
    let fake = FakeClaude::new().wait_for("initialize").say(bash_ask("r1")).say(bash_ask("r2"));
    let (s, rx) = start(&fake);
    let seen = until_event(&rx, |e| matches!(e, HostedEvent::Ask(a) if a.request_id == "r2"));
    assert_eq!(seen.iter().filter(|e| matches!(e, HostedEvent::Ask(_))).count(), 2);
    let deny = Decision { behavior: "deny".into(), message: Some("no".into()), ..Default::default() };
    s.answer("r2", &Answer::Decision(deny)).unwrap();
    s.answer("r1", &Answer::Decision(Decision { behavior: "allow".into(), ..Default::default() })).unwrap();
    let r2 = heard_with(&fake, "r2's answer", |v| v["response"]["request_id"] == "r2");
    let r1 = heard_with(&fake, "r1's answer", |v| v["response"]["request_id"] == "r1");
    assert_eq!((r2["response"]["response"]["behavior"].as_str(), r1["response"]["response"]["behavior"].as_str()), (Some("deny"), Some("allow")));
}
