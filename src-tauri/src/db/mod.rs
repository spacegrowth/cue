//! Everything Cue keeps lives in one SQLite file, cue.db in Cue's data folder: answered history, live sessions
//! (so a restart or crash doesn't forget them), every session's full conversation, and your
//! unsent drafts. Settings stay in config.json (the Pi extension and hooks read it directly) and
//! images stay as files in uploads/ (rows hold their paths).
//!
//! Sessions are never deleted, only marked ended, and `exchanges` is an append-only log of every
//! session's back-and-forth (the window shows the last few). Both are there for the agents that
//! drive other agents (a lead and its helpers): who led whom, and what each one said.
//!
//! Here: the one connection and how every call runs on it. Opening it and its tables are in `schema`; the
//! queries live by area in `items`, `live`, `search`, `drafts`, `commands`, `handoffs` and `insights`.

mod commands;
mod drafts;
mod handoffs;
mod insights;
mod items;
mod live;
mod schema;
#[cfg(test)]
pub use self::schema::tests_seam;
mod search;

pub use self::commands::{commands, set_commands, all_commands, stale_commands};
pub use self::drafts::{draft_text, drafts, set_draft, review_queue, queue_review, unqueue_review, set_aside, add_set_aside};
pub use self::handoffs::{handoff_sent, handoff_drop, ahead_watch, ahead_watched, handoff_noted, handoff_continued, Handoff, handoff, handoffs, handoff_counts};
pub use self::insights::{insight_event, insight_counts, turns_read, TurnsOf, save_turns, StoredTurn, turns_since, turns_of, auto_compacts, start_contexts};
#[cfg(test)]
pub use self::insights::insight_events_of;
pub use self::items::{save_answered, history, DAY_MS, answers_since};
#[cfg(test)]
pub use self::live::sync_live;
pub use self::live::{sync_live_with, save_live, LiveSave, save_ended, live, park, parked, unpark, closed, ended_session, delete_session};
pub use self::search::{search, known_session_ids, session_log, session_log_page};

#[cfg(test)]
use self::handoffs::{AHEAD_WATCHED, HANDOFFS};
use self::schema::open;
use rusqlite::{params, Connection};
use std::sync::Mutex;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// The longest a statement waits for another connection's write to finish. Half the two seconds after
/// which a wait for a lock is reported (locking/mod.rs): threads queued on DB behind such a wait get it well
/// before then, so an outside write never reads as a lock bug in locks.log.
const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

pub(crate) fn path() -> std::path::PathBuf {
    crate::server::cue_dir().join("cue.db")
}

/// Run `f` on the open database. A database that can't be opened is logged once and every call
/// becomes a no-op: Cue keeps working from memory rather than refusing to start. A long hold is counted
/// by the line that asked (`track_caller`, locking::stats).
#[track_caller]
fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    let mut guard = crate::locking::lock(&DB, "DB");
    if guard.is_none() {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        *guard = match open() {
            Ok(c) => Some(c),
            Err(e) => {
                if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("cue: can't open {}: {e}", path().display());
                }
                return None;
            }
        };
    }
    match f(guard.as_ref()?) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("cue: database: {e}");
            None
        }
    }
}

/// After the data folder moved, saved image paths still point at the old one: point them at the new.
pub fn rewrite_paths(old: &str, new: &str) {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("UPDATE items SET data = replace(data, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE sessions SET data = replace(data, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE exchanges SET images = replace(images, ?1, ?2), text = replace(text, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE parked SET data = replace(data, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE drafts SET images = replace(images, ?1, ?2)", params![old, new])?;
        tx.commit()
    });
}

/// Tests that point CUE_HOME somewhere else take this first, so they don't trip over each other.
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// This test's turn at TEST_LOCK, held until the guard goes.
#[cfg(test)]
pub fn test_turn() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tests: the database held, as a slow write holds it: `held` hears once it is, and it's let go when `release`
/// hears (or hangs up).
#[cfg(test)]
pub fn hold(held: std::sync::mpsc::Sender<()>, release: std::sync::mpsc::Receiver<()>) {
    let _db = crate::locking::lock(&DB, "DB");
    let _ = held.send(());
    let _ = release.recv();
}

/// For tests: forget the open connection so the next call opens CUE_HOME's database afresh.
#[cfg(test)]
pub fn reset() {
    *crate::locking::lock(&DB, "DB") = None;
    HANDOFFS.stale();
    AHEAD_WATCHED.stale();
}

/// Tests: `f` on the open database, as every call here runs.
#[cfg(test)]
pub fn raw<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    with(f)
}

/// How many exchanges the log holds for a session.
#[cfg(test)]
pub fn exchange_count(session_id: &str) -> i64 {
    with(|c| c.query_row("SELECT COUNT(*) FROM exchanges WHERE session_id = ?1", params![session_id], |r| r.get(0))).unwrap_or(0)
}

#[cfg(test)]
mod tests;
