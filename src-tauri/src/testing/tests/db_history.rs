//! The database as it grows up and fills (stability plan §4, db / history): a database from each older
//! version brought up to date with what it kept, History trimmed to its setting but never today's answers,
//! and saved paths following the data folder when it moves.

use crate::testing::World;
use rusqlite::{params, Connection};
use serde_json::json;

/// The columns each version added (db/schema.rs `open_at`), as (version, table, column).
const ADDED: [(i64, &str, &str); 10] = [
    (3, "turns", "looks"),
    (3, "turns", "agents"),
    (3, "turns", "relay"),
    (3, "turns", "areas"),
    (4, "handoffs", "note"),
    (4, "handoffs", "note_ms"),
    (5, "turns", "auto_compact"),
    (6, "handoffs", "ahead"),
    (7, "exchanges", "voice"),
    (8, "exchanges", "status_ask"),
];
const LATEST: i64 = 8;

/// The World's database file, opened beside Cue's (Cue's own connection let go first).
fn outside(w: &World) -> Connection {
    crate::db::reset();
    Connection::open(w.home().join("cue.db")).unwrap()
}

/// The World's database turned into one from `version`: the columns later versions added taken out, its
/// version said, and a row in each table a migration touches.
fn database_at(w: &World, version: i64) {
    let c = outside(w);
    for table in ["turns", "handoffs", "exchanges"] {
        let later: Vec<&str> = ADDED.iter().filter(|(v, t, _)| *v > version && *t == table).map(|(.., col)| *col).collect();
        without_columns(&c, table, &later);
    }
    c.execute_batch(
        "INSERT INTO turns (session_id, at_ms) VALUES ('s', 1);
         INSERT INTO turns_read (path, offset, size, mtime_ms) VALUES ('/log', 1, 1, 1);
         INSERT INTO handoffs (session_id, skill, sent_ms) VALUES ('h', 'handoff', 5);
         INSERT INTO exchanges (session_id, at_ms, role, text) VALUES ('x', 1, 'you', 'hi');",
    )
    .unwrap();
    c.pragma_update(None, "user_version", version).unwrap();
}

/// `table` made again without `columns` (SQLite's DROP COLUMN can't edit the commented schema), its rows kept.
fn without_columns(c: &Connection, table: &str, columns: &[&str]) {
    if columns.is_empty() {
        return;
    }
    let mut q = c.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap();
    let all: Vec<String> = q.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let kept: Vec<&str> = all.iter().map(String::as_str).filter(|n| !columns.contains(n)).collect();
    c.execute_batch(&format!("CREATE TABLE older AS SELECT {} FROM {table}; DROP TABLE {table}; ALTER TABLE older RENAME TO {table};", kept.join(", "))).unwrap();
}

/// What Cue's own connection sees once it opens the database again: its version, its columns, and how many
/// rows each table has.
struct Opened {
    version: i64,
    missing: Vec<String>,
    rows: Vec<(&'static str, i64)>,
}

fn opened() -> Opened {
    crate::db::reset();
    crate::db::raw(|c| {
        let version = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let has = |table: &str, column: &str| -> bool { c.prepare(&format!("SELECT {column} FROM {table}")).is_ok() };
        let missing = ADDED.iter().filter(|(_, t, col)| !has(t, col)).map(|(_, t, col)| format!("{t}.{col}")).collect();
        let mut rows = vec![];
        for table in ["turns", "turns_read", "handoffs", "exchanges"] {
            rows.push((table, c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?));
        }
        Ok(Opened { version, missing, rows })
    })
    .expect("the database opens")
}

#[test]
fn a_database_from_each_older_version_is_brought_up_to_the_latest() {
    for version in 0..LATEST {
        let w = World::new();
        database_at(&w, version);
        let o = opened();
        assert_eq!(o.version, LATEST, "from version {version}");
        assert!(o.missing.is_empty(), "from version {version}, columns missing: {:?}", o.missing);
        drop(w);
    }
}

/// Turns read before version 3 are read again from the logs (they lack what later turns hold); from
/// version 3 they're kept, and before version 5 only where each log was read up to is forgotten.
/// Hand-offs and the conversation log are never touched.
#[test]
fn older_turns_are_read_again_and_everything_else_is_kept() {
    for (version, turns, read) in [(0, 0, 0), (2, 0, 0), (3, 1, 0), (4, 1, 0), (5, 1, 1), (7, 1, 1)] {
        let w = World::new();
        database_at(&w, version);
        let o = opened();
        assert_eq!(o.rows, [("turns", turns), ("turns_read", read), ("handoffs", 1), ("exchanges", 1)], "from version {version}");
        drop(w);
    }
}

/// A database a newer Cue wrote (a downgrade): opened as it is, its version left alone, and still written to.
#[test]
fn a_database_from_a_newer_cue_is_used_as_it_is() {
    let w = World::new();
    outside(&w).pragma_update(None, "user_version", LATEST + 1).unwrap();
    assert_eq!(opened().version, LATEST + 1);
    crate::db::set_draft("s:a", "still saved", &json!([]));
    assert_eq!(crate::db::draft_text("s:a"), "still saved");
}

/// An answered card as the database keeps it.
fn answered(id: &str, resolved_ms: u64, images: &[String]) -> crate::model::Item {
    serde_json::from_value(json!({
        "id": id, "kind": "waiting", "session_id": "s", "cwd": "/work/s", "project": "s", "tool_name": "", "tool_input": null,
        "suggestions": null, "context": [], "message": "done", "created_ms": resolved_ms, "status": "answered",
        "outcome": "replied", "resolved_ms": resolved_ms, "images": images,
    }))
    .unwrap()
}

fn answered_rows() -> i64 {
    crate::db::raw(|c| c.query_row("SELECT COUNT(*) FROM items WHERE status != 'pending'", [], |r| r.get(0))).unwrap()
}

/// History keeps the newest `keep` answers, and every answer of the last day besides (today's numbers count
/// them all); older ones beyond `keep` are deleted.
#[test]
fn history_keeps_the_newest_answers_and_all_of_todays() {
    let _w = World::new();
    let now = crate::clock::now_ms();
    let two_days_ago = now - 2 * crate::db::DAY_MS;
    for i in 0..12 {
        crate::db::save_answered(&answered(&format!("old{i:02}"), two_days_ago + i, &[]));
    }
    for i in 0..8 {
        crate::db::save_answered(&answered(&format!("new{i}"), now - 1000 + i, &[]));
    }
    let shown: Vec<String> = crate::db::history(5).into_iter().map(|i| i.id).collect();
    assert_eq!(shown, ["new7", "new6", "new5", "new4", "new3"], "the newest five, newest first");
    assert_eq!(answered_rows(), 8, "every one of today's kept, every older one past the newest five gone");
    crate::db::history(10);
    assert_eq!(answered_rows(), 8, "the two older ones that made the newest ten were already gone: nothing comes back");
}

/// A card still waiting is never trimmed, however old.
#[test]
fn a_waiting_card_is_never_trimmed() {
    let _w = World::new();
    let mut waiting = answered("waiting", crate::clock::now_ms() - 30 * crate::db::DAY_MS, &[]);
    waiting.status = "pending".into();
    waiting.resolved_ms = None;
    crate::db::sync_live(&[], &[&waiting]);
    for i in 0..20 {
        crate::db::save_answered(&answered(&format!("a{i}"), crate::clock::now_ms() - 3 * crate::db::DAY_MS + i, &[]));
    }
    crate::db::history(10);
    let waiting = crate::db::raw(|c| c.query_row("SELECT COUNT(*) FROM items WHERE id = 'waiting' AND status = 'pending'", [], |r| r.get::<_, i64>(0))).unwrap();
    assert_eq!(waiting, 1, "still saved");
    assert_eq!(answered_rows(), 10);
}

/// Settings → History can't keep fewer than ten.
#[test]
fn history_keeps_at_least_ten_whatever_the_setting_says() {
    let w = World::new();
    std::fs::write(w.home().join("config.json"), r#"{"history": {"keep": 3}}"#).unwrap();
    crate::config::refresh();
    assert_eq!(crate::config::history_keep(), 10);
    assert_eq!(crate::config::set("history.keep", json!(2)), Ok(()));
    assert_eq!(crate::config::load()["history"]["keep"], 10, "saved as ten");
}

/// Once the data folder moved, every saved path that pointed into the old one points into the new one:
/// answered cards, live sessions' chats, the conversation log, parked sessions and unsent drafts. A path
/// elsewhere is left alone.
#[test]
fn every_saved_path_follows_the_data_folder_when_it_moves() {
    let w = World::new();
    let (old, new) = ("/Users/me/.cue/", "/Users/me/Library/Application Support/cue/");
    let img = |base: &str| format!("{base}uploads/1.png");
    crate::db::save_answered(&answered("a", crate::clock::now_ms(), &[img(old), "/elsewhere/uploads/1.png".into()]));
    w.session("p").prompt("look at this").stop("Looked.");
    let hub = w.hub().clone();
    hub.add_parked(hub.session_record("p").unwrap());
    let c = outside(&w);
    c.execute("UPDATE parked SET data = json_set(data, '$.prompt', ?1)", params![img(old)]).unwrap();
    c.execute("INSERT INTO exchanges (session_id, at_ms, role, text, images) VALUES ('x', 1, 'you', ?1, ?2)", params![format!("see {}", img(old)), json!([img(old)]).to_string()]).unwrap();
    drop(c);
    crate::db::set_draft("s:p", "draft", &json!([img(old)]));
    crate::db::rewrite_paths(old, new);
    let item = crate::db::history(50).into_iter().find(|i| i.id == "a").unwrap();
    assert_eq!(item.images, [img(new), "/elsewhere/uploads/1.png".into()], "an answered card's, and only the old folder's");
    let log = crate::db::raw(|c| c.query_row("SELECT text || images FROM exchanges WHERE session_id = 'x'", [], |r| r.get::<_, String>(0))).unwrap();
    assert!(!log.contains(old) && log.matches(new).count() == 2, "the conversation log's text and images: {log}");
    let parked = crate::db::raw(|c| c.query_row("SELECT data FROM parked", [], |r| r.get::<_, String>(0))).unwrap();
    assert!(!parked.contains(old) && parked.contains(&img(new)), "a parked session's: {parked}");
    assert_eq!(crate::db::drafts()["s:p"]["images"], json!([img(new)]), "an unsent draft's");
}
