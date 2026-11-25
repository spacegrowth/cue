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
        for table in ["desk_items", "desk_decisions", "desk_lead", "desk_lead_past", "desk_members", "desk_moves", "desk_moves_cleared", "desk_feed", "desk_feed_marks", "desk_holds", "desk_bands"] {
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

/// The holds on code saved (#540, desk/holds.rs).
pub fn desk_holds() -> Vec<crate::desk::holds::Hold> {
    with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_holds ORDER BY rowid")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|data| serde_json::from_str(&data).ok()).collect())
    }).unwrap_or_default()
}

/// The bands between Desks saved since `since` (ms; #540, desk/tell.rs), oldest first; older ones are deleted.
pub fn desk_bands(since: u64) -> Vec<crate::desk::tell::Band> {
    with(|c| {
        c.execute("DELETE FROM desk_bands WHERE at < ?1", params![since as i64])?;
        let mut q = c.prepare("SELECT data FROM desk_bands ORDER BY at, rowid")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|data| serde_json::from_str(&data).ok()).collect())
    }).unwrap_or_default()
}

/// Bands sent or answered (#540), each saved as it is now, in one transaction.
pub fn desk_save_bands(bands: &[crate::desk::tell::Band]) -> Result<(), String> {
    if bands.is_empty() {
        return Ok(());
    }
    let data: Vec<String> = bands.iter().map(|b| serde_json::to_string(b).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    with(|c| {
        let tx = c.unchecked_transaction()?;
        for (b, data) in bands.iter().zip(&data) {
            tx.execute(
                "INSERT INTO desk_bands (key, project, at, data) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(key) DO UPDATE SET data = excluded.data",
                params![b.key(), b.from, b.at as i64, data],
            )?;
        }
        tx.commit()
    })
    .ok_or_else(|| "the database refused it".to_string())
}

/// Holds taken, taken again or gone (#540), in one transaction, in order.
pub fn desk_save_holds(saves: &[crate::desk::holds::HoldSave]) -> Result<(), String> {
    use crate::desk::holds::HoldSave;
    if saves.is_empty() {
        return Ok(());
    }
    let data: Vec<Option<String>> = saves.iter().map(|s| match s {
        HoldSave::Put(h) => serde_json::to_string(h).map(Some).map_err(|e| e.to_string()),
        HoldSave::Gone(_) => Ok(None),
    }).collect::<Result<_, _>>()?;
    with(|c| {
        let tx = c.unchecked_transaction()?;
        for (s, data) in saves.iter().zip(&data) {
            match (s, data) {
                (HoldSave::Put(h), Some(data)) => tx.execute(
                    "INSERT INTO desk_holds (item, project, repo, data) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(item) DO UPDATE SET project = excluded.project, repo = excluded.repo, data = excluded.data",
                    params![h.item, h.desk, h.repo, data],
                )?,
                (HoldSave::Gone(item), _) | (HoldSave::Put(crate::desk::holds::Hold { item, .. }), None) => tx.execute("DELETE FROM desk_holds WHERE item = ?1", params![item])?,
            };
        }
        tx.commit()
    })
    .ok_or_else(|| "the database refused it".to_string())
}

/// An item saved as it is now (`desk_save`; a failed save is logged).
pub fn desk_put_item(it: &DeskItem) {
    if let Err(e) = desk_save(std::slice::from_ref(it), &[], &[]) {
        eprintln!("cue: desk: couldn't save item #{}: {e}", it.id);
    }
}

/// The one save every Desk item goes through: items as they are now, items dropped, and the feed rows their
/// change added (desk/feed.rs), in one transaction, so a row is never saved without its change or the other way.
pub fn desk_save(items: &[DeskItem], dropped: &[String], rows: &[crate::desk::feed::Change]) -> Result<(), String> {
    if items.is_empty() && dropped.is_empty() && rows.is_empty() {
        return Ok(());
    }
    let data: Vec<String> = items.iter().map(|it| serde_json::to_string(it).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    with(|c| {
        let tx = c.unchecked_transaction()?;
        for (it, data) in items.iter().zip(&data) {
            tx.execute(
                "INSERT INTO desk_items (id, project, state, updated_ms, data) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET project = excluded.project, state = excluded.state, updated_ms = excluded.updated_ms, data = excluded.data",
                params![it.id, it.project, it.state, (it.updated * 1000) as i64, data],
            )?;
        }
        for id in dropped {
            tx.execute("DELETE FROM desk_items WHERE id = ?1", params![id])?;
        }
        for r in rows {
            tx.execute(
                // Not one "heard" has passed since it was numbered (its marks saved first, in a write of their own).
                "INSERT OR REPLACE INTO desk_feed (project, n, item, kind, words, key, created_ms) SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7
                 WHERE ?2 > COALESCE((SELECT heard FROM desk_feed_marks WHERE project = ?1), 0)",
                params![r.desk, r.n as i64, r.item, r.kind.as_str(), r.words, r.key, r.created_ms as i64],
            )?;
        }
        tx.commit()
    })
    .ok_or_else(|| "the database refused it".to_string())
}

/// Desk `desk`'s feed marks saved: sent and heard, each with its lead. Rows heard are deleted with it.
pub fn desk_feed_marks(desk: &str, sent: &crate::desk::feed::Mark, heard: &crate::desk::feed::Mark) -> Result<(), String> {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO desk_feed_marks (project, sent, sent_lead, heard, heard_lead) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project) DO UPDATE SET sent = excluded.sent, sent_lead = excluded.sent_lead, heard = excluded.heard, heard_lead = excluded.heard_lead",
            params![desk, sent.n as i64, sent.lead, heard.n as i64, heard.lead],
        )?;
        tx.execute("DELETE FROM desk_feed WHERE project = ?1 AND n <= ?2", params![desk, heard.n as i64])?;
        tx.commit()
    })
    .ok_or_else(|| "the database refused it".to_string())
}

/// Every Desk's feed as saved: its marks, and its rows past "heard", in order (those not past it deleted).
pub fn desk_feed_load() -> crate::desk::feed::Feeds {
    use crate::desk::feed::{Change, ChangeKind, Feed, Mark};
    let mut feeds = crate::desk::feed::Feeds::new();
    let marks: Vec<(String, u64, String, u64, String)> = with(|c| {
        let mut q = c.prepare("SELECT project, sent, sent_lead, heard, heard_lead FROM desk_feed_marks")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?, r.get::<_, i64>(3)? as u64, r.get(4)?)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default();
    for (desk, sent, sent_lead, heard, heard_lead) in marks {
        feeds.insert(desk, Feed { sent: Mark { n: sent, lead: sent_lead }, heard: Mark { n: heard, lead: heard_lead }, next: sent.max(heard), ..Default::default() });
    }
    // Rows heard already: a save of rows that ran after "heard" passed them (each save is its own write) left them.
    with(|c| c.execute("DELETE FROM desk_feed WHERE n <= (SELECT m.heard FROM desk_feed_marks m WHERE m.project = desk_feed.project)", []));
    let rows: Vec<Change> = with(|c| {
        let mut q = c.prepare("SELECT project, n, item, kind, words, key, created_ms FROM desk_feed ORDER BY project, n")?;
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, i64>(6)? as u64))
        })?;
        Ok(rows.flatten().filter_map(|(desk, n, item, kind, words, key, created_ms)| Some(Change { desk, n, item, kind: ChangeKind::parse(&kind)?, words, key, created_ms })).collect())
    })
    .unwrap_or_default();
    for r in rows {
        let f = feeds.entry(r.desk.clone()).or_default();
        if r.n > f.heard.n {
            f.next = f.next.max(r.n);
            f.rows.push(r);
        }
    }
    feeds
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

/// Item `id` as saved, done long ago too (a feature's piece, #282).
pub fn desk_get_item(id: &str) -> Option<DeskItem> {
    with(|c| {
        let d: Option<String> = c.query_row("SELECT data FROM desk_items WHERE id = ?1", params![id], |r| r.get(0)).ok();
        Ok(d.and_then(|d| serde_json::from_str(&d).ok()))
    })
    .flatten()
}

/// The saved pieces of feature `id` (#282): its done page shows them under it.
pub fn desk_pieces(id: &str) -> Vec<DeskItem> {
    with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_items WHERE data LIKE ?1")?;
        let rows = q.query_map(params![format!("%\"part_of\":\"{id}\"%")], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str::<DeskItem>(&d).ok()).filter(|i| i.part_of == id).collect())
    })
    .unwrap_or_default()
}

/// An item dropped.
pub fn desk_drop_item(id: &str) {
    if let Err(e) = desk_save(&[], &[id.to_string()], &[]) {
        eprintln!("cue: desk: couldn't drop item #{id}: {e}");
    }
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
        // One of the lead's own moves (#374): kept in its list too, until you clear it.
        if !d.act.is_empty() {
            c.execute(
                "INSERT INTO desk_moves (id, project, at_ms, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET project = excluded.project, data = excluded.data",
                params![d.id, d.project, (d.at * 1000) as i64, data],
            )?;
        }
        Ok(())
    });
}

/// `project`'s lead's own moves (#374), newest first, up to `take`: after `before` (the last one the list has: its
/// ms and id) when given, else from the newest. The list loads these as you scroll.
pub fn desk_moves_page(project: &str, before: Option<(u64, &str)>, take: u32) -> Vec<DeskDecision> {
    let (ms, id) = before.map_or((i64::MAX, ""), |(ms, id)| (ms as i64, id));
    with(|c| {
        let mut q = c.prepare(
            "SELECT data FROM desk_moves WHERE project = ?1 AND (at_ms < ?2 OR (at_ms = ?2 AND id < ?3))
             ORDER BY at_ms DESC, id DESC LIMIT ?4",
        )?;
        let rows = q.query_map(params![project, ms, id, take], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    })
    .unwrap_or_default()
}

/// `project`'s list of the lead's moves cleared, at `at_ms`: those saved go, and the list shows only later ones.
pub fn desk_moves_clear(project: &str, at_ms: u64) -> Result<(), String> {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM desk_moves WHERE project = ?1", params![project])?;
        tx.execute(
            "INSERT INTO desk_moves_cleared (project, at_ms) VALUES (?1, ?2) ON CONFLICT(project) DO UPDATE SET at_ms = excluded.at_ms",
            params![project, at_ms as i64],
        )?;
        tx.commit()
    })
    .ok_or_else(|| "Couldn't clear the list".to_string())
}

/// When each Desk's list of the lead's moves was last cleared (project, ms).
pub fn desk_moves_cleared() -> Vec<(String, u64)> {
    with(|c| {
        let mut q = c.prepare("SELECT project, at_ms FROM desk_moves_cleared")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        Ok(rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// `project`'s lead: session `session_id`, going by `name`. The lead it replaces is kept as an earlier one (#792).
pub fn desk_put_lead(project: &str, session_id: &str, name: &str, set_ms: u64) {
    with(|c| {
        c.execute(
            "INSERT INTO desk_lead_past (project, session_id, name, until_ms, next_session)
             SELECT project, session_id, name, ?3, ?2 FROM desk_lead WHERE project = ?1 AND session_id != ?2
             ON CONFLICT(project, session_id) DO UPDATE SET name = excluded.name, until_ms = excluded.until_ms, next_session = excluded.next_session",
            params![project, session_id, set_ms as i64],
        )?;
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

/// A change of `project`'s lead to `failed` that didn't happen, put back to `session_id`: neither was the other's
/// earlier lead after all.
pub fn desk_unpast_lead(project: &str, session_id: &str, failed: &str) {
    with(|c| {
        c.execute("DELETE FROM desk_lead_past WHERE project = ?1 AND ((session_id = ?2 AND next_session = ?3) OR (session_id = ?3 AND next_session = ?2))", params![project, session_id, failed])?;
        Ok(())
    });
}

/// The lead `project`'s Desk had before session `sid` ran it (#792): (session, name, until_ms, from a hand-off). Kept
/// since #792; before that, a lead that handed off to `sid` (the handoffs table), its name unknown here.
pub fn desk_lead_before(project: &str, sid: &str) -> Option<(String, String, u64, bool)> {
    use rusqlite::OptionalExtension;
    let kept = with(|c| c.query_row(
        "SELECT p.session_id, p.name, p.until_ms, h.session_id IS NOT NULL FROM desk_lead_past p
         LEFT JOIN handoffs h ON h.session_id = p.session_id AND h.next_session = p.next_session
         WHERE p.project = ?1 AND p.next_session = ?2 ORDER BY p.until_ms DESC LIMIT 1",
        params![project, sid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as u64, r.get::<_, bool>(3)?)),
    ).optional()).flatten();
    kept.or_else(|| with(|c| c.query_row(
        "SELECT session_id, continued_ms FROM handoffs WHERE next_session = ?1 ORDER BY continued_ms DESC LIMIT 1",
        params![sid], |r| Ok((r.get::<_, String>(0)?, String::new(), r.get::<_, i64>(1)? as u64, true)),
    ).optional()).flatten())
}

/// Session `sid`'s messages in Cue's log: how many, the first's time and the last's (0s for none).
pub fn session_log_span(sid: &str) -> (u64, u64, u64) {
    with(|c| c.query_row("SELECT COUNT(*), COALESCE(MIN(at_ms), 0), COALESCE(MAX(at_ms), 0) FROM exchanges WHERE session_id = ?1", params![sid],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64)))).unwrap_or((0, 0, 0))
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
        Ok(rows.flatten().filter_map(|d| serde_json::from_str(&d).ok()).collect::<Vec<DeskItem>>())
    })
    .unwrap_or_default();
    // The pieces done long ago of a feature still shown (#282): its list counts and shows them.
    let mut items = items;
    let older: Vec<DeskItem> = with(|c| {
        let mut q = c.prepare("SELECT data FROM desk_items WHERE state = 'done' AND updated_ms < ?1 AND data LIKE '%\"part_of\":%'")?;
        let rows = q.query_map(params![since], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|d| serde_json::from_str::<DeskItem>(&d).ok()).collect())
    })
    .unwrap_or_default();
    let shown: std::collections::HashSet<String> = items.iter().map(|i| i.id.clone()).collect();
    items.extend(older.into_iter().filter(|i| shown.contains(&i.part_of)));
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
