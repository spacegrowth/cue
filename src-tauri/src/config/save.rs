//! Saving config.json. Settings are saved from many threads at once (the window's commands, ticks, hooks,
//! hand-offs): each save is a read, a change and a write, all under one lock, so no other save lands
//! between its read and its write and loses what it wrote; and the file is replaced whole, so neither a
//! reader (`load`, the hook process) nor the next save ever sees half of one, which `load` would take for
//! no settings at all.

use serde_json::{json, Value};

/// One dotted key read, changed and written back under the config lock. `f` gets the whole config as it is
/// now and returns the key's new value (None: nothing to write) and what the caller wants back. Unchecked:
/// a setting the window may save goes through `config::update`, which checks it as `config::set` does.
pub fn update_with<R>(key: &str, f: impl FnOnce(&Value) -> Result<(Option<Value>, R), String>) -> Result<R, String> {
    static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = crate::locking::lock(&SAVING, "config saves");
    let mut c = super::load();
    let (value, out) = f(&c)?;
    let Some(value) = value else { return Ok(out) };
    let (section, name) = key.split_once('.').ok_or_else(|| format!("{key} isn't a dotted key"))?;
    if !c[section].is_object() {
        c[section] = json!({});
    }
    c[section][name] = value;
    replace(&super::path(), &(serde_json::to_string_pretty(&c).map_err(|e| e.to_string())? + "\n"))?;
    Ok(out)
}

/// `path` holding `text`, replaced whole: written beside it, then renamed over it.
fn replace(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().unwrap_or(path)).map_err(|e| e.to_string())?;
    let next = path.with_extension("json.next");
    std::fs::write(&next, text).map_err(|e| e.to_string())?;
    std::fs::rename(&next, path).map_err(|e| e.to_string())
}
