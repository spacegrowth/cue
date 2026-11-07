//! What the database keeps: live sessions, their log, search, drafts and commands; opening it is in `schema`.

use super::search::snippet;
use super::*;
use crate::model::{now_ms, Exchange, Item, Origin};
use crate::sessions::Session;
use serde_json::{json, Value};

mod busy;
mod schema;

fn session(id: &str, driven_by: &str) -> Session {
    let mut s: Session = serde_json::from_value(json!({
        "session_id": id, "harness": "claude", "cwd": "/x/proj", "project": "proj", "state": "waiting",
        "driven_by": driven_by, "queued": null, "since_ms": now_ms(), "prompt": "", "segments": [], "thread": []
    }))
    .unwrap();
    s.thread = vec![Exchange { role: "agent".into(), text: "done".into(), at_ms: 1, images: vec![], from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![], voice: false, decision: false, status_ask: None }];
    s
}

#[test]
fn snippet_centres_on_the_match() {
    let long = format!("{} needle here {}", "a ".repeat(100), "b ".repeat(100));
    let s = snippet(&long, "NEEDLE");
    assert!(s.starts_with('…') && s.ends_with('…') && s.contains("needle here"), "{s}");
    assert_eq!(snippet("short text", "text"), "short text");
}

#[test]
fn a_session_named_what_you_typed_is_found_however_many_newer_ones_mention_it() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    with(|c| {
        // The oldest is named exactly "web"; 25 newer ones only have "web" somewhere in their names.
        let add = |id: &str, name: &str, at: i64| c.execute("INSERT INTO sessions (id, project, started_ms, updated_ms, ended_ms, data) VALUES (?1, 'x', 1, ?2, ?2, ?3)", params![id, at, json!({ "name": name }).to_string()]);
        add("exact", "web", 1)?;
        add("start", "web-shop", 2)?;
        for n in 0..25 {
            add(&format!("in{n}"), &format!("my-cobweb-{n}"), 100 + n)?;
        }
        Ok(())
    })
    .unwrap();
    let found: Vec<String> = search("Web", 10).iter().filter(|r| r["kind"] == "session").map(|r| r["session_id"].as_str().unwrap().to_string()).collect();
    std::env::remove_var("CUE_HOME");
    reset();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(found.first().map(String::as_str), Some("exact"), "the whole name first: {found:?}");
    assert_eq!(found.get(1).map(String::as_str), Some("start"), "then one it starts: {found:?}");
    assert_eq!(found.len(), 20);
}

#[test]
fn delete_takes_everything_of_that_session_and_nothing_of_another() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-delete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    with(|c| {
        for id in ["gone", "kept"] {
            c.execute("INSERT INTO sessions (id, project, started_ms, updated_ms, ended_ms, data) VALUES (?1, 'x', 1, 2, 2, '{}')", params![id])?;
            c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES (?1, 1, 'you', 'hi')", params![id])?;
            c.execute("INSERT INTO items (id, session_id, kind, status, created_ms, data) VALUES (?1, ?2, 'waiting', 'answered', 1, '{}')", params![format!("i-{id}"), id])?;
            c.execute("INSERT INTO parked (id, parked_ms, data) VALUES (?1, 1, '{}')", params![id])?;
            c.execute("INSERT INTO drafts (key, text, updated_ms) VALUES (?1, 'draft', 1)", params![format!("s:{id}")])?;
        }
        Ok(())
    })
    .unwrap();
    delete_session("gone");
    let count = |id: &str| -> i64 {
        with(|c| {
            c.query_row(
                "SELECT (SELECT COUNT(*) FROM sessions WHERE id = ?1) + (SELECT COUNT(*) FROM exchanges WHERE session_id = ?1) + (SELECT COUNT(*) FROM items WHERE session_id = ?1) + (SELECT COUNT(*) FROM parked WHERE id = ?1) + (SELECT COUNT(*) FROM drafts WHERE key = 's:' || ?1)",
                params![id],
                |r| r.get(0),
            )
        })
        .unwrap_or(-1)
    };
    let (gone, kept) = (count("gone"), count("kept"));
    std::env::remove_var("CUE_HOME");
    reset();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(gone, 0);
    assert_eq!(kept, 5);
}

#[test]
fn the_log_comes_a_page_at_a_time_oldest_first() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-page-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    with(|c| {
        for n in 1..=7 {
            c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES ('s', ?1, ?2, ?3)", params![n * 10, if n % 2 == 1 { "you" } else { "agent" }, format!("m{n}")])?;
        }
        c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES ('other', 15, 'you', 'x')", [])?;
        Ok(())
    })
    .unwrap();
    let texts = |v: Vec<Value>| v.iter().map(|e| e["text"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    // The chat shows m6, m7: the page before m6 is the 3 just before it, oldest first.
    assert_eq!(texts(session_log_page("s", 60, 3)), ["m3", "m4", "m5"]);
    assert_eq!(texts(session_log_page("s", 30, 3)), ["m1", "m2"], "fewer than asked: the start");
    assert!(session_log_page("s", 10, 3).is_empty());
    std::env::remove_var("CUE_HOME");
    reset();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_folders_commands_are_kept_with_when_they_were_read_and_kept_when_out_of_date() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-cmds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    let list = vec![crate::commands::Cmd { name: "compact".into(), kind: "command".into(), desc: "Summarize".into(), hint: String::new(), args: vec![] }];
    assert!(commands("/x/cue").is_none());
    set_commands("/x/cue", 1_000, &list);
    reset(); // a relaunch: read from the file, not from memory
    assert_eq!(commands("/x/cue"), Some((1_000, list.clone())));
    set_commands("/x/cue", 2_000, &[]);
    assert_eq!(commands("/x/cue"), Some((2_000, vec![])), "the newer read wins");
    set_commands("/x/other", 3_000, &list);
    stale_commands(Some("/x/cue"));
    assert_eq!(commands("/x/cue"), Some((0, vec![])), "out of date, still there to show");
    assert_eq!(commands("/x/other"), Some((3_000, list.clone())), "only that folder");
    stale_commands(None);
    assert_eq!(commands("/x/other"), Some((0, list.clone())));
    std::env::remove_var("CUE_HOME");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn live_sessions_cards_log_and_drafts_survive_a_reopen() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();

    let card = Item {
        id: "w1".into(), kind: "waiting".into(), origin: Origin { session_id: "lead".into(), ..Default::default() }, project: "proj".into(),
        tool_name: String::new(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: "done".into(),
        created_ms: 1, status: "pending".into(), outcome: String::new(), resolved_ms: None, thread: vec![], tool_use_id: None, scan_from: 0, at_prompt: false, agent_id: String::new(), agent_name: String::new(), followup: String::new(), interrupted: false, back_ms: None, images: vec![],
    };
    sync_live(&[session("lead", ""), session("exec", "its relay lead")], &[&card]);
    set_draft("s:lead", "half a thought", &json!([]));

    reset(); // as if Cue restarted
    let (sessions, waiting) = live();
    assert_eq!(sessions.len(), 2);
    assert_eq!(waiting.len(), 1, "the finished-turn card is back");
    assert_eq!(drafts()["s:lead"]["text"], "half a thought");
    assert_eq!(exchange_count("exec"), 1);

    // The executor exits: it's marked ended, not deleted, and its log stays.
    sync_live(&[session("lead", "")], &[]);
    let (sessions, waiting) = live();
    assert_eq!(sessions.len(), 1);
    assert!(waiting.is_empty(), "a card that left memory leaves the database");
    assert_eq!(exchange_count("exec"), 1, "the ended session's conversation is kept");
    let driver: String = with(|c| c.query_row("SELECT driven_by FROM sessions WHERE id = 'exec'", [], |r| r.get(0))).unwrap();
    assert_eq!(driver, "its relay lead");

    set_draft("s:lead", "", &json!([]));
    assert!(drafts().get("s:lead").is_none(), "an emptied draft is removed");
    std::fs::remove_dir_all(&dir).unwrap();
}
