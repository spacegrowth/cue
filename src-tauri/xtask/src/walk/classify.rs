//! What a call outside the crate is, from its path (with `use`s expanded): a spawn, an external effect,
//! or nothing this cares about.

use super::events::Effect;

/// `thread::spawn`, `async_runtime::spawn_blocking`, `tokio::spawn`, … as `parent::name`; None for a
/// call that doesn't start a thread or task.
pub fn spawn(path: &[String]) -> Option<String> {
    let last = path.last()?;
    if last != "spawn" && last != "spawn_blocking" {
        return None;
    }
    let parent = path.len().checked_sub(2).map_or("", |i| path[i].as_str());
    Some(if parent.is_empty() { last.clone() } else { format!("{parent}::{last}") })
}

/// The effect of an outside call, with what to show for it; `literal`: its first argument when that is a
/// string literal (`Command::new("tmux")`, `AnyClass::get(c"SMAppService")`).
pub fn effect(path: &[String], literal: Option<&str>) -> Option<(Effect, String)> {
    let tail = path.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("::");
    if tail == "Command::new" {
        return Some((Effect::Process, literal.map_or_else(|| "(program computed)".to_string(), str::to_string)));
    }
    if path.len() > 2 && (path[0] == "std" || path[0] == "tokio") && path[1] == "fs" {
        return Some((Effect::Fs, path[2..].join("::")));
    }
    let names = |s: &str| path.iter().any(|p| p == s);
    if names("UNUserNotificationCenter") {
        return Some((Effect::Notify, tail));
    }
    if names("SMAppService") || literal.is_some_and(|l| l.contains("SMAppService")) {
        return Some((Effect::Login, tail));
    }
    if names("UnixStream") || names("UnixListener") {
        return Some((Effect::Socket, tail));
    }
    None
}

/// `Path` methods that go to the disk (a stat can hang on an automounted `/home`).
pub const DISK_METHODS: &[&str] = &["canonicalize", "exists", "is_dir", "is_file", "is_symlink", "metadata", "read_dir", "read_link", "symlink_metadata", "try_exists"];

/// Taking a lock the plain way: `m.lock()`, `m.try_lock()`, `rw.read()`, `rw.write()` (no arguments).
pub const BARE_LOCK_METHODS: &[&str] = &["lock", "try_lock", "read", "write"];

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<String> {
        s.split("::").map(str::to_string).collect()
    }

    #[test]
    fn spawns_are_named_by_their_last_two_segments() {
        assert_eq!(spawn(&p("std::thread::spawn")).as_deref(), Some("thread::spawn"));
        assert_eq!(spawn(&p("tauri::async_runtime::spawn_blocking")).as_deref(), Some("async_runtime::spawn_blocking"));
        assert_eq!(spawn(&p("std::thread::sleep")), None);
    }

    #[test]
    fn effects_are_told_by_path_and_literal() {
        assert_eq!(effect(&p("std::process::Command::new"), Some("osascript")), Some((Effect::Process, "osascript".into())));
        assert_eq!(effect(&p("std::fs::read_to_string"), None), Some((Effect::Fs, "read_to_string".into())));
        assert_eq!(effect(&p("AnyClass::get"), Some("SMAppService")).map(|e| e.0), Some(Effect::Login));
        assert_eq!(effect(&p("std::path::Path::new"), None), None);
    }
}
