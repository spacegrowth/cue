//! Everything Cue keeps lives in one SQLite file, cue.db in Cue's data folder: answered history, live sessions
//! (so a restart or crash doesn't forget them), every session's full conversation, and your
//! unsent drafts. Settings stay in config.json (the Pi extension and hooks read it directly) and
//! images stay as files in uploads/ (rows hold their paths).
//!
//! Sessions are never deleted, only marked ended, and `exchanges` is an append-only log of every
//! session's back-and-forth (the window shows the last few). Both are there for the agents that
//! drive other agents (a lead and its helpers): who led whom, and what each one said.

use crate::model::{now_ms, Answer, Exchange, Item};
use crate::sessions::{Parked, Session};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  id          TEXT PRIMARY KEY,
  harness     TEXT NOT NULL DEFAULT '',
  project     TEXT NOT NULL DEFAULT '',
  cwd         TEXT NOT NULL DEFAULT '',
  state       TEXT NOT NULL DEFAULT '',
  driven_by   TEXT NOT NULL DEFAULT '',
  started_ms  INTEGER NOT NULL,
  updated_ms  INTEGER NOT NULL,
  ended_ms    INTEGER,
  data        TEXT NOT NULL            -- the whole Session as JSON (origin, bars, recent thread, queue)
);
CREATE INDEX IF NOT EXISTS sessions_live ON sessions(ended_ms, updated_ms);
CREATE INDEX IF NOT EXISTS sessions_driver ON sessions(driven_by);

-- SHORTCUT: exchanges grows forever (each row capped at 6,000 chars); fine for years of normal use.
-- Upgrade path: a retention setting that prunes ended sessions' logs older than N days.
CREATE TABLE IF NOT EXISTS exchanges (
  session_id  TEXT NOT NULL,
  at_ms       INTEGER NOT NULL,
  role        TEXT NOT NULL,           -- 'agent' | 'you'
  text        TEXT NOT NULL,
  images      TEXT NOT NULL DEFAULT '[]',
  PRIMARY KEY (session_id, at_ms, role)
);

CREATE TABLE IF NOT EXISTS items (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL DEFAULT '',
  kind        TEXT NOT NULL,           -- 'permission' | 'question' | 'waiting'
  status      TEXT NOT NULL,           -- 'pending' | 'answered' | 'gone' | ...
  project     TEXT NOT NULL DEFAULT '',
  harness     TEXT NOT NULL DEFAULT '',
  created_ms  INTEGER NOT NULL,
  resolved_ms INTEGER,
  outcome     TEXT NOT NULL DEFAULT '',
  data        TEXT NOT NULL            -- the whole Item as JSON
);
CREATE INDEX IF NOT EXISTS items_history ON items(status, resolved_ms);
CREATE INDEX IF NOT EXISTS items_session ON items(session_id);

-- Sessions you parked (stopped, kept to resume): until you resume or unpark them.
CREATE TABLE IF NOT EXISTS parked (
  id          TEXT PRIMARY KEY,
  parked_ms   INTEGER NOT NULL,
  data        TEXT NOT NULL            -- the whole Parked as JSON (the session as it was, and when)
);

-- Reviews waiting for their lead to be free (Review while it works): sent in order, one at a time.
CREATE TABLE IF NOT EXISTS review_queue (
  exec        TEXT PRIMARY KEY,        -- the executor's Cue session id
  lead        TEXT NOT NULL,           -- its lead's Cue session id
  text        TEXT NOT NULL,           -- what's typed into the lead ("/relay:review <id>")
  at_ms       INTEGER NOT NULL
);

-- The "/" commands Claude Code reports for a folder (see commands.rs): read once, kept a week, so a
-- relaunch of Cue doesn't ask again.
CREATE TABLE IF NOT EXISTS commands (
  cwd         TEXT PRIMARY KEY,
  read_ms     INTEGER NOT NULL,
  data        TEXT NOT NULL            -- the list as JSON
);

-- Insights (see insights.rs): each time one was shown, acted on, put off or dismissed, and what acting
-- on it saved, in tokens, where that's known. What the Insights settings count; nothing else reads it.
-- SHORTCUT: grows by a few rows per session; fine for years. Upgrade path: prune rows older than a year.
CREATE TABLE IF NOT EXISTS insight_events (
  at_ms       INTEGER NOT NULL,
  session_id  TEXT NOT NULL,
  rule        TEXT NOT NULL,           -- 'handoff' | 'cache' | 'queue'
  action      TEXT NOT NULL,           -- 'shown' | 'acted' | 'later' | 'dismissed'
  detail      TEXT NOT NULL DEFAULT '',-- what was picked ('/relay:handoff', '/compact')
  tokens      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS insight_events_at ON insight_events(at_ms);

-- Hand-offs (insights.js): a session that sent its hand-off skill, until you continue it in a new one
-- (then which). Kept here so a hand-off you left mid-way is still offered after a restart.
CREATE TABLE IF NOT EXISTS handoffs (
  session_id   TEXT PRIMARY KEY,
  skill        TEXT NOT NULL,           -- the skill it sent ('handoff'): the new one starts with '/<skill> resume'
  sent_ms      INTEGER NOT NULL,
  next_session TEXT NOT NULL DEFAULT '',
  continued_ms INTEGER NOT NULL DEFAULT 0,
  resume       INTEGER NOT NULL DEFAULT 1, -- 1: the skill takes 'resume'; 0: Cue's own message starts the new one
  note         TEXT NOT NULL DEFAULT '',  -- its hand-off note's path, as its log names it ('' when Cue didn't see it written)
  note_ms      INTEGER NOT NULL DEFAULT 0, -- when that note was written, by its log's clock
  ahead        INTEGER NOT NULL DEFAULT 0  -- written before its cache ran out (1: you, Write hand-off now; 2: its
                                           -- "Hand off if I don't come back"; 3: Settings, every big session): its
                                           -- session stays open when it continues, and a reply there drops it
);

-- Sessions to hand off if you don't come back (insights' "Hand off if I don't come back", per session):
-- Cue asks for the note a minute before their cache runs out.
CREATE TABLE IF NOT EXISTS ahead_watch (
  session_id TEXT PRIMARY KEY,
  set_ms     INTEGER NOT NULL
);

-- The Insights tab (logs/turns.rs reads each session's own log into it, a few MB at a time in the
-- background): one row per turn, what it cost and how it went, with the start of its prompt (and of the
-- next, when that corrected it) for the tab's examples. A turn still open when read is read again next
-- time, so rows are upserted by (session_id, at_ms).
-- SHORTCUT: grows by a row per turn and is never pruned; small rows, fine for years. Upgrade path: drop
-- rows older than the longest range the tab shows.
CREATE TABLE IF NOT EXISTS turns (
  session_id   TEXT NOT NULL,
  at_ms        INTEGER NOT NULL,        -- when its prompt was sent
  tokens       INTEGER NOT NULL DEFAULT 0,
  context      INTEGER NOT NULL DEFAULT 0, -- the context its last reply carried
  prompt_chars INTEGER NOT NULL DEFAULT 0,
  model        TEXT NOT NULL DEFAULT '',
  cwd          TEXT NOT NULL DEFAULT '',
  machine      TEXT NOT NULL DEFAULT '', -- '' = this Mac
  harness      TEXT NOT NULL DEFAULT '', -- 'claude' | 'codex' | 'pi'
  errors       INTEGER NOT NULL DEFAULT 0,
  interrupted  INTEGER NOT NULL DEFAULT 0,
  corrected    INTEGER NOT NULL DEFAULT 0,
  fresh        INTEGER NOT NULL DEFAULT 0, -- tokens by kind (logs/steps.rs Usage)
  output       INTEGER NOT NULL DEFAULT 0,
  cache_read   INTEGER NOT NULL DEFAULT 0,
  write_5m     INTEGER NOT NULL DEFAULT 0,
  write_1h     INTEGER NOT NULL DEFAULT 0,
  first_write  INTEGER NOT NULL DEFAULT 0, -- what its first reply wrote into the cache
  first_read   INTEGER NOT NULL DEFAULT 0,
  waited_ms    INTEGER NOT NULL DEFAULT 0, -- how long the session had waited on you before it
  prompt       TEXT NOT NULL DEFAULT '',
  correction   TEXT NOT NULL DEFAULT '',
  looks        INTEGER NOT NULL DEFAULT 0, -- reads and searches it ran itself
  agents       INTEGER NOT NULL DEFAULT 0, -- background agents it started
  relay        INTEGER NOT NULL DEFAULT 0, -- relay executors it spawned
  areas        TEXT NOT NULL DEFAULT '',   -- the project areas it edited, one per line
  auto_compact INTEGER NOT NULL DEFAULT 0, -- the context Claude Code compacted it at on its own (0: it didn't)
  PRIMARY KEY (session_id, at_ms)
);
CREATE INDEX IF NOT EXISTS turns_at ON turns(at_ms);

-- How far logs/turns.rs has read each log: the byte its next read starts at (where its open turn
-- starts), and the file's size and modified time then, so an unchanged log isn't opened again.
CREATE TABLE IF NOT EXISTS turns_read (
  path        TEXT PRIMARY KEY,
  offset      INTEGER NOT NULL,
  size        INTEGER NOT NULL,
  mtime_ms    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS drafts (
  -- SHORTCUT: draft images are stored inline as data URLs (up to 6 × 15 MB); fine for a few drafts.
  -- Upgrade path: save them to uploads/ and keep paths, like sent images.
  key         TEXT PRIMARY KEY,        -- 's:<session id>'
  text        TEXT NOT NULL DEFAULT '',
  images      TEXT NOT NULL DEFAULT '[]',
  updated_ms  INTEGER NOT NULL
);
"#;

fn path() -> std::path::PathBuf {
    crate::server::cue_dir().join("cue.db")
}

/// Run `f` on the open database. A database that can't be opened is logged once and every call
/// becomes a no-op: Cue keeps working from memory rather than refusing to start.
fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    let mut guard = crate::locking::lock(&DB, "DB");
    if guard.is_none() {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        *guard = match open() {
            Ok(c) => Some(c),
            Err(e) => {
                if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("cue: can't open {}: {e}", path().display());
                }
                return None;
            }
        };
    }
    match f(guard.as_ref()?) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("cue: database: {e}");
            None
        }
    }
}

fn open() -> rusqlite::Result<Connection> {
    let _ = std::fs::create_dir_all(crate::server::cue_dir());
    let c = Connection::open(path())?;
    // WAL: a crash mid-write never corrupts what was already saved.
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "NORMAL")?;
    c.execute_batch(SCHEMA)?;
    import_old_files(&c);
    let version = c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?;
    if version < 1 {
        regroup_projects(&c);
    }
    // Turns read before Cue counted a Codex read's first call, and when it took Claude Code's own turns (a
    // background task reporting back) for answers of yours: they only hold what the logs say, so they're
    // read again.
    if version < 2 {
        c.execute_batch("DELETE FROM turns; DELETE FROM turns_read;")?;
        c.pragma_update(None, "user_version", 2)?;
    }
    // What each turn looked up, delegated and edited (Insights' "could have delegated"): the columns, and
    // every log read again so turns read before have them too. A new database has them already.
    if version < 3 {
        for col in ["looks INTEGER NOT NULL DEFAULT 0", "agents INTEGER NOT NULL DEFAULT 0", "relay INTEGER NOT NULL DEFAULT 0", "areas TEXT NOT NULL DEFAULT ''"] {
            let _ = c.execute(&format!("ALTER TABLE turns ADD COLUMN {col}"), []);
        }
        c.execute_batch("DELETE FROM turns; DELETE FROM turns_read;")?;
        c.pragma_update(None, "user_version", 3)?;
    }
    // A hand-off note Cue saw a session write (its path, and when): the columns. A new database has them already.
    if version < 4 {
        for col in ["note TEXT NOT NULL DEFAULT ''", "note_ms INTEGER NOT NULL DEFAULT 0"] {
            let _ = c.execute(&format!("ALTER TABLE handoffs ADD COLUMN {col}"), []);
        }
        c.pragma_update(None, "user_version", 4)?;
    }
    // Where Claude Code compacted a session on its own (insights' auto-compact heads-up): the column, and
    // every log read again to fill it in (its turns are kept meanwhile, only brought up to date).
    if version < 5 {
        let _ = c.execute("ALTER TABLE turns ADD COLUMN auto_compact INTEGER NOT NULL DEFAULT 0", []);
        c.execute_batch("DELETE FROM turns_read;")?;
        c.pragma_update(None, "user_version", 5)?;
    }
    // A hand-off note written before its session's cache ran out: the column. A new database has it already.
    if version < 6 {
        let _ = c.execute("ALTER TABLE handoffs ADD COLUMN ahead INTEGER NOT NULL DEFAULT 0", []);
        c.pragma_update(None, "user_version", 6)?;
    }
    Ok(c)
}

/// Once: a project used to be its folder's name; now it's the folder's git repository, so a worktree's
/// sessions and answers join their repository's. Rows from other machines, and folders no longer on
/// disk, keep the name they have.
fn regroup_projects(c: &Connection) {
    let regroup = |table: &str| -> rusqlite::Result<usize> {
        let rows: Vec<(String, String)> = {
            let mut q = c.prepare(&format!(
                "SELECT id, json_extract(data, '$.cwd') FROM {table}
                 WHERE COALESCE(json_extract(data, '$.machine'), '') = '' AND COALESCE(json_extract(data, '$.cwd'), '') != ''"))?;
            let r = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            r
        };
        let mut n = 0;
        for (id, cwd) in rows {
            let Some(root) = crate::logs::live::repo_root(&cwd) else { continue };
            let project = crate::logs::live::repo_name(&root);
            let column = if table == "parked" { "" } else { ", project = ?1" };
            n += c.execute(&format!("UPDATE {table} SET data = json_set(data, '$.project', ?1){column} WHERE id = ?2"), params![project, id])?;
        }
        Ok(n)
    };
    for table in ["sessions", "items", "parked"] {
        if let Err(e) = regroup(table) {
            eprintln!("cue: regrouping {table} by repository: {e}");
        }
    }
}

// ---------- items: decisions and finished turns, pending and answered ----------

fn put_item(c: &Connection, it: &Item) -> rusqlite::Result<()> {
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

// ---------- live sessions and their finished-turn cards ----------

/// Mirror what's live in memory: upsert every session (and append its new exchanges to the log),
/// mark the ones that are gone as ended, and keep exactly the pending finished-turn cards.
/// Pending decisions aren't kept: their hook reconnects after a restart and asks again.
pub fn sync_live(sessions: &[Session], waiting: &[&Item]) {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        let now = now_ms() as i64;
        for s in sessions {
            tx.execute(
                "INSERT INTO sessions (id, harness, project, cwd, state, driven_by, started_ms, updated_ms, ended_ms, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL, ?8)
                 ON CONFLICT(id) DO UPDATE SET harness = excluded.harness, project = excluded.project, cwd = excluded.cwd,
                   state = excluded.state, driven_by = excluded.driven_by, updated_ms = excluded.updated_ms,
                   ended_ms = NULL, data = excluded.data",
                params![
                    s.origin.session_id,
                    s.origin.harness,
                    s.project,
                    s.origin.cwd,
                    s.state,
                    s.driven_by,
                    now,
                    serde_json::to_string(s).unwrap_or_default(),
                ],
            )?;
            for e in &s.thread {
                put_exchange(&tx, &s.origin.session_id, e)?;
            }
            // A message that moved (one you queued, put below the reply it waited behind) leaves its
            // old row: within the thread's span, the log keeps only what the thread has.
            if let Some(first) = s.thread.first() {
                let mut q = tx.prepare("SELECT at_ms, role FROM exchanges WHERE session_id = ?1 AND at_ms >= ?2")?;
                let rows: Vec<(i64, String)> = q.query_map(params![s.origin.session_id, first.at_ms as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();
                drop(q);
                for (at, role) in rows.iter().filter(|(at, role)| !s.thread.iter().any(|e| e.at_ms as i64 == *at && e.role == *role)) {
                    tx.execute("DELETE FROM exchanges WHERE session_id = ?1 AND at_ms = ?2 AND role = ?3", params![s.origin.session_id, at, role])?;
                }
            }
        }
        // Whatever isn't live any more has ended (its agent exited, or it said so).
        let live: Vec<&str> = sessions.iter().map(|s| s.origin.session_id.as_str()).collect();
        let mut q = tx.prepare("SELECT id FROM sessions WHERE ended_ms IS NULL")?;
        let open: Vec<String> = q.query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
        drop(q);
        for id in open.iter().filter(|id| !live.contains(&id.as_str())) {
            tx.execute("UPDATE sessions SET ended_ms = ?2 WHERE id = ?1", params![id, now])?;
        }
        tx.execute("DELETE FROM items WHERE status = 'pending'", [])?;
        for it in waiting {
            put_item(&tx, it)?;
        }
        tx.commit()
    });
}

fn put_exchange(c: &Connection, session_id: &str, e: &Exchange) -> rusqlite::Result<()> {
    // Same moment, same speaker: the same exchange (its images may have been filled in since).
    c.execute(
        "INSERT INTO exchanges (session_id, at_ms, role, text, images) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id, at_ms, role) DO UPDATE SET text = excluded.text, images = excluded.images",
        params![session_id, e.at_ms as i64, e.role, e.text, serde_json::to_string(&e.images).unwrap_or_else(|_| "[]".into())],
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

// ---------- parked sessions ----------

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

// ---------- search: everything Cue has logged ----------

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
        let mut st = c.prepare("SELECT at_ms, role, text, images FROM exchanges WHERE session_id = ?1 ORDER BY at_ms")?;
        let rows = st.query_map(params![session_id], log_row)?;
        rows.collect()
    })
    .unwrap_or_default()
}

/// One page of a session's conversation: the `limit` messages before `before_ms`, oldest first (the
/// chat loads earlier messages a page at a time as you scroll up). Fewer than `limit`: that's the start.
pub fn session_log_page(session_id: &str, before_ms: u64, limit: usize) -> Vec<Value> {
    let mut page = with(|c| {
        let mut st = c.prepare("SELECT at_ms, role, text, images FROM exchanges WHERE session_id = ?1 AND at_ms < ?2 ORDER BY at_ms DESC LIMIT ?3")?;
        let rows = st.query_map(params![session_id, before_ms as i64, limit as i64], log_row)?;
        rows.collect::<rusqlite::Result<Vec<Value>>>()
    })
    .unwrap_or_default();
    page.reverse();
    page
}

fn log_row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let images: Vec<String> = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default();
    Ok(json!({ "at_ms": r.get::<_, i64>(0)?, "role": r.get::<_, String>(1)?, "text": r.get::<_, String>(2)?, "images": images }))
}

/// A line of context around the first (case-insensitive) match: ~60 characters before, ~140 after.
fn snippet(text: &str, q: &str) -> String {
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

// ---------- drafts: what you've typed but not sent, per session ----------

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

/// The reviews waiting for their leads, oldest first: (executor, lead, text, when).
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

// ---------- "/" commands by folder ----------

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

/// After the data folder moved, saved image paths still point at the old one: point them at the new.
pub fn rewrite_paths(old: &str, new: &str) {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("UPDATE items SET data = replace(data, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE sessions SET data = replace(data, ?1, ?2)", params![old, new])?;
        tx.execute("UPDATE exchanges SET images = replace(images, ?1, ?2), text = replace(text, ?1, ?2)", params![old, new])?;
        tx.commit()
    });
}

// ---------- insights ----------

/// One thing an insight did, or you did with it (see insight_events above).
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

/// A session sent its hand-off skill (or Cue's own): it's offered "Continue in a new session" until it does.
/// `resume`: the skill takes "resume" for the new session; otherwise Cue's own message starts it. A note
/// seen before is forgotten (this hand-off writes its own; an older one, written before `sent_ms`, isn't taken again).
/// `ahead`: asked before its cache ran out (see the handoffs table), else 0.
pub fn handoff_sent(session_id: &str, skill: &str, resume: bool, ahead: u8) {
    with(|c| c.execute("INSERT INTO handoffs (session_id, skill, sent_ms, resume, ahead) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(session_id) DO UPDATE SET skill = ?2, sent_ms = ?3, resume = ?4, next_session = '', continued_ms = 0, note = '', note_ms = 0, ahead = ?5",
        params![session_id, skill, now_ms() as i64, resume, ahead]));
}

/// A hand-off written ahead that you didn't continue from (you replied in its session): gone, so its
/// session is offered hand-offs as before.
pub fn handoff_drop(session_id: &str) {
    with(|c| c.execute("DELETE FROM handoffs WHERE session_id = ?1", params![session_id]));
}

/// "Hand off if I don't come back" on (or off) for a session.
pub fn ahead_watch(session_id: &str, on: bool) {
    if on {
        with(|c| c.execute("INSERT OR REPLACE INTO ahead_watch (session_id, set_ms) VALUES (?1, ?2)", params![session_id, now_ms() as i64]));
    } else {
        with(|c| c.execute("DELETE FROM ahead_watch WHERE session_id = ?1", params![session_id]));
    }
}

/// The sessions with "Hand off if I don't come back" on.
pub fn ahead_watched() -> Vec<String> {
    with(|c| {
        let mut q = c.prepare("SELECT session_id FROM ahead_watch")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }).unwrap_or_default()
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
}

/// It continued in `next`: the link between the two.
pub fn handoff_continued(session_id: &str, next: &str) {
    with(|c| c.execute("UPDATE handoffs SET next_session = ?2, continued_ms = ?3 WHERE session_id = ?1", params![session_id, next, now_ms() as i64]));
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
    let since = now_ms().saturating_sub(30 * 24 * 3600 * 1000) as i64;
    let rows = with(|c| {
        let mut q = c.prepare("SELECT session_id, skill, sent_ms, next_session, continued_ms, resume, note_ms, ahead FROM handoffs WHERE sent_ms >= ?1")?;
        let rows = q.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, json!({ "skill": r.get::<_, String>(1)?, "sent_ms": r.get::<_, i64>(2)?, "next": r.get::<_, String>(3)?, "continued_ms": r.get::<_, i64>(4)?, "resume": r.get::<_, bool>(5)?, "note_ms": r.get::<_, i64>(6)?, "ahead": r.get::<_, i64>(7)? }))))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }).unwrap_or_default();
    Value::Object(rows.into_iter().collect())
}

/// Per insight since `since_ms`: how often it was shown and acted on, and the tokens acting on it saved.
pub fn insight_counts(since_ms: u64) -> Vec<(String, u64, u64, u64)> {
    with(|c| {
        let mut q = c.prepare("SELECT rule, SUM(action = 'shown'), SUM(action = 'acted'), SUM(CASE WHEN action = 'acted' THEN tokens ELSE 0 END) FROM insight_events WHERE at_ms >= ?1 GROUP BY rule ORDER BY rule")?;
        let rows = q.query_map(params![since_ms as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64, r.get::<_, i64>(3)? as u64)))?;
        rows.collect()
    }).unwrap_or_default()
}

// ---------- turns: the Insights tab ----------

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

/// Hand-offs sent since `since_ms`, and how many of them were continued in a new session.
pub fn handoff_counts(since_ms: u64) -> (u64, u64) {
    with(|c| c.query_row("SELECT COUNT(*), COALESCE(SUM(next_session != ''), 0) FROM handoffs WHERE sent_ms >= ?1", params![since_ms as i64], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)))).unwrap_or((0, 0))
}

// ---------- one-time import of the files Cue used before the database ----------

/// history.jsonl and live.json are read in once, then renamed to *.imported (kept as a backup).
fn import_old_files(c: &Connection) {
    let dir = crate::server::cue_dir();
    let history = dir.join("history.jsonl");
    if let Ok(text) = std::fs::read_to_string(&history) {
        for it in text.lines().filter_map(|l| serde_json::from_str::<Item>(l).ok()) {
            let _ = put_item(c, &it);
        }
        let _ = std::fs::rename(&history, dir.join("history.jsonl.imported"));
    }
    let live = dir.join("live.json");
    if let Ok(v) = std::fs::read_to_string(&live).and_then(|t| serde_json::from_str::<Value>(&t).map_err(std::io::Error::other)) {
        let sessions: Vec<Session> = serde_json::from_value(v["sessions"].clone()).unwrap_or_default();
        let now = now_ms() as i64;
        for s in &sessions {
            let _ = c.execute(
                "INSERT OR IGNORE INTO sessions (id, harness, project, cwd, state, driven_by, started_ms, updated_ms, ended_ms, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL, ?8)",
                params![s.origin.session_id, s.origin.harness, s.project, s.origin.cwd, s.state, s.driven_by, now, serde_json::to_string(s).unwrap_or_default()],
            );
            for e in &s.thread {
                let _ = put_exchange(c, &s.origin.session_id, e);
            }
        }
        for it in serde_json::from_value::<Vec<Item>>(v["waiting"].clone()).unwrap_or_default() {
            let _ = put_item(c, &it);
        }
        let _ = std::fs::rename(&live, dir.join("live.json.imported"));
    }
}

/// Tests that point CUE_HOME somewhere else take this first, so they don't trip over each other.
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// For tests: forget the open connection so the next call opens CUE_HOME's database afresh.
#[cfg(test)]
pub fn reset() {
    *crate::locking::lock(&DB, "DB") = None;
}

/// How many exchanges the log holds for a session.
#[cfg(test)]
pub fn exchange_count(session_id: &str) -> i64 {
    with(|c| c.query_row("SELECT COUNT(*) FROM exchanges WHERE session_id = ?1", params![session_id], |r| r.get(0))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Origin;

    #[test]
    fn a_database_from_before_notes_ahead_keeps_its_hand_offs() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-db-v6-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        // Version 5's hand-offs table (no `ahead`), with one waiting to continue.
        {
            let c = Connection::open(path()).unwrap();
            c.execute_batch("CREATE TABLE handoffs (session_id TEXT PRIMARY KEY, skill TEXT NOT NULL, sent_ms INTEGER NOT NULL, next_session TEXT NOT NULL DEFAULT '', continued_ms INTEGER NOT NULL DEFAULT 0, resume INTEGER NOT NULL DEFAULT 1, note TEXT NOT NULL DEFAULT '', note_ms INTEGER NOT NULL DEFAULT 0);
                INSERT INTO handoffs (session_id, skill, sent_ms, resume, note, note_ms) VALUES ('s1', 'handoff', 100, 1, '/r/HANDOFF.md', 120);
                PRAGMA user_version = 5;").unwrap();
        }
        reset();
        let k = handoff("s1").unwrap();
        assert_eq!((k.skill.as_str(), k.note.as_str(), k.note_ms, k.ahead), ("handoff", "/r/HANDOFF.md", 120, 0), "kept, and not ahead");
        handoff_sent("s2", "cue:handoff", false, 2);
        assert_eq!(handoff("s2").map(|k| k.ahead), Some(2));
        ahead_watch("s2", true);
        assert_eq!(ahead_watched(), ["s2"]);
        // Opened again (a version 6 database): nothing to do, nothing lost.
        reset();
        assert_eq!(handoff("s2").map(|k| k.ahead), Some(2));
        std::env::remove_var("CUE_HOME");
        reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn session(id: &str, driven_by: &str) -> Session {
        let mut s: Session = serde_json::from_value(json!({
            "session_id": id, "harness": "claude", "cwd": "/x/proj", "project": "proj", "state": "waiting",
            "driven_by": driven_by, "queued": null, "since_ms": now_ms(), "prompt": "", "segments": [], "thread": []
        }))
        .unwrap();
        s.thread = vec![Exchange { role: "agent".into(), text: "done".into(), at_ms: 1, images: vec![], from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![] }];
        s
    }

    #[test]
    fn snippet_centres_on_the_match() {
        let long = format!("{} needle here {}", "a ".repeat(100), "b ".repeat(100));
        let s = snippet(&long, "NEEDLE");
        assert!(s.starts_with('…') && s.ends_with('…') && s.contains("needle here"), "{s}");
        assert_eq!(snippet("short text", "text"), "short text");
    }

    #[test]
    fn a_session_named_what_you_typed_is_found_however_many_newer_ones_mention_it() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();
        with(|c| {
            // The oldest is named exactly "web"; 25 newer ones only have "web" somewhere in their names.
            let add = |id: &str, name: &str, at: i64| c.execute("INSERT INTO sessions (id, project, started_ms, updated_ms, ended_ms, data) VALUES (?1, 'x', 1, ?2, ?2, ?3)", params![id, at, json!({ "name": name }).to_string()]);
            add("exact", "web", 1)?;
            add("start", "web-shop", 2)?;
            for n in 0..25 {
                add(&format!("in{n}"), &format!("my-cobweb-{n}"), 100 + n)?;
            }
            Ok(())
        })
        .unwrap();
        let found: Vec<String> = search("Web", 10).iter().filter(|r| r["kind"] == "session").map(|r| r["session_id"].as_str().unwrap().to_string()).collect();
        std::env::remove_var("CUE_HOME");
        reset();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(found.first().map(String::as_str), Some("exact"), "the whole name first: {found:?}");
        assert_eq!(found.get(1).map(String::as_str), Some("start"), "then one it starts: {found:?}");
        assert_eq!(found.len(), 20);
    }

    #[test]
    fn delete_takes_everything_of_that_session_and_nothing_of_another() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();
        with(|c| {
            for id in ["gone", "kept"] {
                c.execute("INSERT INTO sessions (id, project, started_ms, updated_ms, ended_ms, data) VALUES (?1, 'x', 1, 2, 2, '{}')", params![id])?;
                c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES (?1, 1, 'you', 'hi')", params![id])?;
                c.execute("INSERT INTO items (id, session_id, kind, status, created_ms, data) VALUES (?1, ?2, 'waiting', 'answered', 1, '{}')", params![format!("i-{id}"), id])?;
                c.execute("INSERT INTO parked (id, parked_ms, data) VALUES (?1, 1, '{}')", params![id])?;
                c.execute("INSERT INTO drafts (key, text, updated_ms) VALUES (?1, 'draft', 1)", params![format!("s:{id}")])?;
            }
            Ok(())
        })
        .unwrap();
        delete_session("gone");
        let count = |id: &str| -> i64 {
            with(|c| {
                c.query_row(
                    "SELECT (SELECT COUNT(*) FROM sessions WHERE id = ?1) + (SELECT COUNT(*) FROM exchanges WHERE session_id = ?1) + (SELECT COUNT(*) FROM items WHERE session_id = ?1) + (SELECT COUNT(*) FROM parked WHERE id = ?1) + (SELECT COUNT(*) FROM drafts WHERE key = 's:' || ?1)",
                    params![id],
                    |r| r.get(0),
                )
            })
            .unwrap_or(-1)
        };
        let (gone, kept) = (count("gone"), count("kept"));
        std::env::remove_var("CUE_HOME");
        reset();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(gone, 0);
        assert_eq!(kept, 5);
    }

    #[test]
    fn the_log_comes_a_page_at_a_time_oldest_first() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();
        with(|c| {
            for n in 1..=7 {
                c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES ('s', ?1, ?2, ?3)", params![n * 10, if n % 2 == 1 { "you" } else { "agent" }, format!("m{n}")])?;
            }
            c.execute("INSERT INTO exchanges (session_id, at_ms, role, text) VALUES ('other', 15, 'you', 'x')", [])?;
            Ok(())
        })
        .unwrap();
        let texts = |v: Vec<Value>| v.iter().map(|e| e["text"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        // The chat shows m6, m7: the page before m6 is the 3 just before it, oldest first.
        assert_eq!(texts(session_log_page("s", 60, 3)), ["m3", "m4", "m5"]);
        assert_eq!(texts(session_log_page("s", 30, 3)), ["m1", "m2"], "fewer than asked: the start");
        assert!(session_log_page("s", 10, 3).is_empty());
        std::env::remove_var("CUE_HOME");
        reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Saved sessions, answers and parked ones in a worktree move to its repository's project, once;
    /// another machine's keep their folder's name.
    #[test]
    fn saved_projects_are_regrouped_by_repository() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-db-regroup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let git = |at: &std::path::Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(at).args(args).output().unwrap().status.success());
        let main = dir.join("app");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"]);
        git(&main, &["worktree", "add", "-q", "../wt/fix-it"]);
        let wt = dir.join("wt/fix-it").to_string_lossy().to_string();
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("CUE_HOME", &home);
        reset();
        let data = |machine: &str| json!({ "cwd": wt, "machine": machine, "project": "fix-it" }).to_string();
        with(|c| {
            c.execute("INSERT INTO sessions (id, project, cwd, started_ms, updated_ms, data) VALUES ('here', 'fix-it', ?1, 1, 1, ?2)", params![wt, data("")])?;
            c.execute("INSERT INTO sessions (id, project, cwd, started_ms, updated_ms, data) VALUES ('there', 'fix-it', ?1, 1, 1, ?2)", params![wt, data("box")])?;
            c.execute("INSERT INTO items (id, kind, status, project, created_ms, data) VALUES ('i', 'waiting', 'answered', 'fix-it', 1, ?1)", params![data("")])?;
            c.execute("INSERT INTO parked (id, parked_ms, data) VALUES ('p', 1, ?1)", params![data("")])?;
            c.pragma_update(None, "user_version", 0)
        })
        .unwrap();
        reset(); // the next launch
        let row = |sql: &str| with(|c| c.query_row(sql, [], |r| r.get::<_, String>(0))).unwrap();
        assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM sessions WHERE id = 'here'"), "app app");
        assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM sessions WHERE id = 'there'"), "fix-it fix-it");
        assert_eq!(row("SELECT project || ' ' || json_extract(data, '$.project') FROM items WHERE id = 'i'"), "app app");
        assert_eq!(row("SELECT json_extract(data, '$.project') FROM parked WHERE id = 'p'"), "app");
        std::env::remove_var("CUE_HOME");
        reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_folders_commands_are_kept_with_when_they_were_read_and_kept_when_out_of_date() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-db-cmds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();
        let list = vec![crate::commands::Cmd { name: "compact".into(), kind: "command".into(), desc: "Summarize".into(), hint: String::new(), args: vec![] }];
        assert!(commands("/x/cue").is_none());
        set_commands("/x/cue", 1_000, &list);
        reset(); // a relaunch: read from the file, not from memory
        assert_eq!(commands("/x/cue"), Some((1_000, list.clone())));
        set_commands("/x/cue", 2_000, &[]);
        assert_eq!(commands("/x/cue"), Some((2_000, vec![])), "the newer read wins");
        set_commands("/x/other", 3_000, &list);
        stale_commands(Some("/x/cue"));
        assert_eq!(commands("/x/cue"), Some((0, vec![])), "out of date, still there to show");
        assert_eq!(commands("/x/other"), Some((3_000, list.clone())), "only that folder");
        stale_commands(None);
        assert_eq!(commands("/x/other"), Some((0, list.clone())));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_sessions_cards_log_and_drafts_survive_a_reopen() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();

        let card = Item {
            id: "w1".into(), kind: "waiting".into(), origin: Origin { session_id: "lead".into(), ..Default::default() }, project: "proj".into(),
            tool_name: String::new(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: "done".into(),
            created_ms: 1, status: "pending".into(), outcome: String::new(), resolved_ms: None, thread: vec![], tool_use_id: None, scan_from: 0, at_prompt: false, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        sync_live(&[session("lead", ""), session("exec", "its relay lead")], &[&card]);
        set_draft("s:lead", "half a thought", &json!([]));

        reset(); // as if Cue restarted
        let (sessions, waiting) = live();
        assert_eq!(sessions.len(), 2);
        assert_eq!(waiting.len(), 1, "the finished-turn card is back");
        assert_eq!(drafts()["s:lead"]["text"], "half a thought");
        assert_eq!(exchange_count("exec"), 1);

        // The executor exits: it's marked ended, not deleted, and its log stays.
        sync_live(&[session("lead", "")], &[]);
        let (sessions, waiting) = live();
        assert_eq!(sessions.len(), 1);
        assert!(waiting.is_empty(), "a card that left memory leaves the database");
        assert_eq!(exchange_count("exec"), 1, "the ended session's conversation is kept");
        let driver: String = with(|c| c.query_row("SELECT driven_by FROM sessions WHERE id = 'exec'", [], |r| r.get(0))).unwrap();
        assert_eq!(driver, "its relay lead");

        set_draft("s:lead", "", &json!([]));
        assert!(drafts().get("s:lead").is_none(), "an emptied draft is removed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_old_files_are_imported_once_and_kept_as_a_backup() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-imp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        reset();
        // A line exactly as the old history.jsonl held it: one serialized Item.
        let it = Item {
            id: "h1".into(), kind: "permission".into(), origin: Origin { session_id: "s".into(), ..Default::default() }, project: "p".into(),
            tool_name: "Bash".into(), tool_input: json!({"command": "ls"}), suggestions: Value::Null, context: vec![], message: String::new(),
            created_ms: 5, status: "answered".into(), outcome: "allowed".into(), resolved_ms: Some(6), thread: vec![], tool_use_id: None, scan_from: 0, at_prompt: false, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        std::fs::write(dir.join("history.jsonl"), format!("{}\n", serde_json::to_string(&it).unwrap())).unwrap();
        let h = history(100);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].outcome, "allowed");
        assert!(dir.join("history.jsonl.imported").exists() && !dir.join("history.jsonl").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_old_folder_moves_over_and_image_paths_follow() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("cue-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("old"), root.join("new"));
        std::fs::create_dir_all(old.join("uploads")).unwrap();
        std::fs::create_dir_all(old.join("bin")).unwrap();
        std::fs::write(old.join("uploads/1.png"), b"png").unwrap();
        std::fs::write(old.join("config.json"), "{}").unwrap();
        std::fs::write(old.join("bin/cue-hook"), "#!/bin/sh").unwrap();

        let moved = crate::server::move_from_old_folder(&old, &new).expect("moved");
        assert!(new.join("uploads/1.png").exists() && new.join("config.json").exists());
        assert!(old.join("bin/cue-hook").exists(), "forwarders stay for already-running sessions");
        assert_eq!(std::fs::read_link(old.join("cue.sock")).unwrap(), new.join("cue.sock"));

        std::env::set_var("CUE_HOME", &new);
        reset();
        let mut s = session("m", "");
        s.thread[0].images = vec![format!("{moved}/uploads/1.png")];
        sync_live(&[s], &[]);
        rewrite_paths(&format!("{moved}/"), &format!("{}/", new.display()));
        let (live, _) = live();
        assert_eq!(live[0].thread[0].images[0], format!("{}/uploads/1.png", new.display()));
        assert!(crate::server::move_from_old_folder(&old, &new).is_none(), "only once: the database is there now");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
