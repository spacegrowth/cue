//! What was said in a session, as Cue shows it: your prompts and the agent's replies, the lines a
//! prompt hook added to them (said by voice, a status list asked for), and the last few turns as context.

use super::{content_blocks, cut, harness_text, lines, read_from, stamp, tail_offset, teammate_messages};
use crate::model::Ctx;
use serde_json::Value;
use std::fs::File;

const CTX_CHARS: usize = 1500;

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= CTX_CHARS {
        return s.to_string();
    }
    let head: String = s.chars().take(CTX_CHARS).collect();
    format!("{head}…")
}

/// What a prompt hook added for the agent, as the lines of its context (an attachment row after your prompt).
fn hook_context(e: &Value) -> Vec<&str> {
    if e.get("type").and_then(Value::as_str) != Some("attachment") || e.pointer("/attachment/type").and_then(Value::as_str) != Some("hook_additional_context") {
        return vec![];
    }
    e.pointer("/attachment/content").and_then(Value::as_array).map(|c| c.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

/// The prompt hook's added context, with the line Cue adds to what you said by voice.
fn said_by_voice(e: &Value) -> bool {
    hook_context(e).contains(&crate::talk::ASK)
}

/// What marks a status-list ask in a prompt hook's context: the heading the agent ends its replies with.
const STATUS_MARK: &str = "**Where we are**";

/// The text a prompt hook added to ask for (or recall) the agent's status list, word for word. Only
/// context that holds the status heading counts, so another hook's note isn't taken for it.
fn status_ask(e: &Value) -> Option<String> {
    let asks: Vec<&str> = hook_context(e).into_iter().filter(|l| l.contains(STATUS_MARK)).collect();
    (!asks.is_empty()).then(|| asks.join("\n\n"))
}

/// The text blocks of a transcript entry, joined.
fn text_of(e: &Value) -> String {
    content_blocks(e).iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n\n")
}

/// The status lists a prompt hook asked for with your recent prompts (the last 256 KB of the transcript): each
/// prompt's text with the hook's text, for the prompts whose hook context holds the status heading.
pub fn status_asks(path: &str) -> Vec<(String, String)> {
    let Some(log) = std::fs::metadata(path).ok().and_then(|m| read_from(path, m.len().saturating_sub(256 * 1024))) else { return vec![] };
    let mut prompts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut out = vec![];
    for e in lines(&log) {
        if let Some(ask) = status_ask(&e) {
            if let Some(text) = e.get("parentUuid").and_then(Value::as_str).and_then(|u| prompts.get(u)) {
                out.push((text.clone(), ask));
            }
        } else if e.get("type").and_then(Value::as_str) == Some("user") && e.get("isSidechain").and_then(Value::as_bool) != Some(true) {
            if let Some(uuid) = e.get("uuid").and_then(Value::as_str) {
                prompts.insert(uuid.to_string(), text_of(&e).trim().to_string());
            }
        }
    }
    out
}

/// The conversation as Cue's chat shows it, from the transcript: your prompts, and the agent's last
/// text reply of each turn (not its tool calls or what it said between steps). Only what's before
/// `before_ms`, the last `limit` of it, oldest first. For what's older than Cue's own log (a session
/// from before Cue, or one you resumed): read only when you ask for it, from the start of the file,
/// stopping at `before_ms`.
pub fn conversation_before(path: &str, before_ms: u64, limit: usize) -> Vec<Value> {
    use std::io::BufRead;
    let Ok(f) = File::open(path) else { return vec![] };
    let mut out: Vec<Value> = vec![];
    let mut reply: Option<(u64, String)> = None;   // the turn's latest text reply so far
    let mut yours: Option<(String, usize)> = None;   // your latest message: its entry's uuid, where it is in `out`
    let msg = |at: u64, role: &str, text: &str| serde_json::json!({ "at_ms": at, "role": role, "text": cut(text.trim(), 6000), "images": [] });
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        let Ok(e) = serde_json::from_str::<Value>(&line) else { continue };
        let at = stamp(&e);
        if at >= before_ms {
            break;
        }
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) || e.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        // Said by voice: Cue's prompt hook added the line asking for a short spoken answer to that message.
        if said_by_voice(&e) {
            if let Some((_, i)) = yours.as_ref().filter(|(uuid, _)| e.get("parentUuid").and_then(Value::as_str) == Some(uuid)) {
                out[*i]["voice"] = Value::Bool(true);
            }
            continue;
        }
        // A status list asked for: another prompt hook added its ask to that message.
        if let Some(ask) = status_ask(&e) {
            if let Some((_, i)) = yours.as_ref().filter(|(uuid, _)| e.get("parentUuid").and_then(Value::as_str) == Some(uuid)) {
                out[*i]["status_ask"] = Value::String(ask);
            }
            continue;
        }
        let text = text_of(&e);
        // Slash-command plumbing, notices, a Stop hook's feedback and "[Request interrupted…]" aren't conversation.
        if text.trim().is_empty() || text.starts_with('<') || text.starts_with("[Request interrupted") || harness_text(&text) {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("assistant") => reply = Some((at, text)),
            Some("user") => {
                if let Some((t, r)) = reply.take() {
                    out.push(msg(t, "agent", &r));
                }
                // A teammate's message (or its notice, which says nothing to show): not yours.
                if text.starts_with("Another Claude session sent a message:") && text.contains("<teammate-message") {
                    for (who, said) in teammate_messages(&text) {
                        let mut m = msg(at, "peer", &said);
                        m["from"] = Value::String(who);
                        out.push(m);
                    }
                    continue;
                }
                yours = e.get("uuid").and_then(Value::as_str).map(|u| (u.to_string(), out.len()));
                out.push(msg(at, "you", &text));
            }
            _ => {}
        }
    }
    if let Some((t, r)) = reply {
        out.push(msg(t, "agent", &r));
    }
    let skip = out.len().saturating_sub(limit);
    out.into_iter().skip(skip).collect()
}

/// The last `n` human-readable turns: your prompts and Claude's text replies (no tool noise).
pub fn recent_context(path: &str, n: usize) -> Vec<Ctx> {
    let Some(text) = read_from(path, tail_offset(path)) else { return vec![] };
    let mut out: Vec<Ctx> = Vec::new();
    for entry in lines(&text) {
        let role = match entry.get("type").and_then(Value::as_str) {
            Some("assistant") => "assistant",
            Some("user") => "user",
            _ => continue,
        };
        if entry.get("isMeta").and_then(Value::as_bool) == Some(true)
            || entry.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        for b in content_blocks(&entry) {
            if b.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let t = b.get("text").and_then(Value::as_str).unwrap_or("");
            // Slash-command plumbing, system reminders and a Stop hook's feedback aren't conversation.
            if t.trim().is_empty() || t.starts_with('<') || harness_text(t) {
                continue;
            }
            out.push(Ctx { role: role.into(), text: clip(t) });
        }
    }
    let skip = out.len().saturating_sub(n);
    out.into_iter().skip(skip).collect()
}
