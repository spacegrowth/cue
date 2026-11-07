//! Reads Claude Code session transcripts (JSONL). Two jobs:
//! 1. Context: the last few things said, so a request isn't shown cold.
//! 2. "Answered in the terminal": Claude never tells the hook when you answer in the terminal,
//!    but the transcript gets a tool_result for that tool_use within about a second.
//!
//! This file holds reading the log and the small helpers every reader shares; the readers live in
//! `tools` (tool calls and their results), `activity` (what a working session is doing), `senders`
//! (who wrote an entry), `conversation` (what was said), `queue` (where a message you sent stands)
//! and `turns` (when a turn ended, was interrupted or compacted).

mod activity;
mod conversation;
mod queue;
mod senders;
mod tools;
mod turns;
#[cfg(test)]
mod tests;

pub use activity::activity_of;
pub use conversation::{conversation_before, recent_context, status_asks};
pub use queue::{is_message, prompt_since, queued_state, read_state, taken_mid_turn, Queued};
pub use senders::{about, harness_text, is_stop_hook, peer_text, prompt_sender, queued_from_harness, teammate_messages, teammate_messages_after, yours_mid_turn, Sender};
pub use tools::{command_count, find_tool_use_id, has_result, waiting_tool_use};
pub use turns::{compact_finished, interrupted_at, turn_ended_parts};

use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const TAIL_BYTES: u64 = 1024 * 1024;

/// Read from `from` to EOF; drop a partial first line when starting mid-file.
fn read_from(path: &str, from: u64) -> Option<String> {
    crate::chaos::pause(crate::chaos::Knob::FsDelayMs);
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = from.min(len);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf).to_string();
    if start > 0 {
        Some(s.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default())
    } else {
        Some(s)
    }
}

pub fn tail_offset(path: &str) -> u64 {
    std::fs::metadata(path).map(|m| m.len().saturating_sub(TAIL_BYTES)).unwrap_or(0)
}

pub(crate) fn lines(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
}

pub(crate) fn content_blocks(entry: &Value) -> Vec<Value> {
    match entry.pointer("/message/content") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => vec![serde_json::json!({"type": "text", "text": s})],
        _ => vec![],
    }
}

/// When a transcript entry was written ("2026-10-04T14:08:39.507Z" → ms), 0 if it doesn't say.
pub(crate) fn stamp(e: &Value) -> u64 {
    e.get("timestamp").and_then(Value::as_str).and_then(iso_ms).unwrap_or(0)
}

pub(crate) fn iso_ms(t: &str) -> Option<u64> {
    let num = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (num(0, 4)?, num(5, 7)?, num(8, 10)?, num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let ms = t.get(19..).and_then(|r| r.strip_prefix('.')).map(|r| r.chars().take_while(char::is_ascii_digit).take(3).collect::<String>()).filter(|f| !f.is_empty())
        .map(|f| f.parse::<i64>().unwrap_or(0) * 10_i64.pow(3 - f.len() as u32)).unwrap_or(0);
    // Days since 1970-01-01 for a UTC date (Howard Hinnant's days_from_civil).
    let (y, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + d - 1;
    let days = era * 146097 + doe - 719468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60 * 1000 + sec * 1000 + ms).ok()
}

pub(crate) fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

/// A Claude Code session's transcript, wherever its folder is: ~/.claude/projects/<folder>/<id>.jsonl.
pub fn find_claude(session_id: &str) -> Option<String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let home = std::env::var("HOME").ok()?;
    std::fs::read_dir(format!("{home}/.claude/projects")).ok()?.flatten().map(|d| d.path().join(format!("{session_id}.jsonl"))).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}
