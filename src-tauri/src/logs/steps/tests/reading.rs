//! Tests: reading a log as it grows, Pi's logs, the context window, and a fresh feed's answer.

use super::*;

#[test]
fn a_pi_log_reads_as_steps_too() {
    let log = [
        r#"{"type":"session","version":3,"id":"P1","timestamp":"2026-10-04T10:00:00.000Z","cwd":"/a"}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":"<skill name="x">loaded</skill>"}]}}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:02.000Z","message":{"role":"user","content":[{"type":"text","text":"fix the filter"}]}}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":"Looking."},{"type":"toolCall","id":"c1","name":"edit","arguments":{"path":"/a/src/filters.ts"}}]}}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:04.000Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"edit","content":[{"type":"text","text":"ok"}],"isError":false,"details":{"diff":" 56   return [];\n-60 old line\n+60 new line\n+61   added\n    ...\n 90 tail"}}}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:05.000Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c2","name":"bash","arguments":{"command":"npm test"}}]}}"#,
        r#"{"type":"message","timestamp":"2026-10-04T10:00:09.000Z","message":{"role":"toolResult","toolCallId":"c2","toolName":"bash","content":[{"type":"text","text":"FAIL\n\nCommand exited with code 1"}],"isError":true}}"#,
    ]
    .join("\n");
    let dir = std::env::temp_dir().join(format!("cue-steps-pi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jsonl");
    std::fs::write(&path, log + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    assert_eq!(f.kind, Kind::Pi);
    f.catch_up();
    assert_eq!(f.turns.len(), 1, "the skill Pi loaded isn't a turn");
    assert_eq!(f.turns[0].prompt, "fix the filter");
    let items = &f.turns[0].items;
    assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking."));
    assert_eq!(step(&items[1]), ("Edited", "filters.ts", "+2 −1", false, true));
    assert_eq!(step(&items[2]), ("Ran", "npm test", "exit 1", true, true));
    let d = detail(p, &["c1".into()]);
    assert_eq!(d["c1"]["diff"][0], json!([" ", "  return [];"]));
    assert_eq!(d["c1"]["diff"][3], json!(["+", "  added"]));
    assert_eq!(d["c1"]["diff"][4], json!(["…", ""]));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_command_reads_without_its_cd() {
    assert_eq!(without_cd("cd ~/code/app && npm test"), "npm test");
    assert_eq!(without_cd("cd /a; cd b && make"), "make");
    assert_eq!(without_cd("cd /a"), "cd /a");
    assert_eq!(without_cd("npm test && cd x"), "npm test && cd x");
}

#[test]
fn a_new_prompt_starts_a_turn_and_closes_what_was_open() {
    let more = format!("{T}\n{}", r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"[Request interrupted by user]"}}
{"type":"user","timestamp":"2026-10-04T10:01:05.000Z","message":{"content":"<command-name>/rename</command-name>"}}
{"type":"user","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"now the docs"}}
{"type":"assistant","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"text","text":"On it."}]}}"#);
    let f = feed_of(&more);
    assert_eq!(f.turns.len(), 2, "the interrupt and the /rename don't start turns");
    assert_eq!(step(&f.turns[0].items[4]), ("Ran", "npm test", "stopped", false, true));
    assert_eq!(f.turns[1].items.len(), 1);
}

#[test]
fn reads_only_whole_new_lines() {
    let dir = std::env::temp_dir().join(format!("cue-steps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.jsonl");
    let first: Vec<&str> = T.lines().collect();
    // Everything up to the Edit's result, then half of the next line.
    std::fs::write(&path, format!("{}\n{}", first[..5].join("\n"), &first[5][..20])).unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.turns[0].items.len(), 3);
    let v = f.version;
    std::fs::write(&path, format!("{}\n", first.join("\n"))).unwrap();
    f.catch_up();
    assert_eq!(f.turns[0].items.len(), 5, "the half line is read once it's whole");
    assert!(f.version > v);
    let d = detail(p, &["t2".into(), "t3".into()]);
    assert_eq!(d["t2"]["diff"][1], json!(["-", "b"]));
    assert_eq!(d["t3"]["full"], "npm test\n");
    assert_eq!(d["t3"]["output"][1], "Tests: 3 failed");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_window_claude_code_reports_sets_the_context_share() {
    let dir = std::env::temp_dir().join(format!("cue-steps-window-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sid = format!("win-{}", std::process::id());
    let path = dir.join(format!("{sid}.jsonl"));
    // 174k tokens on a model whose name doesn't say its window is 1M.
    std::fs::write(&path, "{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{\"content\":\"hi\"}}\n{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{\"model\":\"claude-opus-5-5\",\"content\":[{\"type\":\"text\",\"text\":\"Hello.\"}],\"usage\":{\"input_tokens\":2,\"cache_read_input_tokens\":173376,\"cache_creation_input_tokens\":710}}}\n").unwrap();
    let p = path.to_str().unwrap();
    assert_eq!(context_pct(p), None, "without word from Claude Code: no share (a 200k guess is 5x off on a 1M window)");
    let before = steps(p, 0)["version"].as_u64().unwrap();
    set_window(&sid, 1_000_000, true);
    assert_eq!(context_pct(p), Some(17), "Claude Code's own window, as its status line shows");
    let after = steps(p, before);
    assert!(after["version"].as_u64().unwrap() > before && after["meta"]["window"] == 1_000_000, "the window redraws: {after}");
    // Another session on that model, before its own status line has run: the model's window, not the guess.
    let other = dir.join("win-other.jsonl");
    std::fs::write(&other, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
    let path2 = dir.join(format!("{sid}-2.jsonl"));
    std::fs::write(&path2, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
    assert_eq!(context_pct(path2.to_str().unwrap()), None, "a model nobody has reported yet: no share");
    set_window(&format!("{sid}-2"), 1_000_000, true);
    assert_eq!(context_pct(other.to_str().unwrap()), Some(17), "the window another session on its model reported");
    // A session on another machine runs that model with 200k (another provider): that's its own window
    // alone, and this Mac's sessions on the model keep the 1M theirs reported.
    let remote = dir.join(format!("{sid}-3.jsonl"));
    std::fs::write(&remote, std::fs::read_to_string(&path2).unwrap()).unwrap();
    assert_eq!(context_pct(remote.to_str().unwrap()), Some(17), "before it says: the model's window, as before");
    set_window(&format!("{sid}-3"), 200_000, false);
    assert_eq!(context_pct(remote.to_str().unwrap()), Some(87), "its own window, once its status line says");
    let mac = dir.join(format!("{sid}-4.jsonl"));
    std::fs::write(&mac, std::fs::read_to_string(&path2).unwrap()).unwrap();
    assert_eq!(context_pct(mac.to_str().unwrap()), Some(17), "this Mac's sessions on the model: still 1M");
    std::fs::remove_dir_all(&dir).ok();
}

/// Your prompt `text`, as a log line.
fn user(text: &str) -> String {
    format!("{{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{{\"content\":\"{text}\"}}}}\n")
}

/// A reply from `model`, as a log line.
fn line(model: &str) -> String {
    format!(
        "{{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{{\"model\":\"{model}\",\"content\":[{{\"type\":\"text\",\"text\":\"Hello.\"}}],\"usage\":{{\"input_tokens\":10,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}}}}\n"
    )
}

#[test]
fn a_fresh_feed_answers_in_full_even_when_its_count_matches() {
    let dir = std::env::temp_dir().join(format!("cue-steps-fresh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let append = |path: &std::path::Path| {
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        std::io::Write::write_all(&mut file, format!("{}", user("switch")).as_bytes()).unwrap();
        std::io::Write::write_all(&mut file, line("claude-opus-5-5").as_bytes()).unwrap();
    };
    // While the feed lives, a model change is read and answered.
    let live = dir.join("live.jsonl");
    std::fs::write(&live, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
    let l1 = steps(live.to_str().unwrap(), 0);
    assert_eq!(l1["meta"]["model"], "claude-sonnet-5-5");
    append(&live);
    let l2 = steps(live.to_str().unwrap(), l1["version"].as_u64().unwrap());
    assert_eq!(l2["meta"]["model"], "claude-opus-5-5", "the model change is read while the feed lives");
    // A feed nobody asked for in a while is dropped: the caller still has its version, 2 here (the
    // first read). A fresh feed reads the whole file in one go and counts 2 again, so the "nothing
    // changed" answer would leave the caller on the old model. It must answer in full.
    let path = dir.join("t.jsonl");
    std::fs::write(&path, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
    let p = path.to_str().unwrap();
    let r1 = steps(p, 0);
    assert_eq!(r1["version"].as_u64(), Some(2));
    append(&path);
    forget(p);
    let r2 = steps(p, r1["version"].as_u64().unwrap());
    assert_eq!(r2["version"].as_u64(), r1["version"].as_u64());
    assert_eq!(r2["meta"]["model"], "claude-opus-5-5", "a fresh feed answers with what's there now, whatever its count");
    assert!(r2["turns"].as_array().is_some_and(|t| !t.is_empty()));
    // Once it has answered, a feed that still counts the same answers with just the version.
    let r3 = steps(p, r2["version"].as_u64().unwrap());
    assert!(r3.get("turns").is_none() && r3.get("meta").is_none(), "only the version comes back when nothing changed");
    std::fs::remove_dir_all(&dir).ok();
}

/// A log that doesn't answer (a stalled disk, an automount gone quiet: `Stall`) holds up the queries about
/// its own session, never another session's: each feed is read under a lock of its own, and the table of
/// feeds is held only to find one. The soak found every session waiting on one slow read.
#[test]
fn a_log_that_doesnt_answer_holds_up_only_its_own_session() {
    let dir = std::env::temp_dir().join(format!("cue-steps-stall-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stalled = crate::testing::Stall::at(&dir.join("stalled.jsonl"));
    let fine = dir.join("fine.jsonl");
    std::fs::write(&fine, format!("{}{}", user("hi"), line("claude-opus-5-5"))).unwrap();
    let path = stalled.path().to_str().unwrap().to_string();
    let (asked, done) = (std::sync::mpsc::channel(), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let d = done.clone();
    let waiting = std::thread::spawn(move || {
        asked.0.send(()).unwrap();
        let _ = model_of(&path);
        d.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    asked.1.recv().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let f = fine.to_str().unwrap().to_string();
    std::thread::spawn(move || tx.send(model_of(&f)).unwrap());
    let other = rx.recv_timeout(std::time::Duration::from_secs(3));
    stalled.answer_until(|| done.load(std::sync::atomic::Ordering::SeqCst));
    waiting.join().unwrap();
    assert_eq!(other.expect("another session's query answered while one log stalled"), Some("claude-opus-5-5".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}
