//! Whose log a file is (Claude Code's, Pi's or Codex's), and each entry put in the shape Claude Code's
//! transcript uses.

use super::codex::from_codex;
use super::pi::from_pi;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Kind {
    Claude,
    Pi,
    Codex,
}

/// Whose log this is, by its first line: Pi's starts `{"type":"session","version":…}`, Codex's
/// `{"type":"session_meta",…}`; anything else is Claude Code's.
pub(super) fn kind_of(path: &str) -> Kind {
    let Ok(f) = std::fs::File::open(path) else { return Kind::Claude };
    let mut first = String::new();
    let _ = std::io::BufRead::read_line(&mut std::io::BufReader::new(f.take(256 * 1024)), &mut first);
    let e: Value = serde_json::from_str(&first).unwrap_or(Value::Null);
    match e.get("type").and_then(Value::as_str) {
        Some("session") if e.get("version").is_some() => Kind::Pi,
        Some("session_meta") => Kind::Codex,
        _ => Kind::Claude,
    }
}

/// Whose log this is, as a harness: "claude", "codex" or "pi" (see `kind_of`).
pub fn harness_of(path: &str) -> &'static str {
    match kind_of(path) {
        Kind::Claude => "claude",
        Kind::Codex => "codex",
        Kind::Pi => "pi",
    }
}

/// One log entry as Claude Code transcript entries (none, one, or a few).
pub(super) fn translate(kind: Kind, e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
    match kind {
        Kind::Claude => vec![e.clone()],
        Kind::Pi => from_pi(e),
        Kind::Codex => from_codex(e, patches),
    }
}
