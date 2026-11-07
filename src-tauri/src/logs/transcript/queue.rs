//! Where a message you sent a session stands: still queued, taken into the running turn, or taken
//! as a turn of its own, from Claude Code's queue log or Codex's rollout.

use super::{content_blocks, iso_ms, lines, read_from, stamp, yours_mid_turn};
use serde_json::Value;

/// Where a message you queued for a busy Claude Code session stands, by its queue log: still in the
/// queue, taken into the running turn between two steps, or taken as the next turn's prompt (when).
#[derive(Debug, PartialEq)]
pub enum Queued {
    Waiting,
    Absorbed,
    Taken(Option<u64>),
}

/// Whether `got` (what the agent received) is the message `sent`, perhaps with image paths after it.
/// Only the start is compared, with spacing ignored: a long paste typed into a terminal comes back
/// with its line breaks, tabs or trailing spaces changed, and must still count as the same message.
pub fn is_message(got: &str, sent: &str) -> bool {
    let start = |t: &str| t.chars().filter(|c| !c.is_whitespace()).take(300).collect::<String>();
    let sent = start(sent);
    !sent.is_empty() && start(got).starts_with(&sent)
}

/// The queue log says `enqueue` (with the text), then `remove` (reason "absorbed_mid_turn", with the
/// text) or `dequeue` (the oldest, no text). None: the log doesn't mention this message.
pub fn queued_state(path: &str, text: &str) -> Option<Queued> {
    if text.trim().is_empty() {
        return None;
    }
    let len = std::fs::metadata(path).ok()?.len();
    let log = read_from(path, len.saturating_sub(256 * 1024))?;
    let mine = |c: &str| is_message(&crate::sessions::unwrap_pasted(c), text);
    let (mut pending, mut last): (Vec<String>, Option<Queued>) = (vec![], None);
    for e in lines(&log) {
        if e.get("type").and_then(Value::as_str) != Some("queue-operation") {
            continue;
        }
        let content = e.get("content").and_then(Value::as_str).unwrap_or("").to_string();
        match e.get("operation").and_then(Value::as_str) {
            Some("enqueue") => {
                if mine(&content) {
                    last = Some(Queued::Waiting);
                }
                pending.push(content);
            }
            Some("remove") => {
                if let Some(i) = pending.iter().position(|c| *c == content) {
                    pending.remove(i);
                }
                if mine(&content) {
                    last = Some(Queued::Absorbed);
                }
            }
            Some("dequeue") if !pending.is_empty() => {
                if mine(&pending.remove(0)) {
                    last = Some(Queued::Taken(e.get("timestamp").and_then(Value::as_str).and_then(iso_ms)));
                }
            }
            _ => {}
        }
    }
    last
}

/// Where a message you queued for a busy session stands, from its agent's own log: Claude Code's
/// queue log, or Codex's rollout. None: no log that says (Pi reads it only once its turn is over).
pub fn read_state(harness: &str, path: &str, text: &str, sent_ms: u64) -> Option<Queued> {
    match harness {
        "claude" => queued_state(path, text),
        "codex" => codex_queued(path, text, sent_ms),
        _ => None,
    }
}

/// Codex logs each message it takes as a `user_message`: right after `task_started` when it's the
/// next turn's prompt, mid-turn when it was steered into the running turn. Only messages since you
/// sent it count (an older "yes" isn't this one).
fn codex_queued(path: &str, text: &str, sent_ms: u64) -> Option<Queued> {
    let len = std::fs::metadata(path).ok()?.len();
    let log = read_from(path, len.saturating_sub(512 * 1024))?;
    let (mut prev, mut state) = (String::new(), Queued::Waiting);
    for e in lines(&log).filter(|e| e.get("type").and_then(Value::as_str) == Some("event_msg")) {
        let p = &e["payload"];
        let kind = p.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let at = e.get("timestamp").and_then(Value::as_str).and_then(iso_ms);
        if kind == "user_message" && at.map_or(true, |a| a + 5000 >= sent_ms) && is_message(p.get("message").and_then(Value::as_str).unwrap_or(""), text) {
            state = if prev == "task_started" { Queued::Taken(at) } else { Queued::Absorbed };
        }
        if kind != "token_count" {
            prev = kind;
        }
    }
    Some(state)
}

/// Whether the log has `text` as a turn of yours, written at `since_ms` or later (a couple of seconds'
/// leeway for clocks): what Cue typed into the session, taken by the agent, even when no hook said so.
pub fn prompt_since(path: &str, text: &str, since_ms: u64) -> bool {
    if path.is_empty() || text.trim().is_empty() {
        return false;
    }
    let Ok(md) = std::fs::metadata(path) else { return false };
    let Some(log) = read_from(path, md.len().saturating_sub(256 * 1024)) else { return false };
    let found = lines(&log).any(|e| {
        e.get("type").and_then(Value::as_str) == Some("user")
            && e.get("isSidechain").and_then(Value::as_bool) != Some(true)
            && stamp(&e) + 2_000 >= since_ms
            && {
                // A long paste is logged wrapped in `<pasted_content>` tags: compared as you sent it.
                let said: String = content_blocks(&e).iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
                is_message(&crate::sessions::unwrap_pasted(&said), text)
            }
    });
    found
}

/// Whether the log has `text` handed to the agent mid-turn, at `since_ms` or later (the same leeway as
/// `prompt_since`): a message you sent while it worked, taken in between two steps. Claude Code writes
/// that as a `queued_command` attachment (`yours_mid_turn`), not as a turn of yours, so `prompt_since`
/// never sees it; its prompt is compared as you sent it, a long paste's wrapper taken off.
pub fn taken_mid_turn(path: &str, text: &str, since_ms: u64) -> bool {
    if path.is_empty() || text.trim().is_empty() {
        return false;
    }
    let Ok(md) = std::fs::metadata(path) else { return false };
    let Some(log) = read_from(path, md.len().saturating_sub(256 * 1024)) else { return false };
    let found = lines(&log).any(|e| {
        yours_mid_turn(&e)
            && stamp(&e) + 2_000 >= since_ms
            && e.pointer("/attachment/prompt").and_then(Value::as_str).is_some_and(|p| is_message(&crate::sessions::unwrap_pasted(p), text))
    });
    found
}
