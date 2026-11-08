//! Pi's protocol over lines a real Pi wrote (fixtures/pi_rpc.jsonl: Pi 0.87.1 in RPC mode with DeepSeek, Cue's
//! extension loaded; the streaming updates and system prompt left out, as Cue ignores them). The run: a message
//! answered; a message whose command was gated, with a second message sent mid-turn; a command stopped by abort.

use super::pi::Pi;
use super::protocol::Protocol;
use super::{Answer, AskKind, HostedEvent, InputAction};
use crate::model::Decision;
use serde_json::{json, Value};

fn events(lines: &str) -> Vec<HostedEvent> {
    let mut p = Pi::default();
    lines.lines().flat_map(|l| p.read(l).0).collect()
}

fn decision(behavior: &str, answers: Option<Value>) -> Answer {
    Answer::Decision(Decision { behavior: behavior.into(), message: None, answers, permission: None })
}

#[test]
fn turns_start_take_your_messages_say_and_end_in_order() {
    let es = events(include_str!("fixtures/pi_rpc.jsonl"));
    let order: Vec<String> = es.iter().filter_map(|e| match e {
        HostedEvent::TurnStarted => Some("start".into()),
        HostedEvent::Taken { text } => Some(format!("taken:{}", text.split_whitespace().take(2).collect::<Vec<_>>().join(" "))),
        HostedEvent::TurnEnded { .. } => Some("end".into()),
        _ => None,
    }).collect();
    assert_eq!(order, ["start", "taken:Reply with", "end", "start", "taken:Run the", "taken:Also say", "end", "start", "taken:Run the", "end"]);
}

#[test]
fn a_turns_end_says_its_last_words_and_what_it_cost_so_far() {
    let es = events(include_str!("fixtures/pi_rpc.jsonl"));
    let ends: Vec<(String, bool, f64)> = es.iter().filter_map(|e| if let HostedEvent::TurnEnded { text, failed, cost_usd, unprompted } = e { assert!(!unprompted); Some((text.clone(), *failed, *cost_usd)) } else { None }).collect();
    assert_eq!(ends[0].0, "ok");
    assert!(ends[1].0.contains("BANANA"), "the message sent mid-turn was read in the same turn: {ends:?}");
    assert!(!ends[0].1 && !ends[1].1, "{ends:?}");
    assert!(ends[2].1, "the last turn's abort came from outside this protocol (no interrupt was written): Pi calls it an error: {ends:?}");
    assert!(ends[0].2 > 0.0 && ends[1].2 > ends[0].2 && ends[2].2 >= ends[1].2, "running total: {ends:?}");
}

#[test]
fn its_tool_calls_and_their_results_pair_up_and_an_aborted_one_failed() {
    let es = events(include_str!("fixtures/pi_rpc.jsonl"));
    let calls: Vec<(String, Value)> = es.iter().filter_map(|e| if let HostedEvent::ToolCall { id, name, input, .. } = e { assert_eq!(name, "bash"); Some((id.clone(), input.clone())) } else { None }).collect();
    let results: Vec<(String, bool)> = es.iter().filter_map(|e| if let HostedEvent::ToolResult { id, failed, .. } = e { Some((id.clone(), *failed)) } else { None }).collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1["command"], "sleep 4 && echo first-done");
    assert_eq!(results, vec![(calls[0].0.clone(), false), (calls[1].0.clone(), true)]);
}

#[test]
fn a_message_goes_as_a_prompt_that_steers_when_it_is_busy_and_stop_aborts() {
    let mut p = Pi::default();
    let m: Value = serde_json::from_str(&p.message("add a test")[0]).unwrap();
    assert_eq!((m["type"].as_str(), m["message"].as_str(), m["streamingBehavior"].as_str()), (Some("prompt"), Some("add a test"), Some("steer")));
    let a: Value = serde_json::from_str(&p.interrupt()[0]).unwrap();
    assert_eq!(a["type"], "abort");
    assert_ne!(m["id"], a["id"]);
}

#[test]
fn a_message_pi_turns_down_is_not_taken_with_why() {
    let mut p = Pi::default();
    let m: Value = serde_json::from_str(&p.message("carry on")[0]).unwrap();
    let ok = json!({ "id": m["id"], "type": "response", "command": "prompt", "success": true }).to_string();
    assert!(p.read(&ok).0.is_empty(), "accepted: its events say the rest");
    let m: Value = serde_json::from_str(&p.message("and this")[0]).unwrap();
    let no = json!({ "id": m["id"], "type": "response", "command": "prompt", "success": false, "error": "No API key found for deepseek.\n\nUse /login to log in" }).to_string();
    match p.read(&no).0.as_slice() {
        [HostedEvent::NotTaken { text, why }] => assert_eq!((text.as_str(), why.as_str()), ("and this", "No API key found for deepseek.")),
        es => panic!("{es:?}"),
    }
}

#[test]
fn a_dialog_is_a_card_answered_as_the_dialog_takes_it() {
    let mut p = Pi::default();
    let line = |v: Value| v.to_string();
    let ask = |es: Vec<HostedEvent>| match es.into_iter().next() { Some(HostedEvent::Ask(a)) => a, e => panic!("{e:?}") };
    // A choice: a question with its options; the one you pick goes back.
    let a = ask(p.read(&line(json!({ "type": "extension_ui_request", "id": "d1", "method": "select", "title": "Which branch?", "options": ["main", "dev"] }))).0);
    assert_eq!((a.kind, a.input["questions"][0]["options"][1]["label"].as_str()), (AskKind::Question, Some("dev")));
    let r: Value = serde_json::from_str(&p.answer("d1", &decision("allow", Some(json!({ "Which branch?": "dev" })))).unwrap()[0]).unwrap();
    assert_eq!((r["id"].as_str(), r["value"].as_str()), (Some("d1"), Some("dev")));
    assert!(p.answer("d1", &decision("allow", None)).is_err(), "answered once");
    // Yes or no.
    ask(p.read(&line(json!({ "type": "extension_ui_request", "id": "d2", "method": "confirm", "title": "Delete it?" }))).0);
    let r: Value = serde_json::from_str(&p.answer("d2", &decision("allow", Some(json!({ "Delete it?": "No" })))).unwrap()[0]).unwrap();
    assert_eq!(r["confirmed"], false);
    // A line to type: a form with one field; declined, it's cancelled.
    let a = ask(p.read(&line(json!({ "type": "extension_ui_request", "id": "d3", "method": "input", "title": "Ticket number?" }))).0);
    assert_eq!((a.kind, a.message.as_str()), (AskKind::Input, "Ticket number?"));
    let r: Value = serde_json::from_str(&p.answer("d3", &Answer::Input { action: InputAction::Decline, content: Value::Null }).unwrap()[0]).unwrap();
    assert_eq!(r["cancelled"], true);
    // A status line asks nothing.
    assert!(p.read(&line(json!({ "type": "extension_ui_request", "id": "n1", "method": "setStatus", "text": "busy" }))).0.is_empty());
}

#[test]
fn a_turn_you_stopped_ends_as_stopped_not_failed() {
    let mut p = Pi::default();
    p.read(r#"{"type":"agent_start"}"#);
    p.read(r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"Running it now."}]}}"#);
    p.interrupt();
    let es: Vec<HostedEvent> = [r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"This operation was aborted"}}"#, r#"{"type":"agent_settled"}"#].iter().flat_map(|l| p.read(l).0).collect();
    assert!(matches!(es.last(), Some(HostedEvent::TurnEnded { failed: false, text, .. }) if text == "Running it now."), "{es:?}");
}

#[test]
fn a_reply_that_failed_ends_the_turn_as_failed_with_why() {
    let mut p = Pi::default();
    let es: Vec<HostedEvent> = [r#"{"type":"agent_start"}"#, r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"401 invalid api key"}}"#, r#"{"type":"agent_settled"}"#].iter().flat_map(|l| p.read(l).0).collect();
    assert!(matches!(es.last(), Some(HostedEvent::TurnEnded { failed: true, text, unprompted: true, .. }) if text == "401 invalid api key"), "{es:?}");
}

#[test]
fn cues_own_gate_asks_through_a_dialog_and_gets_the_decision_back() {
    let mut p = Pi::default();
    let gate = |id: &str, ask: Value| json!({ "type": "extension_ui_request", "id": id, "method": "input", "title": "cue:ask", "placeholder": ask.to_string() }).to_string();
    let ask = |es: Vec<HostedEvent>| match es.into_iter().next() { Some(HostedEvent::Ask(a)) => a, e => panic!("{e:?}") };
    let value = |lines: Vec<String>| -> Value { let r: Value = serde_json::from_str(&lines[0]).unwrap(); serde_json::from_str(r["value"].as_str().unwrap()).unwrap() };
    // A gated command: a permission card for the tool call, not a form.
    let a = ask(p.read(&gate("g1", json!({ "kind": "permission", "tool_name": "bash", "tool_input": { "command": "rm -rf target" } }))).0);
    assert_eq!((a.kind, a.tool_name.as_str(), a.input["command"].as_str(), a.request_id.as_str()), (AskKind::Permission, "bash", Some("rm -rf target"), "g1"));
    let d = value(p.answer("g1", &Answer::Decision(Decision { behavior: "deny".into(), message: Some("use cargo clean".into()), ..Default::default() })).unwrap());
    assert_eq!((d["behavior"].as_str(), d["message"].as_str()), (Some("deny"), Some("use cargo clean")));
    // Its ask_user: a question card; the answer goes back as the decision's answers.
    let a = ask(p.read(&gate("g2", json!({ "kind": "question", "tool_name": "ask_user", "tool_input": { "questions": [{ "question": "Which?", "options": [{ "label": "A" }] }] } }))).0);
    assert_eq!((a.kind, a.tool_name.as_str()), (AskKind::Question, "ask_user"));
    let d = value(p.answer("g2", &decision("allow", Some(json!({ "Which?": "A" })))).unwrap());
    assert_eq!(d["answers"]["Which?"], "A");
    // Gone unanswered (a form's answer can't be one): a denial, so the gate never lets it through.
    ask(p.read(&gate("g3", json!({ "kind": "permission", "tool_name": "bash", "tool_input": { "command": "sudo ls" } }))).0);
    assert_eq!(value(p.answer("g3", &Answer::Input { action: InputAction::Cancel, content: Value::Null }).unwrap())["behavior"], "deny");
    // Anything that only looks like it is another extension's dialog: a form, answered as typed.
    let a = ask(p.read(&json!({ "type": "extension_ui_request", "id": "x1", "method": "input", "title": "cue:ask", "placeholder": "not json" }).to_string()).0);
    assert_eq!(a.kind, AskKind::Input);
}
