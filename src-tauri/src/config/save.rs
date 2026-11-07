//! Saving config.json. Settings are saved from many threads at once (the window's commands, ticks, hooks,
//! hand-offs): each save is a read, a change and a write, all under one lock, so no other save lands
//! between its read and its write and loses what it wrote; and the file is replaced whole, so neither a
//! reader (the refresh in cache.rs, the hook process) nor the next save ever sees half of one, which it
//! would take for no settings at all. What a save writes is kept in memory as it's written (cache.rs).

use serde_json::{json, Value};

/// Who a save is for: you, from the window (a setting you changed), or Cue itself (a tick, a hook, a hand-off).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum By {
    You,
    Cue,
}

/// How long a save waits for a config.json it can't read to be readable again: an editor writing it in place
/// (cut short, then written) is done well within it.
const SETTLE_MS: u64 = 200;

/// One dotted key read, changed and written back under the config lock, as Cue saves its own (`By::Cue`).
/// `f` gets the whole config as it is now and returns the key's new value (None: nothing to write) and what the
/// caller wants back. Unchecked: a setting the window may save goes through `config::update`, which checks it
/// as `config::set` does.
pub fn update_with<R>(key: &str, f: impl FnOnce(&Value) -> Result<(Option<Value>, R), String>) -> Result<R, String> {
    save(key, By::Cue, f)
}

/// `update_with`, for a setting you changed in the window (`By::You`).
pub fn update_yours<R>(key: &str, f: impl FnOnce(&Value) -> Result<(Option<Value>, R), String>) -> Result<R, String> {
    save(key, By::You, f)
}

fn save<R>(key: &str, by: By, f: impl FnOnce(&Value) -> Result<(Option<Value>, R), String>) -> Result<R, String> {
    let _one = super::cache::hold_file();
    let path = super::path();
    // The file as it is now, not the settings in memory: a hand edit not yet refreshed isn't lost.
    let mut c = current(&path, by)?;
    let (value, out) = f(&c)?;
    let Some(value) = value else { return Ok(out) };
    let (section, name) = key.split_once('.').ok_or_else(|| format!("{key} isn't a dotted key"))?;
    if !c[section].is_object() {
        c[section] = json!({});
    }
    c[section][name] = value;
    let stamp = replace(&path, &(serde_json::to_string_pretty(&c).map_err(|e| e.to_string())? + "\n"))?;
    super::cache::keep(&path, stamp, c);
    Ok(out)
}

/// The settings `path` holds now, for a save `by` you or Cue. One it can't read is read again a moment later
/// (an editor caught mid-write has finished by then). Still unreadable: Cue's own save leaves it for you to put
/// right; yours keeps it aside and starts a new file, so a hand edit with a typo isn't every setting gone.
fn current(path: &std::path::Path, by: By) -> Result<Value, String> {
    use super::cache::{read, File};
    let why = match read(path) {
        File::Settings(v) => return Ok(v),
        File::Missing => return Ok(json!({})),
        File::Unreadable(_) => {
            #[cfg(test)]
            tests::REREAD.with(|then| then.take().map(|f| f(path)));
            crate::clock::sleep_ms(SETTLE_MS);
            match read(path) {
                File::Settings(v) => return Ok(v),
                File::Missing => return Ok(json!({})),
                File::Unreadable(why) => why,
            }
        }
    };
    if by == By::Cue {
        return Err(format!("{} isn't valid settings ({why}): not saved until it's put right", path.display()));
    }
    let kept = keep_aside(path)?;
    eprintln!("cue: {} isn't valid settings ({why}): kept as {}, saving a new one", path.display(), kept.display());
    Ok(json!({}))
}

/// A copy of `path` beside it, as `<name>.unreadable-<ms>`, for a person to put right.
fn keep_aside(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".unreadable-{}", crate::clock::now_ms()));
    let kept = path.with_file_name(name);
    std::fs::copy(path, &kept).map_err(|e| format!("couldn't keep the unreadable {} aside: {e}", path.display()))?;
    Ok(kept)
}

/// `path` holding `text`, replaced whole: written beside it and flushed to disk, then renamed over it, so a
/// crash leaves the old file or the new one, never an empty one. A `path` that's a link (config.json kept with
/// your dotfiles) has the file it points to replaced, the link left as it is; the file keeps its permissions.
/// The stamp of what was written, taken before it's in place: a hand edit right after is a different stamp,
/// so the next refresh reads it.
fn replace(path: &std::path::Path, text: &str) -> Result<Option<super::cache::Stamp>, String> {
    use std::io::Write;
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    std::fs::create_dir_all(path.parent().unwrap_or(&path)).map_err(|e| e.to_string())?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".next");
    let next = path.with_file_name(name);
    let mut f = std::fs::File::create(&next).map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    if let Ok(was) = std::fs::metadata(&path) {
        f.set_permissions(was.permissions()).map_err(|e| e.to_string())?;
    }
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    let stamp = super::cache::stamp_of(&next);
    std::fs::rename(&next, &path).map_err(|e| e.to_string())?;
    #[cfg(test)]
    tests::PLACED.with(|then| then.take().map(|f| f(&path)));
    Ok(stamp)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::os::unix::fs::PermissionsExt;

    type Then = Box<dyn FnOnce(&std::path::Path)>;

    thread_local! {
        /// What happens to the file the moment a save on this thread has put it in place (once).
        pub(super) static PLACED: Cell<Option<Then>> = const { Cell::new(None) };
        /// What happens to an unreadable file between a save's first read of it and its second (once).
        pub(super) static REREAD: Cell<Option<Then>> = const { Cell::new(None) };
    }

    #[test]
    fn a_linked_config_has_its_target_replaced_and_the_file_keeps_its_permissions() {
        let dir = std::env::temp_dir().join(format!("cue-cfg-replace-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("dotfiles")).unwrap();
        let target = dir.join("dotfiles/cue.json");
        std::fs::write(&target, "{}\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.join("config.json");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        super::replace(&link, "{\"a\": 1}\n").unwrap();
        let still_link = std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink();
        let text = std::fs::read_to_string(&target).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("dotfiles")).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
        std::fs::remove_dir_all(&dir).ok();
        assert!(still_link, "the link stays a link");
        assert_eq!(text, "{\"a\": 1}\n", "its target has the new text");
        assert_eq!(mode, 0o600, "and its permissions");
        assert_eq!(leftovers.len(), 1, "nothing left beside it: {leftovers:?}");
    }

    /// The stamp kept with a save is the file that was written: an edit made right after it is a new stamp, so
    /// the next refresh reads that edit rather than taking it for the save.
    #[test]
    fn a_saves_stamp_is_what_it_wrote_and_an_edit_after_it_differs() {
        let dir = std::env::temp_dir().join(format!("cue-cfg-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let wrote = super::replace(&path, "{\"a\": 1}\n").unwrap();
        assert_eq!(wrote, super::super::cache::stamp_of(&path), "the file in place is the one written");
        std::fs::write(&path, "{\"a\": 22}\n").unwrap();
        assert_ne!(wrote, super::super::cache::stamp_of(&path), "an edit after it is another stamp");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A hand edit that lands between a save putting its file in place and the save noting what it wrote is
    /// read by the next refresh, not taken for the save.
    #[test]
    fn a_hand_edit_right_after_a_save_is_read_by_the_next_refresh() {
        let _g = crate::db::test_turn();
        let dir = std::env::temp_dir().join(format!("cue-cfg-placed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        std::fs::write(dir.join("config.json"), "{}\n").unwrap();
        crate::config::refresh();
        PLACED.set(Some(Box::new(|path| std::fs::write(path, "{\"hand\": {\"edited\": true}}\n").unwrap())));
        crate::config::remember("tips.on", serde_json::json!(false)).unwrap();
        let changed = crate::config::refresh();
        let c = crate::config::load();
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(changed, "the refresh saw the edit");
        assert_eq!(c["hand"]["edited"], true, "what's on disk now: {c}");
    }

    /// A Cue folder of the test's own (CUE_HOME, its turn at the test lock) whose config.json holds `bytes`,
    /// read into memory; put back when it goes.
    struct Home {
        dir: std::path::PathBuf,
        _g: std::sync::MutexGuard<'static, ()>,
    }

    impl Home {
        fn with(name: &str, bytes: &[u8]) -> Self {
            let g = crate::db::test_turn();
            let dir = std::env::temp_dir().join(format!("cue-cfg-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("CUE_HOME", &dir);
            std::fs::write(dir.join("config.json"), bytes).unwrap();
            crate::config::refresh();
            Home { dir, _g: g }
        }

        fn file(&self) -> Vec<u8> {
            std::fs::read(self.dir.join("config.json")).unwrap()
        }

        fn kept_aside(&self) -> Vec<Vec<u8>> {
            std::fs::read_dir(&self.dir).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().contains(".unreadable-")).map(|e| std::fs::read(e.path()).unwrap()).collect()
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(self.dir.join("config.json"), std::fs::Permissions::from_mode(0o644));
            std::env::remove_var("CUE_HOME");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A byte that isn't UTF-8 makes config.json unreadable, not missing: your save keeps it aside, as it was.
    #[test]
    fn a_config_that_isnt_utf8_is_kept_aside_by_your_save_not_taken_for_none() {
        let broken = b"{\"history\": {\"keep\": 120}, \"name\": \"caf\xe9\"}\n".to_vec();
        let h = Home::with("utf8", &broken);
        crate::config::set("tips.on", serde_json::json!(false)).unwrap();
        assert_eq!(h.kept_aside(), [broken], "kept aside as it was");
    }

    /// A config.json Cue may not read: your save fails and the file is left as it is.
    #[test]
    fn a_config_cue_may_not_read_is_left_alone() {
        let h = Home::with("perm", b"{\"history\": {\"keep\": 120}}\n");
        std::fs::set_permissions(h.dir.join("config.json"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = crate::config::set("tips.on", serde_json::json!(false));
        std::fs::set_permissions(h.dir.join("config.json"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(r.is_err(), "{r:?}");
        assert_eq!(h.file(), b"{\"history\": {\"keep\": 120}}\n", "left as it was");
    }

    /// Cue's own saves (a tick's, a hook's) never replace a config.json that isn't settings: that's yours to put
    /// right, or to save over from the window.
    #[test]
    fn cues_own_save_never_replaces_a_broken_config() {
        let h = Home::with("own", b"{\"tips\": {\"on\": fal");
        let r = crate::config::remember("status.sessions", serde_json::json!(["s1"]));
        assert!(r.as_ref().is_err_and(|e| e.contains("not saved until it's put right")), "{r:?}");
        assert_eq!(h.file(), b"{\"tips\": {\"on\": fal", "untouched");
        assert!(h.kept_aside().is_empty());
    }

    /// An editor writing config.json in place, caught mid-write by a save: read again a moment later, whole,
    /// and saved with it; nothing kept aside.
    #[test]
    fn a_save_catching_an_editor_mid_write_waits_for_the_whole_file() {
        let h = Home::with("midwrite", b"{\"tips\": {\"on\": fal");
        REREAD.set(Some(Box::new(|path| std::fs::write(path, "{\"tips\": {\"on\": false}, \"hand\": {\"kept\": true}}\n").unwrap())));
        crate::config::remember("status.sessions", serde_json::json!(["s1"])).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&h.file()).unwrap();
        assert_eq!((saved["hand"]["kept"].clone(), saved["status"]["sessions"].clone()), (serde_json::json!(true), serde_json::json!(["s1"])));
        assert!(h.kept_aside().is_empty());
    }
}
