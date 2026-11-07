//! The steps' tests: reading logs as turns and steps, and what's asked of them.

mod agents;
mod codex;
mod hooks;
mod reading;
mod scripts;
mod turns;

use super::codex::*;
use super::edits::*;
use super::feed::*;
use super::helper::*;
use super::kind::*;
use super::model::*;
use super::queries::*;
use super::step_line::*;
use super::turn_rows::*;
use super::*;
use crate::logs::transcript::{lines, stamp};
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;

#[test]
fn an_edited_file_is_in_the_area_under_its_session_folder() {
    assert_eq!(area_of("/r/cue", "/r/cue/ui/app.js").as_deref(), Some("ui"));
    assert_eq!(area_of("/r/cue/", "/r/cue/README.md").as_deref(), Some("."));
    assert_eq!(area_of("/r/cue", "/r/other/src/a.rs").as_deref(), Some("/r/other/src"), "outside it: its own folder");
    assert_eq!(area_of("/r/cue", "/Users/x/.claude/projects/m/memory/a.md"), None);
    assert_eq!(area_of("/r/cue", "/private/tmp/scratch/a.py"), None);
    assert_eq!(area_of("/r/cue", "/r/cuex/a.rs").as_deref(), Some("/r/cuex"), "a sibling folder isn't inside it");
}

#[test]
fn a_turns_work_counts_its_looks_agents_relay_and_edits() {
    let mut w = Work::default();
    for b in [
        json!({"name":"Read","input":{"file_path":"/r/a.rs"}}),
        json!({"name":"Grep","input":{"pattern":"x"}}),
        json!({"name":"Bash","input":{"command":"cd /r && grep -n x a.rs"}}),
        json!({"name":"Bash","input":{"command":"cargo test"}}),
        json!({"name":"Bash","input":{"command":"relay spawn --topic x"}}),
        json!({"name":"Skill","input":{"skill":"relay:spawn"}}),
        json!({"name":"Agent","input":{"description":"look"}}),
        json!({"name":"Edit","input":{"file_path":"/r/a.rs"}}),
        json!({"name":"Write","input":{"file_path":"/r/a.rs"}}),
        json!({"name":"NotebookEdit","input":{"notebook_path":"/r/n.ipynb"}}),
    ] {
        w.count(&b);
    }
    assert_eq!((w.looks, w.agents, w.relay), (3, 1, 2));
    assert_eq!(w.edited, vec!["/r/a.rs", "/r/n.ipynb"], "each file once");
}


#[test]
fn edited_files_are_read_from_every_agents_log() {
    let dir = std::env::temp_dir().join(format!("cue-edited-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let claude = dir.join("c.jsonl");
    std::fs::write(&claude, [
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/r/x.rs"}}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Write","input":{"file_path":"/other/b.md"}},{"type":"tool_use","id":"t4","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#,
    ].join("\n")).unwrap();
    assert_eq!(edited_files(claude.to_str().unwrap()), vec!["/r/a.rs", "/other/b.md"]);
    // A background agent's edits, from its own log under the session's folder, come after the session's own.
    let sub = dir.join("c").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-a1.jsonl"), r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"h1","name":"Edit","input":{"file_path":"/r/c.rs"}},{"type":"tool_use","id":"h2","name":"Edit","input":{"file_path":"/r/a.rs"}}]}}"#).unwrap();
    assert_eq!(edited_files(claude.to_str().unwrap()), vec!["/r/a.rs", "/other/b.md", "/r/c.rs"]);
    // Who edited what: the session itself (""), or the agent by the description its session gave it; a
    // file both edited keeps the first. Without a meta file, the agent's id names it.
    let by = |p: &std::path::Path| edited_files_by(p.to_str().unwrap()).into_iter().map(|(f, b)| format!("{f} by {b}")).collect::<Vec<_>>();
    assert_eq!(by(&claude), vec!["/r/a.rs by ", "/other/b.md by ", "/r/c.rs by a1"]);
    std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","description":"Backend: names"}"#).unwrap();
    assert_eq!(by(&claude)[2], "/r/c.rs by Backend: names");
    std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore"}"#).unwrap();
    assert_eq!(by(&claude)[2], "/r/c.rs by Explore");
    let pi = dir.join("p.jsonl");
    std::fs::write(&pi, [
        r#"{"type":"session","version":3,"id":"s","cwd":"/a"}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"edit","arguments":{"path":"/a/src/f.ts"}}]}}"#,
    ].join("\n")).unwrap();
    assert_eq!(edited_files(pi.to_str().unwrap()), vec!["/a/src/f.ts"]);
    let codex = dir.join("x.jsonl");
    std::fs::write(&codex, [
        r#"{"timestamp":"2026-10-05T06:36:49.000Z","type":"session_meta","payload":{"id":"x","cwd":"/w"}}"#,
        r#"{"timestamp":"2026-10-05T06:36:50.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","call_id":"p1","input":"*** Begin Patch\n*** Update File: src/one.rs\n@@\n-a\n+b\n*** Add File: src/two.rs\n+x\n*** End Patch"}}"#,
    ].join("\n")).unwrap();
    assert_eq!(edited_files(codex.to_str().unwrap()), vec!["src/one.rs", "src/two.rs"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_1m_session_continues_on_1m() {
    assert_eq!(model_for_window("claude-opus-5-5", 1_000_000), "claude-opus-5-5[1m]");
    assert_eq!(model_for_window("claude-opus-5-5[1m]", 1_000_000), "claude-opus-5-5[1m]");
    assert_eq!(model_for_window("claude-opus-5-5", 200_000), "claude-opus-5-5");
    assert_eq!(model_for_window("claude-opus-5-5", 0), "claude-opus-5-5");
    assert_eq!(model_for_window("", 1_000_000), "");
}

/// A teammate's message to its lead wakes it, but it's the teammate talking: the turn it starts has no prompt
/// of yours, and Cue's Stop hook keeps the lead's reply before it in that turn's parts.
#[test]
fn a_teammates_message_is_never_your_prompt() {
    let log = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"look at the hook"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Asked reader to check it."}]}}
{"type":"user","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"text","text":"Another Claude session sent a message:\n<teammate-message teammate_id=\"reader\">It's fine.</teammate-message>"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"Reader says it's fine."}]}}
"#;
    let f = feed_of(log);
    assert_eq!(f.turns[0].prompt, "look at the hook");
    assert!(f.turns.iter().skip(1).all(|t| t.prompt.is_empty()), "no prompt of yours from the teammate");
    let path = std::env::temp_dir().join(format!("cue-peer-{}.jsonl", std::process::id()));
    std::fs::write(&path, log).unwrap();
    let parts = crate::hook::final_turn(&serde_json::json!({ "transcript_path": path.to_string_lossy(), "last_assistant_message": "Reader says it's fine." }));
    let _ = std::fs::remove_file(&path);
    let texts: Vec<&str> = parts.iter().filter_map(|v| v["text"].as_str()).collect();
    assert_eq!(texts, ["Asked reader to check it.", "Reader says it's fine."], "the lead's reply before the teammate's message stays in the turn");
}

fn feed_of(jsonl: &str) -> Feed {
    let mut f = Feed { path: String::new(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false), max_helpers: MAX_HELPERS };
    for e in lines(jsonl) {
        f.take(&e);
    }
    f
}
fn step(item: &Item) -> (&str, &str, &str, bool, bool) {
    match item {
        Item::Step { verb, subject, result, bad, done, .. } => (verb, subject, result, *bad, *done),
        Item::Say { .. } => panic!("not a step"),
    }
}

const T: &str = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Looking at it."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"..."}]},"toolUseResult":{"type":"text","file":{"filePath":"/a/src/token.ts","numLines":84}}}
{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"structuredPatch":[{"lines":[" a","-b","+c","+d"]}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"npm test\n"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:20.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t3","is_error":true,"content":"Exit code 1\nTests: 3 failed"}]},"toolUseResult":"Error: Exit code 1"}
{"type":"assistant","timestamp":"2026-10-04T10:00:22.000Z","isSidechain":true,"message":{"content":[{"type":"text","text":"a helper's words"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:25.000Z","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"npm test"}}]}}"#;
