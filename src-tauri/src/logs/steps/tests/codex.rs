//! Tests: Codex's logs and its app's scripts.

use super::*;

#[test]
fn a_codex_log_reads_as_steps_too() {
    let log = [
        r#"{"timestamp":"2026-10-05T06:36:45.800Z","type":"session_meta","payload":{"session_id":"X1","cwd":"/a"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:45.816Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"AGENTS.md instructions"}]}}"#,
        r#"{"timestamp":"2026-10-05T06:36:45.831Z","type":"event_msg","payload":{"type":"user_message","message":"fix the menu"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:46.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking at the menu code."}]}}"#,
        r#"{"timestamp":"2026-10-05T06:36:47.000Z","type":"event_msg","payload":{"type":"agent_message","message":"Looking at the menu code."}}"#,
        r#"{"timestamp":"2026-10-05T06:36:48.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"cd /a && rg --files\"}","call_id":"k1"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:48.500Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k1","output":"Chunk ID: 1\nWall time: 0.03 seconds\nProcess exited with code 0\nOutput:\napp.py\nREADME.md\n"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:49.000Z","type":"response_item","payload":{"type":"function_call","name":"write_stdin","arguments":"{\"session_id\":1,\"chars\":\"\"}","call_id":"k2"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:50.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /a/app.py\n@@\n-    old()\n+    new()\n+    more()\n*** End Patch","call_id":"k3"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:50.500Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"k3","output":"Exit code: 0\nWall time: 0.2 seconds\nOutput:\nSuccess. Updated the following files:\nM /a/app.py\n"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:51.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pytest -q\"}","call_id":"k4"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:55.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k4","output":"Chunk ID: 2\nProcess exited with code 1\nOutput:\n1 failed\n"}}"#,
    ]
    .join("\n");
    let dir = std::env::temp_dir().join(format!("cue-steps-codex-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rollout.jsonl");
    std::fs::write(&path, log + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    assert_eq!(f.kind, Kind::Codex);
    f.catch_up();
    assert_eq!(f.turns.len(), 1, "Codex's instructions aren't a turn of yours");
    assert_eq!(f.turns[0].prompt, "fix the menu");
    let items = &f.turns[0].items;
    let line = |i: usize| match &items[i] {
        Item::Step { verb, subject, result, bad, .. } => (verb.as_str(), subject.as_str(), result.as_str(), *bad),
        Item::Say { text, .. } => ("say", text.as_str(), "", false),
    };
    assert_eq!(items.len(), 4, "its words once (not again from agent_message), no write_stdin");
    assert_eq!(line(0), ("say", "Looking at the menu code.", "", false));
    assert_eq!(line(1), ("Ran", "rg --files", "README.md", false));
    assert_eq!(line(2), ("Edited", "app.py", "+2 −1", false));
    assert_eq!(line(3), ("Ran", "pytest -q", "exit 1", true));
    let d = detail(p, &["k3".into(), "k4".into()]);
    assert_eq!(d["k3"]["diff"][1], json!(["+", "    new()"]));
    assert_eq!(d["k4"]["output"][1], "1 failed");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_script_that_only_runs_a_command_reads_as_that_command() {
    assert_eq!(super::script_command(r#"const r = await tools.exec_command({"cmd":"git status --short","yield_time_ms":1000}); text(r);"#).as_deref(), Some("git status --short"));
    assert_eq!(super::script_command("const m = ALL_TOOLS.filter(x => x); text(m);"), None);
}
