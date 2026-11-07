//! Tests: hand-off notes, and each turn's tokens, context and row for Insights.

use super::*;

#[test]
fn a_hand_off_note_is_known_by_its_name() {
    for yes in ["/r/HANDOFF.md", "handoff.md", "/r/docs/mac-security-handoff.md", "/r/handoff-2026-10-18.md", "/r/Hand-Off.MD", "/r/next_hand_off.md"] {
        assert!(is_handoff_note(yes), "{yes}");
    }
    for no in ["/r/src/handoff.rs", "/r/handoff/notes.md", "/r/handoff/", "/r/HANDOFF.txt", "/r/README.md", ""] {
        assert!(!is_handoff_note(no), "{no}");
    }
}

#[test]
fn a_hand_off_note_counts_when_written_whole_and_it_went_through() {
    let call = |id: &str, tool: &str, file: &str| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"{tool}","input":{{"file_path":"{file}"}}}}]}}}}"#);
    let result = |id: &str, at: &str, error: bool| format!(r#"{{"type":"user","timestamp":"{at}","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","is_error":{error},"content":"ok"}}]}}}}"#);
    let prompt = |text: &str| format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{{"content":"{text}"}}}}"#);
    let note = |log: Vec<String>| feed_of(&log.join("\n")).note;
    // Written, and it went through: the path as written, and when its result came.
    let at = crate::logs::transcript::iso_ms("2026-10-04T10:00:06.000Z").unwrap();
    assert_eq!(note(vec![prompt("hand off"), call("w1", "Write", "/r/HANDOFF.md"), result("w1", "2026-10-04T10:00:06.000Z", false)]), Some(("/r/HANDOFF.md".to_string(), at)));
    // Edited rather than written whole, or another file: not a hand-off.
    assert_eq!(note(vec![call("e1", "Edit", "/r/HANDOFF.md"), result("e1", "2026-10-04T10:00:06.000Z", false)]), None);
    assert_eq!(note(vec![call("e2", "MultiEdit", "/r/HANDOFF.md"), result("e2", "2026-10-04T10:00:06.000Z", false)]), None);
    assert_eq!(note(vec![call("w2", "Write", "/r/src/handoff.rs"), result("w2", "2026-10-04T10:00:06.000Z", false)]), None);
    // The write failed, or its result isn't in yet.
    assert_eq!(note(vec![call("w3", "Write", "/r/HANDOFF.md"), result("w3", "2026-10-04T10:00:06.000Z", true)]), None);
    assert_eq!(note(vec![call("w4", "Write", "/r/HANDOFF.md")]), None);
    // The newest one counts.
    let two = note(vec![call("w5", "Write", "/r/HANDOFF.md"), result("w5", "2026-10-04T10:00:06.000Z", false), call("w6", "Write", "/r/b-handoff.md"), result("w6", "2026-10-04T10:00:09.000Z", false)]);
    assert_eq!(two.map(|(p, _)| p).as_deref(), Some("/r/b-handoff.md"));
    // You gave it more to do after it: that note isn't where it stopped.
    assert_eq!(note(vec![call("w7", "Write", "/r/HANDOFF.md"), result("w7", "2026-10-04T10:00:06.000Z", false), prompt("now fix the test")]), None);
    // Cue's hand-off ask, pasted in and come back wrapped: its older note edited into the new one counts.
    let ask = format!(r#"<pasted_content id=\"a1\">\n{HANDOFF_ASK_START} so a fresh session can carry on.\n</pasted_content id=\"a1\">"#);
    let f = feed_of(&[prompt(&ask), call("e3", "Edit", "/r/HANDOFF-ab12cd34.md"), result("e3", "2026-10-04T10:00:06.000Z", false)].join("\n"));
    assert_eq!(f.note.map(|(p, _)| p).as_deref(), Some("/r/HANDOFF-ab12cd34.md"));
    assert!(f.prompted.1, "the wrapped ask is still known as Cue's");
}

#[test]
fn each_turn_counts_its_replies_tokens_once_and_the_cache_lifetime_is_kept() {
    // Claude Code writes a reply as one line per content block, each repeating the reply's usage.
    let path = std::env::temp_dir().join(format!("cue-turn-tokens-{}.jsonl", std::process::id()));
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"On it."}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":200,"ephemeral_5m_input_tokens":0},"output_tokens":50}}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/r/a.rs"}}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":200,"ephemeral_5m_input_tokens":0},"output_tokens":50}}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"and the total"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:01:09.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":5,"cache_read_input_tokens":3000,"cache_creation_input_tokens":0,"output_tokens":20}}}"#,
    ].join("\n") + "\n").unwrap();
    let mut f = Feed::open(path.to_str().unwrap(), 1);
    f.catch_up();
    let _ = std::fs::remove_file(&path);
    assert_eq!(f.turns.iter().map(|t| t.tokens).collect::<Vec<_>>(), vec![1260, 3025]);
    assert_eq!(f.turns.iter().map(|t| t.context).collect::<Vec<_>>(), vec![1210, 3005], "each turn keeps the context its last reply carried");
    assert_eq!(f.meta.cache_ttl_s, 3600, "a reply that only read the cache keeps the lifetime it was written with");
    assert_eq!(f.meta.start_context, 1210, "the session's start is its first reply's context");
    assert_eq!(f.meta.last_call_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:09.000Z").unwrap());
}

#[test]
fn a_compact_leaves_its_context_at_once_not_at_the_next_reply() {
    let path = std::env::temp_dir().join(format!("cue-compact-context-{}.jsonl", std::process::id()));
    let reply = |at: &str, id: &str, read: u64| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:{at}.000Z","message":{{"id":"{id}","model":"claude-opus-5-5","content":[{{"type":"text","text":"Done."}}],"usage":{{"input_tokens":5,"cache_read_input_tokens":{read},"cache_creation_input_tokens":0,"output_tokens":20}}}}}}"#);
    let mut log = [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#.to_string(),
        reply("00:05", "m1", 600_000),
        // A background agent compacting its own context isn't this session's.
        r#"{"type":"system","subtype":"compact_boundary","isSidechain":true,"timestamp":"2026-10-04T10:00:30.000Z","compactMetadata":{"trigger":"auto","preTokens":90000,"postTokens":4000}}"#.to_string(),
    ]
    .join("\n") + "\n";
    std::fs::write(&path, &log).unwrap();
    let mut f = Feed::open(path.to_str().unwrap(), 1);
    f.catch_up();
    assert_eq!(f.meta.context, 600_005);
    log += r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:01:00.000Z","compactMetadata":{"trigger":"manual","preTokens":600005,"postTokens":15959}}"#;
    log += "\n";
    std::fs::write(&path, &log).unwrap();
    f.catch_up();
    assert_eq!(f.meta.context, 15_959, "what the compact left, before any reply");
    log += &(reply("02:00", "m2", 18_000) + "\n");
    std::fs::write(&path, &log).unwrap();
    f.catch_up();
    let _ = std::fs::remove_file(&path);
    assert_eq!(f.meta.context, 18_005, "then its replies again");
}

#[test]
fn a_snippet_leaves_out_appended_instructions() {
    assert_eq!(snippet("No, the card did show up (Voice: listening, not reading. Answer in one or two short sentences.)"), "No, the card did show up");
    assert_eq!(snippet("Fix the fill (partial) price"), "Fix the fill (partial) price");
    assert_eq!(snippet("(only a long bracketed note, nothing before it at all here)"), "(only a long bracketed note, nothing before it at all here)");
}

#[test]
fn a_turns_row_splits_its_tokens_and_knows_how_long_it_waited() {
    let path = std::env::temp_dir().join(format!("cue-turn-rows-{}.jsonl", std::process::id()));
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"add the tax column"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"On it."}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":150,"ephemeral_5m_input_tokens":50},"output_tokens":50}}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/r/a.rs"}}],"usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200,"cache_creation":{"ephemeral_1h_input_tokens":150,"ephemeral_5m_input_tokens":50},"output_tokens":50}}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T12:00:05.000Z","message":{"content":"no, the total column"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T12:00:09.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":3000,"output_tokens":20}}}"#,
    ].join("\n") + "\n").unwrap();
    let (rows, _) = turn_rows(path.to_str().unwrap(), 0, 1 << 20);
    let _ = std::fs::remove_file(&path);
    let u = rows[0].usage;
    assert_eq!((u.fresh, u.read, u.write_5m, u.write_1h, u.output), (10, 1000, 50, 150, 50), "each reply once, its writes by lifetime");
    assert_eq!(rows[0].waited_ms, 0, "nothing before the first turn");
    assert_eq!(rows[1].waited_ms, 2 * 3600 * 1000, "from its reply before to this prompt");
    assert_eq!((rows[1].usage.first_write, rows[1].usage.first_read), (3000, 0), "after two hours it wrote its context again");
    assert!(rows[0].corrected);
    assert_eq!((rows[0].prompt.as_str(), rows[0].correction.as_str()), ("add the tax column", "no, the total column"));
    assert_eq!((rows[0].looks, rows[1].looks), (1, 0), "its read counted on its own turn");
}

#[test]
fn a_turns_row_keeps_where_claude_code_compacted_it_on_its_own() {
    let path = std::env::temp_dir().join(format!("cue-turn-auto-{}.jsonl", std::process::id()));
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"go on"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"cache_read_input_tokens":250000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
        r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:01:00.000Z","compactMetadata":{"trigger":"auto","preTokens":252123}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"and then"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:02:05.000Z","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"cache_read_input_tokens":9000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
        r#"{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-04T10:03:00.000Z","compactMetadata":{"trigger":"manual","preTokens":9000}}"#,
    ].join("\n") + "\n").unwrap();
    let (rows, _) = turn_rows(path.to_str().unwrap(), 0, 1 << 20);
    let _ = std::fs::remove_file(&path);
    assert_eq!(rows.iter().map(|r| r.auto_compact).collect::<Vec<_>>(), vec![252123, 0], "on its own: kept with its turn; your /compact: not");
}

#[test]
fn a_feed_opened_mid_file_still_knows_the_context_its_session_started_with() {
    // A long session: only its last START_BYTES are read for the chat, so its first reply is read
    // from the top of the log on its own.
    let path = std::env::temp_dir().join(format!("cue-start-ctx-{}.jsonl", std::process::id()));
    let reply = |id: &str, ctx: u64| format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{{"id":"{id}","model":"claude-opus-5-5","content":[{{"type":"text","text":"ok"}}],"usage":{{"input_tokens":0,"cache_read_input_tokens":{ctx},"cache_creation_input_tokens":0,"output_tokens":5}}}}}}"#);
    let mut log = String::new();
    log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"start"}}"#);
    log.push('\n');
    log.push_str(&reply("m0", 20000));
    log.push('\n');
    let filler = format!(r#"{{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{{"content":"{}"}}}}"#, "x".repeat(4000));
    while (log.len() as u64) < START_BYTES + 200_000 {
        log.push_str(&filler);
        log.push('\n');
    }
    log.push_str(&reply("m9", 300000));
    log.push('\n');
    std::fs::write(&path, log).unwrap();
    let mut f = Feed::open(path.to_str().unwrap(), 1);
    f.catch_up();
    let _ = std::fs::remove_file(&path);
    assert_eq!(f.meta.context, 300000);
    assert_eq!(f.meta.start_context, 20000, "read from the top of the log, not the first turn the chat sees");
    assert_eq!(f.meta.start_ms, crate::logs::transcript::iso_ms("2026-10-04T10:00:05.000Z").unwrap());
}
