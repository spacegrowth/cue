//! Another program holding cue.db (sqlite3 mid-transaction, a backup; CUE_CHAOS_DB_LOCK_SECS in the soak):
//! a write that has to wait for it waits with DB let go, so no other caller queues behind it.

use super::*;
use std::time::{Duration, Instant};

#[test]
fn a_write_waiting_on_another_programs_lock_holds_up_no_other_caller() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-db-busy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CUE_HOME", &dir);
    reset();
    with(|c| c.execute_batch("CREATE TABLE IF NOT EXISTS busy_test (x)")).unwrap();
    let outside = rusqlite::Connection::open(path()).unwrap();
    outside.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let writer = std::thread::spawn(|| with(|c| c.execute("INSERT INTO busy_test VALUES (1)", [])));
    // Reads all through the writer's wait (BUSY_WAIT): none waits on DB behind it.
    let (t0, mut worst) = (Instant::now(), Duration::ZERO);
    while t0.elapsed() < BUSY_WAIT / 2 {
        let t = Instant::now();
        with(|c| c.query_row("SELECT count(*) FROM busy_test", [], |r| r.get::<_, i64>(0))).unwrap();
        worst = worst.max(t.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
    outside.execute_batch("COMMIT").unwrap();
    assert_eq!(writer.join().unwrap(), Some(1), "the write went in once the other program let go");
    assert!(worst < Duration::from_millis(200), "a read waited {worst:?} behind a write waiting on the other program");
    reset();
    std::env::remove_var("CUE_HOME");
    let _ = std::fs::remove_dir_all(&dir);
}
