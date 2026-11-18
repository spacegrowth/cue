//! Desk records. Historical SQL columns named `project` contain the Desk identity.
//! desk_spaces stores each independent Desk's workspace project, name and folder.

use super::with;
use crate::desk::{DeskDecision, DeskItem};
use rusqlite::params;

/// How long done items and decisions are loaded back for after a restart.
const KEEP_DONE_MS: u64 = 2 * super::DAY_MS;
/// How long done items and decisions are kept at all: deleted after that, at the next start.
const DROP_DONE_MS: u64 = 30 * super::DAY_MS;

pub fn desk_put_space(desk: &crate::desk::model::DeskSpace) -> Result<(), String> {
    let data = serde_json::to_string(desk).map_err(|e| e.to_string())?;
    with(|c| c.execute("INSERT INTO desk_spaces (id, data) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET data = excluded.data", params![desk.id, data]))
        .map(|_| ()).ok_or_else(|| "Couldn't save the Desk".to_string())
}

/// Delete a Desk and all of its persisted records atomically.
pub fn desk_delete(id: &str) -> Result<(), String> {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        for table in ["desk_items", "desk_decisions", "desk_lead", "desk_members"] {
            tx.execute(&format!("DELETE FROM {table} WHERE project = ?1"), params![id])?;
        }
        tx.execute("DELETE FROM desk_spaces WHERE id = ?1", params![id])?;
        tx.commit()
    }).ok_or_else(|| "Couldn't delete the Desk".to_string())
}

pub fn desk_spaces() -> Vec<crate::desk::model::DeskSpace> {
    with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_spaces ORDER BY rowid")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|data| serde_json::from_str(&data).ok()).collect())
    }).unwrap_or_default()
}

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

/// A dropped item's kept copy, saved.
pub fn desk_put_kept(k: &crate::desk::KeptTree) {
    let data = serde_json::to_string(k).unwrap_or_default();
    with(|c| {
        c.execute(
            "INSERT INTO desk_kept (id, project, data) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET project = excluded.project, data = excluded.data",
            params![k.id, k.project, data],
        )?;
        Ok(())
    });
}

/// A kept copy removed from the list.
pub fn desk_drop_kept(id: &str) {
    with(|c| c.execute("DELETE FROM desk_kept WHERE id = ?1", params![id]).map(|_| ()));
}

/// Every kept copy, oldest first.
pub fn desk_kept() -> Vec<crate::desk::KeptTree> {
    with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_kept ORDER BY rowid")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    })
    .unwrap_or_default()
}

/// An item dropped.
pub fn desk_drop_item(id: &str) {
    with(|c| c.execute("DELETE FROM desk_items WHERE id = ?1", params![id]).map(|_| ()));
}

/// Items `Desk::number_old` numbered (old id, new id) saved under their numbers (the old rows dropped), with the
/// items and decisions that named them.
pub fn desk_put_numbered(desk: &crate::desk::Desk, changed: &[(String, String)]) {
    let named = |id: Option<&str>| changed.iter().any(|(_, n)| id == Some(n.as_str()));
    for (was, _) in changed {
        desk_drop_item(was);
    }
    for it in desk.items.iter().filter(|i| named(Some(&i.id)) || named(i.after.as_deref())) {
        desk_put_item(it);
    }
    for d in desk.decisions.iter().filter(|d| named(d.item_id.as_deref())) {
        desk_put_decision(d);
    }
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

/// `project`'s lead: session `session_id`, going by `name`.
pub fn desk_put_lead(project: &str, session_id: &str, name: &str, set_ms: u64) {
    with(|c| {
        c.execute(
            "INSERT INTO desk_lead (project, session_id, name, set_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project) DO UPDATE SET session_id = excluded.session_id, name = excluded.name, set_ms = excluded.set_ms",
            params![project, session_id, name, set_ms as i64],
        )?;
        Ok(())
    });
}

/// Session `session_id` on `project`'s Desk.
pub fn desk_put_member(project: &str, session_id: &str, added_ms: u64) {
    with(|c| {
        c.execute("INSERT OR IGNORE INTO desk_members (project, session_id, added_ms) VALUES (?1, ?2, ?3)", params![project, session_id, added_ms as i64])?;
        Ok(())
    });
}

/// Session `session_id` off `project`'s Desk.
pub fn desk_drop_member(project: &str, session_id: &str) {
    with(|c| {
        c.execute("DELETE FROM desk_members WHERE project = ?1 AND session_id = ?2", params![project, session_id])?;
        Ok(())
    });
}

/// Each project's sessions on its Desk (project, session), in the order they joined.
pub fn desk_members() -> Vec<(String, String)> {
    with(|c| {
        let mut q = c.prepare("SELECT project, session_id FROM desk_members ORDER BY added_ms, rowid")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// `project`'s Desk has no lead (a change of lead that failed, where it had none before).
pub fn desk_drop_lead(project: &str) {
    with(|c| {
        c.execute("DELETE FROM desk_lead WHERE project = ?1", params![project])?;
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

/// `project`'s done items, newest first (by when each last changed), up to `take` of them: after `before` (the last one
/// the window has: its updated ms and id) when given, else from the newest. The Desk's list loads these as you scroll.
pub fn desk_done_page(project: &str, before: Option<(u64, &str)>, take: u32) -> Vec<DeskItem> {
    let (ms, id) = before.map_or((i64::MAX, ""), |(ms, id)| (ms as i64, id));
    with(|c| {
        let mut q = c.prepare(
            "SELECT data FROM desk_items WHERE project = ?1 AND state = 'done' AND (updated_ms < ?2 OR (updated_ms = ?2 AND id < ?3))
             ORDER BY updated_ms DESC, id DESC LIMIT ?4",
        )?;
        let rows = q.query_map(params![project, ms, id, take], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    })
    .unwrap_or_default()
}

/// What the Desk starts with: every item not done and those done lately, the recent decisions, and each
/// project's lead (project, session, name). Those done long ago are deleted first.
pub fn desk_load(now_ms: u64) -> (Vec<DeskItem>, Vec<DeskDecision>, Vec<(String, String, String)>) {
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
    let leads = with(|c| {
        let mut q = c.prepare("SELECT project, session_id, name FROM desk_lead")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default();
    (items, decisions, leads)
}
