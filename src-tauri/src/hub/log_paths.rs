//! Where a session's log is on this Mac: a Claude Code transcript, a Codex rollout log or a Pi session log. Read by
//! the window's chat (ipc/chat.rs) and by the hub (hand-offs, insights).

use super::Hub;
use crate::logs::live;

/// The log a session's steps come from: a Claude Code session's transcript (a quiet one's found through
/// Claude Code's own list of running sessions), a Codex session's rollout log, or a Pi session's log.
pub(crate) fn steps_path(h: &Hub, session_id: &str) -> Option<String> {
    let origin = h.session_origin(session_id);
    if origin.as_ref().is_some_and(|o| o.harness == "pi") {
        return pi_log(session_id);
    }
    origin
        .filter(|o| (o.harness == "claude" || o.harness == "codex") && !o.transcript_path.is_empty())
        .map(|o| o.transcript_path)
        .or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).and_then(|q| live::claude_transcript(&q.cwd, session_id)))
        .or_else(|| pi_log(session_id))
        .or_else(|| codex_log(session_id))
}

/// Where Codex keeps its sessions' logs: ~/.codex/sessions (CODEX_HOME moves ~/.codex).
pub(crate) fn codex_sessions_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::env::var("CODEX_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".codex")).join("sessions")
}

/// Where Pi keeps its sessions' logs: ~/.pi/agent/sessions (PI_CODING_AGENT_DIR moves ~/.pi/agent).
pub(crate) fn pi_sessions_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent")).join("sessions")
}

/// A Codex session's rollout log, `~/.codex/sessions/<year>/<month>/<day>/rollout-<time>-<session id>.jsonl`
/// (CODEX_HOME moves ~/.codex), newest days first. Looked up once per session.
pub(crate) fn codex_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.len() < 8 {
        return None;
    }
    if let Some(p) = crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let root = codex_sessions_dir();
    let tail = format!("-{session_id}.jsonl");
    let sorted = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
        v.sort();
        v.reverse();
        v
    };
    for year in sorted(&root) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                if let Some(f) = sorted(&day).into_iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail))) {
                    let found = f.to_string_lossy().into_owned();
                    crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
                    return Some(found);
                }
            }
        }
    }
    None
}

/// A Pi session's log: `<Pi's folder>/sessions/<its folder>/<time>_<session id>.jsonl` (Pi's folder is
/// ~/.pi/agent, or PI_CODING_AGENT_DIR). Looked up once per session.
pub(crate) fn pi_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.is_empty() {
        return None;
    }
    if let Some(p) = crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let root = pi_sessions_dir();
    let tail = format!("_{session_id}.jsonl");
    let found = std::fs::read_dir(root).ok()?.filter_map(Result::ok).filter_map(|d| std::fs::read_dir(d.path()).ok()).flatten().filter_map(Result::ok)
        .map(|f| f.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail)))?
        .to_string_lossy().into_owned();
    crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
    Some(found)
}
