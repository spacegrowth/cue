//! Codex's app server over lines a real Codex wrote (fixtures/codex_*.jsonl: Codex 0.166 through `codex app-server`,
//! signed in with ChatGPT; streamed deltas, usage and account notices left out, paths and machine names made
//! neutral). The run: a command approved, a file change approved, a message sent mid-turn, a turn stopped; then
//! the conversation resumed in a new process.

use super::codex::Codex;
use super::protocol::Protocol;
use super::{Agent, Answer, AskKind, HostedEvent, Program, Spec, Start};
use crate::model::Decision;
use serde_json::{json, Value};

fn spec(start: Start, permissions: &str) -> Spec {
    Spec { agent: Agent::Codex, cwd: "/work/demo".into(), start, permissions: permissions.into(), model: None, args: vec![], program: Program::Agent, machine: String::new() }
}

/// The protocol after the recorded run, and every event it made.
fn run(fixture: &str, start: Start) -> (Codex, Vec<HostedEvent>) {
    let mut c = Codex::new(&spec(start, ""));
    c.opening();
    let mut es = vec![];
    for l in fixture.lines() {
        es.extend(c.read(l).0);
    }
    (c, es)
}

fn decision(behavior: &str) -> Answer {
    Answer::Decision(Decision { behavior: behavior.into(), message: None, answers: None, permission: None })
}

const NEW: &str = include_str!("fixtures/codex_new.jsonl");

#[test]
fn it_sets_up_then_opens_the_conversation_with_the_permissions_chosen() {
    let mut c = Codex::new(&spec(Start::New("ignored".into()), "skip"));
    let first: Value = serde_json::from_str(&c.opening()[0]).unwrap();
    assert_eq!(first["method"], "initialize");
    assert!(!c.ready());
    let (_, lines) = c.read(&json!({ "id": first["id"], "result": {} }).to_string());
    let open: Vec<Value> = lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(open[0]["method"], "initialized");
    assert_eq!((open[1]["method"].as_str(), open[1]["params"]["approvalPolicy"].as_str(), open[1]["params"]["sandbox"].as_str()), (Some("thread/start"), Some("never"), Some("danger-full-access")));
    let ask = Codex::new(&spec(Start::New("x".into()), "ask")).open;
    assert_eq!(ask["params"]["approvalPolicy"], "untrusted", "Ask asks, whatever Codex's config says");
    let resume = Codex::new(&spec(Start::Resume("T1".into()), "")).open;
    assert_eq!((resume["method"].as_str(), resume["params"]["threadId"].as_str()), (Some("thread/resume"), Some("T1")));
    assert!(resume["params"].get("approvalPolicy").is_none(), "Codex's own default");
}

#[test]
fn the_conversation_is_known_by_codexs_id_once_it_is_open_with_its_file() {
    let (c, _) = run(NEW, Start::New("ignored".into()));
    assert!(c.ready());
    assert_eq!(c.session_id().as_deref(), Some("01a1b9b3-f822-7743-9e01-55ded38cf8a5"));
    assert!(c.transcript().unwrap().ends_with("01a1b9b3-f822-7743-9e01-55ded38cf8a5.jsonl"));
}

#[test]
fn turns_start_take_your_messages_say_and_end_in_order() {
    let (_, es) = run(NEW, Start::New("x".into()));
    let order: Vec<String> = es.iter().filter_map(|e| match e {
        HostedEvent::TurnStarted => Some("start".into()),
        HostedEvent::Taken { text } => Some(format!("taken:{}", text.split_whitespace().take(2).collect::<Vec<_>>().join(" "))),
        HostedEvent::TurnEnded { text, failed, .. } => Some(format!("end:{text}:{failed}")),
        _ => None,
    }).collect();
    assert_eq!(order, ["start", "taken:Run the", "end:hi-from-codex:false", "start", "taken:Create a", "taken:Also say", "end:done BANANA:false", "start", "taken:Run the", "end:Running that command now.:false"]);
}

#[test]
fn a_command_and_a_file_change_are_permission_cards_answered_as_codex_takes_them() {
    let (mut c, es) = run(NEW, Start::New("x".into()));
    let asks: Vec<_> = es.iter().filter_map(|e| if let HostedEvent::Ask(a) = e { Some(a.clone()) } else { None }).collect();
    assert_eq!(asks[0].kind, AskKind::Permission);
    assert_eq!((asks[0].tool_name.as_str(), asks[0].input["command"].as_str()), ("Bash", Some("echo hi-from-codex")));
    let patch = asks.iter().find(|a| a.tool_name == "apply_patch").unwrap();
    assert_eq!(patch.input["file_path"], "/work/demo/note.txt");
    assert!(patch.input["patch"].as_str().unwrap().contains("hello"));
    // Answered in the run already, so nothing waits: a fresh request shows how each answer is spelled.
    let ask = |c: &mut Codex, id: u64, method: &str, params: Value| match c.read(&json!({ "id": id, "method": method, "params": params }).to_string()).0.pop() { Some(HostedEvent::Ask(a)) => a, e => panic!("{e:?}") };
    let a = ask(&mut c, 90, "item/commandExecution/requestApproval", json!({ "itemId": "i", "command": "/bin/zsh -lc 'rm -rf build'", "proposedExecpolicyAmendment": ["rm", "-rf", "build"], "availableDecisions": ["accept", "cancel"] }));
    let said: Value = serde_json::from_str(&c.answer(&a.request_id, &decision("allow_always")).unwrap()[0]).unwrap();
    assert_eq!(said, json!({ "jsonrpc": "2.0", "id": 90, "result": { "decision": { "acceptWithExecpolicyAmendment": { "execpolicy_amendment": ["rm", "-rf", "build"] } } } }));
    let a = ask(&mut c, 91, "item/commandExecution/requestApproval", json!({ "itemId": "i", "command": "ls", "availableDecisions": ["accept", "cancel"] }));
    let said: Value = serde_json::from_str(&c.answer(&a.request_id, &decision("deny")).unwrap()[0]).unwrap();
    assert_eq!(said["result"]["decision"], "cancel", "no decline offered");
    assert!(c.answer(&a.request_id, &decision("allow")).is_err(), "answered once");
}

#[test]
fn deny_with_what_to_do_instead_declines_then_says_it() {
    let (mut c, _) = run(NEW, Start::New("x".into()));
    c.read(&json!({ "method": "turn/started", "params": { "turn": { "id": "T9" } } }).to_string());
    let a = match c.read(&json!({ "id": 7, "method": "item/fileChange/requestApproval", "params": { "itemId": "f" } }).to_string()).0.pop() { Some(HostedEvent::Ask(a)) => a, e => panic!("{e:?}") };
    let lines = c.answer(&a.request_id, &Answer::Decision(Decision { behavior: "deny".into(), message: Some("write it under docs/".into()), answers: None, permission: None })).unwrap();
    let (first, then): (Value, Value) = (serde_json::from_str(&lines[0]).unwrap(), serde_json::from_str(&lines[1]).unwrap());
    assert_eq!(first["result"]["decision"], "decline");
    assert_eq!((then["method"].as_str(), then["params"]["expectedTurnId"].as_str()), (Some("turn/steer"), Some("T9")));
}

#[test]
fn a_message_starts_a_turn_steers_one_running_and_waits_while_the_conversation_opens() {
    let mut c = Codex::new(&spec(Start::New("x".into()), ""));
    c.opening();
    assert!(c.message("early").is_empty(), "held until the conversation is open");
    c.read(&json!({ "id": 1, "result": {} }).to_string());
    let (_, sent) = c.read(&json!({ "id": 2, "result": { "thread": { "id": "T1" } } }).to_string());
    let held: Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!((held["method"].as_str(), held["params"]["input"][0]["text"].as_str()), (Some("turn/start"), Some("early")));
    c.read(&json!({ "method": "turn/started", "params": { "turn": { "id": "U1" } } }).to_string());
    let steer: Value = serde_json::from_str(&c.message("more")[0]).unwrap();
    assert_eq!((steer["method"].as_str(), steer["params"]["expectedTurnId"].as_str()), (Some("turn/steer"), Some("U1")));
    let stop: Value = serde_json::from_str(&c.interrupt()[0]).unwrap();
    assert_eq!((stop["method"].as_str(), stop["params"]["turnId"].as_str()), (Some("turn/interrupt"), Some("U1")));
    let refused = json!({ "id": steer["id"], "error": { "code": -32600, "message": "expected active turn id" } }).to_string();
    assert!(matches!(c.read(&refused).0.as_slice(), [HostedEvent::NotTaken { text, .. }] if text == "more"));
}

#[test]
fn a_request_cue_doesnt_know_is_refused_at_once() {
    let mut c = Codex::new(&spec(Start::New("x".into()), ""));
    let (es, lines) = c.read(&json!({ "id": 5, "method": "account/chatgptAuthTokens/refresh", "params": {} }).to_string());
    assert!(es.is_empty());
    let r: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!((r["id"].as_u64(), r["error"]["code"].as_i64()), (Some(5), Some(-32601)));
}

#[test]
fn a_resumed_conversation_answers_from_what_came_before() {
    let (c, es) = run(include_str!("fixtures/codex_resume.jsonl"), Start::Resume("01a1b9b2-d1a0-71e2-88a9-df79897d8d8d".into()));
    assert_eq!(c.session_id().as_deref(), Some("01a1b9b2-d1a0-71e2-88a9-df79897d8d8d"));
    assert!(es.iter().any(|e| matches!(e, HostedEvent::TurnEnded { text, failed: false, .. } if text == "late")), "{es:?}");
}
