//! Saving config.json. Settings are saved from many threads at once (the window's commands, ticks, hooks,
//! hand-offs): each save is a read, a change and a write, all under one lock, so no other save lands
//! between its read and its write and loses what it wrote; and the file is replaced whole, so neither a
//! reader (the refresh in cache.rs, the hook process) nor the next save ever sees half of one, which it
//! would take for no settings at all. What a save writes is kept in memory as it's written (cache.rs).

use serde_json::{json, Value};

/// One dotted key read, changed and written back under the config lock. `f` gets the whole config as it is
/// now and returns the key's new value (None: nothing to write) and what the caller wants back. Unchecked:
/// a setting the window may save goes through `config::update`, which checks it as `config::set` does.
pub fn update_with<R>(key: &str, f: impl FnOnce(&Value) -> Result<(Option<Value>, R), String>) -> Result<R, String> {
    let _one = super::cache::hold_file();
    let path = super::path();
    // The file as it is now, not the settings in memory: a hand edit not yet refreshed isn't lost.
    let mut c = super::cache::read_file(&path);
    let (value, out) = f(&c)?;
    let Some(value) = value else { return Ok(out) };
    let (section, name) = key.split_once('.').ok_or_else(|| format!("{key} isn't a dotted key"))?;
    if !c[section].is_object() {
        c[section] = json!({});
    }
    c[section][name] = value;
    replace(&path, &(serde_json::to_string_pretty(&c).map_err(|e| e.to_string())? + "\n"))?;
    super::cache::keep(&path, super::cache::stamp_of(&path), c);
    Ok(out)
}

/// `path` holding `text`, replaced whole: written beside it and flushed to disk, then renamed over it, so a
/// crash leaves the old file or the new one, never an empty one. A `path` that's a link (config.json kept with
/// your dotfiles) has the file it points to replaced, the link left as it is; the file keeps its permissions.
fn replace(path: &std::path::Path, text: &str) -> Result<(), String> {
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
    std::fs::rename(&next, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

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
}
