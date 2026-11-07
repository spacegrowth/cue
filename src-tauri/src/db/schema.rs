//! Opening the database: its tables, bringing an older file up to date, setting aside one that can't be
//! read, and the one-time import of the files Cue used before it.

use super::{path, BUSY_WAIT};
use super::items::put_item;
use super::live::put_exchange;
use crate::model::{now_ms, Item};
use crate::sessions::Session;
use rusqlite::{params, Connection};
use serde_json::Value;

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
  voice       INTEGER NOT NULL DEFAULT 0, -- yours, said by voice
  status_ask  TEXT NOT NULL DEFAULT '',    -- yours: what a prompt hook added to ask for a status list
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

-- Packets you set aside on a Review card: the executor's relay id and the packet.
CREATE TABLE IF NOT EXISTS set_aside (
  exec        TEXT NOT NULL,
  packet      INTEGER NOT NULL,
  at_ms       INTEGER NOT NULL,
  PRIMARY KEY (exec, packet)
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
  fresh        INTEGER NOT NULL DEFAULT 0, -- tokens by kind (logs/steps/model.rs Usage)
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

/// The database, opened; one that isn't a database (or is damaged where opening reads) is set aside
/// (`keep_aside`) and a new one started, so Cue keeps what it does from here on instead of nothing at all.
pub(super) fn open() -> rusqlite::Result<Connection> {
    let _ = std::fs::create_dir_all(crate::server::cue_dir());
    match open_at(&path()) {
        Err(e) if unreadable(&e) => {
            let kept = keep_aside(&path());
            eprintln!("cue: {} can't be read ({e}): kept as {}, starting a new one", path().display(), kept.display());
            open_at(&path())
        }
        r => r,
    }
}

/// SQLite saying the file isn't a database it can read.
fn unreadable(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt))
}

/// `db` (and its write-ahead log) moved aside, as `<name>.unreadable-<ms>`: the backup, for a person to look at.
fn keep_aside(db: &std::path::Path) -> std::path::PathBuf {
    let stamp = format!("unreadable-{}", now_ms());
    let kept = db.with_extension(format!("db.{stamp}"));
    let _ = std::fs::rename(db, &kept);
    for side in ["db-wal", "db-shm"] {
        let _ = std::fs::rename(db.with_extension(side), db.with_extension(format!("{side}.{stamp}")));
    }
    kept
}

fn open_at(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    // Another connection writing (a test Cue, sqlite3 in a terminal) makes a write wait its turn, for up to
    // BUSY_WAIT and no longer: DB stays locked for every other caller in Cue while it waits.
    c.busy_timeout(BUSY_WAIT)?;
    // WAL: a crash mid-write never corrupts what was already saved.
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "NORMAL")?;
    c.execute_batch(SCHEMA)?;
    import_old_files(&c);
    let version = c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?;
    migrate(&c, version)?;
    Ok(c)
}

/// A database at `version` (its user_version) brought up to date, one version at a time.
fn migrate(c: &Connection, version: i64) -> rusqlite::Result<()> {
    if version < 1 {
        regroup_projects(c);
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
    // A message of yours said by voice: the column. A new database has it already.
    if version < 7 {
        let _ = c.execute("ALTER TABLE exchanges ADD COLUMN voice INTEGER NOT NULL DEFAULT 0", []);
        c.pragma_update(None, "user_version", 7)?;
    }
    // The status list a prompt hook asked for with a message of yours: the column. A new database has it already.
    if version < 8 {
        let _ = c.execute("ALTER TABLE exchanges ADD COLUMN status_ask TEXT NOT NULL DEFAULT ''", []);
        c.pragma_update(None, "user_version", 8)?;
    }
    Ok(())
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
            // Once, as the database opens: each folder looked at first (asking alone doesn't wait).
            crate::logs::repos::first_look(&cwd);
            let Some(root) = crate::logs::repos::repo_root(&cwd) else { continue };
            let project = crate::logs::repos::repo_name(&root);
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
