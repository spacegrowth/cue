//! CUE_CHAOS_DB_LOCK_SECS: another program holds cue.db (sqlite3 in a terminal mid-transaction, a backup
//! tool) for that many seconds, every HELD_EVERY: a connection of its own takes the database's write lock
//! and keeps it. Cue's writes meanwhile wait their busy timeout and give up, and Cue must keep answering.

/// How often the database is held.
const HELD_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The thread that holds the database, when the knob is on.
pub fn start() {
    let Some(secs) = super::get(super::Knob::DbLockSecs) else { return };
    let path = crate::db::path();
    let run = move || loop {
        std::thread::sleep(HELD_EVERY);
        if let Err(e) = hold_for(&path, std::time::Duration::from_secs(secs)) {
            eprintln!("cue: chaos: couldn't hold {}: {e}", path.display());
        }
    };
    let _ = std::thread::Builder::new().name("cue chaos db".into()).spawn(run);
}

/// The database at `path` held for `how_long` by a connection that isn't Cue's.
fn hold_for(path: &std::path::Path, how_long: std::time::Duration) -> rusqlite::Result<()> {
    let c = hold(path)?;
    std::thread::sleep(how_long);
    c.execute_batch("COMMIT")
}

/// A connection that holds the database at `path` until it commits or goes.
fn hold(path: &std::path::Path) -> rusqlite::Result<rusqlite::Connection> {
    let c = rusqlite::Connection::open(path)?;
    c.execute_batch("BEGIN EXCLUSIVE")?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_held_database_refuses_another_connections_write_until_it_is_let_go() {
        let dir = std::env::temp_dir().join(format!("cue-chaos-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("held.db");
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE IF NOT EXISTS t (x)").unwrap();
        c.busy_timeout(std::time::Duration::from_millis(20)).unwrap();
        let held = super::hold(&path).unwrap();
        let refused = c.execute("INSERT INTO t VALUES (1)", []).unwrap_err();
        assert!(refused.to_string().contains("locked"), "{refused}");
        held.execute_batch("COMMIT").unwrap();
        assert!(c.execute("INSERT INTO t VALUES (2)", []).is_ok(), "free again once it's let go");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
