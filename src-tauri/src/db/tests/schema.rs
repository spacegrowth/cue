//! Opening the database: older files brought up to date, an outside writer waited for, and the old files imported.

use super::*;
use crate::db::live::put_exchange;

#[test]
fn a_database_from_before_notes_ahead_keeps_its_hand_offs() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-v6-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    // Version 5's hand-offs table (no `ahead`), with one waiting to continue.
    {
        let c = Connection::open(path()).unwrap();
        c.execute_batch("CREATE TABLE handoffs (session_id TEXT PRIMARY KEY, skill TEXT NOT NULL, sent_ms INTEGER NOT NULL, next_session TEXT NOT NULL DEFAULT '', continued_ms INTEGER NOT NULL DEFAULT 0, resume INTEGER NOT NULL DEFAULT 1, note TEXT NOT NULL DEFAULT '', note_ms INTEGER NOT NULL DEFAULT 0);
            INSERT INTO handoffs (session_id, skill, sent_ms, resume, note, note_ms) VALUES ('s1', 'handoff', 100, 1, '/r/HANDOFF.md', 120);
            PRAGMA user_version = 5;").unwrap();
    }
    reset();
    let k = handoff("s1").unwrap();
    assert_eq!((k.skill.as_str(), k.note.as_str(), k.note_ms, k.ahead), ("handoff", "/r/HANDOFF.md", 120, 0), "kept, and not ahead");
    handoff_sent("s2", "cue:handoff", false, 2);
    assert_eq!(handoff("s2").map(|k| k.ahead), Some(2));
    ahead_watch("s2", true);
    assert_eq!(ahead_watched(), ["s2"]);
    // Opened again (a version 6 database): nothing to do, nothing lost.
    reset();
    assert_eq!(handoff("s2").map(|k| k.ahead), Some(2));
    std::env::remove_var("CUE_HOME");
    reset();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_write_waits_for_another_connections_write_instead_of_failing() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-busy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    set_draft("k", "before", &json!([]));
    // Another connection takes the write lock and keeps it for a moment.
    let other = Connection::open(path()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE; INSERT INTO drafts (key, text, images, updated_ms) VALUES ('other', 'x', '[]', 1);").unwrap();
    let let_go = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        other.execute_batch("COMMIT").unwrap();
    });
    set_draft("k", "after", &json!([]));
    let_go.join().unwrap();
    assert_eq!(draft_text("k"), "after", "saved once the other write was done, not dropped as locked");
    assert_eq!(draft_text("other"), "x");
    // A write held longer than BUSY_WAIT: given up after BUSY_WAIT (rusqlite's own default is 5 s,
    // all that time with DB locked for every other caller in Cue).
    let other = Connection::open(path()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let t0 = std::time::Instant::now();
    set_draft("k", "too late", &json!([]));
    let waited = t0.elapsed();
    other.execute_batch("ROLLBACK").unwrap();
    assert!(waited >= BUSY_WAIT && waited < BUSY_WAIT + std::time::Duration::from_secs(1), "{waited:?}");
    assert_eq!(draft_text("k"), "after");
    std::env::remove_var("CUE_HOME");
    reset();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_database_from_before_status_asks_gets_the_column_and_keeps_its_log() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-v7-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    // Version 7's exchanges table (no `status_ask`), with a message of yours said by voice.
    {
        let c = Connection::open(path()).unwrap();
        c.execute_batch("CREATE TABLE exchanges (session_id TEXT NOT NULL, at_ms INTEGER NOT NULL, role TEXT NOT NULL, text TEXT NOT NULL, images TEXT NOT NULL DEFAULT '[]', voice INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (session_id, at_ms, role));
            INSERT INTO exchanges (session_id, at_ms, role, text, voice) VALUES ('s', 10, 'you', 'hello', 1);
            PRAGMA user_version = 7;").unwrap();
    }
    reset();
    let old = session_log("s");
    assert_eq!((old[0]["text"].as_str(), old[0]["voice"] == true, old[0].get("status_ask")), (Some("hello"), true, None));
    let mut e = session("s", "").thread[0].clone();
    (e.role, e.at_ms, e.text, e.status_ask) = ("you".into(), 20, "next".into(), Some("the ask\n**Where we are**".into()));
    with(|c| put_exchange(c, "s", &e)).unwrap();
    assert_eq!(session_log("s")[1]["status_ask"], "the ask\n**Where we are**");
    std::env::remove_var("CUE_HOME");
    reset();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Saved sessions, answers and parked ones in a worktree move to its repository's project, once;
/// another machine's keep their folder's name.
#[test]
fn saved_projects_are_regrouped_by_repository() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-regroup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let git = |at: &std::path::Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(at).args(args).output().unwrap().status.success());
    let main = dir.join("app");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"]);
    git(&main, &["worktree", "add", "-q", "../wt/fix-it"]);
    let wt = dir.join("wt/fix-it").to_string_lossy().to_string();
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CUE_HOME", &home);
    reset();
    let data = |machine: &str| json!({ "cwd": wt, "machine": machine, "project": "fix-it" }).to_string();
    with(|c| {
        c.execute("INSERT INTO sessions (id, project, cwd, started_ms, updated_ms, data) VALUES ('here', 'fix-it', ?1, 1, 1, ?2)", params![wt, data("")])?;
        c.execute("INSERT INTO sessions (id, project, cwd, started_ms, updated_ms, data) VALUES ('there', 'fix-it', ?1, 1, 1, ?2)", params![wt, data("box")])?;
        c.execute("INSERT INTO items (id, kind, status, project, created_ms, data) VALUES ('i', 'waiting', 'answered', 'fix-it', 1, ?1)", params![data("")])?;
        c.execute("INSERT INTO parked (id, parked_ms, data) VALUES ('p', 1, ?1)", params![data("")])?;
        c.pragma_update(None, "user_version", 0)
    })
    .unwrap();
    reset(); // the next launch
    let row = |sql: &str| with(|c| c.query_row(sql, [], |r| r.get::<_, String>(0))).unwrap();
    assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM sessions WHERE id = 'here'"), "app app");
    assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM sessions WHERE id = 'there'"), "fix-it fix-it");
    assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM items WHERE id = 'i'"), "app app");
    assert_eq!(row("SELECT json_extract(data, '$.project') FROM parked WHERE id = 'p'"), "app");
    std::env::remove_var("CUE_HOME");
    reset();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_old_files_are_imported_once_and_kept_as_a_backup() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-imp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    // A line exactly as the old history.jsonl held it: one serialized Item.
    let it = Item {
        id: "h1".into(), kind: "permission".into(), origin: Origin { session_id: "s".into(), ..Default::default() }, project: "p".into(),
        tool_name: "Bash".into(), tool_input: json!({"command": "ls"}), suggestions: Value::Null, context: vec![], message: String::new(),
        created_ms: 5, status: "answered".into(), outcome: "allowed".into(), resolved_ms: Some(6), thread: vec![], tool_use_id: None, request_id: None, scan_from: 0, at_prompt: false, agent_id: String::new(), agent_name: String::new(), followup: String::new(), interrupted: false, back_ms: None, images: vec![],
    };
    std::fs::write(dir.join("history.jsonl"), format!("{}\n", serde_json::to_string(&it).unwrap())).unwrap();
    let h = history(100);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].outcome, "allowed");
    assert!(dir.join("history.jsonl.imported").exists() && !dir.join("history.jsonl").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_old_folder_moves_over_and_image_paths_follow() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("cue-move-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (old, new) = (root.join("old"), root.join("new"));
    std::fs::create_dir_all(old.join("uploads")).unwrap();
    std::fs::create_dir_all(old.join("bin")).unwrap();
    std::fs::write(old.join("uploads/1.png"), b"png").unwrap();
    std::fs::write(old.join("config.json"), "{}").unwrap();
    std::fs::write(old.join("bin/cue-hook"), "#!/bin/sh").unwrap();

    let moved = crate::server::move_from_old_folder(&old, &new).expect("moved");
    assert!(new.join("uploads/1.png").exists() && new.join("config.json").exists());
    assert!(old.join("bin/cue-hook").exists(), "forwarders stay for already-running sessions");
    assert_eq!(std::fs::read_link(old.join("cue.sock")).unwrap(), new.join("cue.sock"));

    std::env::set_var("CUE_HOME", &new);
    reset();
    let mut s = session("m", "");
    s.thread[0].images = vec![format!("{moved}/uploads/1.png")];
    sync_live(&[s], &[]);
    rewrite_paths(&format!("{moved}/"), &format!("{}/", new.display()));
    let (live, _) = live();
    assert_eq!(live[0].thread[0].images[0], format!("{}/uploads/1.png", new.display()));
    assert!(crate::server::move_from_old_folder(&old, &new).is_none(), "only once: the database is there now");
    std::fs::remove_dir_all(&root).unwrap();
}
