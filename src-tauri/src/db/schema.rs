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
  from_name   TEXT NOT NULL DEFAULT '',    -- another session's message: who sent it
  from_sid    TEXT NOT NULL DEFAULT '',    -- and that session's id, when Cue knew it
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
  exec        TEXT PRIMARY KEY,        -- the teammate's key (team:<lead>:<agent id>)
  lead        TEXT NOT NULL,           -- its lead's Cue session id
  text        TEXT NOT NULL,           -- what's typed into the lead
  at_ms       INTEGER NOT NULL
);

-- No longer read or written: kept so older databases need no migration.
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
  detail      TEXT NOT NULL DEFAULT '',-- what was picked ('/compact')
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
  relay        INTEGER NOT NULL DEFAULT 0, -- no longer written: kept so older databases need no migration
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
-- The Desk (desk/): each project's items and the decisions made on them, as JSON with the columns
-- they're looked up by. Done items and old decisions stay; loading takes the recent ones.
CREATE TABLE IF NOT EXISTS desk_spaces (
  id TEXT PRIMARY KEY,
  data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS desk_items (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL,
  state       TEXT NOT NULL,
  updated_ms  INTEGER NOT NULL,
  data        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS desk_items_project ON desk_items(project, state);
CREATE INDEX IF NOT EXISTS desk_items_done ON desk_items(project, state, updated_ms);

CREATE TABLE IF NOT EXISTS desk_decisions (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL,
  item_id     TEXT NOT NULL DEFAULT '',
  at_ms       INTEGER NOT NULL,
  data        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS desk_decisions_at ON desk_decisions(project, at_ms);

-- What each Desk's lead did on its own (#374: closed, started, sent back, moved up, took back a copy, answered a
-- session), a copy of its step kept until you clear the list (never pruned with the decisions); and when you last
-- cleared it.
CREATE TABLE IF NOT EXISTS desk_moves (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL,
  at_ms       INTEGER NOT NULL,
  data        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS desk_moves_at ON desk_moves(project, at_ms);
CREATE TABLE IF NOT EXISTS desk_moves_cleared (
  project     TEXT PRIMARY KEY,
  at_ms       INTEGER NOT NULL
);

-- Each Desk's change feed (desk/feed.rs): numbered rows, saved in the same write as the item change they come from.
CREATE TABLE IF NOT EXISTS desk_feed (
  project     TEXT NOT NULL,
  n           INTEGER NOT NULL,
  item        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  words       TEXT NOT NULL,
  key         TEXT NOT NULL DEFAULT '',
  created_ms  INTEGER NOT NULL,
  PRIMARY KEY (project, n)
);
-- Each Desk's two feed marks: rows sent up to `sent` (to lead `sent_lead`), heard up to `heard` (by `heard_lead`).
CREATE TABLE IF NOT EXISTS desk_feed_marks (
  project     TEXT PRIMARY KEY,
  sent        INTEGER NOT NULL,
  sent_lead   TEXT NOT NULL DEFAULT '',
  heard       INTEGER NOT NULL,
  heard_lead  TEXT NOT NULL DEFAULT ''
);

-- Each project's Desk lead: the session that runs its Desk, and the name it went by when last seen.
CREATE TABLE IF NOT EXISTS desk_lead (
  project     TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL,
  name        TEXT NOT NULL DEFAULT '',
  set_ms      INTEGER NOT NULL
);

-- Dropped Desk items' own copies that had work in them, kept and listed on their Desk (desk/model.rs KeptTree).
CREATE TABLE IF NOT EXISTS desk_kept (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL,
  data        TEXT NOT NULL
);

-- Each project's sessions on its Desk: the ones its lead may route to.
CREATE TABLE IF NOT EXISTS desk_members (
  project     TEXT NOT NULL,
  session_id  TEXT NOT NULL,
  added_ms    INTEGER NOT NULL,
  PRIMARY KEY (project, session_id)
);
"#;

/// The database, opened. One whose first read fails as not a database (or damaged there) is tried once more a
/// moment later (a synced folder replacing the file mid-copy); still so, it's set aside (`keep_aside`) and a new
/// one started, so Cue keeps what it does from here on instead of nothing at all. Past that first read, a
/// failure while it's set up (a damaged page a migration meets) is said and the database used as it is: one bad
/// page never costs everything else it holds.
pub(super) fn open() -> rusqlite::Result<Connection> {
    let _ = std::fs::create_dir_all(crate::server::cue_dir());
    let c = match first_read(&path()) {
        Err(e) if unreadable(&e) => {
            #[cfg(test)]
            tests_seam::BEFORE_RETRY.with(|then| then.take().map(|f| f()));
            crate::clock::sleep_ms(RETRY_MS);
            match first_read(&path()) {
                Err(e) if unreadable(&e) => {
                    let kept = keep_aside(&path());
                    eprintln!("cue: {} can't be read ({e}): kept as {}, starting a new one", path().display(), kept.display());
                    first_read(&path())?
                }
                r => r?,
            }
        }
        r => r?,
    };
    if let Err(e) = set_up(&c) {
        eprintln!("cue: {} isn't all readable ({e}): using it as it is", path().display());
    }
    // Busy from here on is said at once, and waited out by `with` with DB let go.
    c.busy_timeout(std::time::Duration::ZERO)?;
    Ok(c)
}

/// How long `open` waits before it tries a database that couldn't be read once more.
const RETRY_MS: u64 = 500;

/// Tests: what happens between `open`'s first try and its retry (once, on this thread).
#[cfg(test)]
pub mod tests_seam {
    use std::cell::Cell;
    thread_local! {
        pub static BEFORE_RETRY: Cell<Option<Box<dyn FnOnce()>>> = const { Cell::new(None) };
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

/// The database at `path`, opened and its header read (where a file that isn't a database says so).
fn first_read(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    // Another connection writing (a test Cue, sqlite3 in a terminal) makes setting it up wait its turn, for
    // up to BUSY_WAIT. Once it's open it doesn't wait at all (`open`): `with` waits instead, with DB let go.
    c.busy_timeout(BUSY_WAIT)?;
    c.pragma_query_value(None, "schema_version", |r| r.get::<_, i64>(0))?;
    Ok(c)
}

/// The database set up: its journal, its tables, what older Cues kept in files, and its migrations.
fn set_up(c: &Connection) -> rusqlite::Result<()> {
    // WAL: a crash mid-write never corrupts what was already saved.
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "NORMAL")?;
    c.execute_batch(SCHEMA)?;
    // The optional add-on's own tables.
    #[cfg(feature = "ext")]
    c.execute_batch(super::ext::SCHEMA)?;
    import_old_files(c);
    let version = c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?;
    migrate(c, version)
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
    migrate_on(c, version)
}

/// `migrate` from version 6 on: the columns added since, the Desk's tables marked, the old Dispatch dropped.
fn migrate_on(c: &Connection, version: i64) -> rusqlite::Result<()> {
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
    // The Desk's tables (in SCHEMA, so a database of any version has them by now): the version marks them.
    if version < 9 {
        c.pragma_update(None, "user_version", 9)?;
    }
    if version < 10 {
        drop_dispatch(c)?;
    }
    seed_desk_members(c, version)?;
    // Who sent another session's message: the columns, so an older page of a chat keeps its sender (#513). A new
    // database has them already.
    if version < 12 {
        let _ = c.execute("ALTER TABLE exchanges ADD COLUMN from_name TEXT NOT NULL DEFAULT ''", []);
        let _ = c.execute("ALTER TABLE exchanges ADD COLUMN from_sid TEXT NOT NULL DEFAULT ''", []);
        c.pragma_update(None, "user_version", 12)?;
    }
    Ok(())
}

/// Version 11 (a database older than that): each Desk keeps its own list of sessions. It starts with the sessions that hold its items now (not
/// done; a crew member's lead for one its crew holds), and its lead.
fn seed_desk_members(c: &Connection, version: i64) -> rusqlite::Result<()> {
    if version >= 11 {
        return Ok(());
    }
    c.execute_batch(
        "INSERT OR IGNORE INTO desk_members (project, session_id, added_ms)
         SELECT project, COALESCE(NULLIF(json_extract(data, '$.holder.lead_sid'), ''), json_extract(data, '$.holder.sid')), 0
         FROM desk_items
         WHERE state != 'done' AND COALESCE(NULLIF(json_extract(data, '$.holder.lead_sid'), ''), json_extract(data, '$.holder.sid'), '') != ''
         GROUP BY 1, 2;
         INSERT OR IGNORE INTO desk_members (project, session_id, added_ms)
         SELECT project, session_id, 0 FROM desk_lead WHERE session_id != '';",
    )?;
    c.pragma_update(None, "user_version", 11)
}

/// Version 10: a Desk is run by a lead you pick, not a hidden Dispatch. Each Desk keeps its items and decisions and
/// has no lead until you pick one, and an old Dispatch session is a plain session from here on (a session row's old
/// `dispatch` key is ignored when it's read, and gone the next time it's saved).
fn drop_dispatch(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("DROP TABLE IF EXISTS desk_dispatch;")?;
    c.pragma_update(None, "user_version", 10)
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

#[cfg(test)]
mod desk_members_seed {
    use super::*;

    #[test]
    fn a_desk_starts_with_the_sessions_holding_its_open_items_and_its_lead() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        let item = |id: &str, project: &str, state: &str, holder: &str| {
            c.execute("INSERT INTO desk_items (id, project, state, updated_ms, data) VALUES (?1, ?2, ?3, 0, ?4)", rusqlite::params![id, project, state, holder]).unwrap();
        };
        item("1", "cue", "working", r#"{"holder":{"kind":"session","sid":"term","name":"cue-terminal"}}"#);
        item("2", "cue", "to_check", r#"{"holder":{"kind":"crew","sid":"agent9","name":"tests","lead_sid":"teams"}}"#);
        item("3", "cue", "done", r#"{"holder":{"kind":"session","sid":"old","name":"old"}}"#);
        item("4", "cue", "new", r#"{"holder":null}"#);
        item("5", "cue", "queued", r#"{"holder":{"kind":"session","sid":"term","name":"cue-terminal"}}"#);
        c.execute("INSERT INTO desk_lead (project, session_id, name, set_ms) VALUES ('cue', 'lead1', 'cue-lead', 0)", []).unwrap();
        seed_desk_members(&c, 10).unwrap();
        let mut q = c.prepare("SELECT project, session_id FROM desk_members ORDER BY session_id").unwrap();
        let got: Vec<(String, String)> = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        let want: Vec<(String, String)> = ["lead1", "teams", "term"].iter().map(|s| ("cue".to_string(), s.to_string())).collect();
        assert_eq!(got, want, "an open item's holder (a crew member's lead), and the lead; not a done item's");
        assert_eq!(c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(), 11);
    }
}
