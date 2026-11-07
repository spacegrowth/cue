//! Insights: what each insight did, and the turns read from agents' logs for the Insights tab.

use super::with;
use crate::model::now_ms;
use rusqlite::params;

/// One thing an insight did, or you did with it (see the insight_events table).
pub fn insight_event(session_id: &str, rule: &str, action: &str, detail: &str, tokens: u64) {
    with(|c| c.execute("INSERT INTO insight_events (at_ms, session_id, rule, action, detail, tokens) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![now_ms() as i64, session_id, rule, action, detail, tokens as i64]));
}

/// A session's insight events, oldest first: (rule, action, detail). For tests.
#[cfg(test)]
pub fn insight_events_of(session_id: &str) -> Vec<(String, String, String)> {
    with(|c| {
        let mut q = c.prepare("SELECT rule, action, detail FROM insight_events WHERE session_id = ?1 ORDER BY at_ms, rowid")?;
        let rows = q.query_map(params![session_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }).unwrap_or_default()
}

/// Per insight since `since_ms`: how often it was shown and acted on, and the tokens acting on it saved.
pub fn insight_counts(since_ms: u64) -> Vec<(String, u64, u64, u64)> {
    with(|c| {
        let mut q = c.prepare("SELECT rule, SUM(action = 'shown'), SUM(action = 'acted'), SUM(CASE WHEN action = 'acted' THEN tokens ELSE 0 END) FROM insight_events WHERE at_ms >= ?1 GROUP BY rule ORDER BY rule")?;
        let rows = q.query_map(params![since_ms as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64, r.get::<_, i64>(3)? as u64)))?;
        rows.collect()
    }).unwrap_or_default()
}

/// Where logs/turns.rs left off in a log: (offset, size, modified time), if it has read it before.
pub fn turns_read(path: &str) -> Option<(u64, u64, u64)> {
    use rusqlite::OptionalExtension;
    with(|c| c.query_row("SELECT offset, size, mtime_ms FROM turns_read WHERE path = ?1", params![path], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64))).optional()).flatten()
}

/// Who a log's turns belong to: its session, its machine ('' = this Mac) and its agent.
pub struct TurnsOf<'a> {
    pub session_id: &'a str,
    pub machine: &'a str,
    pub harness: &'a str,
}

/// A log's turns (new ones added, ones read before brought up to date) and where its next read starts,
/// in one go: a crash between the two never counts a turn twice or skips one.
pub fn save_turns(path: &str, of: &TurnsOf, rows: &[crate::logs::steps::TurnRow], read: (u64, u64, u64)) {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        {
            let mut q = tx.prepare_cached(
                "INSERT INTO turns (session_id, at_ms, tokens, context, prompt_chars, model, cwd, machine, harness, errors, interrupted, corrected,
                   fresh, output, cache_read, write_5m, write_1h, first_write, first_read, waited_ms, prompt, correction, looks, agents, relay, areas, auto_compact)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
                 ON CONFLICT(session_id, at_ms) DO UPDATE SET tokens = excluded.tokens, context = excluded.context, prompt_chars = excluded.prompt_chars,
                   model = excluded.model, cwd = excluded.cwd, machine = excluded.machine, harness = excluded.harness, errors = excluded.errors,
                   interrupted = excluded.interrupted, corrected = excluded.corrected, fresh = excluded.fresh, output = excluded.output,
                   cache_read = excluded.cache_read, write_5m = excluded.write_5m, write_1h = excluded.write_1h, first_write = excluded.first_write,
                   first_read = excluded.first_read, prompt = excluded.prompt, correction = excluded.correction,
                   looks = excluded.looks, agents = excluded.agents, relay = excluded.relay, areas = excluded.areas, auto_compact = excluded.auto_compact,
                   -- An open turn is read again from its own start, without the reply before it: keep the wait read the first time.
                   waited_ms = CASE WHEN excluded.waited_ms > 0 THEN excluded.waited_ms ELSE turns.waited_ms END",
            )?;
            for t in rows.iter().filter(|t| t.at_ms > 0) {
                let u = &t.usage;
                q.execute(params![of.session_id, t.at_ms as i64, t.tokens as i64, t.context as i64, t.prompt_chars as i64, t.model, t.cwd, of.machine, of.harness, t.errors as i64, t.interrupted, t.corrected,
                    u.fresh as i64, u.output as i64, u.read as i64, u.write_5m as i64, u.write_1h as i64, u.first_write as i64, u.first_read as i64, t.waited_ms as i64, t.prompt, t.correction,
                    t.looks as i64, t.agents as i64, t.relay as i64, t.areas, t.auto_compact as i64])?;
            }
        }
        tx.execute(
            "INSERT INTO turns_read (path, offset, size, mtime_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, size = excluded.size, mtime_ms = excluded.mtime_ms",
            params![path, read.0 as i64, read.1 as i64, read.2 as i64],
        )?;
        tx.commit()
    });
}

/// One stored turn, as the Insights tab sums it: `day` is its local date ("2026-10-17").
pub struct StoredTurn {
    pub day: String,
    pub session_id: String,
    pub tokens: u64,
    pub context: u64,
    pub model: String,
    pub cwd: String,
    pub machine: String,
    pub errors: u64,
    pub interrupted: bool,
    pub corrected: bool,
    pub at_ms: u64,
    pub harness: String,
    pub usage: crate::logs::steps::Usage,
    pub waited_ms: u64,
    pub prompt: String,
    pub correction: String,
    pub looks: u64,
    pub agents: u64,
    pub relay: u64,
    /// The project areas it edited, one per line.
    pub areas: String,
}

const STORED_TURN: &str = "SELECT date(at_ms / 1000, 'unixepoch', 'localtime'), session_id, tokens, context, model, cwd, machine, errors, interrupted, corrected,
   at_ms, harness, fresh, output, cache_read, write_5m, write_1h, first_write, first_read, waited_ms, prompt, correction, looks, agents, relay, areas
 FROM turns";

/// Every turn since `since_ms`, oldest first.
pub fn turns_since(since_ms: u64) -> Vec<StoredTurn> {
    with(|c| {
        let mut q = c.prepare(&format!("{STORED_TURN} WHERE at_ms >= ?1 ORDER BY at_ms"))?;
        let rows = q.query_map(params![since_ms as i64], stored_turn)?;
        rows.collect()
    })
    .unwrap_or_default()
}

/// One session's turns, oldest first.
pub fn turns_of(session_id: &str) -> Vec<StoredTurn> {
    with(|c| {
        let mut q = c.prepare_cached(&format!("{STORED_TURN} WHERE session_id = ?1 ORDER BY at_ms"))?;
        let rows = q.query_map(params![session_id], stored_turn)?;
        rows.collect()
    })
    .unwrap_or_default()
}

fn stored_turn(r: &rusqlite::Row) -> rusqlite::Result<StoredTurn> {
    Ok(StoredTurn {
        day: r.get(0)?,
        session_id: r.get(1)?,
        tokens: r.get::<_, i64>(2)? as u64,
        context: r.get::<_, i64>(3)? as u64,
        model: r.get(4)?,
        cwd: r.get(5)?,
        machine: r.get(6)?,
        errors: r.get::<_, i64>(7)? as u64,
        interrupted: r.get(8)?,
        corrected: r.get(9)?,
        at_ms: r.get::<_, i64>(10)? as u64,
        harness: r.get(11)?,
        usage: crate::logs::steps::Usage {
            fresh: r.get::<_, i64>(12)? as u64,
            output: r.get::<_, i64>(13)? as u64,
            read: r.get::<_, i64>(14)? as u64,
            write_5m: r.get::<_, i64>(15)? as u64,
            write_1h: r.get::<_, i64>(16)? as u64,
            first_write: r.get::<_, i64>(17)? as u64,
            first_read: r.get::<_, i64>(18)? as u64,
            last_ms: 0,
        },
        waited_ms: r.get::<_, i64>(19)? as u64,
        prompt: r.get(20)?,
        correction: r.get(21)?,
        looks: r.get::<_, i64>(22)? as u64,
        agents: r.get::<_, i64>(23)? as u64,
        relay: r.get::<_, i64>(24)? as u64,
        areas: r.get(25)?,
    })
}

/// The contexts Claude Code last compacted sessions on `model` at on its own, newest first (up to `n`).
pub fn auto_compacts(model: &str, n: u32) -> Vec<u64> {
    with(|c| {
        let mut q = c.prepare_cached("SELECT auto_compact FROM turns WHERE model = ?1 AND auto_compact > 0 ORDER BY at_ms DESC LIMIT ?2")?;
        let rows = q.query_map(params![model, n], |r| Ok(r.get::<_, i64>(0)? as u64))?;
        rows.collect()
    })
    .unwrap_or_default()
}

/// What each of `sessions` started at: the context of its first turn that carried one.
pub fn start_contexts(sessions: &[&str]) -> std::collections::HashMap<String, u64> {
    with(|c| {
        // SQLite takes the bare `context` from the row MIN(at_ms) picked.
        let mut q = c.prepare_cached("SELECT context, MIN(at_ms) FROM turns WHERE session_id = ?1 AND context > 0")?;
        let mut out = std::collections::HashMap::new();
        for sid in sessions {
            if let Some(ctx) = q.query_row(params![sid], |r| r.get::<_, Option<i64>>(0))? {
                out.insert(sid.to_string(), ctx as u64);
            }
        }
        Ok(out)
    })
    .unwrap_or_default()
}
