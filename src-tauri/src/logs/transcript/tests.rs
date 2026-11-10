//! Tests for reading transcripts: each writes a small log and reads it back.

use super::*;
use super::activity::{activity, describe_tool};
use super::turns::turn_ended;
use crate::model::{Ctx, TurnPart};
use serde_json::json;
use std::io::Write;

fn write_transcript(lines: &[Value]) -> tempfile_path::TempPath {
    let p = tempfile_path::TempPath::new();
    let mut f = File::create(&p.0).unwrap();
    for l in lines {
        writeln!(f, "{}", l).unwrap();
    }
    p
}

#[test]
fn the_tool_call_a_terminal_prompt_is_about_is_the_newest_without_a_result() {
    let call = |id: &str, cmd: &str| json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":cmd}}]}});
    let result = |id: &str| json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}});
    let p = write_transcript(&[call("a", "ls"), result("a"), call("b", "curl -sI https://example.com"), call("c", "echo x"), result("c")]);
    let (id, name, input) = waiting_tool_use(&p.0).unwrap();
    assert_eq!((id.as_str(), name.as_str(), input["command"].as_str().unwrap()), ("b", "Bash", "curl -sI https://example.com"));
    let done = write_transcript(&[call("a", "ls"), result("a")]);
    assert!(waiting_tool_use(&done.0).is_none(), "every call answered: nothing waiting");
}

#[test]
fn the_conversation_before_a_time_is_prompts_and_each_turns_last_reply() {
    let at = |m: u32| format!("2026-10-04T08:{m:02}:00.000Z");
    let p = write_transcript(&[
        json!({"type":"user","timestamp":at(1),"message":{"content":"fix the bar"}}),
        json!({"type":"assistant","timestamp":at(2),"message":{"content":[{"type":"text","text":"Looking at it."},{"type":"tool_use","name":"Read","input":{}}]}}),
        json!({"type":"user","timestamp":at(3),"message":{"content":[{"type":"tool_result","content":"…"}]}}),
        json!({"type":"assistant","timestamp":at(4),"message":{"content":[{"type":"text","text":"Fixed: the bar now fills."}]}}),
        json!({"type":"user","timestamp":at(5),"isMeta":true,"message":{"content":"<local-command-stdout>x</local-command-stdout>"}}),
        json!({"type":"user","timestamp":at(6),"message":{"content":"thanks, now commit"}}),
        json!({"type":"assistant","timestamp":at(7),"message":{"content":[{"type":"text","text":"Committed."}]}}),
        json!({"type":"user","timestamp":at(9),"message":{"content":"later message"}}),
    ]);
    let got = |before: &str, limit| conversation_before(&p.0, iso_ms(before).unwrap(), limit).iter().map(|e| format!("{}: {}", e["role"].as_str().unwrap(), e["text"].as_str().unwrap())).collect::<Vec<_>>();
    assert_eq!(got(&at(8), 10), ["you: fix the bar", "agent: Fixed: the bar now fills.", "you: thanks, now commit", "agent: Committed."]);
    assert_eq!(got(&at(8), 2), ["you: thanks, now commit", "agent: Committed."], "the last `limit` before that time");
    assert_eq!(got(&at(1), 10), Vec::<String>::new());
}

#[test]
fn a_message_said_by_voice_is_marked_by_the_line_its_hook_added() {
    let at = |m: u32| format!("2026-10-04T08:{m:02}:00.000Z");
    let added = |parent: &str, line: &str| json!({"type":"attachment","timestamp":at(2),"parentUuid":parent,"attachment":{"type":"hook_additional_context","content":[line],"hookName":"UserPromptSubmit","hookEvent":"UserPromptSubmit"}});
    let p = write_transcript(&[
        json!({"type":"user","uuid":"u1","timestamp":at(1),"message":{"content":"is the fix installed?"}}),
        added("u1", crate::talk::ASK),
        json!({"type":"assistant","timestamp":at(3),"message":{"content":[{"type":"text","text":"Yes."}]}}),
        json!({"type":"user","uuid":"u2","timestamp":at(4),"message":{"content":"now commit it"}}),
        added("u2", "some other hook's note"),
        json!({"type":"assistant","timestamp":at(5),"message":{"content":[{"type":"text","text":"Committed."}]}}),
    ]);
    let got = conversation_before(&p.0, iso_ms(&at(9)).unwrap(), 10);
    let voice: Vec<(&str, bool)> = got.iter().filter(|e| e["role"] == "you").map(|e| (e["text"].as_str().unwrap(), e["voice"] == true)).collect();
    assert_eq!(voice, [("is the fix installed?", true), ("now commit it", false)]);
}

#[test]
fn a_message_whose_hook_asked_for_a_status_list_carries_that_text() {
    let at = |m: u32| format!("2026-10-04T08:{m:02}:00.000Z");
    let added = |parent: &str, line: &str| json!({"type":"attachment","timestamp":at(2),"parentUuid":parent,"attachment":{"type":"hook_additional_context","content":[line],"hookName":"UserPromptSubmit","hookEvent":"UserPromptSubmit"}});
    let ask = "End this reply with the status list, exactly this shape:\n**Where we are**\n\u{2713} done";
    let recall = "Your status list from the end of your last reply:\n**Where we are**\n\u{25CB} next";
    let p = write_transcript(&[
        json!({"type":"user","uuid":"u1","timestamp":at(1),"message":{"content":"first"}}),
        added("u1", ask),
        json!({"type":"assistant","timestamp":at(3),"message":{"content":[{"type":"text","text":"Ok."}]}}),
        json!({"type":"user","uuid":"u2","timestamp":at(4),"message":{"content":"second"}}),
        added("u2", "some other hook's note"),
        json!({"type":"assistant","timestamp":at(5),"message":{"content":[{"type":"text","text":"Ok."}]}}),
        json!({"type":"user","uuid":"u3","timestamp":at(6),"message":{"content":"third, by voice"}}),
        added("u3", recall),
        added("u3", crate::talk::ASK),
    ]);
    let got = conversation_before(&p.0, iso_ms(&at(9)).unwrap(), 10);
    let you: Vec<(&str, Option<&str>, bool)> = got.iter().filter(|e| e["role"] == "you").map(|e| (e["text"].as_str().unwrap(), e["status_ask"].as_str(), e["voice"] == true)).collect();
    assert_eq!(you, [("first", Some(ask), false), ("second", None, false), ("third, by voice", Some(recall), true)]);
}

#[test]
fn the_status_asks_of_recent_prompts_are_read_from_the_transcript_tail() {
    let at = |m: u32| format!("2026-10-04T08:{m:02}:00.000Z");
    let added = |parent: &str, line: &str| json!({"type":"attachment","timestamp":at(2),"parentUuid":parent,"attachment":{"type":"hook_additional_context","content":[line]}});
    let p = write_transcript(&[
        json!({"type":"user","uuid":"u1","timestamp":at(1),"message":{"content":"fix the bar"}}),
        added("u1", "End this reply with the status list:\n**Where we are**"),
        json!({"type":"user","uuid":"u2","timestamp":at(4),"message":{"content":"and commit"}}),
        added("u2", "another hook's note"),
    ]);
    assert_eq!(status_asks(&p.0), [("fix the bar".to_string(), "End this reply with the status list:\n**Where we are**".to_string())]);
    assert!(status_asks("/no/such/file.jsonl").is_empty());
}

/// As Claude Code writes them into the lead's log (from a real lead and its teammate "reader").
fn from_teammate(body: &str, summary: &str) -> String {
    let summary = if summary.is_empty() { String::new() } else { format!(" summary=\"{summary}\"") };
    format!("Another Claude session sent a message:\n<teammate-message teammate_id=\"reader\" color=\"blue\"{summary}>\n{body}\n</teammate-message>\n\nThis came from another Claude session \u{2014} not typed by your user, but very likely working on their behalf.")
}

#[test]
fn a_teammates_message_is_read_whole_and_its_notices_only_when_they_carry_an_answer() {
    let said = |t: &str| teammate_messages(t).into_iter().map(|(w, s)| format!("{w}: {s}")).collect::<Vec<_>>();
    assert_eq!(said(&from_teammate("PROBE-SHORT: hello from reader", "Short probe reply to team-lead")), ["reader: PROBE-SHORT: hello from reader"]);
    let long = std::iter::once("PROBE-LONG: a multi-line message from reader.".to_string())
        .chain((1..=40).map(|n| format!("Line {n}: the quick brown fox jumps over the lazy dog, checking long messages arrive whole.")))
        .chain(std::iter::once("PROBE-LONG-END".to_string())).collect::<Vec<_>>().join("\n");
    let got = teammate_messages(&from_teammate(&long, "Long multi-line probe reply"));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1, long, "every line of a long one");
    // Its idle notice: shown only for the answer in it.
    let idle = |extra: &str| from_teammate(&format!(r#"{{"type":"idle_notification","from":"reader","timestamp":"2026-10-24T01:03:09.000Z","idleReason":"available"{extra}}}"#), "");
    assert_eq!(said(&idle(r#","result":"I sent \"hello\" to team-lead.""#)), ["reader: I sent \"hello\" to team-lead."]);
    assert!(said(&idle("")).is_empty(), "just idle: nothing to show");
    assert!(said(&from_teammate(r#"{"type":"shutdown_request","request_id":"x"}"#, "")).is_empty(), "Claude Code's own notice");
    // Not a teammate's: yours, or another session's.
    assert!(said("what does <teammate-message teammate_id=\"x\">hi</teammate-message> mean?").is_empty());
    assert!(said("Another Claude session sent a message:\n<cross-session-message from=\"uds:/tmp/s.sock\">\nhi\n</cross-session-message>").is_empty());
}

#[test]
fn a_leads_log_gives_its_teammates_messages_after_a_time_and_its_history_shows_them_as_theirs() {
    let at = |s: u32| format!("2026-10-24T01:03:{s:02}.000Z");
    let p = write_transcript(&[
        json!({"type":"user","timestamp":at(1),"origin":{"kind":"human"},"message":{"role":"user","content":"Start one teammate named reader"}}),
        json!({"type":"assistant","timestamp":at(2),"message":{"content":[{"type":"text","text":"The reader teammate is running."}]}}),
        json!({"type":"user","timestamp":at(3),"message":{"role":"user","content":from_teammate("PROBE-SHORT: hello from reader", "Short")}}),
        json!({"type":"assistant","timestamp":at(4),"message":{"content":[{"type":"text","text":"The short one arrived."}]}}),
        json!({"type":"user","timestamp":at(5),"isSidechain":true,"message":{"role":"user","content":from_teammate("not the lead's", "")}}),
        json!({"type":"user","timestamp":at(6),"message":{"role":"user","content":from_teammate("second message\nwith two lines", "Long")}}),
        json!({"type":"user","timestamp":at(7),"message":{"role":"user","content":from_teammate(r#"{"type":"idle_notification","from":"reader","idleReason":"available"}"#, "")}}),
        json!({"type":"assistant","timestamp":at(8),"message":{"content":[{"type":"text","text":"All lines arrived."}]}}),
    ]);
    let after = |s: u32| teammate_messages_after(&p.0, iso_ms(&at(s)).unwrap()).into_iter().map(|(t, w, m)| (t, format!("{w}: {m}"))).collect::<Vec<_>>();
    assert_eq!(after(0), [(iso_ms(&at(3)).unwrap(), "reader: PROBE-SHORT: hello from reader".to_string()), (iso_ms(&at(6)).unwrap(), "reader: second message\nwith two lines".to_string())]);
    assert_eq!(after(3).len(), 1, "only what came after");
    assert!(after(6).is_empty());
    let got = conversation_before(&p.0, iso_ms(&at(9)).unwrap(), 20).iter().map(|e| format!("{} {}: {}", e["role"].as_str().unwrap(), e["from"].as_str().unwrap_or(""), e["text"].as_str().unwrap())).collect::<Vec<_>>();
    assert_eq!(got, ["you : Start one teammate named reader", "agent : The reader teammate is running.", "peer reader: PROBE-SHORT: hello from reader", "agent : The short one arrived.", "peer reader: second message\nwith two lines", "agent : All lines arrived."]);
}

#[test]
fn an_interrupt_counts_only_while_nothing_follows_it() {
    let stop = json!({"type":"user","timestamp":"2026-10-04T19:47:22.000Z","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}});
    let work = json!({"type":"assistant","timestamp":"2026-10-04T19:47:20.000Z","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}});
    let p = write_transcript(&[work.clone(), stop.clone(), json!({"type":"file-history-snapshot"})]);
    assert_eq!(interrupted_at(&p.0), iso_ms("2026-10-04T19:47:22.000Z"));
    // You said something after it (or it went on): no longer interrupted.
    let p = write_transcript(&[work.clone(), stop.clone(), json!({"type":"user","timestamp":"2026-10-04T19:50:27.000Z","message":{"content":"continue"}})]);
    assert_eq!(interrupted_at(&p.0), None);
    let p = write_transcript(&[work]);
    assert_eq!(interrupted_at(&p.0), None);
}

#[test]
fn follows_a_queued_message_through_the_queue_log() {
    let op = |op: &str, c: Option<&str>| match c {
        Some(c) => json!({"type":"queue-operation","operation":op,"content":c}),
        None => json!({"type":"queue-operation","operation":op}),
    };
    let p = write_transcript(&[op("enqueue", Some("is it done?"))]);
    assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Waiting));
    // Taken into the running turn between steps: Esc now would interrupt the turn that has it.
    let p = write_transcript(&[op("enqueue", Some("is it done?")), json!({"type":"queue-operation","operation":"remove","content":"is it done?","reason":"absorbed_mid_turn"})]);
    assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Absorbed));
    // dequeue takes the oldest: here someone else's first, then ours.
    let p = write_transcript(&[op("enqueue", Some("<task-notification>")), op("enqueue", Some("is it done? /tmp/a.png")), op("dequeue", None)]);
    assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Waiting));
    let p = write_transcript(&[op("enqueue", Some("<task-notification>")), op("enqueue", Some("is it done? /tmp/a.png")), op("dequeue", None), op("dequeue", None)]);
    assert_eq!(queued_state(&p.0, "is it done?"), Some(Queued::Taken(None)));
    assert_eq!(queued_state(&p.0, "something else"), None);
    // A long paste comes back with its spacing changed: still the same message.
    let long = format!("first line\n\n{}\tend", "word ".repeat(1200));
    let p = write_transcript(&[op("enqueue", Some(&long.replace('\n', "\r\n").replace('\t', "    "))), json!({"type":"queue-operation","operation":"remove","content":long.replace('\n', "\r\n").replace('\t', "    "),"reason":"absorbed_mid_turn"})]);
    assert_eq!(queued_state(&p.0, &long), Some(Queued::Absorbed));
    // Or wrapped as a paste, as Claude Code logs a message of several lines.
    let wrapped = format!("\n\n<pasted_content id=\"1\">\n{long}\n</pasted_content id=\"1\">\n");
    let p = write_transcript(&[op("enqueue", Some(&wrapped)), op("dequeue", None)]);
    assert_eq!(queued_state(&p.0, &long), Some(Queued::Taken(None)));
}

#[test]
fn a_prompt_is_found_in_the_log_as_sent_even_wrapped_as_a_paste() {
    let at = |s: u32| format!("2026-10-22T18:08:{s:02}.000Z");
    let t0 = iso_ms(&at(50)).unwrap();
    let long = format!("you can make the change, but after that\n\ncheck why {}\n\nWant me to do that now?", "the message did not show ".repeat(20));
    let wrapped = format!("\n\n<pasted_content id=\"673a\">\n{long}\n</pasted_content id=\"673a\">\n");
    let p = write_transcript(&[
        json!({"type":"user","timestamp":at(10),"message":{"content":long}}),
        json!({"type":"user","timestamp":at(57),"isSidechain":false,"message":{"content":wrapped}}),
    ]);
    assert!(prompt_since(&p.0, &long, t0), "the paste wrapper is taken off before comparing");
    assert!(!prompt_since(&p.0, "something else", t0));
    let p = write_transcript(&[json!({"type":"user","timestamp":at(10),"message":{"content":long}})]);
    assert!(!prompt_since(&p.0, &long, t0), "an older copy of the same words isn't this send");
    let p = write_transcript(&[json!({"type":"user","timestamp":at(57),"isSidechain":true,"message":{"content":wrapped}})]);
    assert!(!prompt_since(&p.0, &long, t0), "a background agent's turn isn't the session's");
}

#[test]
fn a_message_handed_over_mid_turn_is_found_in_the_log_as_sent() {
    let at = |s: u32| format!("2026-10-23T09:00:{s:02}.000Z");
    let t0 = iso_ms(&at(30)).unwrap();
    let long = format!("also check the tests\n\n{}\n\nthen stop", "and the docs ".repeat(40));
    let wrapped = format!("\n\n<pasted_content id=\"7\">\n{long}\n</pasted_content id=\"7\">\n");
    let queued = |ts: String, prompt: &str, origin: Option<Value>| {
        let mut a = json!({"type":"queued_command","commandMode":"prompt","prompt":prompt});
        if let Some(o) = origin {
            a["origin"] = o;
        }
        json!({"type":"attachment","timestamp":ts,"attachment":a})
    };
    let p = write_transcript(&[json!({"type":"assistant","timestamp":at(31),"message":{"content":[{"type":"text","text":"Working."}]}}), queued(at(40), &wrapped, None)]);
    assert!(taken_mid_turn(&p.0, &long, t0), "the wrapped prompt, compared as sent");
    assert!(!taken_mid_turn(&p.0, "something else", t0));
    let p = write_transcript(&[queued(at(40), &long, Some(json!({"kind":"human"})))]);
    assert!(taken_mid_turn(&p.0, &long, t0), "yours, said so");
    let p = write_transcript(&[queued(at(10), &long, None)]);
    assert!(!taken_mid_turn(&p.0, &long, t0), "from before this send");
    let p = write_transcript(&[queued(at(40), "<task-notification>agents reported</task-notification>", None)]);
    assert!(!taken_mid_turn(&p.0, "<task-notification>agents reported</task-notification>", t0), "Claude Code's own notice");
    let p = write_transcript(&[queued(at(40), &long, Some(json!({"kind":"peer","name":"helper"})))]);
    assert!(!taken_mid_turn(&p.0, &long, t0), "another agent's message, not yours");
    let p = write_transcript(&[json!({"type":"attachment","timestamp":at(40),"attachment":{"type":"queued_command","commandMode":"bash","prompt":long}})]);
    assert!(!taken_mid_turn(&p.0, &long, t0), "a command, not a prompt");
}

#[test]
fn follows_a_message_queued_for_codex_through_its_rollout() {
    let ev = |t: &str, kind: &str, msg: Option<&str>| {
        let mut p = json!({"type": kind});
        if let Some(m) = msg { p["message"] = json!(m); }
        json!({"timestamp": format!("2026-10-05T06:{t}Z"), "type":"event_msg", "payload": p})
    };
    let sent = iso_ms("2026-10-05T06:40:00.000Z").unwrap();
    let start = [ev("39:00.000", "task_started", None), ev("39:00.010", "user_message", Some("fix it")), ev("39:30.000", "agent_message", Some("on it"))];
    let p = write_transcript(&start);
    assert_eq!(read_state("codex", &p.0, "fix it", sent), Some(Queued::Waiting), "the same words, before you sent this one");
    // Steered into the running turn.
    let mut l = start.to_vec();
    l.push(ev("40:01.000", "user_message", Some("fix it\n")));
    assert_eq!(read_state("codex", &write_transcript(&l).0, "fix it", sent), Some(Queued::Absorbed));
    // Or taken as the next turn's prompt.
    let mut l = start.to_vec();
    l.extend([ev("40:05.000", "task_complete", None), ev("40:05.100", "task_started", None), ev("40:05.200", "token_count", None), ev("40:05.300", "user_message", Some("fix it"))]);
    assert_eq!(read_state("codex", &write_transcript(&l).0, "fix it", sent), Some(Queued::Taken(iso_ms("2026-10-05T06:40:05.300Z"))));
    assert_eq!(read_state("pi", &p.0, "fix it", sent), None);
}

#[test]
fn a_turn_has_ended_only_when_nothing_follows_its_turn_duration() {
    let reply = json!({"type":"assistant","timestamp":"2026-10-04T13:35:00.000Z","message":{"content":[{"type":"text","text":"Done: header moved."}]}});
    let end = json!({"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T13:35:01.000Z"});
    let p = write_transcript(&[json!({"type":"user","message":{"content":"move the header"}}), reply.clone(), end.clone()]);
    assert_eq!(turn_ended(&p.0), Some((iso_ms("2026-10-04T13:35:01.000Z").unwrap(), "Done: header moved.".to_string())));
    // A Stop hook sent it back to work, or you sent the next message: not ended.
    let p = write_transcript(&[reply.clone(), end.clone(), json!({"type":"user","message":{"content":"next"}})]);
    assert_eq!(turn_ended(&p.0), None);
    let p = write_transcript(&[json!({"type":"user","message":{"content":"go"}}), reply.clone()]);
    assert_eq!(turn_ended(&p.0), None);
    // Commands you ran in Claude Code since (/login, /model, a resume's notes) start no turn: still ended.
    let caveat = json!({"type":"user","isMeta":true,"message":{"content":"<local-command-caveat>The command below was run directly in Claude Code</local-command-caveat>"}});
    let login = json!({"type":"user","message":{"content":"<command-name>/login</command-name>\n<command-message>login</command-message>"}});
    let out = json!({"type":"user","message":{"content":[{"type":"text","text":"<local-command-stdout>Login interrupted</local-command-stdout>"}]}});
    let p = write_transcript(&[reply.clone(), end.clone(), caveat, login.clone(), out]);
    assert_eq!(turn_ended(&p.0).map(|(at, _)| at), iso_ms("2026-10-04T13:35:01.000Z"), "still the turn that ended");
    // One that sets it to work (a custom command it answers) is a new turn.
    let p = write_transcript(&[reply.clone(), end, login, reply]);
    assert_eq!(turn_ended(&p.0), None);
}

#[test]
fn a_turn_read_from_the_transcript_marks_what_came_after_a_stop_hook() {
    let say = |at: &str, text: &str| json!({"type":"assistant","timestamp":at,"message":{"content":[{"type":"text","text":text}]}});
    let p = write_transcript(&[
        json!({"type":"user","message":{"content":"move the header"}}),
        say("2026-10-04T13:35:00.000Z", "Done: header moved."),
        json!({"type":"user","isMeta":true,"message":{"content":"Stop hook feedback:\nAnswer the checklist"}}),
        say("2026-10-04T13:35:20.000Z", "1. SHAPE: fine."),
        json!({"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T13:35:21.000Z"}),
    ]);
    let (at, reply, parts) = turn_ended_parts(&p.0).unwrap();
    assert_eq!((at, reply.as_str()), (iso_ms("2026-10-04T13:35:21.000Z").unwrap(), "1. SHAPE: fine."));
    assert_eq!(parts, vec![TurnPart { text: "Done: header moved.".into(), hooked: false }, TurnPart { text: "1. SHAPE: fine.".into(), hooked: true }]);
    assert_eq!(crate::hub::split_turn(reply, &parts, "answer"), ("Done: header moved.".to_string(), "1. SHAPE: fine.".to_string()), "the answer is the card; the checklist folds");
}

#[test]
fn compact_is_finished_once_claude_code_reports_it() {
    let at = |t: &str| format!("2026-10-05T13:{t}Z");
    let typed = json!({"type":"user","timestamp":at("02:30.869"),"message":{"content":"<command-name>/compact</command-name>\n<command-message>compact</command-message>"}});
    let since = iso_ms(&at("02:30.500")).unwrap();
    let p = write_transcript(&[json!({"type":"user","timestamp":at("01:00.000"),"message":{"content":"<local-command-stdout>an older one</local-command-stdout>"}}), typed.clone()]);
    assert!(!compact_finished(&p.0, since), "typed, still summarizing");
    // One it started itself has no command, only the boundary when it's done.
    let p2 = write_transcript(&[json!({"type":"system","subtype":"compact_boundary","timestamp":at("03:04.877")})]);
    assert!(compact_finished(&p2.0, since));
    let done = json!({"type":"user","timestamp":at("03:04.976"),"message":{"content":"<local-command-stdout>Compacted (ctrl+o to see full summary)</local-command-stdout>"}});
    let p = write_transcript(&[typed.clone(), json!({"type":"system","subtype":"compact_boundary","timestamp":at("03:04.877")}), done]);
    assert!(compact_finished(&p.0, since));
    // It couldn't compact: that's the end of it too.
    let p = write_transcript(&[typed, json!({"type":"user","timestamp":at("02:31.000"),"message":{"content":"<local-command-stderr>Error: Not enough messages to compact.</local-command-stderr>"}})]);
    assert!(compact_finished(&p.0, since));
}

// Tiny self-cleaning temp file so tests need no extra crate.
mod tempfile_path {
    pub struct TempPath(pub String);
    impl TempPath {
        pub fn new() -> Self {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Self(format!("{}/cue-test-{}-{}.jsonl", std::env::temp_dir().display(), std::process::id(), n))
        }
    }
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

fn tool_use(id: &str, cmd: &str) -> Value {
    json!({"type":"assistant","message":{"content":[
        {"type":"text","text":"Let me run it."},
        {"type":"tool_use","id":id,"name":"Bash","input":{"command":cmd,"description":"d"}}]}})
}
fn tool_result(id: &str) -> Value {
    json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}})
}

#[test]
fn matches_the_right_tool_use_and_its_result() {
    let input = json!({"command":"touch b","description":"d"});
    let t = write_transcript(&[tool_use("t1", "touch a"), tool_result("t1"), tool_use("t2", "touch b")]);
    let id = find_tool_use_id(&t.0, 0, "Bash", &input).unwrap();
    assert_eq!(id, "t2");
    assert!(!has_result(&t.0, 0, "t2"), "t2 not answered yet");
    assert!(has_result(&t.0, 0, "t1"), "t1 answered");

    let mut f = std::fs::OpenOptions::new().append(true).open(&t.0).unwrap();
    writeln!(f, "{}", tool_result("t2")).unwrap();
    assert!(has_result(&t.0, 0, "t2"), "t2 answered after result appended");
}

#[test]
fn no_match_when_input_differs() {
    let t = write_transcript(&[tool_use("t1", "touch a")]);
    assert!(find_tool_use_id(&t.0, 0, "Bash", &json!({"command":"touch zzz","description":"d"})).is_none());
}

#[test]
fn context_keeps_prose_drops_tool_noise_and_meta() {
    let t = write_transcript(&[
        json!({"type":"user","message":{"content":"please fix the bug"}}),
        json!({"type":"user","isMeta":true,"message":{"content":"meta stuff"}}),
        json!({"type":"user","message":{"content":"<command-name>/model</command-name>"}}),
        tool_use("t1", "touch a"),
        tool_result("t1"),
    ]);
    let c = recent_context(&t.0, 5);
    assert_eq!(
        c,
        vec![
            Ctx { role: "user".into(), text: "please fix the bug".into() },
            Ctx { role: "assistant".into(), text: "Let me run it.".into() },
        ]
    );
}

#[test]
fn tells_who_sent_each_prompt() {
    let t = write_transcript(&[
        json!({"type":"user","promptId":"p1","origin":{"kind":"human"},"message":{"content":"fix it"}}),
        json!({"type":"user","promptId":"p2","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>…"}}),
        json!({"type":"user","promptId":"p3","message":{"content":"from an older Claude"}}),
        json!({"type":"user","promptId":"p4","origin":{"kind":"peer","from":"uds:/tmp/cc-socks/37929.sock","verifiedPeerPid":37929,"name":"cue-3e","body":"done, go ahead"},"message":{"content":"…"}}),
        json!({"type":"user","promptId":"p5","origin":{"kind":"peer","from":"a1","body":"report","handback":true},"message":{"content":"…"}}),
        // The tool results after a prompt share its promptId but carry no origin.
        json!({"type":"user","promptId":"p4","message":{"content":[{"type":"tool_result","tool_use_id":"t9","content":"ok"}]}}),
    ]);
    assert_eq!(prompt_sender(&t.0, "p1"), Some(Sender::Human));
    assert_eq!(prompt_sender(&t.0, "p2"), Some(Sender::Harness));
    assert_eq!(prompt_sender(&t.0, "p3"), Some(Sender::Human));
    assert_eq!(prompt_sender(&t.0, "p4"), Some(Sender::Peer { name: "cue-3e".into(), pid: Some(37929), body: "done, go ahead".into() }));
    assert_eq!(prompt_sender(&t.0, "p5"), Some(Sender::Harness));
    assert_eq!(prompt_sender(&t.0, "not-written-yet"), None);
}

#[test]
fn about_reads_the_last_prompt() {
    let t = write_transcript(&[
        json!({"type":"ai-title","aiTitle":"Old title","sessionId":"s"}),
        json!({"type":"user","message":{"content":"first ask"}}),
        json!({"type":"last-prompt","lastPrompt":"first ask","sessionId":"s"}),
        json!({"type":"ai-title","aiTitle":"Agent decision notification system","sessionId":"s"}),
        json!({"type":"last-prompt","lastPrompt":"see it looks odd on sides of keyboard top","sessionId":"s"}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"\"type\":\"ai-title\" mentioned in prose"}]}}),
    ]);
    assert_eq!(about(&t.0).as_deref(), Some("see it looks odd on sides of keyboard top"));
    let empty = write_transcript(&[json!({"type":"user","message":{"content":"hi"}})]);
    assert_eq!(about(&empty.0).as_deref(), Some(""));
}

#[test]
fn a_task_notification_handed_over_from_the_queue_is_not_yours() {
    // A background task finished while the session was busy: no prompt entry, an attachment instead.
    let note = "<task-notification>\n<task-id>bgkpah4ly</task-id>\n<status>completed</status>\n</task-notification>";
    let t = write_transcript(&[
        json!({"type":"queue-operation","operation":"enqueue","content":note}),
        json!({"type":"queue-operation","operation":"remove","content":note}),
        json!({"type":"attachment","attachment":{"type":"queued_command","prompt":note,"commandMode":"task-notification","origin":{"kind":"task-notification","producer":"session-task"}}}),
        json!({"type":"attachment","attachment":{"type":"queued_command","prompt":"also check the logs","commandMode":"prompt","origin":{"kind":"human"}}}),
    ]);
    assert_eq!(prompt_sender(&t.0, "p-hook"), None, "no prompt entry for it");
    assert!(queued_from_harness(&t.0, note));
    assert!(queued_from_harness(&t.0, &format!("{note}\n")), "the hook's copy may differ in trailing space");
    assert!(!queued_from_harness(&t.0, "also check the logs"), "something you queued while it worked is yours");
    assert!(!queued_from_harness(&t.0, "never queued"));
    assert!(!queued_from_harness(&t.0, ""));
}

#[test]
fn says_what_a_working_session_is_doing() {
    let tool = |name: &str, input: Value| json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":name,"input":input}]}});
    let t = write_transcript(&[
        json!({"type":"user","message":{"content":"fix it"}}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"## Looking at the hub first\nthen the UI"}]}}),
        tool("Bash", json!({"command":"cd src-tauri && cargo test\n"})),
    ]);
    assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Running: cd src-tauri && cargo test"));
    let t = write_transcript(&[tool("Edit", json!({"file_path":"/x/ui/app.js"}))]);
    assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Editing app.js"));
    // Its words aren't shown ("It works live" isn't what it's doing): just that it's writing.
    let t = write_transcript(&[json!({"type":"assistant","message":{"content":[{"type":"text","text":"It works live."}]}})]);
    assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Writing…"));
    // A step that finished: it's thinking about the next one, not still running the last.
    let t = write_transcript(&[tool("Bash", json!({"command":"cargo test"})), json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}})]);
    assert_eq!(activity(&t.0).map(|a| a.0).as_deref(), Some("Thinking…"));
    assert_eq!(iso_ms("2026-10-04T14:08:39.507Z"), Some(1_791_122_919_507));
    assert_eq!(describe_tool(&json!({"name":"mcp__claude-in-chrome__navigate"})), "Using claude-in-chrome: navigate");
    // Just after a new prompt: nothing from the last turn.
    let t = write_transcript(&[tool("Bash", json!({"command":"old"})), json!({"type":"user","message":{"content":"next task"}})]);
    assert_eq!(activity(&t.0), None);
}

#[test]
fn says_what_a_codex_session_is_doing() {
    let call = |name: &str, args: Value| json!({"type":"response_item","payload":{"type":"function_call","name":name,"arguments":args.to_string()}});
    let t = write_transcript(&[
        json!({"type":"event_msg","payload":{"type":"user_message","message":"fix it"}}),
        call("exec_command", json!({"cmd":"rg --files\n","workdir":"/x"})),
    ]);
    assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Running: rg --files"));
    let t = write_transcript(&[call("shell", json!({"command":["bash","-lc","cargo test"]}))]);
    assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Running: cargo test"));
    let t = write_transcript(&[json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /x/src/hub.rs\n@@"}})]);
    assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Editing hub.rs"));
    let t = write_transcript(&[call("exec_command", json!({"cmd":"ls"})), json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"ok"}})]);
    assert_eq!(activity_of("codex", &t.0).map(|a| a.0).as_deref(), Some("Thinking…"));
    // A new turn: nothing from the last one.
    let t = write_transcript(&[call("exec_command", json!({"cmd":"old"})), json!({"type":"event_msg","payload":{"type":"task_started"}})]);
    assert_eq!(activity_of("codex", &t.0), None);
}

#[test]
fn counts_commands_that_ran() {
    let t = write_transcript(&[
        json!({"type":"system","subtype":"local_command","content":"<command-name>/rename</command-name>\n<command-args>cue-2</command-args>"}),
        json!({"type":"system","subtype":"local_command","content":"<command-name>/rename</command-name>\n<command-args>cue-ios</command-args>"}),
        // Claude Code 2.1.29x logs a local command as a "user" entry with the same record (and
        // a caveat/stdout entry around it that must not count).
        json!({"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>The command below was run directly in Claude Code</local-command-caveat>"}}),
        json!({"type":"user","message":{"role":"user","content":"<command-name>/rename</command-name>\n            <command-message>rename</command-message>\n            <command-args>cue-3</command-args>"}}),
        json!({"type":"user","message":{"role":"user","content":"<local-command-stdout>Renamed to cue-3</local-command-stdout>"}}),
    ]);
    assert_eq!(command_count(&t.0, "rename"), 3);
    assert_eq!(command_count(&t.0, "reomte"), 0);
    assert_eq!(command_count(&t.0, "local"), 0, "the caveat/stdout wrappers aren't commands");
    // /compact: what you typed is logged as it starts.
    let t = write_transcript(&[json!({"type":"user","message":{"role":"user","content":"/compact"}}), json!({"type":"user","message":{"content":"/compacting is a word"}})]);
    assert_eq!(command_count(&t.0, "compact"), 1);
}

#[test]
fn reading_mid_file_drops_the_partial_line() {
    let t = write_transcript(&[tool_use("t1", "touch a"), tool_result("t1")]);
    // Offset 5 lands inside line 1: it must be skipped, not mis-parsed.
    assert!(find_tool_use_id(&t.0, 5, "Bash", &json!({"command":"touch a","description":"d"})).is_none());
    assert!(has_result(&t.0, 5, "t1"));
}

#[test]
fn a_task_notification_is_never_yours() {
    assert!(harness_text("<task-notification>\n<task-id>ba2xv14y3</task-id>\n<status>failed</status>\n</task-notification>"));
    assert!(harness_text("  \n<task-notification><task-id>x</task-id></task-notification>"));
    assert!(!harness_text("why is <task-notification> showing in Cue?"));
}

#[test]
fn a_stop_hook_s_feedback_is_the_harness_s_not_yours_even_without_its_meta_flag() {
    assert!(harness_text("Stop hook feedback:\nUnaudited edits this turn. Read rules-check.md and answer every line."));
    assert!(!harness_text("the Stop hook feedback: showed in the chat"));
    // As Claude Code 2.1.310 writes it (a plain string, `isMeta`), and the same entry with the flag gone.
    let at = |m: u32| format!("2026-10-26T01:{m:02}:00.000Z");
    let hook = |meta: bool| json!({"type":"user","isMeta":meta,"promptId":"p1","timestamp":at(2),"message":{"role":"user","content":"Stop hook feedback:\nUnaudited edits this turn. Read rules-check.md and answer every line."}});
    for meta in [true, false] {
        let p = write_transcript(&[
            json!({"type":"user","promptId":"p1","timestamp":at(1),"message":{"content":"make the chart taller"}}),
            json!({"type":"assistant","timestamp":at(2),"message":{"content":[{"type":"text","text":"Done. The chart is taller."}]}}),
            hook(meta),
            json!({"type":"assistant","timestamp":at(3),"message":{"content":[{"type":"text","text":"1. Shape: one file.\n2. N/A."}]}}),
            json!({"type":"user","timestamp":at(5),"message":{"content":"thanks"}}),
        ]);
        let got = conversation_before(&p.0, iso_ms(&at(9)).unwrap(), 10).iter().map(|e| format!("{}: {}", e["role"].as_str().unwrap(), e["text"].as_str().unwrap())).collect::<Vec<_>>();
        assert_eq!(got, vec!["you: make the chart taller", "agent: 1. Shape: one file.\n2. N/A.", "you: thanks"], "isMeta {meta}: the hook's text is no message of yours");
        let ctx = recent_context(&p.0, 10).iter().map(|c| format!("{}: {}", c.role, c.text)).collect::<Vec<_>>();
        assert!(!ctx.iter().any(|c| c.contains("Stop hook feedback")), "isMeta {meta}: {ctx:?}");
        assert_eq!(ctx.iter().filter(|c| c.starts_with("user: ")).count(), 2, "isMeta {meta}: {ctx:?}");
    }
}
