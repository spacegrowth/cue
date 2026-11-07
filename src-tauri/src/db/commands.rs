//! The "/" commands each folder offers, kept with when they were read.

use super::with;
use rusqlite::params;

/// A folder's kept command list and when it was read, if any.
pub fn commands(cwd: &str) -> Option<(u64, Vec<crate::commands::Cmd>)> {
    with(|c| {
        let mut q = c.prepare("SELECT read_ms, data FROM commands WHERE cwd = ?1")?;
        let row = q.query_row(params![cwd], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))).ok();
        Ok(row.and_then(|(at, data)| serde_json::from_str(&data).ok().map(|list| (at, list))))
    })
    .flatten()
}

pub fn set_commands(cwd: &str, read_ms: u64, list: &[crate::commands::Cmd]) {
    with(|c| {
        c.execute(
            "INSERT INTO commands (cwd, read_ms, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(cwd) DO UPDATE SET read_ms = excluded.read_ms, data = excluded.data",
            params![cwd, read_ms as i64, serde_json::to_string(list).unwrap_or_else(|_| "[]".into())],
        )?;
        Ok(())
    });
}

/// Every folder's kept list, by its key as commands.rs makes it ("<machine>:<folder>" on another machine).
pub fn all_commands() -> Vec<(String, Vec<crate::commands::Cmd>)> {
    with(|c| {
        let mut q = c.prepare("SELECT cwd, data FROM commands")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.flatten().filter_map(|(k, d)| serde_json::from_str(&d).ok().map(|l| (k, l))).collect())
    })
    .unwrap_or_default()
}

/// Put one folder's kept list (its key as commands.rs makes it), or every folder's (None), out of date:
/// it's read again when next asked, and kept to show until then.
pub fn stale_commands(cwd: Option<&str>) {
    with(|c| match cwd {
        Some(cwd) => c.execute("UPDATE commands SET read_ms = 0 WHERE cwd = ?1", params![cwd]).map(|_| ()),
        None => c.execute("UPDATE commands SET read_ms = 0", []).map(|_| ()),
    });
}
