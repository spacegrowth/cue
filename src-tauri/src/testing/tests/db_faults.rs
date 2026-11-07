//! The database failing under Cue (stability plan §4, db / history, failure injection): another program
//! holding it, a file that isn't a database, a disk that's full, a folder it can't be made in. Cue keeps
//! working from memory through each, and saves again once it can.

use crate::testing::World;
use rusqlite::Connection;
use serde_json::json;
use std::time::{Duration, Instant};

/// Another program's connection to the World's database (sqlite3 in a terminal, another Cue).
fn another_program(w: &World) -> Connection {
    Connection::open(w.home().join("cue.db")).unwrap()
}

/// Session `sid`'s turn finished: its card's id.
fn finished_turn(w: &World, sid: &str) -> String {
    w.session(sid).prompt("fix it").stop("Fixed.");
    w.snapshot().card(sid)["id"].as_str().unwrap().to_string()
}

fn reply(w: &World, id: &str) {
    assert_eq!(w.reply(id, "thanks")["ok"], true);
}

/// Session `sid`'s turn finished and its card replied to: the card's id.
fn reply_to_a_finished_turn(w: &World, sid: &str) -> String {
    let id = finished_turn(w, sid);
    reply(w, &id);
    id
}

fn in_history_shown(w: &World, id: &str) -> bool {
    w.snapshot().0["history"].as_array().is_some_and(|h| h.iter().any(|i| i["id"] == id))
}

fn saved(id: &str) -> bool {
    crate::db::history(50).iter().any(|i| i.id == id)
}

/// Another program writing to the database holds a reply's save up for the busy wait (a second) and no
/// longer: the reply is answered in the window all the same, and the next save after the other write lands.
#[test]
fn a_reply_while_another_program_writes_waits_a_second_at_most() {
    let w = World::new();
    let id = finished_turn(&w, "a");
    let other = another_program(&w);
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    // The reply as its connection waits for it; the draw after it (the window's worker's, in the app) apart.
    let t0 = Instant::now();
    let told = w.client().ask(json!({"type": "reply", "id": id, "text": "thanks"}));
    let took = t0.elapsed();
    w.settle();
    assert_eq!(told["ok"], true);
    other.execute_batch("ROLLBACK").unwrap();
    assert!(took < Duration::from_secs(2), "held up {took:?}");
    assert!(in_history_shown(&w, &id), "answered in the window all the same");
    let next = reply_to_a_finished_turn(&w, "b");
    assert!(saved(&next), "the next save, the other write done, lands");
}

/// Reading never waits on another program's write (write-ahead logging): it sees what was saved before it.
#[test]
fn reading_goes_on_while_another_program_writes() {
    let w = World::new();
    crate::db::set_draft("s:a", "saved before", &json!([]));
    let other = another_program(&w);
    other.execute_batch("BEGIN IMMEDIATE; INSERT INTO drafts (key, text, images, updated_ms) VALUES ('s:b', 'not yet', '[]', 1);").unwrap();
    let t0 = Instant::now();
    let drafts = crate::db::drafts();
    assert!(t0.elapsed() < Duration::from_millis(500), "a read waited {:?}", t0.elapsed());
    other.execute_batch("COMMIT").unwrap();
    assert_eq!(drafts["s:a"]["text"], "saved before");
    assert!(drafts.get("s:b").is_none(), "an uncommitted write isn't read");
}

/// `cue.db`, and its write-ahead log, overwritten with what isn't a database, the way a bad copy or a
/// disk fault leaves it.
fn garbage(w: &World, with_log: bool) -> Vec<u8> {
    crate::db::reset();
    let junk: Vec<u8> = (0..8192u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::write(w.home().join("cue.db"), &junk).unwrap();
    let _ = std::fs::remove_file(w.home().join("cue.db-shm"));
    if with_log {
        std::fs::write(w.home().join("cue.db-wal"), b"not a log").unwrap();
    } else {
        let _ = std::fs::remove_file(w.home().join("cue.db-wal"));
    }
    junk
}

/// The files beside the database whose name says they were set aside.
fn set_aside(w: &World) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(w.home()).unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".unreadable-")).collect();
    names.sort();
    names
}

/// A database file that isn't one is set aside, untouched, for a person to look at, and a new one started:
/// what Cue saves from then on is kept.
#[test]
fn a_database_that_cant_be_read_is_kept_aside_and_a_new_one_started() {
    let w = World::new();
    let junk = garbage(&w, false);
    crate::db::set_draft("s:a", "after", &json!([]));
    assert_eq!(crate::db::draft_text("s:a"), "after", "saved in the new database");
    let aside = set_aside(&w);
    assert_eq!(aside.len(), 1, "{aside:?}");
    assert!(aside[0].starts_with("cue.db.unreadable-"));
    assert_eq!(std::fs::read(w.home().join(&aside[0])).unwrap(), junk, "kept as it was");
    let id = reply_to_a_finished_turn(&w, "a");
    assert!(saved(&id), "a reply after it is saved");
}

/// Its write-ahead log is never replayed into the new database: it goes aside with it, or SQLite has
/// already let it go, and the new database reopens as it was saved.
#[test]
fn an_unreadable_databases_log_is_never_read_into_the_new_one() {
    let w = World::new();
    garbage(&w, true);
    crate::db::set_draft("s:a", "after", &json!([]));
    let log = std::fs::read(w.home().join("cue.db-wal")).unwrap_or_default();
    assert!(!log.starts_with(b"not a log"), "the old log beside the new database");
    crate::db::reset();
    assert_eq!(crate::db::draft_text("s:a"), "after", "reopened, the new database is read");
}

/// An empty file is a new database, not a broken one: nothing is set aside.
#[test]
fn an_empty_database_file_is_simply_a_new_database() {
    let w = World::new();
    crate::db::reset();
    std::fs::write(w.home().join("cue.db"), b"").unwrap();
    crate::db::set_draft("s:a", "fine", &json!([]));
    assert_eq!(crate::db::draft_text("s:a"), "fine");
    assert!(set_aside(&w).is_empty());
}

/// The disk full, as SQLite meets it: the database can't grow past its pages now (`max_page_count`).
/// None lifts it again.
fn disk_full(pages: Option<i64>) {
    let cap = match pages {
        Some(p) => p,
        None => crate::db::raw(|c| c.pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))).unwrap(),
    };
    crate::db::raw(|c| c.query_row(&format!("PRAGMA max_page_count = {cap}"), [], |_| Ok(()))).unwrap();
}

/// With the disk full a reply is still answered and shown in History (from memory); saves start again
/// once there's room, and what was saved before is intact.
#[test]
fn a_full_disk_loses_the_saves_not_the_answers() {
    let w = World::new();
    let before = reply_to_a_finished_turn(&w, "a");
    crate::db::set_draft("s:x", "kept", &json!([]));
    disk_full(None);
    crate::db::set_draft("s:x", &"big ".repeat(50_000), &json!([]));
    let during = reply_to_a_finished_turn(&w, "b");
    assert!(in_history_shown(&w, &during), "answered and shown");
    assert_eq!(crate::db::draft_text("s:x"), "kept", "a save that didn't fit changed nothing");
    disk_full(Some(1_073_741_823));
    let after = reply_to_a_finished_turn(&w, "c");
    assert!(saved(&before) && saved(&after), "saved before, and again once there's room");
    crate::db::reset();
    assert_eq!(crate::db::draft_text("s:x"), "kept", "reopened intact");
}

/// A data folder where the database can't even be made (a folder in its place): every save is skipped and
/// Cue runs from memory, cards, replies and History alike.
#[test]
fn without_a_database_cue_runs_from_memory() {
    let w = World::new();
    crate::db::reset();
    for f in ["cue.db", "cue.db-wal", "cue.db-shm"] {
        let _ = std::fs::remove_file(w.home().join(f));
    }
    std::fs::create_dir(w.home().join("cue.db")).unwrap();
    let id = reply_to_a_finished_turn(&w, "a");
    assert!(in_history_shown(&w, &id), "History from memory");
    assert!(!saved(&id), "nothing saved");
    assert_eq!(crate::db::draft_text("s:a"), "", "a read is empty, not an error");
}
