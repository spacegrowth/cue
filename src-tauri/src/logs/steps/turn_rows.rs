//! A session's turns as Insights keeps them (logs/turns.rs): what each cost and how it went, read from the
//! log a stretch at a time.

use super::feed::Feed;
use super::kind::{kind_of, translate, Kind};
use super::model::{area_of, Item, Turn, Usage};
use crate::logs::transcript::content_blocks;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};

/// A time a reply's line really carried (2001 on): a line without one still marks the turn as replied to.
const SANE_MS: u64 = 1_000_000_000_000;

/// One turn of a session, as Insights keeps it (logs/turns.rs): what it cost and how it went, and the start
/// of its prompt (and of the next one, when that corrected it).
pub struct TurnRow {
    pub at_ms: u64,
    pub tokens: u64,
    pub context: u64,
    pub prompt_chars: u64,
    pub model: String,
    pub cwd: String,
    /// Steps that failed (a command's non-zero exit, an edit that didn't apply).
    pub errors: u64,
    /// You stopped it mid-turn.
    pub interrupted: bool,
    /// Your next prompt read as a correction ("no, …", "actually …", "still broken").
    pub corrected: bool,
    /// Its tokens by kind (for prices, and what a late answer re-wrote).
    pub usage: Usage,
    /// How long the session had waited on you since its reply before (0: not known, as for a log's first
    /// turn read here).
    pub waited_ms: u64,
    /// Its prompt and, when corrected, your next one: the start of each, for Insights' examples.
    pub prompt: String,
    pub correction: String,
    /// Reads and searches it ran itself, background agents it started.
    pub looks: u64,
    pub agents: u64,
    /// The areas of the project it edited (area_of), each once, one per line.
    pub areas: String,
    /// The context Claude Code compacted it at on its own (its log's compact_boundary; 0: it didn't).
    pub auto_compact: u64,
}

/// Whether a prompt reads as correcting the turn before it. English openers only.
/// SHORTCUT: a fixed list of openers, blind to corrections phrased any other way; upgrade to a judgment
/// the person can tune when the turns history is used for more than counting.
fn reads_as_correction(prompt: &str) -> bool {
    let p = prompt.trim_start().to_lowercase();
    const OPENERS: [&str; 30] = ["no ", "no,", "no.", "nope", "wrong", "that's not", "thats not", "not that", "not what i", "actually", "wait", "stop", "undo", "revert", "i meant", "i said", "you missed", "you forgot", "you didn't", "you broke", "why did you", "why are you", "still not", "still broken", "still failing", "it's still", "doesn't work", "didn't work", "does not work", "did not work"];
    OPENERS.iter().any(|o| p.starts_with(o)) || p == "no"
}

/// A prompt's first 160 characters as you wrote them: a long block of instructions appended in brackets at
/// its end (a dictation's or a tool's) is left out, or it would fill the snippet.
pub(super) fn snippet(prompt: &str) -> String {
    let p = prompt.trim_end();
    let bare = p.strip_suffix(')').and_then(|rest| rest.rfind('(').filter(|&i| p.len() - i > 40 && !rest[i + 1..].contains(['(', ')'])).map(|i| rest[..i].trim_end())).filter(|b| !b.is_empty());
    bare.unwrap_or(p).chars().take(160).collect()
}

/// The turns in `path` from byte `from` on, for Insights: every complete turn, and the open last one too.
/// Returns them with the byte where that open turn starts, so the next read begins there and finishes it.
/// Reads at most `max` bytes, whole lines only.
pub fn turn_rows(path: &str, from: u64, max: u64) -> (Vec<TurnRow>, u64) {
    let Some((buf, end)) = whole_lines(path, from, max) else { return (Vec::new(), from) };
    let kind = kind_of(path);
    let mut feed = Feed::start(path, 0, 0);
    feed.kind = kind;
    let mut beside: Vec<Beside> = Vec::new();
    let mut cwd = String::new();
    let mut pos = 0usize;
    for line in buf[..end].split(|&b| b == b'\n') {
        let at = from + pos as u64;
        pos += line.len() + 1;
        let Ok(e) = serde_json::from_slice::<Value>(line) else { continue };
        read_entry(&mut feed, kind, &e, at, &mut cwd, &mut beside);
    }
    let rows = beside.iter().enumerate().map(|(i, b)| row(&feed.turns, i, b)).collect();
    // The open turn starts the next read; none read: carry on after what was read.
    let next = beside.last().map_or(from + end as u64, |b| b.start);
    (rows, next)
}

/// Per turn, beside the feed's: when it started in the file, its model and folder, whether you stopped
/// it, and the context Claude Code compacted it at on its own.
struct Beside {
    start: u64,
    model: String,
    cwd: String,
    stopped: bool,
    auto: u64,
}

/// The bytes of `path` from `from` on (at most `max`), and where its last whole line ends; None when
/// there's no whole line.
fn whole_lines(path: &str, from: u64, max: u64) -> Option<(Vec<u8>, usize)> {
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    file.take(max).read_to_end(&mut buf).ok()?;
    let end = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1)?;
    Some((buf, end))
}

/// One log entry, read into the feed, with what each turn needs beside it. `at`: where the entry starts in the file.
fn read_entry(feed: &mut Feed, kind: Kind, e: &Value, at: u64, cwd: &mut String, beside: &mut Vec<Beside>) {
    if let Some(c) = e.get("cwd").or_else(|| e.pointer("/payload/cwd")).and_then(Value::as_str).filter(|c| !c.is_empty()) {
        *cwd = c.to_string();
    }
    feed.observe(e);
    for c in translate(kind, e, &mut feed.patches) {
        if c.get("type").and_then(Value::as_str) == Some("user") {
            let blocks = content_blocks(&c);
            let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
            if text.trim_start().starts_with("[Request interrupted") {
                if let Some(b) = beside.last_mut() {
                    b.stopped = true;
                }
            }
        }
        feed.take(&c);
    }
    while beside.len() < feed.turns.len() {
        beside.push(Beside { start: at, model: String::new(), cwd: cwd.clone(), stopped: false, auto: 0 });
    }
    // Claude Code compacted it on its own, at this much context: kept with the turn it happened in.
    if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") && e.pointer("/compactMetadata/trigger").and_then(Value::as_str) == Some("auto") {
        if let (Some(b), Some(pre)) = (beside.last_mut(), e.pointer("/compactMetadata/preTokens").and_then(Value::as_u64)) {
            b.auto = pre;
        }
    }
    if let Some(b) = beside.last_mut() {
        if !feed.meta.model.is_empty() {
            b.model.clone_from(&feed.meta.model);
        }
    }
}

/// Turn `i` of `turns` as its row.
fn row(turns: &[Turn], i: usize, b: &Beside) -> TurnRow {
    let t = &turns[i];
    // A turn Claude Code started itself (a background task reporting back) has no prompt: it wasn't you
    // answering late, and the turn a correction corrects is the one before your next prompt.
    let next_yours = || turns[i + 1..].iter().find(|t| !t.prompt.is_empty());
    TurnRow {
        at_ms: t.at_ms,
        tokens: t.tokens,
        context: t.context,
        prompt_chars: t.prompt.chars().count() as u64,
        model: b.model.clone(),
        cwd: b.cwd.clone(),
        errors: t.items.iter().filter(|x| matches!(x, Item::Step { bad: true, .. })).count() as u64,
        interrupted: b.stopped,
        corrected: next_yours().is_some_and(|next| reads_as_correction(&next.prompt)),
        usage: t.usage,
        waited_ms: i.checked_sub(1).filter(|_| !t.prompt.is_empty()).map(|p| turns[p].usage.last_ms).filter(|&l| l > SANE_MS && t.at_ms > l).map_or(0, |l| t.at_ms - l),
        prompt: snippet(&t.prompt),
        correction: next_yours().filter(|next| reads_as_correction(&next.prompt)).map(|next| snippet(&next.prompt)).unwrap_or_default(),
        looks: t.work.looks,
        agents: t.work.agents,
        areas: {
            let mut a: Vec<String> = t.work.edited.iter().filter_map(|f| area_of(&b.cwd, f)).collect();
            a.sort();
            a.dedup();
            a.join("\n")
        },
        auto_compact: b.auto,
    }
}
