//! Hand-offs: a session's note and the session it continued in, and "Hand off if I don't come back".

use super::with;
use crate::model::now_ms;
use rusqlite::params;
use serde_json::{json, Value};
use std::sync::Mutex;

/// A query's answer kept in memory, so the window's every redraw doesn't ask the database: read again after a
/// write to its table (`stale`), or once it's a minute old (it can depend on the time). A read that raced a
/// write isn't kept: the write's generation moved on meanwhile.
pub(super) struct Kept<T> {
    generation: std::sync::atomic::AtomicU64,
    kept: Mutex<Option<(u64, std::time::Instant, T)>>,
}

impl<T: Clone> Kept<T> {
    pub(super) const fn new() -> Self {
        Self { generation: std::sync::atomic::AtomicU64::new(0), kept: Mutex::new(None) }
    }

    pub(super) fn get(&self, read: impl FnOnce() -> T) -> T {
        use std::sync::atomic::Ordering::SeqCst;
        let generation = self.generation.load(SeqCst);
        if let Some((g, at, v)) = &*crate::locking::lock(&self.kept, "a kept db answer") {
            if *g == generation && at.elapsed() < std::time::Duration::from_secs(60) {
                return v.clone();
            }
        }
        let v = read();
        let mut kept = crate::locking::lock(&self.kept, "a kept db answer");
        if self.generation.load(SeqCst) == generation {
            *kept = Some((generation, std::time::Instant::now(), v.clone()));
        }
        v
    }

    pub(super) fn stale(&self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// `handoffs()` and `ahead_watched()`, kept (see `Kept`).
pub(super) static HANDOFFS: Kept<Value> = Kept::new();
pub(super) static AHEAD_WATCHED: Kept<Vec<String>> = Kept::new();

/// A session sent its hand-off skill (or Cue's own): it's offered "Continue in a new session" until it does.
/// `resume`: the skill takes "resume" for the new session; otherwise Cue's own message starts it. A note
/// seen before is forgotten (this hand-off writes its own; an older one, written before `sent_ms`, isn't taken again).
/// `ahead`: asked before its cache ran out (see the handoffs table), else 0.
pub fn handoff_sent(session_id: &str, skill: &str, resume: bool, ahead: u8) {
    with(|c| c.execute("INSERT INTO handoffs (session_id, skill, sent_ms, resume, ahead) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(session_id) DO UPDATE SET skill = ?2, sent_ms = ?3, resume = ?4, next_session = '', continued_ms = 0, note = '', note_ms = 0, ahead = ?5",
        params![session_id, skill, now_ms() as i64, resume, ahead]));
    HANDOFFS.stale();
}

/// A hand-off written ahead that you didn't continue from (you replied in its session): gone, so its
/// session is offered hand-offs as before.
pub fn handoff_drop(session_id: &str) {
    with(|c| c.execute("DELETE FROM handoffs WHERE session_id = ?1", params![session_id]));
    HANDOFFS.stale();
}

/// "Hand off if I don't come back" on (or off) for a session.
pub fn ahead_watch(session_id: &str, on: bool) {
    if on {
        with(|c| c.execute("INSERT OR REPLACE INTO ahead_watch (session_id, set_ms) VALUES (?1, ?2)", params![session_id, now_ms() as i64]));
    } else {
        with(|c| c.execute("DELETE FROM ahead_watch WHERE session_id = ?1", params![session_id]));
    }
    AHEAD_WATCHED.stale();
}

/// The sessions with "Hand off if I don't come back" on.
pub fn ahead_watched() -> Vec<String> {
    AHEAD_WATCHED.get(|| {
        with(|c| {
            let mut q = c.prepare("SELECT session_id FROM ahead_watch")?;
            let rows = q.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default()
    })
}

/// A session wrote its hand-off note (`note`: its path as its log names it; `at_ms`: when, by the log).
/// `fresh`: a new hand-off (none waiting to continue), kept as `skill` with Cue's own message starting the
/// next session; otherwise the one waiting keeps its skill and only learns where its note is.
pub fn handoff_noted(session_id: &str, skill: &str, note: &str, at_ms: u64, fresh: bool) {
    if fresh {
        with(|c| c.execute("INSERT INTO handoffs (session_id, skill, sent_ms, resume, note, note_ms) VALUES (?1, ?2, ?3, 0, ?4, ?5) ON CONFLICT(session_id) DO UPDATE SET skill = ?2, sent_ms = ?3, resume = 0, next_session = '', continued_ms = 0, note = ?4, note_ms = ?5",
            params![session_id, skill, now_ms() as i64, note, at_ms as i64]));
        // A note of its own isn't one written ahead.
        with(|c| c.execute("UPDATE handoffs SET ahead = 0 WHERE session_id = ?1", params![session_id]));
    } else {
        with(|c| c.execute("UPDATE handoffs SET note = ?2, note_ms = ?3 WHERE session_id = ?1", params![session_id, note, at_ms as i64]));
    }
    HANDOFFS.stale();
}

/// It continued in `next`: the link between the two.
pub fn handoff_continued(session_id: &str, next: &str) {
    with(|c| c.execute("UPDATE handoffs SET next_session = ?2, continued_ms = ?3 WHERE session_id = ?1", params![session_id, next, now_ms() as i64]));
    HANDOFFS.stale();
}

/// One session's hand-off, as kept (see the handoffs table).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handoff {
    pub skill: String,
    pub sent_ms: u64,
    /// The session it continued in ("" while it hasn't), and when.
    pub next: String,
    pub continued_ms: u64,
    /// The skill takes "resume".
    pub resume: bool,
    /// Its note's path as its log names it ("" when Cue didn't see it written), and when it was written.
    pub note: String,
    pub note_ms: u64,
    /// Written before its cache ran out, and who asked (see the handoffs table); 0: not.
    pub ahead: u8,
}

/// One session's hand-off, if it has one.
pub fn handoff(session_id: &str) -> Option<Handoff> {
    use rusqlite::OptionalExtension;
    with(|c| c.query_row("SELECT skill, sent_ms, next_session, continued_ms, resume, note, note_ms, ahead FROM handoffs WHERE session_id = ?1", params![session_id], |r| Ok(Handoff {
        skill: r.get(0)?, sent_ms: r.get::<_, i64>(1)? as u64, next: r.get(2)?, continued_ms: r.get::<_, i64>(3)? as u64, resume: r.get(4)?, note: r.get(5)?, note_ms: r.get::<_, i64>(6)? as u64, ahead: r.get(7)?,
    })).optional()).flatten()
}

/// Hand-offs of the last 30 days: session → { skill, sent_ms, next, continued_ms, resume, note_ms, ahead }.
pub fn handoffs() -> Value {
    HANDOFFS.get(read_handoffs)
}

fn read_handoffs() -> Value {
    let since = now_ms().saturating_sub(30 * 24 * 3600 * 1000) as i64;
    let rows = with(|c| {
        let mut q = c.prepare("SELECT session_id, skill, sent_ms, next_session, continued_ms, resume, note_ms, ahead FROM handoffs WHERE sent_ms >= ?1")?;
        let rows = q.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, json!({ "skill": r.get::<_, String>(1)?, "sent_ms": r.get::<_, i64>(2)?, "next": r.get::<_, String>(3)?, "continued_ms": r.get::<_, i64>(4)?, "resume": r.get::<_, bool>(5)?, "note_ms": r.get::<_, i64>(6)?, "ahead": r.get::<_, i64>(7)? }))))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }).unwrap_or_default();
    Value::Object(rows.into_iter().collect())
}

/// Hand-offs sent since `since_ms`, and how many of them were continued in a new session.
pub fn handoff_counts(since_ms: u64) -> (u64, u64) {
    with(|c| c.query_row("SELECT COUNT(*), COALESCE(SUM(next_session != ''), 0) FROM handoffs WHERE sent_ms >= ?1", params![since_ms as i64], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)))).unwrap_or((0, 0))
}
