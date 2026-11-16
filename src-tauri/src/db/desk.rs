//! The Desk's items, decisions and each project's Dispatch session (desk/).

use super::with;
use crate::desk::{DeskDecision, DeskItem};
use rusqlite::params;

/// How long done items and decisions are loaded back for after a restart.
const KEEP_DONE_MS: u64 = 2 * super::DAY_MS;
/// How long done items and decisions are kept at all: deleted after that, at the next start.
const DROP_DONE_MS: u64 = 30 * super::DAY_MS;

/// An item saved as it is now.
pub fn desk_put_item(it: &DeskItem) {
    let data = serde_json::to_string(it).unwrap_or_default();
    with(|c| {
        c.execute(
            "INSERT INTO desk_items (id, project, state, updated_ms, data) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET project = excluded.project, state = excluded.state, updated_ms = excluded.updated_ms, data = excluded.data",
            params![it.id, it.project, it.state, (it.updated * 1000) as i64, data],
        )?;
        Ok(())
    });
}

/// An item dropped.
pub fn desk_drop_item(id: &str) {
    with(|c| c.execute("DELETE FROM desk_items WHERE id = ?1", params![id]).map(|_| ()));
}

/// A decision saved as it is now.
pub fn desk_put_decision(d: &DeskDecision) {
    let data = serde_json::to_string(d).unwrap_or_default();
    with(|c| {
        c.execute(
            "INSERT INTO desk_decisions (id, project, item_id, at_ms, data) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![d.id, d.project, d.item_id.clone().unwrap_or_default(), (d.at * 1000) as i64, data],
        )?;
        Ok(())
    });
}

/// `project`'s Dispatch session.
pub fn desk_put_dispatch(project: &str, session_id: &str, started_ms: u64) {
    with(|c| {
        c.execute(
            "INSERT INTO desk_dispatch (project, session_id, started_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(project) DO UPDATE SET session_id = excluded.session_id, started_ms = excluded.started_ms",
            params![project, session_id, started_ms as i64],
        )?;
        Ok(())
    });
}

/// Done items and decisions older than `DROP_DONE_MS` deleted.
fn desk_prune(now_ms: u64) {
    let before = now_ms.saturating_sub(DROP_DONE_MS) as i64;
    with(|c| {
        c.execute("DELETE FROM desk_items WHERE state = 'done' AND updated_ms < ?1", params![before])?;
        c.execute("DELETE FROM desk_decisions WHERE at_ms < ?1", params![before])?;
        Ok(())
    });
}

/// What the Desk starts with: every item not done and those done lately, the recent decisions, and each
/// project's Dispatch. Those done long ago are deleted first.
pub fn desk_load(now_ms: u64) -> (Vec<DeskItem>, Vec<DeskDecision>, Vec<(String, String)>) {
    desk_prune(now_ms);
    let since = now_ms.saturating_sub(KEEP_DONE_MS) as i64;
    let items = with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_items WHERE state != 'done' OR updated_ms >= ?1 ORDER BY rowid")?;
        let rows = q.query_map(params![since], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    })
    .unwrap_or_default();
    let decisions = with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_decisions WHERE at_ms >= ?1 ORDER BY at_ms")?;
        let rows = q.query_map(params![since], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    })
    .unwrap_or_default();
    let dispatch = with(|c| {
        let mut q = c.prepare("SELECT project, session_id FROM desk_dispatch")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default();
    (items, decisions, dispatch)
}
