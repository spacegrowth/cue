//! The translator over lines real Claude Code wrote (fixtures/, recorded from Claude Code 2.1.319 in its
//! headless mode, one file per case of docs/dev/cue-hosted-sessions.md §2; paths and lists of the recording
//! machine's own plugins and commands taken out). Each case checks what Cue would show: who asks, for what,
//! and when a turn ends and whether you started it.

use super::translate::Translator;
use super::wire;
use super::{Ask, AskKind, HostedEvent};
use serde_json::json;

fn events(fixture: &str) -> Vec<HostedEvent> {
    let mut t = Translator::default();
    fixture.lines().flat_map(|l| t.step(&wire::parse(l))).collect()
}

fn asks(es: &[HostedEvent]) -> Vec<&Ask> {
    es.iter().filter_map(|e| if let HostedEvent::Ask(a) = e { Some(a) } else { None }).collect()
}

/// Each turn's end: (its text, whether nothing of yours started it).
fn turns(es: &[HostedEvent]) -> Vec<(String, bool)> {
    es.iter().filter_map(|e| if let HostedEvent::TurnEnded { text, unprompted, .. } = e { Some((text.clone(), *unprompted)) } else { None }).collect()
}

#[test]
fn a_bash_permission_is_asked_of_cue_by_the_session_itself() {
    let es = events(include_str!("fixtures/bash_permission.jsonl"));
    let a = asks(&es);
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].kind, a[0].tool_name.as_str(), a[0].agent_id.as_str()), (AskKind::Permission, "Bash", ""));
    assert_eq!(a[0].input["command"], "touch made_by_s1.txt");
    assert!(!a[0].request_id.is_empty() && !a[0].tool_use_id.is_empty());
    let started = es.iter().filter(|e| matches!(e, HostedEvent::Started { .. })).count();
    assert_eq!(started, 1);
    assert!(matches!(es.last(), Some(HostedEvent::TurnEnded { failed: false, cost_usd, .. }) if *cost_usd > 0.0));
}

#[test]
fn a_background_agents_permission_names_the_agent() {
    let es = events(include_str!("fixtures/agent_permission.jsonl"));
    let a = asks(&es);
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].kind, a[0].tool_name.as_str()), (AskKind::Permission, "Bash"));
    assert!(!a[0].agent_id.is_empty(), "asked from inside the agent: {a:?}");
    let by_agent = es.iter().any(|e| matches!(e, HostedEvent::ToolCall { name, agent: Some(_), .. } if name == "Bash"));
    assert!(by_agent, "the agent's own tool call is marked as the agent's");
    assert!(es.iter().any(|e| matches!(e, HostedEvent::Task { what, .. } if what == "task_started")));
}

#[test]
fn a_question_arrives_as_a_question_with_its_choices() {
    let es = events(include_str!("fixtures/question.jsonl"));
    let a = asks(&es);
    assert_eq!((a.len(), a[0].kind), (1, AskKind::Question));
    assert_eq!(a[0].input["questions"][0]["options"].as_array().map(Vec::len), Some(2));
}

#[test]
fn a_sandboxed_commands_network_access_is_asked_of_cue_not_its_terminal() {
    let es = events(include_str!("fixtures/network.jsonl"));
    let a = asks(&es);
    assert_eq!((a.len(), a[0].kind, a[0].tool_name.as_str()), (1, AskKind::Network, "SandboxNetworkAccess"));
    assert_eq!(a[0].input, json!({ "host": "example.com" }));
}

#[test]
fn an_mcp_servers_request_for_input_arrives_with_its_form() {
    let es = events(include_str!("fixtures/mcp_input.jsonl"));
    let a = asks(&es);
    assert_eq!(a.len(), 2, "the tool's own permission, then the server's form: {a:?}");
    assert_eq!((a[0].kind, a[0].tool_name.as_str()), (AskKind::Permission, "mcp__spike__need_input"));
    assert_eq!((a[1].kind, a[1].server.as_str(), a[1].message.as_str()), (AskKind::Input, "spike", "Pick a color"));
    assert_eq!(a[1].form["properties"]["color"]["enum"], json!(["red", "blue"]));
}

#[test]
fn a_named_agents_permission_comes_like_a_background_agents() {
    let es = events(include_str!("fixtures/named_agent.jsonl"));
    let a = asks(&es);
    assert_eq!(a.len(), 1);
    assert!(!a[0].agent_id.is_empty());
    assert!(!es.iter().any(|e| matches!(e, HostedEvent::Refused { .. })));
}

#[test]
fn messages_are_taken_in_order_and_a_turn_no_message_started_is_unprompted() {
    let es = events(include_str!("fixtures/queue_interrupt_unprompted.jsonl"));
    let taken: Vec<&str> = es.iter().filter_map(|e| if let HostedEvent::Taken { text } = e { Some(text.as_str()) } else { None }).collect();
    assert_eq!(taken.len(), 3);
    assert!(taken[1].starts_with("Second message, sent while you were busy"), "{taken:?}");
    let t = turns(&es);
    assert_eq!(t.len(), 3, "{t:?}");
    assert!(t[0].0.contains("BANANA") && !t[0].1, "the message sent mid-turn is answered in the same turn: {t:?}");
    assert!(!t[1].1, "{t:?}");
    assert!(t[2].1, "a background task finishing started the last turn by itself: {t:?}");
    assert_eq!(es.iter().filter(|e| matches!(e, HostedEvent::Started { .. })).count(), 1, "one Started however many turns");
}

#[test]
fn a_resumed_session_keeps_its_id_and_its_conversation() {
    let first = events(include_str!("fixtures/queue_interrupt_unprompted.jsonl"));
    let resumed = events(include_str!("fixtures/resume.jsonl"));
    let id = |es: &[HostedEvent]| es.iter().find_map(|e| if let HostedEvent::Started { session_id, .. } = e { Some(session_id.clone()) } else { None });
    assert_eq!(id(&resumed), id(&first));
    assert_eq!(turns(&resumed), vec![("BANANA".to_string(), false)]);
}

#[test]
fn started_says_which_commands_only_work_in_a_terminal() {
    let es = events(include_str!("fixtures/bash_permission.jsonl"));
    let Some(HostedEvent::Started { terminal_only, slash_commands, .. }) = es.first() else { panic!("{es:?}") };
    assert_eq!(terminal_only, &["config", "theme"]);
    assert!(slash_commands.contains(&"compact".to_string()));
}

#[test]
fn a_request_of_an_unknown_kind_is_refused_and_lines_it_doesnt_know_are_skipped() {
    let mut t = Translator::default();
    let refused = t.step(&wire::parse(r#"{"type":"control_request","request_id":"r9","request":{"subtype":"something_new"}}"#));
    assert_eq!(refused, vec![HostedEvent::Refused { request_id: "r9".into(), subtype: "something_new".into() }]);
    for line in ["zsh: a greeting", r#"{"type":"rate_limit_event"}"#, r#"{"type":"system","subtype":"thinking_tokens"}"#, "{}"] {
        assert_eq!(t.step(&wire::parse(line)), vec![], "{line}");
    }
}

#[test]
fn a_turn_is_under_way_from_your_message_to_its_end() {
    let mut t = Translator::default();
    assert!(!t.in_turn());
    t.step(&wire::parse(r#"{"type":"user","isReplay":true,"message":{"role":"user","content":"hi"}}"#));
    assert!(t.in_turn());
    t.step(&wire::parse(r#"{"type":"result","subtype":"success","result":"hello","total_cost_usd":0.01}"#));
    assert!(!t.in_turn());
}

#[test]
fn a_turn_that_fails_before_saying_anything_says_why() {
    let mut t = Translator::default();
    let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["No conversation found with session ID: x"],"total_cost_usd":0}"#;
    let e = t.step(&crate::hosted::wire::parse(line));
    assert!(matches!(&e[..], [HostedEvent::TurnEnded { text, failed: true, .. }] if text == "No conversation found with session ID: x"), "{e:?}");
}
