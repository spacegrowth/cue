//! Bounded approval delivery evidence. Identifiers and phases only, without commands or question contents.

use serde_json::json;
use std::io::Write;
use std::sync::Mutex;

pub(crate) fn record(sid: &str, request: &str, card: &str, phase: &str) {
    if cfg!(test) { return; }   // protocol fixture tests must never write to the user's Cue folder
    static WRITER: Mutex<()> = Mutex::new(());
    let _guard = crate::locking::lock(&WRITER, "approval trace");
    let dir = crate::server::cue_dir();
    let path = dir.join("approvals.log");
    if std::fs::create_dir_all(&dir).is_err() { return; }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= 1 << 20) {
        let _ = std::fs::rename(&path, dir.join("approvals.log.1"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let line = json!({ "at_ms": crate::clock::now_ms(), "session": sid, "request": request, "card": card, "phase": phase });
        let _ = writeln!(file, "{line}");
    }
}
