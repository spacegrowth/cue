//! When a session's turn ended, was interrupted or finished compacting, by its transcript, and what
//! the agent wrote in it.

use super::{content_blocks, harness_text, is_stop_hook, lines, peer_text, read_from, stamp, yours_mid_turn};
use crate::model::TurnPart;
use serde_json::Value;

/// A turn that was interrupted (Esc: "[Request interrupted by user]") with nothing since: Claude Code
/// sends no Stop for that, so the session would look busy while it waits at "What should Claude do
/// instead?". Returns when.
pub fn interrupted_at(path: &str) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(64 * 1024))?;
    let mut at = None;
    for e in lines(&text) {
        if !matches!(e.get("type").and_then(Value::as_str), Some("user" | "assistant")) || e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let said = content_blocks(&e).iter().any(|b| b.get("text").and_then(Value::as_str).is_some_and(|t| t.starts_with("[Request interrupted by user")));
        at = (e.get("type").and_then(Value::as_str) == Some("user") && said).then(|| stamp(&e)).filter(|t| *t > 0);
    }
    at
}

/// Whether a compaction started at `since_ms` has finished: Claude Code writes the command, then (when the
/// summary is done, or it couldn't compact) a `<local-command-stdout>` / `-stderr` entry after it. One it
/// started itself (context full) has no command: its `compact_boundary` marks the end.
pub fn compact_finished(path: &str, since_ms: u64) -> bool {
    let Some(text) = std::fs::metadata(path).ok().and_then(|m| read_from(path, m.len().saturating_sub(256 * 1024))) else { return false };
    let mut typed = false;
    for e in lines(&text) {
        if stamp(&e) + 2000 < since_ms {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("system") if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => return true,
            Some("user") => {}
            _ => continue,
        }
        let said = content_blocks(&e).iter().filter_map(|b| b.get("text").and_then(Value::as_str).map(String::from)).collect::<String>();
        if said.contains("<command-name>/compact</command-name>") {
            typed = true;
        } else if typed && (said.contains("<local-command-stdout>") || said.contains("<local-command-stderr>")) {
            return true;
        }
    }
    false
}

/// A command you ran in Claude Code itself (/login, /model, /resume…) or a note it adds (isMeta): written
/// as "user" entries, but no turn: the agent isn't working on anything because of them.
fn local_command(e: &Value) -> bool {
    if e.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let text = match e.pointer("/message/content") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(blocks)) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join(""),
        _ => return false,
    };
    let t = text.trim_start();
    ["<command-name>", "<local-command-stdout>", "<local-command-stderr>", "<local-command-caveat>"].iter().any(|tag| t.starts_with(tag))
}

/// A turn that has ended, by the transcript: Claude Code writes a `turn_duration` entry once a turn is
/// really over (after its Stop hooks, and not when one sends it back to work). Returns when, and the
/// turn's last text reply, if nothing has happened since. Catches a Stop hook that never reached Cue
/// (Cue was restarting at that moment).
pub fn turn_ended(path: &str) -> Option<(u64, String)> {
    turn_ended_parts(path).map(|(at, reply, _)| (at, reply))
}

/// The same, with everything the agent wrote in that turn as parts, `hooked` once a Stop hook sent it
/// back to work (as the Stop hook reports it): so a turn Cue learns of from the transcript alone (after a
/// restart, or a Stop that never reached it) folds the hook's follow-up the same way.
pub fn turn_ended_parts(path: &str) -> Option<(u64, String, Vec<TurnPart>)> {
    let len = std::fs::metadata(path).ok()?.len();
    let text = read_from(path, len.saturating_sub(256 * 1024))?;
    let (mut ended, mut reply, mut last_reply) = (None, String::new(), String::new());
    let (mut parts, mut last_parts, mut hooked): (Vec<TurnPart>, Vec<TurnPart>, bool) = (vec![], vec![], false);
    for e in lines(&text) {
        match e.get("type").and_then(Value::as_str) {
            Some("system") if e.get("subtype").and_then(Value::as_str) == Some("turn_duration") => {
                ended = Some(stamp(&e));
                last_reply = reply.clone();
                last_parts = parts.clone();
            }
            // A Stop hook sent it back to work (a meta entry, as a command's is): what follows is its follow-up.
            Some("user") if is_stop_hook(&e) => {
                ended = None;
                hooked = true;
            }
            Some("attachment") if yours_mid_turn(&e) => hooked = false,
            // A command you ran in Claude Code since (/login, /model…) starts no turn: still ended.
            Some("user") if local_command(&e) => {}
            Some("user" | "assistant") if e.get("isSidechain").and_then(Value::as_bool) != Some(true) => {
                ended = None; // a new turn (or more of this one) since
                if e.get("type").and_then(Value::as_str) == Some("assistant") {
                    for t in content_blocks(&e).iter().filter_map(|b| (b.get("type").and_then(Value::as_str) == Some("text")).then(|| b.get("text").and_then(Value::as_str).unwrap_or(""))).filter(|t| !t.trim().is_empty()) {
                        reply = t.to_string();
                        parts.push(TurnPart { text: t.trim().to_string(), hooked });
                    }
                } else {
                    let blocks = content_blocks(&e);
                    if blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                        continue;
                    }
                    let t: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
                    if peer_text(&t) {
                        // A teammate's message: not a turn of yours, the parts so far stay as they are.
                    } else if harness_text(&t) {
                        hooked = false;
                    } else if !t.trim().is_empty() && !t.trim_start().starts_with('<') {
                        parts.clear(); // your next prompt: a new turn
                        hooked = false;
                    }
                }
            }
            _ => {}
        }
    }
    ended.filter(|t| *t > 0).map(|t| (t, last_reply, last_parts))
}
