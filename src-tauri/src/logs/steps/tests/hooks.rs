//! Tests: step lines, and a turn a Stop hook sent back to work, read the same by every reader.

use super::*;

#[test]
fn steps_read_as_lines_with_how_they_went() {
    let f = feed_of(T);
    assert_eq!(f.turns.len(), 1);
    let items = &f.turns[0].items;
    assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking at it."));
    assert_eq!(step(&items[1]), ("Read", "token.ts", "84 lines", false, true));
    assert_eq!(step(&items[2]), ("Edited", "token.ts", "+2 −1", false, true));
    assert_eq!(step(&items[3]), ("Ran", "npm test", "exit 1", true, true));
    // Still running: the present-tense verb, no result. The helper's words aren't here.
    assert_eq!(step(&items[4]), ("Running", "npm test", "", false, false));
    assert_eq!(items.len(), 5);
}

/// A turn a Stop hook sent back to work, as Claude Code writes it: the hook's entry is a meta one
/// (`isMeta`), its content a plain string. The first line stands in for the partial line a tail read drops.
const HOOKED: &str = r#"{"type":"summary","summary":"(the line a tail read skips)"}
{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"role":"user","content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Done: expiry fixed."}]}}
{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-10-04T10:00:02.500Z","hookCount":1}
{"type":"user","isMeta":true,"timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"Stop hook feedback:\n[bash rules-check-stop.sh]: Unaudited edits this turn. Read rules-check.md"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat rules.md"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"1. SHAPE: fine."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}
"#;

#[test]
fn a_stop_hooks_follow_up_stays_in_the_turn_it_continues_marked_from_where_the_hook_sent_it_back() {
    let mut log = HOOKED.to_string();
    log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:30.000Z","message":{"content":"and the refresh token?"}}"#);
    let f = feed_of(&log);
    assert_eq!(f.turns.iter().map(|t| t.prompt.as_str()).collect::<Vec<_>>(), ["fix the token expiry", "and the refresh token?"], "the hook's entry opens no turn");
    let t = &f.turns[0];
    assert_eq!(t.items.len(), 3, "answer, the hook's step, its checklist: all one turn");
    assert_eq!(t.hooked_ms, stamp(&serde_json::json!({"timestamp": "2026-10-04T10:00:03.000Z"})), "marked where the hook sent it back");
    let at = |i: &Item| match i { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms };
    let after: Vec<bool> = t.items.iter().map(|i| at(i) >= t.hooked_ms).collect();
    assert_eq!(after, [false, true, true], "the answer before it; the hook's step and checklist after");
    assert_eq!(step(&t.items[1]), ("Ran", "cat rules.md", "ok", false, true));
    assert_eq!(f.turns[1].hooked_ms, 0, "the next turn starts unhooked");
    // Sent to the window as hooked_ms, left out when there's none.
    assert!(serde_json::to_value(t).unwrap().get("hooked_ms").is_some() && serde_json::to_value(&f.turns[1]).unwrap().get("hooked_ms").is_none());
}

#[test]
fn a_hook_read_before_its_turns_start_still_marks_what_follows() {
    // Opened mid-file (the last 4 MB): the hook's entry is the first thing read.
    let log = HOOKED.lines().skip(4).collect::<Vec<_>>().join("\n");
    let f = feed_of(&log);
    assert_eq!(f.turns.len(), 1);
    assert!(f.turns[0].hooked_ms > 0, "the turn it opens is hooked");
    assert!(f.turns[0].items.iter().all(|i| match i { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms >= f.turns[0].hooked_ms }), "all it reads after is the hook's");
}

#[test]
fn a_background_task_reporting_after_the_hook_starts_unhooked_work() {
    let mut log = HOOKED.replace(r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}"#, "");
    log.push_str(r#"{"type":"user","timestamp":"2026-10-04T10:00:20.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n</task-notification>"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:22.000Z","message":{"content":[{"type":"text","text":"The agent reports: all three paths agree."}]}}
"#);
    let f = feed_of(&log);
    assert_eq!(f.turns.len(), 2, "the report starts new work");
    assert!(f.turns[0].hooked_ms > 0 && f.turns[1].hooked_ms == 0, "the hook's part stays with the turn it came in; the new work is an answer again");
}

/// You wrote to it while the hook's follow-up ran: what it does for your message is an answer again, in
/// every reader (it was folded under "After a stop hook", and the card kept the reply from before the hook).
#[test]
fn your_message_after_the_hook_starts_unhooked_work_in_every_reader() {
    let mut log = HOOKED.replace(r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:07.000Z","durationMs":7000}"#, "");
    log.push_str(r#"{"type":"attachment","timestamp":"2026-10-04T10:00:06.500Z","attachment":{"type":"queued_command","prompt":"also check the hand-off","commandMode":"prompt","origin":{"kind":"human"},"humanTurn":true}}
{"type":"assistant","timestamp":"2026-10-04T10:00:09.000Z","message":{"content":[{"type":"text","text":"The hand-off note is found now."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-10-04T10:00:10.000Z","durationMs":10000}
"#);
    let f = feed_of(&log);
    assert_eq!(f.turns.len(), 2, "your message starts new work");
    assert!(f.turns[0].hooked_ms > 0 && f.turns[1].hooked_ms == 0);
    assert!(matches!(f.turns[1].items.last(), Some(Item::Say { text, .. }) if text == "The hand-off note is found now."));
    let path = std::env::temp_dir().join(format!("cue-hooked-yours-{}.jsonl", std::process::id()));
    std::fs::write(&path, &log).unwrap();
    let p = path.to_string_lossy().to_string();
    let (_, last, parts) = crate::logs::transcript::turn_ended_parts(&p).expect("the turn ended");
    let card = crate::hub::split_turn(last, &parts, "answer");
    let hook = crate::hook::final_turn(&serde_json::json!({ "transcript_path": p, "last_assistant_message": "The hand-off note is found now." }));
    let parts: Vec<crate::model::TurnPart> = hook.iter().map(|v| crate::model::TurnPart { text: v["text"].as_str().unwrap_or("").to_string(), hooked: v["hooked"].as_bool().unwrap_or(false) }).collect();
    let stop = crate::hub::split_turn("The hand-off note is found now.".into(), &parts, "answer");
    let _ = std::fs::remove_file(&path);
    assert_eq!(card.0, "The hand-off note is found now.", "a turn read from the transcript");
    assert_eq!(stop.0, "The hand-off note is found now.", "Cue's Stop hook");
    // A task's notice queued the same way isn't yours.
    assert!(!crate::logs::transcript::yours_mid_turn(&serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","prompt":"<task-notification>x</task-notification>","commandMode":"task-notification"}})));
}

/// Every reader of a hooked turn agrees on what's the answer and what's the hook's follow-up: the steps
/// feed (the chat), the turn's parts read from the transcript (a card Cue learns of after a restart),
/// and Cue's Stop hook (a card as the turn ends). One transcript, three readers, one split.
#[test]
fn every_reader_of_a_hooked_turn_splits_it_the_same_way() {
    let path = std::env::temp_dir().join(format!("cue-hooked-{}.jsonl", std::process::id()));
    std::fs::write(&path, HOOKED).unwrap();
    let p = path.to_string_lossy().to_string();
    // The steps feed: the last words before the mark are the answer, the words after it the follow-up.
    let f = feed_of(HOOKED);
    let t = &f.turns[0];
    let says = |hooked: bool| t.items.iter().filter_map(|i| match i { Item::Say { text, at_ms } if (*at_ms >= t.hooked_ms) == hooked => Some(text.clone()), _ => None }).collect::<Vec<_>>();
    let feed = (says(false).last().cloned().unwrap_or_default(), says(true).join("\n\n"));
    // The transcript's parts, through the card's split.
    let (_, last, parts) = crate::logs::transcript::turn_ended_parts(&p).expect("the turn ended");
    let card = crate::hub::split_turn(last, &parts, "answer");
    // Cue's Stop hook, as it ends the turn.
    let hook = crate::hook::final_turn(&serde_json::json!({ "transcript_path": p, "last_assistant_message": "1. SHAPE: fine." }));
    let parts: Vec<crate::model::TurnPart> = hook.iter().map(|v| crate::model::TurnPart { text: v["text"].as_str().unwrap_or("").to_string(), hooked: v["hooked"].as_bool().unwrap_or(false) }).collect();
    let stop = crate::hub::split_turn("1. SHAPE: fine.".into(), &parts, "answer");
    let _ = std::fs::remove_file(&path);
    let want = ("Done: expiry fixed.".to_string(), "1. SHAPE: fine.".to_string());
    assert_eq!(feed, want, "the steps feed");
    assert_eq!(card, want, "a turn read from the transcript");
    assert_eq!(stop, want, "Cue's Stop hook");
}

/// Resumed after a turn was cut off, Claude Code writes "No response requested." in the model's place: not a
/// reply, so no reader shows it. Its usage-limit notice is synthetic too, and stays.
#[test]
fn claude_codes_placeholder_reply_is_not_a_reply() {
    let filler = serde_json::json!({"type":"assistant","isApiErrorMessage":false,"message":{"model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}});
    let limit = serde_json::json!({"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit"}]}});
    let real = serde_json::json!({"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}]}});
    assert!(crate::logs::transcript::placeholder_reply(&filler));
    assert!(!crate::logs::transcript::placeholder_reply(&limit));
    assert!(!crate::logs::transcript::placeholder_reply(&real));
    // The steps feed (the chat): a cut-off turn, resumed, then your next message and its real answer.
    let log = [
        r#"{"type":"summary","summary":"(the line a tail read skips)"}"#,
        r#"{"type":"user","timestamp":"2026-11-08T03:42:30.000Z","message":{"role":"user","content":"build it"}}"#,
        r#"{"type":"assistant","timestamp":"2026-11-08T03:42:38.643Z","isApiErrorMessage":false,"message":{"model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}}"#,
        r#"{"type":"user","timestamp":"2026-11-08T03:42:38.768Z","message":{"role":"user","content":"why did the name change"}}"#,
        r#"{"type":"assistant","timestamp":"2026-11-08T03:42:45.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"A new process each time."}]}}"#,
    ]
    .join("\n");
    let f = feed_of(&format!("{log}\n"));
    let said: Vec<String> = f.turns.iter().flat_map(|t| t.items.iter()).filter_map(|i| match i { Item::Say { text, .. } => Some(text.clone()), _ => None }).collect();
    assert_eq!(said, vec!["A new process each time.".to_string()]);
}
