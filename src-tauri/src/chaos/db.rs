//! CUE_CHAOS_DB_LOCK_SECS: another program holds cue.db (sqlite3 in a terminal mid-transaction, a backup
//! tool) for that many seconds, every HELD_EVERY: a connection of its own takes the database's write lock
//! and keeps it. Cue's writes meanwhile wait their busy timeout and give up, and Cue must keep answering.

/// How often the database is held.
const HELD_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The thread that holds the database, when the knob is on.
pub fn start() {
    let Some(secs) = super::get(super::Knob::DbLockSecs) else { return };
    let path = crate::db::path();
    let hold = move || loop {
        std::thread::sleep(HELD_EVERY);
        if let Err(e) = hold_for(&path, std::time::Duration::from_secs(secs)) {
            eprintln!("cue: chaos: couldn't hold {}: {e}", path.display());
        }
    };
    let _ = std::thread::Builder::new().name("cue chaos db".into()).spawn(hold);
}

/// The database at `path` held for `how_long` by a connection that isn't Cue's.
pub fn hold_for(path: &std::path::Path, how_long: std::time::Duration) -> rusqlite::Result<()> {
    let c = rusqlite::Connection::open(path)?;
    c.execute_batch("BEGIN EXCLUSIVE")?;
    std::thread::sleep(how_long);
    c.execute_batch("COMMIT")
}
