//! What waits to be sent: your unsent drafts per session, and teammates' reviews waiting for their leads.

use super::with;
use crate::model::now_ms;
use rusqlite::params;
use serde_json::{json, Value};

/// The text of one unsent draft ("" when there's none).
pub fn draft_text(key: &str) -> String {
    use rusqlite::OptionalExtension;
    with(|c| c.query_row("SELECT text FROM drafts WHERE key = ?1", params![key], |r| r.get::<_, String>(0)).optional()).flatten().unwrap_or_default()
}

pub fn drafts() -> Value {
    with(|c| {
        let mut q = c.prepare("SELECT key, text, images FROM drafts")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = serde_json::Map::new();
        for (key, text, images) in rows.filter_map(|r| r.ok()) {
            out.insert(key, json!({ "text": text, "images": serde_json::from_str::<Value>(&images).unwrap_or(json!([])) }));
        }
        Ok(Value::Object(out))
    })
    .unwrap_or_else(|| json!({}))
}

/// Save one draft; an empty one (no text, no images) is removed.
pub fn set_draft(key: &str, text: &str, images: &Value) {
    with(|c| {
        if text.trim().is_empty() && images.as_array().map_or(true, |a| a.is_empty()) {
            c.execute("DELETE FROM drafts WHERE key = ?1", params![key])?;
        } else {
            c.execute(
                "INSERT INTO drafts (key, text, images, updated_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET text = excluded.text, images = excluded.images, updated_ms = excluded.updated_ms",
                params![key, text, images.to_string(), now_ms() as i64],
            )?;
        }
        Ok(())
    });
}

/// The reviews waiting for their leads, oldest first: (teammate's key, lead, text, when).
pub fn review_queue() -> Vec<(String, String, String, u64)> {
    with(|c| {
        let mut q = c.prepare("SELECT exec, lead, text, at_ms FROM review_queue ORDER BY at_ms")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as u64)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

/// Queue a review (or keep the one queued).
pub fn queue_review(exec: &str, lead: &str, text: &str, at_ms: u64) {
    with(|c| c.execute("INSERT OR IGNORE INTO review_queue (exec, lead, text, at_ms) VALUES (?1, ?2, ?3, ?4)", params![exec, lead, text, at_ms as i64]));
}

/// A queued review sent, or taken back.
pub fn unqueue_review(exec: &str) {
    with(|c| c.execute("DELETE FROM review_queue WHERE exec = ?1", params![exec]));
}
