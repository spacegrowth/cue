//! Who wrote a transcript entry: you, another agent session, a teammate, or Claude Code itself (a
//! queued task notification, a Stop hook's feedback).

use super::{content_blocks, lines, read_from, stamp, tail_offset};
use serde_json::Value;

/// Who sent a prompt, as Claude records it in `origin`.
#[derive(Debug, PartialEq)]
pub enum Sender {
    /// You: typed, queued, or a suggestion you accepted. Also any transcript too old to say.
    Human,
    /// Another agent session messaged this one: Claude's name for it ("cue-3e") and its process.
    Peer { name: String, pid: Option<i32>, body: String },
    /// Claude Code's own plumbing: a background agent's task-notification, a subagent's hand-back.
    Harness,
}

/// Who sent prompt `prompt_id`; None until Claude has written it to the transcript.
pub fn prompt_sender(path: &str, prompt_id: &str) -> Option<Sender> {
    let text = read_from(path, tail_offset(path))?;
    // The prompt and the tool results that follow it all carry its promptId; only the prompt says
    // who sent it. A transcript too old to carry `origin` at all counts as you.
    let mine: Vec<Value> = lines(&text)
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("user") && e.get("promptId").and_then(Value::as_str) == Some(prompt_id))
        .collect();
    let e = mine.iter().find(|e| e.get("origin").is_some()).or(mine.first())?;
    let o = |k: &str| e.pointer(&format!("/origin/{k}")).cloned().unwrap_or(Value::Null);
    Some(match o("kind").as_str() {
        None | Some("human") => Sender::Human,
        Some("peer") if o("handback").as_bool() != Some(true) => Sender::Peer {
            name: o("name").as_str().unwrap_or("").to_string(),
            pid: o("verifiedPeerPid").as_i64().map(|p| p as i32),
            body: o("body").as_str().unwrap_or("").to_string(),
        },
        _ => Sender::Harness,
    })
}

/// Your latest request to a session (`last-prompt`), from its own transcript, no model. Claude
/// re-writes it every turn, so the last 256 KB has it; only those lines are parsed.
pub fn about(path: &str) -> String {
    let from = std::fs::metadata(path).map(|m| m.len().saturating_sub(256 * 1024)).unwrap_or(0);
    let Some(tail) = read_from(path, from) else { return String::new() };
    tail.lines()
        .rev()
        .filter(|l| l.contains("\"type\":\"last-prompt\""))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|e| e.get("lastPrompt").and_then(Value::as_str).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()))
        .unwrap_or_default()
}

/// Whether `text` reached the session from Claude Code's own queue, not from you. A background
/// task that finishes while the session is busy is queued, then handed over when the turn ends as
/// a `queued_command` attachment: there is no prompt entry with the hook's promptId to read
/// `origin` from, so `prompt_sender` never finds one. The attachment carries the text and origin.
pub fn queued_from_harness(path: &str, text: &str) -> bool {
    let Some(tail) = read_from(path, tail_offset(path)) else { return false };
    let text = text.trim();
    !text.is_empty()
        && lines(&tail).any(|e| {
            let a = |k: &str| e.pointer(&format!("/attachment/{k}")).and_then(Value::as_str).map(str::to_string);
            e.get("type").and_then(Value::as_str) == Some("attachment")
                && a("type").as_deref() == Some("queued_command")
                && a("prompt").is_some_and(|p| p.trim() == text)
                && a("origin/kind").is_some_and(|k| k != "human")
        })
}

/// How Claude Code records a Stop hook's "keep going" in the transcript: a user entry starting with this.
pub const STOP_HOOK_MARK: &str = "Stop hook feedback:";

/// A Stop hook sent the agent back to work: Claude Code writes the hook's output as a user entry (a meta
/// one, `isMeta`) starting with STOP_HOOK_MARK. What the agent writes after it in the same turn is the
/// hook's follow-up (a checklist, say), not its answer. Every reader of a transcript asks this, so all of
/// them (the steps feed, a turn's parts, Cue's Stop hook) agree on where the follow-up starts.
pub fn is_stop_hook(e: &Value) -> bool {
    e.get("type").and_then(Value::as_str) == Some("user")
        && content_blocks(e).iter().filter_map(|b| b.get("text").and_then(Value::as_str)).next().is_some_and(|t| t.trim_start().starts_with(STOP_HOOK_MARK))
}

/// Your message, taken into a running turn: Claude Code hands it over as a `queued_command` attachment, not
/// a prompt entry. New work, so after a Stop hook's follow-up what the agent writes is an answer again.
pub fn yours_mid_turn(e: &Value) -> bool {
    let a = |k: &str| e.pointer(&format!("/attachment/{k}")).and_then(Value::as_str);
    e.get("type").and_then(Value::as_str) == Some("attachment")
        && a("type") == Some("queued_command")
        && a("commandMode") == Some("prompt")
        && a("origin/kind").is_none_or(|k| k == "human")
        && a("prompt").is_some_and(|p| !p.trim().is_empty() && !p.contains("<task-notification>"))
}

/// Text only Claude Code itself sends, written as a "user" entry but never yours: a background task's
/// `<task-notification>`, and a Stop hook's feedback (STOP_HOOK_MARK). The chat never shows either as
/// your message, whatever flag (`isMeta`) Claude Code puts on the entry.
pub fn harness_text(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<task-notification>") || t.starts_with(STOP_HOOK_MARK)
}

/// A teammate's message to its lead (or its notice), as Claude Code writes it into the lead's log: from it, not
/// you, so it's never your prompt and never starts a turn of yours.
pub fn peer_text(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("Another Claude session sent a message:") || t.starts_with("<teammate-message")
}

/// What a session's teammates (agents started with a name) said to it, in a user entry's text: each
/// teammate's name and its message. Claude Code writes them into the lead's log mid-turn, with no
/// `origin` and no prompt hook, as "Another Claude session sent a message:" then
/// `<teammate-message teammate_id="reader" …>…</teammate-message>`. Its own notices come the same way,
/// as JSON: an idle notice counts only when it carries the teammate's answer (`result`), the rest not at all.
pub fn teammate_messages(text: &str) -> Vec<(String, String)> {
    let t = text.trim_start();
    if !t.starts_with("Another Claude session sent a message:") && !t.starts_with("<teammate-message") {
        return vec![];
    }
    t.split("<teammate-message").skip(1).filter_map(|r| {
        let (attrs, rest) = r.split_once('>')?;
        let body = rest.split("</teammate-message>").next()?.trim();
        let name = attrs.split("teammate_id=\"").nth(1)?.split('"').next()?.to_string();
        let body = match serde_json::from_str::<Value>(body) {
            Ok(v) if v.is_object() => match v.get("type").and_then(Value::as_str) {
                Some("idle_notification") => v.get("result").and_then(Value::as_str)?.trim().to_string(),
                _ => return None,
            },
            _ => body.to_string(),
        };
        (!name.is_empty() && !body.is_empty()).then_some((name, body))
    }).collect()
}

/// Teammates' messages in the session's log written after `after_ms` (by its clock): when, who, what.
pub fn teammate_messages_after(path: &str, after_ms: u64) -> Vec<(u64, String, String)> {
    let Some(text) = read_from(path, tail_offset(path)) else { return vec![] };
    lines(&text)
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("user") && e.get("isSidechain").and_then(Value::as_bool) != Some(true) && e.get("origin").is_none_or(Value::is_null))
        .filter(|e| stamp(e) > after_ms)
        .flat_map(|e| {
            let at = stamp(&e);
            content_blocks(&e).iter().filter_map(|b| b.get("text").and_then(Value::as_str)).flat_map(teammate_messages).map(|(who, said)| (at, who, said)).collect::<Vec<_>>()
        })
        .collect()
}
