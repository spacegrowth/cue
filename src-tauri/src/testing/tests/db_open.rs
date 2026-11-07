//! Opening a database that's only half there (stability plan §4, db / history): a file caught mid-replace is
//! tried again rather than set aside, and a damaged page a migration meets costs nothing else the file holds.

use crate::testing::World;
use rusqlite::Connection;
use serde_json::json;

fn set_aside(w: &World) -> Vec<String> {
    std::fs::read_dir(w.home()).unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".unreadable-")).collect()
}

/// The World's database written out whole (its log folded in) and let go, as a copy of it would be.
fn settled_bytes(w: &World) -> Vec<u8> {
    crate::db::raw(|c| c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))).unwrap();
    crate::db::reset();
    std::fs::read(w.home().join("cue.db")).unwrap()
}

/// A synced folder replacing cue.db: the first read meets half a file, the retry the whole one. Kept, not set
/// aside, everything in it there.
#[test]
fn a_database_caught_mid_replace_is_read_again_not_set_aside() {
    let w = World::new();
    crate::db::set_draft("s:a", "kept", &json!([]));
    let whole = settled_bytes(&w);
    std::fs::write(w.home().join("cue.db"), &whole[..whole.len() / 3].iter().map(|b| b ^ 0x5a).collect::<Vec<u8>>()).unwrap();
    let home = w.home().to_path_buf();
    crate::db::tests_seam::BEFORE_RETRY.set(Some(Box::new(move || std::fs::write(home.join("cue.db"), &whole).unwrap())));
    assert_eq!(crate::db::draft_text("s:a"), "kept");
    assert!(set_aside(&w).is_empty(), "{:?}", set_aside(&w));
}

/// A damaged page in one table a migration reads (the turns, read again from before version 2): the database
/// is used as it is, History and drafts still there, nothing set aside.
#[test]
fn a_damaged_page_a_migration_meets_costs_nothing_else() {
    let w = World::new();
    crate::db::set_draft("s:a", "kept", &json!([]));
    {
        let c = Connection::open(w.home().join("cue.db")).unwrap();
        for i in 0..400 {
            c.execute("INSERT INTO turns (session_id, at_ms, prompt) VALUES ('s', ?1, ?2)", rusqlite::params![i, "x".repeat(300)]).unwrap();
        }
        c.pragma_update(None, "user_version", 1).unwrap();
    }
    let page: i64 = crate::db::raw(|c| c.query_row("SELECT rootpage FROM sqlite_master WHERE name = 'turns'", [], |r| r.get(0))).unwrap();
    let size: i64 = crate::db::raw(|c| c.pragma_query_value(None, "page_size", |r| r.get(0))).unwrap();
    let mut bytes = settled_bytes(&w);
    let at = ((page - 1) * size) as usize;
    bytes[at..at + size as usize].fill(0xee);
    std::fs::write(w.home().join("cue.db"), &bytes).unwrap();
    assert_eq!(crate::db::draft_text("s:a"), "kept", "the rest is read");
    assert!(set_aside(&w).is_empty(), "{:?}", set_aside(&w));
}
