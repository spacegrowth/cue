//! config.json as last read, kept in memory. Every setting Cue reads comes from here, so a read never waits on
//! the disk (the hub reads settings while it holds its store). It's kept current two ways: a save puts what it
//! wrote here as it writes it (save.rs), and `refresh` (a background tick, every couple of seconds) reads the
//! file again when it changed on disk, so a hand edit of config.json is picked up within a tick.
//!
//! The file is read under the same lock saves take ("config file"), so a refresh that read the file just
//! before a save can't put the older settings back after it.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

/// What the file was when it was read: its time, size and inode. A change to any of them is a new file
/// (a save replaces it whole, an editor may too).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    inode: u64,
}

/// The settings in memory and the file they came from.
struct Kept {
    path: PathBuf,
    /// None: there was no file (no settings yet).
    stamp: Option<Stamp>,
    value: Arc<Value>,
}

static KEPT: RwLock<Option<Kept>> = RwLock::new(None);

/// Taken by every save and every refresh: the file is read and the result kept with no other save between.
static FILE: Mutex<()> = Mutex::new(());

/// The file to this thread: no other save or refresh until the guard goes.
pub(super) fn hold_file() -> crate::locking::Guard<'static, ()> {
    crate::locking::lock(&FILE, "config file")
}

/// The settings in memory, read from `path`. No settings (`{}`) when nothing has been read from that file yet:
/// another folder's are never handed out (tests point CUE_HOME elsewhere).
pub(super) fn current(path: &Path) -> Arc<Value> {
    let kept = crate::locking::read(&KEPT, "config in memory");
    match kept.as_ref() {
        Some(k) if k.path == path => k.value.clone(),
        _ => Arc::new(json!({})),
    }
}

/// `value` is what `path` holds now (just written, or just read): kept, with the file's stamp. True when the
/// settings changed.
pub(super) fn keep(path: &Path, stamp: Option<Stamp>, value: Value) -> bool {
    let mut kept = crate::locking::write(&KEPT, "config in memory");
    let changed = kept.as_ref().is_none_or(|k| k.path != path || *k.value != value);
    *kept = Some(Kept { path: path.to_path_buf(), stamp, value: Arc::new(value) });
    changed
}

/// The stamp the settings in memory were read with (None: there was no file); Err when they weren't read
/// from `path`, or nothing has been read yet.
fn kept_stamp(path: &Path) -> Result<Option<Stamp>, ()> {
    let kept = crate::locking::read(&KEPT, "config in memory");
    kept.as_ref().filter(|k| k.path == path).map(|k| k.stamp).ok_or(())
}

/// The file's stamp now; None when there's no file.
pub(super) fn stamp_of(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    Some(Stamp { modified: m.modified().ok(), len: m.len(), inode: m.ino() })
}

/// What config.json holds.
pub(super) enum File {
    /// No file (or one that can't be opened): no settings yet.
    Missing,
    Settings(Value),
    /// A file that isn't settings (cut short, hand-edited into bad JSON, not an object): why.
    Unreadable(String),
}

/// The file, read from disk.
pub(super) fn read(path: &Path) -> File {
    let Ok(text) = std::fs::read_to_string(path) else { return File::Missing };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => File::Settings(v),
        Ok(_) => File::Unreadable("it isn't a JSON object".into()),
        Err(e) => File::Unreadable(e.to_string()),
    }
}

/// The file's settings: `{}` when it's missing or isn't settings (said, so a hand edit gone wrong isn't
/// silently every setting back to its default).
fn read_file(path: &Path) -> Value {
    match read(path) {
        File::Settings(v) => v,
        File::Missing => json!({}),
        File::Unreadable(why) => {
            eprintln!("cue: {} isn't valid settings ({why}): every setting is at its default until it's put right", path.display());
            json!({})
        }
    }
}

/// Read `path` again if it changed on disk since the settings in memory were read from it (or none were).
/// True when the settings changed.
pub(super) fn refresh(path: &Path) -> bool {
    let _file = hold_file();
    let now = stamp_of(path);
    if kept_stamp(path) == Ok(now) {
        return false;
    }
    keep(path, now, read_file(path))
}

#[cfg(test)]
mod tests {
    use crate::config::{load, refresh, remember};
    use serde_json::json;

    /// A Cue folder of the test's own, with `config.json` holding `text`, read into memory.
    fn home(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cue-cfg-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        std::fs::write(dir.join("config.json"), text).unwrap();
        refresh();
        dir
    }

    fn done(dir: &std::path::Path) {
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_read_comes_from_memory_never_the_disk() {
        let _g = crate::db::test_turn();
        let dir = home("memory", r#"{"context":{"keep":7}}"#);
        std::fs::remove_file(dir.join("config.json")).unwrap();
        let (keep, c) = (crate::config::context_keep(), load());
        done(&dir);
        assert_eq!(keep, 7, "the file is gone, the setting read from memory");
        assert_eq!(c["context"]["keep"], 7);
    }

    #[test]
    fn a_hand_edit_is_picked_up_by_the_next_refresh() {
        let _g = crate::db::test_turn();
        let dir = home("edit", r#"{"turn":{"mode":"answer"}}"#);
        assert!(!refresh(), "nothing changed on disk");
        std::fs::write(dir.join("config.json"), r#"{"turn":{"mode":"last"},"x":1}"#).unwrap();
        let before = crate::config::turn_mode();
        let changed = refresh();
        let after = crate::config::turn_mode();
        done(&dir);
        assert_eq!(before, "answer", "memory until the refresh");
        assert!(changed, "the refresh says the settings changed (the window redraws)");
        assert_eq!(after, "last");
    }

    #[test]
    fn a_save_is_in_memory_at_once_and_keeps_a_hand_edit_not_yet_refreshed() {
        let _g = crate::db::test_turn();
        let dir = home("save", r#"{"a":{"x":1}}"#);
        std::fs::write(dir.join("config.json"), r#"{"a":{"x":1},"hand":{"y":2}}"#).unwrap();
        remember("tips.on", json!(false)).unwrap();
        let c = load();
        let unchanged = refresh();
        done(&dir);
        assert_eq!(c["tips"]["on"], false, "saved and in memory, no refresh needed");
        assert_eq!(c["hand"]["y"], 2, "the save started from the file, so the hand edit stays");
        assert!(!unchanged, "what the save wrote is what memory has");
    }

    #[test]
    fn a_refresh_racing_saves_never_puts_older_settings_back() {
        let _g = crate::db::test_turn();
        let dir = home("race", "{}");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = stop.clone();
        let refresher = std::thread::spawn(move || {
            while !s.load(std::sync::atomic::Ordering::SeqCst) {
                refresh();
            }
        });
        let savers: Vec<_> = (0..8).map(|i| std::thread::spawn(move || remember(&format!("race.k{i}"), json!(i)).unwrap())).collect();
        for t in savers {
            t.join().unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        refresher.join().unwrap();
        let c = load();
        done(&dir);
        for i in 0..8 {
            assert_eq!(c["race"][format!("k{i}")], i, "k{i} saved but not in memory");
        }
    }

    #[test]
    fn another_folders_settings_are_never_handed_out() {
        let _g = crate::db::test_turn();
        let dir = home("other", r#"{"context":{"keep":9}}"#);
        std::env::set_var("CUE_HOME", dir.join("elsewhere"));
        let c = load();
        done(&dir);
        assert_eq!(*c, json!({}), "nothing read from this folder yet: no settings");
    }
}
