//! The real `cue hook <event>` (stability plan §4, hooks CLI): what each event sends Cue, that it never gets in
//! the agent's way with no Cue running, and what it does with input it can't read.

mod common;

use common::{printed, FakeCue};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Every event an agent's settings run the hook for, with input it would get.
fn every_event() -> Vec<(&'static str, Value)> {
    let base = json!({"session_id": "s1", "cwd": "/work/app", "transcript_path": "/nowhere/s1.jsonl"});
    let with = |more: Value| {
        let mut v = base.clone();
        v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        v
    };
    vec![
        ("permission", with(json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}))),
        ("stop", with(json!({"last_assistant_message": "Done."}))),
        ("prompt", with(json!({"prompt": "fix it"}))),
        ("end", base.clone()),
        ("compact", with(json!({"trigger": "auto"}))),
        ("notice", with(json!({"notification_type": "permission_prompt", "message": "Claude needs your permission"}))),
        ("failure", with(json!({"error_type": "rate_limit", "error_details": "limit reached"}))),
        ("usage", with(json!({"cost": {"total_cost_usd": 1.5}}))),
    ]
}

/// The one message Cue got for `event` with `input`; a panic naming what came instead.
#[track_caller]
fn sent(event: &str, input: Value) -> Value {
    let cue = FakeCue::running();
    let out = cue.run(&[event], &input);
    assert!(out.status.success(), "{event}: {out:?}");
    let got = cue.received();
    assert_eq!(got.len(), 1, "{event}: {got:?}");
    got.into_iter().next().unwrap()
}

#[test]
fn with_no_cue_running_every_event_exits_at_once_saying_nothing() {
    let cue = FakeCue::absent();
    for (event, input) in every_event() {
        let start = Instant::now();
        let out = cue.run(&[event], &input);
        assert!(out.status.success(), "{event}: {out:?}");
        assert_eq!(printed(&out), Value::Null, "{event} printed something");
        assert!(out.stderr.is_empty(), "{event}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(start.elapsed() < Duration::from_secs(3), "{event} waited for a Cue that isn't there: {:?}", start.elapsed());
    }
}

#[test]
fn input_the_hook_cant_read_is_left_alone_for_every_event() {
    let cue = FakeCue::running();
    for (event, _) in every_event() {
        for bad in [&b""[..], b"not json", b"{\"session_id\": \"s1\"", b"\xff\xfe\x00binary"] {
            let out = cue.hook(&[event], bad, &[]).wait_with_output().unwrap();
            assert!(out.status.success(), "{event} on {bad:?}");
            assert_eq!(printed(&out), Value::Null, "{event} on {bad:?}");
        }
    }
    assert_eq!(cue.received(), Vec::<Value>::new(), "nothing reaches Cue from input it couldn't read");
}

#[test]
fn a_stop_tells_cue_the_turn_ended_with_its_reply_and_turn() {
    let dir = std::env::temp_dir().join(format!("cue-hook-turn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("s1.jsonl");
    let lines = [json!({"type": "summary"}), json!({"type": "user", "message": {"content": "fix it"}}), json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "Fixed."}]}})];
    std::fs::write(&log, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let m = sent("stop", json!({"session_id": "s1", "cwd": "/work/app", "transcript_path": log, "last_assistant_message": "Fixed."}));
    assert_eq!((m["type"].clone(), m["event"].clone(), m["message"].clone()), (json!("event"), json!("stopped"), json!("Fixed.")));
    assert_eq!((m["session_id"].clone(), m["cwd"].clone(), m["harness"].clone()), (json!("s1"), json!("/work/app"), json!("claude")));
    assert_eq!(m["turn"], json!([{"text": "Fixed.", "hooked": false}]));
    assert!(m.get("driven_by").is_none(), "yours, unless something drives it: {m}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_stop_in_a_session_another_agent_drives_says_who() {
    let cue = FakeCue::running();
    let out = cue.hook(&["stop"], json!({"session_id": "s1", "last_assistant_message": "ok"}).to_string().as_bytes(), &[("CUE_DRIVEN_BY", " relay ")]).wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(cue.received()[0]["driven_by"], "relay");
}

#[test]
fn a_prompt_tells_cue_youre_back_with_its_prompt_id() {
    let m = sent("prompt", json!({"session_id": "s1", "prompt": "fix it", "prompt_id": "p-9"}));
    assert_eq!((m["event"].clone(), m["message"].clone(), m["prompt_id"].clone()), (json!("active"), json!("fix it"), json!("p-9")));
}

#[test]
fn an_end_a_compaction_and_a_failure_each_say_what_happened() {
    assert_eq!(sent("end", json!({"session_id": "s1"}))["event"], "ended");
    let c = sent("compact", json!({"session_id": "s1", "trigger": "manual"}));
    assert_eq!((c["event"].clone(), c["message"].clone()), (json!("compacting"), json!("manual")));
    let f = sent("failure", json!({"session_id": "s1", "error_type": "rate_limit", "error_details": "limit reached"}));
    assert_eq!((f["event"].clone(), f["message"].clone(), f["error_type"].clone()), (json!("failed"), json!("limit reached"), json!("rate_limit")));
    let bare = sent("failure", json!({"session_id": "s1", "error_details": "  ", "last_assistant_message": "API Error: 529"}));
    assert_eq!((bare["message"].clone(), bare["error_type"].clone()), (json!("API Error: 529"), json!("error")), "no details: its last words, and a plain error");
}

#[test]
fn a_notice_reaches_cue_only_for_a_permission_prompt_showing_in_the_terminal() {
    let m = sent("notice", json!({"session_id": "s1", "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash"}));
    assert_eq!((m["event"].clone(), m["message"].clone()), (json!("terminal_ask"), json!("Claude needs your permission to use Bash")));
    let cue = FakeCue::running();
    assert!(cue.run(&["notice"], &json!({"session_id": "s1", "notification_type": "idle_prompt", "message": "waiting"})).status.success());
    assert!(cue.run(&["no-such-event"], &json!({"session_id": "s1"})).status.success());
    assert_eq!(cue.received(), Vec::<Value>::new(), "nothing for an idle notice or an event Cue doesn't know");
}

#[test]
fn usage_passes_the_plan_limits_cost_and_window_on_and_nothing_when_theres_none() {
    let m = sent("usage", json!({"session_id": "s1", "rate_limits": {"five_hour": {"used_percentage": 40}}, "cost": {"total_cost_usd": 1.5}, "context_window": {"context_window_size": 1000000}}));
    assert_eq!(m, json!({"type": "usage", "session_id": "s1", "rate_limits": {"five_hour": {"used_percentage": 40}}, "cost": 1.5, "window": 1000000}));
    let cue = FakeCue::running();
    assert!(cue.run(&["usage"], &json!({"session_id": "s1"})).status.success());
    assert!(cue.run(&["usage"], &json!({"cost": {"total_cost_usd": 1.5}})).status.success());
    assert_eq!(cue.received(), Vec::<Value>::new(), "nothing to say, or no session to say it of");
}

#[test]
fn a_codex_session_is_told_apart_from_claudes() {
    let cue = FakeCue::running();
    assert!(cue.run(&["stop", "codex"], &json!({"session_id": "c1", "last_assistant_message": "done"})).status.success());
    assert_eq!(cue.received()[0]["harness"], "codex");
}
