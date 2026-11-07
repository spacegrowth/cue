//! Tests: background scripts and their output, and a session's history of background work.

use super::*;

#[test]
fn a_background_script_shows_its_output_tail_until_its_session_hears_it_ended() {
    let dir = std::env::temp_dir().join(format!("cue-steps-script-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("b1.output");
    std::fs::write(&out, "   Compiling cue v0.1.41\n").unwrap();
    let launch = format!("Command running in background with ID: b1. Output is being written to: {}. You will be notified when it completes.", out.display());
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"build it"}}"#.to_string(),
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cd /x && npx tauri build --bundles app","description":"Build Cue","run_in_background":true}}]}}"#.to_string(),
        format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t1","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"b1"}}}}"#),
    ].join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers.len(), 1);
    let h = &f.helpers[0];
    assert!(h.script);
    assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.command.as_str()), ("b1", "t1", "Build Cue", "cd /x && npx tauri build --bundles app"));
    assert!(h.running());
    assert_eq!((h.output.clone(), h.lines), (vec!["   Compiling cue v0.1.41".to_string()], 1));
    assert_eq!(step(&f.turns[0].items[0]), ("Ran", "npx tauri build --bundles app", "in the background", false, true));
    assert!(running_helpers(p).is_empty(), "a script isn't an agent the session waits on");
    assert_eq!(running_background(p), 1, "but it's a background task the card counts");
    let written = mtime_ms(&out).unwrap();
    assert_eq!((h.last_ms, h.tokens), (written, 0), "last did something: its output file last written; no tokens");
    assert!(!h.stuck(written + STUCK_MS) && h.stuck(written + STUCK_MS + 1));
    // More output: shown on the next read.
    let v = f.version;
    std::fs::write(&out, "   Compiling cue v0.1.41\n\x1b[32m    Finished\x1b[0m release\n").unwrap();
    f.catch_up();
    assert!(f.version > v);
    assert_eq!(f.helpers[0].output[1], "    Finished release");
    // Its notice: ended, and how.
    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>b1</task-id>\n<status>failed</status>\n<summary>Background command failed</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    let h = &f.helpers[0];
    assert!(!h.running() && h.failed);
    assert_eq!(h.finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:00.000Z").unwrap());
    assert_eq!(running_background(p), 0);
    assert!(f.failures.is_empty(), "failed within a minute of starting: the session sees to it, no word");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Claude Code delivers a task notice three ways: as the prompt of a queued command (an `attachment`
/// entry), wrapped in a system notice inside a user message's text (beside a tool result, two at a
/// time sometimes), and as a user message of its own. And a script's output file ends on
/// "[exited with code N]" whether or not a notice ever shows up.
#[test]
fn a_background_script_ends_on_any_form_of_its_notice_or_on_its_exit_line() {
    let dir = std::env::temp_dir().join(format!("cue-steps-notices-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let start = |id: &str, out: &std::path::Path| {
        let launch = format!("Command running in background with ID: {id}. Output is being written to: {}. You will be notified when it completes.", out.display());
        [
            format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{{"content":[{{"type":"tool_use","id":"t-{id}","name":"Bash","input":{{"command":"make {id}","description":"Build {id}","run_in_background":true}}}}]}}}}"#),
            format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-{id}","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"{id}"}}}}"#),
        ].join("\n")
    };
    // (As JSON string text: escaped newlines and quotes.)
    let notice = |id: &str, status: &str| format!("<task-notification>\\n<task-id>{id}</task-id>\\n<status>{status}</status>\\n<summary>Background command \\\"Build {id}\\\" {status}</summary>\\n</task-notification>");
    let outs: Vec<std::path::PathBuf> = ["b1", "b2", "b3", "b4"].iter().map(|id| dir.join(format!("{id}.output"))).collect();
    for o in &outs {
        std::fs::write(o, "building\n").unwrap();
    }
    let path = dir.join("s1.jsonl");
    let mut log = format!("{}\n{}\n{}\n{}\n{}\n", r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"build them"}}"#, start("b1", &outs[0]), start("b2", &outs[1]), start("b3", &outs[2]), start("b4", &outs[3]));
    std::fs::write(&path, &log).unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers.iter().filter(|h| h.running()).count(), 4);
    // (a) A queued command, delivered mid-turn: an attachment entry with the notice as its prompt.
    log.push_str(&format!(r#"{{"type":"attachment","timestamp":"2026-10-04T10:02:00.000Z","attachment":{{"type":"queued_command","commandMode":"task-notification","prompt":"{}"}}}}"#, notice("b1", "completed")));
    log.push('\n');
    // (b) Two notices wrapped in a system notice, in a user message that also carries a tool result.
    log.push_str(&format!(r#"{{"type":"user","timestamp":"2026-10-04T10:03:00.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-none","content":"ok"}},{{"type":"text","text":"[SYSTEM NOTIFICATION - NOT USER INPUT]\\nThis is an automated background-task event.\\n\\n{}\\n\\n{}\\n\\n# Environment update"}}]}}}}"#, notice("b2", "completed"), notice("b3", "failed")));
    log.push('\n');
    let before = f.version;
    std::fs::write(&path, &log).unwrap();
    f.catch_up();
    assert!(f.version > before, "tasks ending count as a change");
    let by = |f: &Feed, id: &str| f.helpers.iter().find(|h| h.id == id).map(|h| (h.finished_ms, h.failed)).unwrap();
    assert_eq!(by(&f, "b1"), (crate::logs::transcript::iso_ms("2026-10-04T10:02:00.000Z").unwrap(), false), "the attachment's notice");
    assert_eq!(by(&f, "b2"), (crate::logs::transcript::iso_ms("2026-10-04T10:03:00.000Z").unwrap(), false), "the first wrapped notice");
    assert_eq!(by(&f, "b3"), (crate::logs::transcript::iso_ms("2026-10-04T10:03:00.000Z").unwrap(), true), "the second wrapped notice, beside a tool result");
    assert_eq!(f.turns.len(), 1, "a notice beside a tool result opens no turn");
    assert!(f.helpers.iter().find(|h| h.id == "b4").unwrap().running());
    // b3 failed three minutes after it started: worth a word, with its output's last line.
    assert_eq!(f.failures, vec![TaskFailure { path: p.to_string(), at_ms: by(&f, "b3").0, desc: "Build b3".into(), line: "building".into() }]);
    // (c) No notice for b4, but Claude Code ended its output file on the exit line: over, and failed.
    std::fs::write(&outs[3], "building\nerror: no rule\n[exited with code 2]\n").unwrap();
    f.catch_up();
    let h = f.helpers.iter().find(|h| h.id == "b4").unwrap();
    assert!(!h.running() && h.failed);
    assert_eq!(h.finished_ms, mtime_ms(&outs[3]).unwrap(), "finished when the file was last written");
    assert_eq!(h.last_line(), "error: no rule", "the exit line isn't what it last said");
    assert_eq!(f.failures.len(), 2, "it failed long after it started (the file was written just now): noted too");
    assert_eq!(f.failures[1].line, "error: no rule");
    // The hub takes the failures of every open feed, once.
    crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).insert(p.to_string(), f);
    let taken = take_task_failures();
    assert_eq!(taken.iter().filter(|t| t.path == p).count(), 2);
    assert!(take_task_failures().iter().all(|t| t.path != p));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A script stopped before it ended gets no notice: its output file ends on "[killed]" instead.
/// Over, stopped, not failed, and nothing noted for the hub.
#[test]
fn a_background_script_stopped_by_hand_ends_on_its_killed_line() {
    let dir = std::env::temp_dir().join(format!("cue-steps-killed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("k1.output");
    std::fs::write(&out, "building\n").unwrap();
    let launch = format!("Command running in background with ID: k1. Output is being written to: {}. You will be notified when it completes.", out.display());
    let log = [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"build it"}}"#.to_string(),
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t-k1","name":"Bash","input":{"command":"make","description":"Build it","run_in_background":true}}]}}"#.to_string(),
        format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-k1","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"k1"}}}}"#),
    ].join("\n") + "\n";
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, &log).unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert!(f.helpers[0].running());
    std::fs::write(&out, "building\nstep 3 of 9\n\n[killed]\n").unwrap();
    f.catch_up();
    let h = &f.helpers[0];
    assert!(!h.running() && h.stopped && !h.failed);
    assert_eq!(h.last_line(), "step 3 of 9", "the killed line isn't what it last said");
    assert_eq!(running_background(p), 0);
    assert!(f.failures.is_empty(), "stopped on purpose: no word");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A session's history reads its whole transcript and keeps every piece of background work, past the
/// MAX_HELPERS a live feed trims to.
#[test]
fn a_sessions_history_keeps_all_its_background_work() {
    let dir = std::env::temp_dir().join(format!("cue-steps-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let n = MAX_HELPERS + 3;
    let mut log = vec![r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"run them"}}"#.to_string()];
    for i in 0..n {
        let out = dir.join(format!("h{i}.output"));
        std::fs::write(&out, format!("line {i}\n[exited with code 0]\n")).unwrap();
        let launch = format!("Command running in background with ID: h{i}. Output is being written to: {}. You will be notified when it completes.", out.display());
        log.push(format!(r#"{{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{{"content":[{{"type":"tool_use","id":"t-h{i}","name":"Bash","input":{{"command":"make {i}","description":"Build {i}","run_in_background":true}}}}]}}}}"#));
        log.push(format!(r#"{{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t-h{i}","content":"{launch}"}}]}},"toolUseResult":{{"stdout":"","stderr":"","interrupted":false,"backgroundTaskId":"h{i}"}}}}"#));
    }
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, log.join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let all = history(p);
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), n, "the history has every one");
    assert_eq!(all[0]["desc"], "Build 0");
    assert_eq!(all[0]["latest"], "line 0", "its newest line, not the exit line");
    assert_eq!(all[0]["output"], json!([]), "the list carries no output");
    assert_eq!(history_helper(p, "h0")["output"], json!(["line 0", "[exited with code 0]"]), "one opened carries it");
    assert!(history_helper(p, "nope").is_null());
    let _ = std::fs::remove_dir_all(&dir);
}
