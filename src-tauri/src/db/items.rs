//! Items: decisions and finished turns, pending and answered.

use super::with;
use crate::model::{now_ms, Answer, Item};
use rusqlite::{params, Connection};
use std::collections::VecDeque;

pub(super) fn put_item(c: &Connection, it: &Item) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO items (id, session_id, kind, status, project, harness, created_ms, resolved_ms, outcome, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET status = excluded.status, resolved_ms = excluded.resolved_ms,
           outcome = excluded.outcome, data = excluded.data",
        params![
            it.id,
            it.origin.session_id,
            it.kind,
            it.status,
            it.project,
            it.origin.harness,
            it.created_ms as i64,
            it.resolved_ms.map(|v| v as i64),
            it.outcome,
            serde_json::to_string(it).unwrap_or_default(),
        ],
    )?;
    Ok(())
}

/// An answered item, saved the moment it's answered.
pub fn save_answered(it: &Item) {
    with(|c| put_item(c, it));
}

/// The last `keep` answered items, newest first; older ones are deleted (Settings → History), except
/// the last day's: today's numbers in History count every answer, not just the newest `keep`.
pub fn history(keep: usize) -> VecDeque<Item> {
    with(|c| {
        c.execute(
            "DELETE FROM items WHERE status != 'pending' AND resolved_ms < ?2 AND id NOT IN
               (SELECT id FROM items WHERE status != 'pending' ORDER BY resolved_ms DESC, created_ms DESC LIMIT ?1)",
            params![keep as i64, now_ms().saturating_sub(DAY_MS) as i64],
        )?;
        let mut q = c.prepare("SELECT data FROM items WHERE status != 'pending' ORDER BY resolved_ms DESC, created_ms DESC LIMIT ?1")?;
        let rows = q.query_map(params![keep as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|d| serde_json::from_str::<Item>(&d).ok()).collect())
    })
    .unwrap_or_default()
}

pub const DAY_MS: u64 = 24 * 3600 * 1000;

/// Every item answered since `since_ms`, newest first, in brief: what today's numbers in History need.
pub fn answers_since(since_ms: u64) -> Vec<Answer> {
    with(|c| {
        let mut q = c.prepare("SELECT session_id, project, status, created_ms, resolved_ms, outcome FROM items
                               WHERE status != 'pending' AND resolved_ms >= ?1 ORDER BY resolved_ms DESC")?;
        let rows = q.query_map(params![since_ms as i64], |r| {
            Ok(Answer { session_id: r.get(0)?, project: r.get(1)?, status: r.get(2)?, created_ms: r.get::<_, i64>(3)? as u64, resolved_ms: r.get::<_, i64>(4)? as u64, outcome: r.get(5)? })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}
