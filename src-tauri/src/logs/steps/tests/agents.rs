//! Tests: background agents and teammates shown at work until their session hears they finished.

use super::*;

#[test]
fn a_background_agent_shows_at_work_under_its_step_until_its_session_hears_it_finished() {
    let dir = std::env::temp_dir().join(format!("cue-steps-helper-{}", std::process::id()));
    let sub = dir.join("s1").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","description":"Backend: names","toolUseId":"t1","requestShape":"background"}"#).unwrap();
    std::fs::write(sub.join("agent-a1.jsonl"), [
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"find every name"}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":100,"output_tokens":20},"content":[{"type":"tool_use","id":"h1","name":"Grep","input":{"pattern":"ai-title"}}]}}"#,
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"Found 3 files"}]}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"model":"claude-opus-5-5","usage":{"input_tokens":5,"cache_read_input_tokens":1100,"output_tokens":30},"content":[{"type":"tool_use","id":"h2","name":"Read","input":{"file_path":"/a/hub.rs"}}]}}"#,
    ].join("\n") + "\n").unwrap();
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"map the names"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Backend: names","subagent_type":"Explore","prompt":"find every name"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Backend: names","resolvedModel":"claude-opus-5-5"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"text","text":"An agent is on it."}]}}"#,
    ].join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers.len(), 1);
    let h = &f.helpers[0];
    assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.kind.as_str(), h.model.as_str()), ("a1", "t1", "Backend: names", "Explore", "claude-opus-5-5"));
    assert!(!h.teammate && h.name.is_empty(), "a background agent isn't a teammate");
    assert!(h.running());
    assert_eq!(h.items.len(), 2, "its own steps, from its own log");
    assert_eq!(step(&h.items[0]), ("Searched", "“ai-title”", "3 files", false, true));
    assert_eq!(step(&h.items[1]).0, "Reading");
    assert_eq!(step(&f.turns[0].items[0]), ("Delegated", "Backend: names", "in the background", false, true));
    assert_eq!(h.tokens, 10 + 1000 + 100 + 20 + 5 + 1100 + 30, "its tokens: every reply's input, cache reads and writes, and output");
    let last = crate::logs::transcript::iso_ms("2026-10-04T10:00:06.000Z").unwrap();
    assert_eq!(h.last_ms, last, "last did something: its newest step");
    // Quiet past the threshold (sessions/quiet.rs) with a step open ("Reading …"): a long run, left to it, not stuck.
    let quiet = crate::sessions::quiet::QUIET_MS;
    assert_eq!((h.quiet(last + quiet - 1), h.quiet(last + quiet)), ("", "tool"));
    assert!(!h.stuck(last + quiet), "a step open is a long run, not stuck");
    assert!(h.open_step().starts_with("Reading"), "{}", h.open_step());
    assert_eq!(running_helpers(p), vec!["Backend: names".to_string()]);
    assert_eq!(running_background(p), 1);
    assert_eq!(agent_count(p, true), Some(1), "its agents: the same list the window shows");
    assert_eq!(agent_name(p, "a1"), "Backend: names", "an ask from inside it is named by its task");
    assert_eq!(agent_name(p, "nope"), "");
    let shown = &steps(p, 0)["helpers"][0];
    assert_eq!((shown["tokens"].as_u64(), shown["last_ms"].as_u64(), shown["running"].as_bool(), shown["stuck"].as_bool()), (Some(2265), Some(last), Some(true), Some(false)), "sent as such: {shown}");
    // That step is long open by now: a long run, left to it (sessions/quiet.rs), the step it has open said.
    assert_eq!((shown["quiet"].as_str(), shown["quiet_what"].as_str()), (Some("tool"), Some("Reading hub.rs")));
    // It writes its report: it's done, even before its session hears so.
    let v = f.version;
    let mut log = std::fs::OpenOptions::new().append(true).open(sub.join("agent-a1.jsonl")).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h2","content":"ok"}]}}"#, "\n",
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:08.000Z","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Names: project, name."}]}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert!(f.version > v, "its new steps count as a change");
    assert!(!f.helpers[0].running());
    assert_eq!(f.helpers[0].finished_ms, 0);
    // Its session is told: finished for good.
    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Backend: names\" finished</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:01:00.000Z").unwrap());
    assert!(running_helpers(p).is_empty());
    assert_eq!(running_background(p), 0);
    assert_eq!(agent_count(p, false), Some(1), "a finished agent still counts: it's in the list");
    assert_eq!(agent_count(&format!("{p}.none"), false), None, "no feed: not opened just to count");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_teammate_shows_at_work_until_it_answers_and_again_when_its_session_messages_it() {
    // As Claude Code writes them: an Agent call given a name starts a teammate.
    let dir = std::env::temp_dir().join(format!("cue-steps-teammate-{}", std::process::id()));
    let sub = dir.join("s1").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-aprobe-30f8.meta.json"), r#"{"agentType":"probe","description":"Probe teammate reply","name":"probe","requestShape":"background","taskKind":"in_process_teammate","teamName":"session-804699a9"}"#).unwrap();
    let agent_log = sub.join("agent-aprobe-30f8.jsonl");
    std::fs::write(&agent_log, concat!(r#"{"type":"user","isSidechain":true,"timestamp":"2026-10-20T20:34:45.000Z","message":{"role":"user","content":"<teammate-message teammate_id=\"team-lead\" summary=\"Probe teammate reply\">\nReply with exactly the text: probe ok\n</teammate-message>"}}"#, "\n")).unwrap();
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-20T20:34:42.000Z","message":{"content":"start a teammate"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-20T20:34:44.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Probe teammate reply","name":"probe","prompt":"Reply with exactly the text: probe ok"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-20T20:34:45.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Spawned successfully.\nagent_id: aprobe-30f8\nname: probe\nThe agent is now running and will receive instructions via mailbox."}]}]},"toolUseResult":{"status":"teammate_spawned","agentId":"aprobe-30f8","resolvedModel":"claude-haiku-5-5","teammate_id":"probe@session-804699a9","name":"probe","team_name":"session-804699a9"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-20T20:34:46.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"It's running."}]}}"#,
    ].join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers.len(), 1, "a teammate is one of its background agents");
    assert_eq!((f.helpers[0].id.as_str(), f.helpers[0].desc.as_str(), f.helpers[0].kind.as_str()), ("aprobe-30f8", "Probe teammate reply", "probe"));
    assert!(f.helpers[0].teammate && f.helpers[0].name == "probe", "known as a teammate, by its name");
    assert_eq!(steps(p, 0)["helpers"][0]["teammate"], true, "and sent to the window as one");
    assert_eq!(step(&f.turns[0].items[0]).2, "in the background");
    assert_eq!(running_helpers(p), vec!["Probe teammate reply".to_string()]);
    // It answers: done (its idle notice to the session says the same, and changes nothing).
    let append = |file: &std::path::Path, line: &str| std::io::Write::write_all(&mut std::fs::OpenOptions::new().append(true).open(file).unwrap(), format!("{line}\n").as_bytes()).unwrap();
    append(&agent_log, r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-20T20:34:48.000Z","message":{"model":"claude-haiku-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"probe ok"}]}}"#);
    append(&path, r#"{"type":"user","timestamp":"2026-10-20T20:34:49.000Z","message":{"role":"user","content":"Another Claude session sent a message:\n<teammate-message teammate_id=\"probe\" color=\"blue\">\n{\"type\":\"idle_notification\",\"from\":\"probe\",\"idleReason\":\"available\",\"result\":\"probe ok\"}\n</teammate-message>"}}"#);
    f.catch_up();
    assert!(!f.helpers[0].running());
    assert!(running_helpers(p).is_empty());
    // Its session messages it: at work again until it answers again.
    append(&agent_log, r#"{"type":"user","isSidechain":true,"timestamp":"2026-10-20T20:35:10.000Z","message":{"role":"user","content":"<teammate-message teammate_id=\"team-lead\">\nnow say it twice\n</teammate-message>"}}"#);
    f.catch_up();
    assert!(f.helpers[0].running(), "a message to it puts it back to work");
    append(&agent_log, r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-20T20:35:12.000Z","message":{"model":"claude-haiku-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"probe ok probe ok"}]}}"#);
    f.catch_up();
    assert!(!f.helpers[0].running());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_background_agent_waiting_on_its_own_background_work_is_still_at_work() {
    let dir = std::env::temp_dir().join(format!("cue-steps-waits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sub = dir.join("s1").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    // The agent starts a long command in the background and ends its turn to wait for it.
    std::fs::write(sub.join("agent-a1.jsonl"), [
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"rebase it"}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"h1","name":"Bash","input":{"command":"git rebase main","run_in_background":true}}]}}"#,
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"Command running in background with ID: b9."}]},"toolUseResult":{"backgroundTaskId":"b9"}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Waiting for the rebase."}]}}"#,
    ].join("\n") + "\n").unwrap();
    let path = dir.join("s1.jsonl");
    // Its session is told "completed", but that it's waiting on its own background work.
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"rebase the branch"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Rebase","prompt":"rebase it","run_in_background":true}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Rebase"}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Rebase\" is waiting on its own background work</summary>\n<note>This agent stopped with background work of its own still running.</note>\n</task-notification>"}}"#,
    ].join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers[0].finished_ms, 0, "not finished on a notice that says it waits");
    assert!(f.helpers[0].running(), "its log ended its turn, but its own command still runs");
    assert_eq!(running_background(p), 1);
    // Its command ends, it writes its report: done, before its session hears so.
    let mut log = std::fs::OpenOptions::new().append(true).open(sub.join("agent-a1.jsonl")).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"<task-notification>\n<task-id>b9</task-id>\n<status>completed</status>\n</task-notification>"}}"#, "\n",
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:01.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Rebased."}]}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert!(!f.helpers[0].running());
    // Its session's second notice: finished for good.
    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:02:02.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Rebase\" finished</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:02:02.000Z").unwrap());
    assert_eq!(running_background(p), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_message_to_an_agent_cue_has_no_log_of_doesnt_leave_the_session_at_work() {
    let dir = std::env::temp_dir().join(format!("cue-steps-ghost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"ask it again"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"zz9","message":"and the rest"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"success":true,"resumedAgentId":"zz9"}}"#,
    ].join("\n") + "\n").unwrap();
    assert_eq!(running_background(path.to_str().unwrap()), 0, "no log of it here: not counted as at work");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_background_agent_sent_a_message_after_it_finished_is_at_work_again_until_it_reports_again() {
    let dir = std::env::temp_dir().join(format!("cue-steps-resume-{}", std::process::id()));
    let sub = dir.join("s1").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"general-purpose","description":"Review the plan","toolUseId":"t1","requestShape":"background"}"#).unwrap();
    let agent_log = sub.join("agent-a1.jsonl");
    std::fs::write(&agent_log, [
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"review the plan"}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:08.000Z","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Don't build it as is."}]}}"#,
    ].join("\n") + "\n").unwrap();
    let path = dir.join("s1.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"get the plan reviewed"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Review the plan","prompt":"review the plan"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Review the plan","resolvedModel":"claude-opus-5-5"}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#,
    ].join("\n") + "\n").unwrap();
    let p = path.to_str().unwrap();
    let mut f = Feed::open(p, 1);
    f.catch_up();
    assert_eq!(f.helpers.len(), 1);
    assert!(!f.helpers[0].running(), "finished: its session heard so");
    // The session sends it a message: it's put back to work, on the same log.
    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut log, concat!(
        r#"{"type":"assistant","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"a1","summary":"Review the revised plan","message":"Please look again."}}]}}"#, "\n",
        r#"{"type":"user","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"{\"success\":true}"}]},"toolUseResult":{"success":true,"message":"Resuming agent a1","resumedAgentId":"a1","revivedAgentId":"a1"}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert_eq!(f.helpers.len(), 1, "the same agent, not a second one");
    let h = &f.helpers[0];
    assert!(h.running(), "at work again, before its log shows the message");
    assert_eq!((h.finished_ms, h.last_ms, h.step.as_str()), (0, crate::logs::transcript::iso_ms("2026-10-04T10:02:01.000Z").unwrap(), "t2"));
    assert_eq!(step(f.turns.iter().flat_map(|t| t.items.iter()).last().unwrap()), ("Messaged its agent", "Review the revised plan", "back at work", false, true));
    assert_eq!(running_helpers(p), vec!["Review the plan".to_string()]);
    assert_eq!(running_background(p), 1);
    // Its log goes on: its new steps are read on from where they were.
    let mut log = std::fs::OpenOptions::new().append(true).open(&agent_log).unwrap();
    std::io::Write::write_all(&mut log, concat!(
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:01.000Z","message":{"role":"user","content":"Please look again."}}"#, "\n",
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:02:05.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"h1","name":"Read","input":{"file_path":"/a/plan.md"}}]}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    let h = &f.helpers[0];
    assert!(h.running());
    assert_eq!(step(h.items.last().unwrap()).0, "Reading");
    // Its second report: finished again.
    let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:05:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t2</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
    f.catch_up();
    assert_eq!(f.helpers[0].finished_ms, crate::logs::transcript::iso_ms("2026-10-04T10:05:00.000Z").unwrap());
    assert!(!f.helpers[0].running());
    assert!(running_helpers(p).is_empty());
    // Read from where the launch isn't seen: the message takes it up, named by its meta file.
    let only = dir.join("s1-tail.jsonl");
    std::fs::create_dir_all(dir.join("s1-tail").join("subagents")).unwrap();
    std::fs::copy(sub.join("agent-a1.meta.json"), dir.join("s1-tail").join("subagents").join("agent-a1.meta.json")).unwrap();
    std::fs::copy(&agent_log, dir.join("s1-tail").join("subagents").join("agent-a1.jsonl")).unwrap();
    std::fs::write(&only, [
        r#"{"type":"assistant","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"SendMessage","input":{"to":"a1","summary":"Review the revised plan","message":"Please look again."}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"{\"success\":true}"}]},"toolUseResult":{"success":true,"message":"Resuming agent a1","revivedAgentId":"a1"}}"#,
    ].join("\n") + "\n").unwrap();
    let mut g = Feed::open(only.to_str().unwrap(), 1);
    g.catch_up();
    assert_eq!(g.helpers.len(), 1);
    let h = &g.helpers[0];
    assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.kind.as_str()), ("a1", "t2", "Review the plan", "general-purpose"));
    assert!(h.running());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A teammate launched in a log whose path names no agents' folder (not a `.jsonl`: no log or meta file of
/// its own to find) goes by its name, as its launch says.
#[test]
fn a_teammate_with_no_log_of_its_own_goes_by_its_name() {
    let dir = std::env::temp_dir().join(format!("cue-steps-teammate-nolog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s1.log");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-20T20:34:42.000Z","message":{"content":"start a teammate"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-20T20:34:44.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"name":"probe","prompt":"Reply with exactly the text: probe ok"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-20T20:34:45.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Spawned successfully."}]}]},"toolUseResult":{"status":"teammate_spawned","agentId":"aprobe-30f8","name":"probe"}}"#,
    ].join("\n") + "\n").unwrap();
    let mut f = Feed::open(path.to_str().unwrap(), 1);
    f.catch_up();
    assert_eq!(f.helpers.len(), 1);
    assert_eq!((f.helpers[0].desc.as_str(), f.helpers[0].name.as_str()), ("probe", "probe"));
    std::fs::remove_dir_all(&dir).ok();
}

/// A session `s` whose background agent "a1" (a fork, `kind`) wrote `lines` to its own log, its last one long ago, and
/// never sent its notice: the feed, caught up.
fn quiet_agent(name: &str, lines: &[&str]) -> Feed {
    let dir = std::env::temp_dir().join(format!("cue-steps-quiet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sub = dir.join("s").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"fork","description":"Review the change","toolUseId":"t1","requestShape":"background"}"#).unwrap();
    let mut log = vec![r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"review it"}}"#];
    log.extend_from_slice(lines);
    std::fs::write(sub.join("agent-a1.jsonl"), log.join("\n") + "\n").unwrap();
    let path = dir.join("s.jsonl");
    std::fs::write(&path, [
        r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"review the change"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Review the change","subagent_type":"fork","prompt":"review it","run_in_background":true}}]}}"#,
        r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Review the change"}}"#,
    ].join("\n") + "\n").unwrap();
    let mut f = Feed::open(path.to_str().unwrap(), 1);
    f.catch_up();
    f
}

#[test]
fn an_agent_quiet_for_long_whose_last_line_is_its_answer_finished_and_one_after_a_result_is_stuck() {
    // Its last line its own words, no notice, no end_turn (a fork's report, written long ago): done, not running.
    let f = quiet_agent("done", &[
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"h1","name":"Read","input":{"file_path":"/p/a.md"}}]}}"#,
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"text"}]}}"#,
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"Reviewed: 2 findings, both minor."}]}}"#,
    ]);
    let h = &f.helpers[0];
    assert!(!h.running(), "finished and lingering: done, not quiet");
    assert_eq!(h.quiet(crate::model::now_ms()), "");
    // Its last line a tool's result, and nothing after: stuck (quiet, nothing open).
    let f = quiet_agent("stuck", &[
        r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"h1","name":"Read","input":{"file_path":"/p/a.md"}}]}}"#,
        r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"text"}]}}"#,
    ]);
    let h = &f.helpers[0];
    assert!(h.running());
    assert!(h.stuck(crate::model::now_ms()) && h.open_step().is_empty(), "quiet with nothing open: stuck");
}
