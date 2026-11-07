//! Search through everything Cue has logged, and a session's log a page at a time.

use super::with;
use rusqlite::params;
use serde_json::{json, Value};

/// Find `q` (case-insensitive) in every session Cue has seen, ended ones too: session names and
/// projects, your messages and the agents' replies. Newest first, at most `limit` messages.
pub fn search(q: &str, limit: usize) -> Vec<Value> {
    let q = q.trim();
    if q.is_empty() {
        return vec![];
    }
    let lit = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let (pat, starts) = (format!("%{lit}%"), format!("{lit}%"));
    with(|c| {
        let meta = |data: &str| serde_json::from_str::<Value>(data).unwrap_or(Value::Null);
        let who = |m: &Value, row_project: String| {
            let name = m.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            (if name.is_empty() { row_project } else { name }, m.get("harness").and_then(Value::as_str).unwrap_or("").to_string())
        };
        let mut out = vec![];
        // Sessions whose name or project matches.
        // With its last message, so you can tell which one you want before opening it.
        let mut st = c.prepare(
            "SELECT s.id, s.project, s.data, s.updated_ms, s.ended_ms,
                    (SELECT role || char(31) || text FROM exchanges e WHERE e.session_id = s.id ORDER BY e.at_ms DESC LIMIT 1)
             FROM sessions s
             WHERE s.project LIKE ?1 ESCAPE '\\' OR json_extract(s.data, '$.name') LIKE ?1 ESCAPE '\\'
             ORDER BY CASE WHEN lower(COALESCE(json_extract(s.data, '$.name'), '')) = lower(?2) OR lower(s.project) = lower(?2) THEN 0
                           WHEN COALESCE(json_extract(s.data, '$.name'), '') LIKE ?3 ESCAPE '\\' OR s.project LIKE ?3 ESCAPE '\\' THEN 1
                           ELSE 2 END, s.updated_ms DESC LIMIT 20",
        )?;
        // The best name matches first (the whole name, then its start), so the 20 kept are the ones you meant.
        for r in st.query_map(params![pat, q, starts], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<String>>(5)?))
        })? {
            let (id, project, data, at, ended, last) = r?;
            let (name, harness) = who(&meta(&data), project.clone());
            let (last_role, last_text) = last.as_deref().and_then(|l| l.split_once('\u{1f}')).map(|(r, t)| (r.to_string(), snippet(t, ""))).unwrap_or_default();
            out.push(json!({ "kind": "session", "session_id": id, "name": name, "project": project, "harness": harness, "at_ms": at, "ended": ended.is_some(),
                             "last_role": last_role, "last": last_text }));
        }
        // Messages, with a snippet around the first match.
        let mut st = c.prepare(
            "SELECT e.session_id, e.at_ms, e.role, e.text, COALESCE(s.project, ''), COALESCE(s.data, '{}'), s.ended_ms
             FROM exchanges e LEFT JOIN sessions s ON s.id = e.session_id
             WHERE e.text LIKE ?1 ESCAPE '\\' ORDER BY e.at_ms DESC LIMIT ?2",
        )?;
        for r in st.query_map(params![pat, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<i64>>(6)?))
        })? {
            let (sid, at, role, text, project, data, ended) = r?;
            let (name, harness) = who(&meta(&data), project.clone());
            out.push(json!({ "kind": "message", "session_id": sid, "name": name, "project": project, "harness": harness, "at_ms": at,
                             "role": role, "snippet": snippet(&text, q), "text": text, "ended": ended.is_some() }));
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// Every session Cue has ever seen (live or ended): older-session search leaves these out.
pub fn known_session_ids() -> std::collections::HashSet<String> {
    with(|c| {
        let mut st = c.prepare("SELECT id FROM sessions")?;
        let ids = st.query_map([], |r| r.get::<_, String>(0))?.filter_map(Result::ok).collect();
        Ok(ids)
    })
    .unwrap_or_default()
}

/// Everything Cue logged for one session, oldest first: the whole conversation, ended sessions too.
pub fn session_log(session_id: &str) -> Vec<Value> {
    with(|c| {
        let mut st = c.prepare("SELECT at_ms, role, text, images, voice, status_ask FROM exchanges WHERE session_id = ?1 ORDER BY at_ms")?;
        let rows = st.query_map(params![session_id], log_row)?;
        rows.collect()
    })
    .unwrap_or_default()
}

/// One page of a session's conversation: the `limit` messages before `before_ms`, oldest first (the
/// chat loads earlier messages a page at a time as you scroll up). Fewer than `limit`: that's the start.
pub fn session_log_page(session_id: &str, before_ms: u64, limit: usize) -> Vec<Value> {
    let mut page = with(|c| {
        let mut st = c.prepare("SELECT at_ms, role, text, images, voice, status_ask FROM exchanges WHERE session_id = ?1 AND at_ms < ?2 ORDER BY at_ms DESC LIMIT ?3")?;
        let rows = st.query_map(params![session_id, before_ms as i64, limit as i64], log_row)?;
        rows.collect::<rusqlite::Result<Vec<Value>>>()
    })
    .unwrap_or_default();
    page.reverse();
    page
}

fn log_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let images: Vec<String> = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default();
    let mut row = json!({ "at_ms": r.get::<_, i64>(0)?, "role": r.get::<_, String>(1)?, "text": r.get::<_, String>(2)?, "images": images });
    if r.get::<_, bool>(4)? {
        row["voice"] = json!(true);
    }
    let ask = r.get::<_, String>(5)?;
    if !ask.is_empty() {
        row["status_ask"] = json!(ask);
    }
    Ok(row)
}

/// A line of context around the first (case-insensitive) match: ~60 characters before, ~140 after.
pub(super) fn snippet(text: &str, q: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    let needle: Vec<char> = q.to_lowercase().chars().collect();
    // Lowercasing can change a string's length (rare letters); then just start at the top.
    let at = if lower.len() == chars.len() { lower.windows(needle.len().max(1)).position(|w| w == needle.as_slice()).unwrap_or(0) } else { 0 };
    let from = at.saturating_sub(60);
    let to = (at + needle.len() + 140).min(chars.len());
    let body: String = chars[from..to].iter().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{}{body}{}", if from > 0 { "…" } else { "" }, if to < chars.len() { "…" } else { "" })
}
