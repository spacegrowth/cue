//! The real `cue hook permission` (stability plan §4, hooks CLI): it asks Cue and prints your decision as the
//! agent reads it; answered elsewhere it lets the agent's own prompt stand; when Cue restarts mid-wait it asks
//! again once Cue is back, and gives up on a Cue gone longer than its reconnect window
//! (CUE_HOOK_RECONNECT_FOR, seconds here instead of 15 minutes).

mod common;

use common::{first_line, printed, FakeCue};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn bash_ls() -> Value {
    json!({"session_id": "s1", "cwd": "/work/app", "tool_name": "Bash", "tool_input": {"command": "ls"}, "permission_suggestions": [{"type": "addRules"}]})
}

/// Cue says `line` on the ask's connection.
fn say(s: &mut UnixStream, line: Value) {
    s.write_all(format!("{line}\n").as_bytes()).unwrap();
}

/// A permission asked of a running Cue, which answers `with`: what Cue was asked, and what the hook printed.
fn asked(input: Value, with: Value) -> (Value, Value) {
    let cue = FakeCue::running();
    let hook = cue.hook(&["permission"], input.to_string().as_bytes(), &[]);
    let mut conn = cue.accept();
    let ask = first_line(&conn);
    say(&mut conn, json!({"type": "hello"}));
    say(&mut conn, with);
    let out = hook.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    (ask, printed(&out))
}

#[test]
fn a_permission_asks_cue_with_the_tool_and_prints_an_allow() {
    let (ask, out) = asked(bash_ls(), json!({"type": "decision", "behavior": "allow"}));
    assert_eq!((ask["type"].clone(), ask["kind"].clone(), ask["tool_name"].clone()), (json!("ask"), json!("permission"), json!("Bash")));
    assert_eq!(ask["tool_input"], json!({"command": "ls"}));
    assert_eq!(ask["suggestions"], json!([{"type": "addRules"}]));
    assert_eq!(out, json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}), "a line Cue said before the decision is passed over");
}

#[test]
fn a_denial_carries_its_reason_and_an_always_carries_its_rule() {
    let (_, deny) = asked(bash_ls(), json!({"type": "decision", "behavior": "deny", "message": "not on main"}));
    assert_eq!(deny["hookSpecificOutput"]["decision"], json!({"behavior": "deny", "message": "not on main"}));
    let (_, always) = asked(bash_ls(), json!({"type": "decision", "behavior": "allow_always", "permission": {"type": "addRules", "rules": ["Bash(ls)"]}}));
    assert_eq!(always["hookSpecificOutput"]["decision"]["updatedPermissions"], json!([{"type": "addRules", "rules": ["Bash(ls)"]}]));
}

#[test]
fn a_question_is_asked_as_one_and_its_answers_go_back_in_the_tools_input() {
    let input = json!({"session_id": "s1", "tool_name": "AskUserQuestion", "tool_input": {"questions": [{"question": "Which?"}]}});
    let (ask, out) = asked(input, json!({"type": "decision", "behavior": "allow", "answers": {"Which?": "B"}}));
    assert_eq!(ask["kind"], "question");
    assert_eq!(out["hookSpecificOutput"]["decision"]["updatedInput"], json!({"questions": [{"question": "Which?"}], "answers": {"Which?": "B"}}));
}

#[test]
fn an_ask_answered_elsewhere_lets_the_agents_own_prompt_stand() {
    for told in [json!({"type": "cancel"}), json!({"type": "error", "message": "no"})] {
        let (_, out) = asked(bash_ls(), told.clone());
        assert_eq!(out, Value::Null, "{told}: nothing printed");
    }
}

#[test]
fn when_cue_restarts_mid_wait_the_question_is_asked_again_and_answered() {
    let mut cue = FakeCue::running();
    let hook = cue.hook(&["permission"], bash_ls().to_string().as_bytes(), &[]);
    let conn = cue.accept();
    let first = first_line(&conn);
    // Cue quits with the question open, and comes back.
    drop(conn);
    cue.stop();
    cue.start();
    let mut again = cue.accept();
    assert_eq!(first_line(&again), first, "the same question, whole");
    say(&mut again, json!({"type": "decision", "behavior": "allow"}));
    let out = hook.wait_with_output().unwrap();
    assert_eq!(printed(&out)["hookSpecificOutput"]["decision"]["behavior"], "allow");
}

#[test]
fn a_cue_gone_longer_than_the_reconnect_window_is_given_up_on_quietly() {
    let mut cue = FakeCue::running();
    let hook = cue.hook(&["permission"], bash_ls().to_string().as_bytes(), &[("CUE_HOOK_RECONNECT_FOR", "3")]);
    let conn = cue.accept();
    first_line(&conn);
    let gone = Instant::now();
    drop(conn);
    cue.stop();
    let out = hook.wait_with_output().unwrap();
    let waited = gone.elapsed();
    assert!(out.status.success());
    assert_eq!(printed(&out), Value::Null, "the agent asks in its terminal");
    assert!(waited >= Duration::from_secs(3), "it kept the question for its window: {waited:?}");
    assert!(waited < Duration::from_secs(9), "and not much past it: {waited:?}");
}
