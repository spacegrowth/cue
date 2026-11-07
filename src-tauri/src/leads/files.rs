//! Reading relay's and pi-lead's state folders: where they are, and each JSON record and packet line,
//! parsed again only when its file changes.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Each file's last parse, by path, with the modification time it was parsed at. relay never deletes
/// executor records (hundreds of closed ones pile up), and a closed record never changes again: a
/// rescan stats every file but parses only the ones that changed. Each entry also keeps the scan
/// that last saw it, so a record that's gone (pruned) is dropped at the end of the next scan.
#[derive(Default)]
pub(super) struct Parsed {
    scan: u64,
    pub(super) files: HashMap<PathBuf, (std::time::SystemTime, Option<Value>, u64)>,
}
pub(super) static PARSED: Mutex<Option<Parsed>> = Mutex::new(None);

pub(super) fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}
pub(super) fn relay_dir() -> PathBuf {
    std::env::var_os("RELAY_STATE_ROOT").map(PathBuf::from).unwrap_or_else(|| home().join(".relay-tasks"))
}
pub(super) fn claude_sessions_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude")).join("sessions")
}

/// Claude sessions open in a live process right now (Claude Code's `sessions/<pid>.json`).
pub(super) fn live_claude_sessions(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashSet::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| read_json(&p))
        .filter(|r| r.get("pid").and_then(Value::as_i64).is_some_and(|pid| pid > 0 && crate::server::alive(pid as i32)))
        .map(|r| s(&r, "sessionId"))
        .filter(|sid| !sid.is_empty())
        .collect()
}

pub(super) fn pilead_dir() -> PathBuf {
    std::env::var_os("PI_LEAD_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".pi-lead"))
}

pub(super) fn read_json(p: &Path) -> Option<Value> {
    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok()?;
    let mut parsed = crate::locking::lock(&PARSED, "PARSED");
    let parsed = parsed.get_or_insert_with(Parsed::default);
    let scan = parsed.scan;
    if let Some((at, v, seen)) = parsed.files.get_mut(p) {
        if *at == mtime {
            *seen = scan;
            return v.clone();
        }
    }
    let v = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok());
    parsed.files.insert(p.to_path_buf(), (mtime, v.clone(), scan));
    v
}
pub(super) fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
pub(super) fn color(v: &Value) -> Option<[u8; 3]> {
    let a = v.get("color")?.as_array()?;
    let c = |i: usize| a.get(i)?.as_u64().filter(|n| *n <= 255).map(|n| n as u8);
    Some([c(0)?, c(1)?, c(2)?])
}
/// A relay lead record's predecessor: the session id of the lead it took over from, "" for none.
pub(super) fn predecessor(v: &Value) -> String {
    v.get("predecessor").map(|p| s(p, "session_id")).unwrap_or_default()
}
pub(super) fn subdirs(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// The first non-empty line of a packet or report (≤ 300 chars), "GOAL: " dropped. Read again
/// only when the file changes (they're written once per packet).
static FIRST_LINES: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, String)>>> = Mutex::new(None);
pub(super) fn first_line(p: &Path) -> String {
    let Ok(mtime) = std::fs::metadata(p).and_then(|m| m.modified()) else { return String::new() };
    let mut cache = crate::locking::lock(&FIRST_LINES, "FIRST_LINES");
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((at, line)) = cache.get(p) {
        if *at == mtime {
            return line.clone();
        }
    }
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line = line.strip_prefix("GOAL:").unwrap_or(line).trim();
    let line: String = if line.chars().count() > 300 { format!("{}…", line.chars().take(300).collect::<String>()) } else { line.to_string() };
    cache.insert(p.to_path_buf(), (mtime, line.clone()));
    line
}

/// The end of a scan: the records it didn't see are dropped from the cache, and the next scan begins.
pub(super) fn end_scan() {
    if let Some(parsed) = crate::locking::lock(&PARSED, "PARSED").as_mut() {
        let scan = parsed.scan;
        parsed.files.retain(|_, (_, _, seen)| *seen == scan);
        parsed.scan += 1;
    }
}
