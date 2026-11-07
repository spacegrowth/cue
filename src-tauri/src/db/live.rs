//! Live sessions and their finished-turn cards, the conversation log, parked sessions and closed ones.

use super::with;
use super::items::put_item;
use crate::model::{now_ms, Exchange, Item};
use crate::sessions::{Parked, Session};
use rusqlite::{params, Connection};

/// Tests: mirror what's live in memory, all of it (the app saves only what changed: hub/live_writes.rs).
#[cfg(test)]
pub fn sync_live(sessions: &[Session], waiting: &[&Item]) {
    let live: Vec<&str> = sessions.iter().map(|s| s.origin.session_id.as_str()).collect();
    save_live(&LiveSave { write: &sessions.iter().collect::<Vec<_>>(), live: &live, waiting: Some(waiting), answered: &[] });
}

/// One save of what's live, as only what changed since the last: the hub's draws save after every change, and
/// writing every session and its whole conversation each time held the database for the length of the lot.
pub struct LiveSave<'a> {
    /// The sessions whose records changed since they were last saved (all of them, the first time).
    pub write: &'a [&'a Session],
    /// Every live session's id: the open rows not among them have ended, and the rest are still heard from.
    pub live: &'a [&'a str],
    /// The pending finished-turn cards, when they changed (None: as last saved).
    pub waiting: Option<&'a [&'a Item]>,
    /// Answered cards to save in the same write.
    pub answered: &'a [Item],
}

/// `save` in one transaction; whether it was written.
pub fn save_live(save: &LiveSave) -> bool {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        let now = now_ms() as i64;
        for s in save.write {
            put_session(&tx, s, now)?;
        }
        end_the_rest(&tx, save.live, now)?;
        if let Some(waiting) = save.waiting {
            tx.execute("DELETE FROM items WHERE status = 'pending'", [])?;
            for it in waiting {
                put_item(&tx, it)?;
            }
        }
        for it in save.answered {
            put_item(&tx, it)?;
        }
        tx.commit()
    })
    .is_some()
}

/// A live session's record and its conversation as it has it now.
fn put_session(tx: &Connection, s: &Session, now: i64) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO sessions (id, harness, project, cwd, state, driven_by, started_ms, updated_ms, ended_ms, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL, ?8)
         ON CONFLICT(id) DO UPDATE SET harness = excluded.harness, project = excluded.project, cwd = excluded.cwd,
           state = excluded.state, driven_by = excluded.driven_by, updated_ms = excluded.updated_ms,
           ended_ms = NULL, data = excluded.data",
        params![s.origin.session_id, s.origin.harness, s.project, s.origin.cwd, s.state, s.driven_by, now, serde_json::to_string(s).unwrap_or_default()],
    )?;
    for e in &s.thread {
        put_exchange(tx, &s.origin.session_id, e)?;
    }
    // A message that moved (one you queued, put below the reply it waited behind) leaves its
    // old row: within the thread's span, the log keeps only what the thread has.
    let Some(first) = s.thread.first() else { return Ok(()) };
    let mut q = tx.prepare("SELECT at_ms, role FROM exchanges WHERE session_id = ?1 AND at_ms >= ?2")?;
    let rows: Vec<(i64, String)> = q.query_map(params![s.origin.session_id, first.at_ms as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();
    drop(q);
    for (at, role) in rows.iter().filter(|(at, role)| !s.thread.iter().any(|e| e.at_ms as i64 == *at && e.role == *role)) {
        tx.execute("DELETE FROM exchanges WHERE session_id = ?1 AND at_ms = ?2 AND role = ?3", params![s.origin.session_id, at, role])?;
    }
    Ok(())
}

/// Whatever isn't live any more has ended (its agent exited, or it said so); what is, is still heard from
/// as of `now` (a relaunch restores the sessions heard from within the last day).
fn end_the_rest(tx: &Connection, live: &[&str], now: i64) -> rusqlite::Result<()> {
    let mut q = tx.prepare("SELECT id FROM sessions WHERE ended_ms IS NULL")?;
    let open: Vec<String> = q.query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
    drop(q);
    for id in open.iter().filter(|id| !live.contains(&id.as_str())) {
        tx.execute("UPDATE sessions SET ended_ms = ?2 WHERE id = ?1", params![id, now])?;
    }
    tx.execute("UPDATE sessions SET updated_ms = ?1 WHERE ended_ms IS NULL", params![now])?;
    Ok(())
}

/// Sessions that ended, each its record as it was when it ended and when that was: their saved rows say so,
/// whatever changed since the last save before the end (a rename landing just as it ended), so Closed after a
/// relaunch shows them as they were.
pub fn save_ended(ended: &[(u64, Session)]) {
    if ended.is_empty() {
        return;
    }
    with(|c| {
        let tx = c.unchecked_transaction()?;
        for (at, s) in ended {
            tx.execute(
                "INSERT INTO sessions (id, harness, project, cwd, state, driven_by, started_ms, updated_ms, ended_ms, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET harness = excluded.harness, project = excluded.project, cwd = excluded.cwd,
                   state = excluded.state, driven_by = excluded.driven_by, ended_ms = excluded.ended_ms, data = excluded.data",
                params![s.origin.session_id, s.origin.harness, s.project, s.origin.cwd, s.state, s.driven_by, *at as i64, serde_json::to_string(s).unwrap_or_default()],
            )?;
            for e in &s.thread {
                put_exchange(&tx, &s.origin.session_id, e)?;
            }
        }
        tx.commit()
    });
}

pub(super) fn put_exchange(c: &Connection, session_id: &str, e: &Exchange) -> rusqlite::Result<()> {
    // Same moment, same speaker: the same exchange (its images may have been filled in since).
    c.execute(
        "INSERT INTO exchanges (session_id, at_ms, role, text, images, voice, status_ask) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id, at_ms, role) DO UPDATE SET text = excluded.text, images = excluded.images, voice = excluded.voice, status_ask = excluded.status_ask",
        params![session_id, e.at_ms as i64, e.role, e.text, serde_json::to_string(&e.images).unwrap_or_else(|_| "[]".into()), e.voice, e.status_ask.as_deref().unwrap_or("")],
    )?;
    Ok(())
}

/// Sessions still open (heard from within the last day) and their finished-turn cards.
pub fn live() -> (Vec<Session>, Vec<Item>) {
    with(|c| {
        let day_ago = now_ms().saturating_sub(24 * 3600 * 1000) as i64;
        let mut q = c.prepare("SELECT data FROM sessions WHERE ended_ms IS NULL AND updated_ms > ?1")?;
        let sessions: Vec<Session> =
            q.query_map(params![day_ago], |r| r.get::<_, String>(0))?.filter_map(|r| r.ok()).filter_map(|d| serde_json::from_str(&d).ok()).collect();
        let ids: Vec<&str> = sessions.iter().map(|s| s.origin.session_id.as_str()).collect();
        let mut q = c.prepare("SELECT data FROM items WHERE status = 'pending' AND kind = 'waiting'")?;
        let waiting: Vec<Item> = q
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|d| serde_json::from_str::<Item>(&d).ok())
            .filter(|i| ids.contains(&i.origin.session_id.as_str()))
            .collect();
        Ok((sessions, waiting))
    })
    .unwrap_or_default()
}

/// Keep a parked session (again, if it was already: the newer record wins).
pub fn park(p: &Parked) {
    with(|c| {
        c.execute(
            "INSERT INTO parked (id, parked_ms, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET parked_ms = excluded.parked_ms, data = excluded.data",
            params![p.session.origin.session_id, p.parked_ms as i64, serde_json::to_string(p).unwrap_or_default()],
        )
    });
}

/// Every parked session, the latest parked first.
pub fn parked() -> Vec<Parked> {
    with(|c| {
        let mut q = c.prepare("SELECT data FROM parked ORDER BY parked_ms DESC")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?.filter_map(|r| r.ok()).filter_map(|d| serde_json::from_str(&d).ok()).collect();
        Ok(rows)
    })
    .unwrap_or_default()
}

/// It isn't parked any more (resumed, unparked, or running again by itself).
pub fn unpark(session_id: &str) {
    with(|c| c.execute("DELETE FROM parked WHERE id = ?1", params![session_id]));
}

/// Sessions that ended since `since_ms`, the latest ended first, at most `limit`: when each ended, and
/// its record as it was.
pub fn closed(since_ms: u64, limit: usize) -> Vec<(u64, Session)> {
    with(|c| {
        let mut q = c.prepare("SELECT ended_ms, data FROM sessions WHERE ended_ms IS NOT NULL AND ended_ms > ?1 ORDER BY ended_ms DESC LIMIT ?2")?;
        let rows = q
            .query_map(params![since_ms as i64, limit as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .filter_map(|(at, d)| serde_json::from_str::<Session>(&d).ok().map(|s| (at as u64, s)))
            .collect();
        Ok(rows)
    })
    .unwrap_or_default()
}

/// One session that has ended: when, and its record as it was.
pub fn ended_session(id: &str) -> Option<(u64, Session)> {
    with(|c| {
        c.query_row("SELECT ended_ms, data FROM sessions WHERE id = ?1 AND ended_ms IS NOT NULL", params![id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
    })
    .and_then(|(at, d)| serde_json::from_str::<Session>(&d).ok().map(|s| (at as u64, s)))
}

/// Delete: everything Cue kept of a session, gone (its record, its conversation log, its cards, parked
/// or not, its draft, a review waiting on it).
pub fn delete_session(id: &str) {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM exchanges WHERE session_id = ?1", params![id])?;
        tx.execute("DELETE FROM items WHERE session_id = ?1", params![id])?;
        tx.execute("DELETE FROM parked WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM drafts WHERE key = ?1", params![format!("s:{id}")])?;
        tx.execute("DELETE FROM review_queue WHERE exec = ?1", params![id])?;
        tx.commit()
    });
}
