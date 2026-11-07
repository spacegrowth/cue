//! One log's feed: how far into the file it got, the turns read so far and the session's background
//! work, and the reading of new whole lines. What it makes of each line is in `observe` (the model,
//! context and tokens) and `take` (the turns and steps).

use super::helper::{Helper, TaskFailure, MAX_HELPERS};
use super::kind::{kind_of, translate, Kind};
use super::model::{Item, Meta, Turn};
use super::windows::auto_compact_at;
use super::START_BYTES;
use crate::logs::transcript::lines;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

const MAX_TURNS: usize = 30;
/// Per turn: a very long turn keeps its newest steps and says how many came before.
const MAX_ITEMS: usize = 400;

pub(super) struct Feed {
    pub(super) path: String,
    pub(super) meta: Meta,
    /// Whose log it is (its first line says): Claude Code's, Pi's or Codex's.
    pub(super) kind: Kind,
    /// Codex: each apply_patch's patch, by call id (its result doesn't repeat it; the step's size and
    /// diff come from it).
    pub(super) patches: HashMap<String, String>,
    pub(super) offset: u64,
    /// Started mid-file: the first (partial) line still has to be skipped.
    pub(super) align: bool,
    pub(super) turns: Vec<Turn>,
    pub(super) version: u64,
    pub(super) used: Instant,
    /// A background agent's own log (every entry is a sidechain one, and read as this feed's).
    pub(super) helper: bool,
    /// Helper: when its last answer ended (its report is written, no step open); 0 while it works.
    pub(super) ended_ms: u64,
    /// The background agents this session started (Claude Code's Agent tool), each read from its own log.
    pub(super) helpers: Vec<Helper>,
    /// How many helpers it keeps (finished ones go first): MAX_HELPERS, or all for a session's history.
    pub(super) max_helpers: usize,
    /// Helper: its tokens so far, summed over its replies (input, output, cache reads and writes).
    pub(super) tokens: u64,
    /// Background tasks of this session that failed after a while, until the hub takes them.
    pub(super) failures: Vec<TaskFailure>,
    /// Claude Code: the reply whose usage was counted last. It writes one line per content block, each
    /// with the same usage, so a turn's tokens count each reply once.
    pub(super) last_reply: String,
    /// Codex: its running total of tokens; a turn's tokens are what it grew by.
    pub(super) codex_total: u64,
    /// Its writes of a hand-off note (`is_handoff_note`) still waiting for their result: path by call id.
    pub(super) note_calls: HashMap<String, String>,
    /// The newest hand-off note it wrote since your last prompt, and when (its result's time, by the log).
    pub(super) note: Option<(String, u64)>,
    /// Your latest prompt: when (by the log), and whether it was Cue's hand-off ask (HANDOFF_ASK_START).
    pub(super) prompted: (u64, bool),
}

/// How far into a log to look for its first reply's context (the start a feed opened mid-file misses).
const HEAD_BYTES: u64 = 1024 * 1024;

/// The context a session's first reply carried, and when, from the top of its log: what it started at.
fn start_of(path: &str, kind: Kind) -> (u64, u64) {
    let Ok(file) = std::fs::File::open(path) else { return (0, 0) };
    let mut buf = Vec::new();
    if file.take(HEAD_BYTES).read_to_end(&mut buf).is_err() {
        return (0, 0);
    }
    let end = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut head = Feed::start(path, 0, 0);
    head.kind = kind;
    for e in lines(&String::from_utf8_lossy(&buf[..end])) {
        head.observe(&e);
        for c in translate(kind, &e, &mut head.patches) {
            head.take(&c);
        }
        if head.meta.start_context > 0 {
            return (head.meta.start_context, head.meta.start_ms);
        }
    }
    (0, 0)
}

impl Feed {
    pub(super) fn open(path: &str, version: u64) -> Feed {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let offset = len.saturating_sub(START_BYTES);
        let mut f = Feed::start(path, version, offset);
        if offset > 0 {
            (f.meta.start_context, f.meta.start_ms) = start_of(path, f.kind);
        }
        f
    }

    pub(super) fn start(path: &str, version: u64, offset: u64) -> Feed {
        Feed { path: path.to_string(), meta: Meta::default(), kind: kind_of(path), patches: HashMap::new(), offset, align: offset > 0, turns: Vec::new(), version, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false), max_helpers: MAX_HELPERS }
    }

    /// A background agent's log, read from its start (it's small: one task).
    pub(super) fn open_helper(path: &str) -> Feed {
        Feed { path: path.to_string(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: true, ended_ms: 0, helpers: Vec::new(), tokens: 0, failures: Vec::new(), last_reply: String::new(), codex_total: 0, note_calls: HashMap::new(), note: None, prompted: (0, false), max_helpers: MAX_HELPERS }
    }

    /// Read what was added since last time (whole lines only; a half-written line waits for the next call).
    pub(super) fn catch_up(&mut self) {
        let mut changed = self.read_new();
        // Its background agents' logs too: their steps are part of what this session is doing.
        for h in &mut self.helpers {
            changed |= h.catch_up();
        }
        self.close_scripts();
        if changed {
            self.meta.cold_x = crate::logs::prices::cold_x(&self.meta.model, self.meta.cache_ttl_s);
            self.meta.cold_cents = crate::logs::prices::cold_cents(&self.meta.model, self.meta.cache_ttl_s, self.meta.context);
            self.meta.auto_at = if self.kind == Kind::Claude { auto_compact_at(&self.meta.model) } else { 0 };
            if self.turns.len() > MAX_TURNS {
                self.turns.drain(..self.turns.len() - MAX_TURNS);
            }
            self.version += 1;
        }
    }

    /// Read this log's new lines; true when anything shown changed.
    fn read_new(&mut self) -> bool {
        crate::chaos::pause(crate::chaos::Knob::FsDelayMs);
        let Ok(mut file) = std::fs::File::open(&self.path) else { return false };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut changed = false;
        if len < self.offset {
            // The file was replaced: start over (and count a version, so callers redraw), still a background
            // agent's log if it was one (else its sidechain lines would all be skipped from here on).
            let helper = self.helper;
            *self = Feed::open(&self.path, self.version);
            self.helper = helper;
            changed = true;
        }
        if len <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return changed;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return changed;
        }
        let mut from = 0;
        if self.align {
            let Some(nl) = buf.iter().position(|&b| b == b'\n') else { return changed };
            from = nl + 1;
            self.align = false;
        }
        let Some(end) = buf.iter().rposition(|&b| b == b'\n').map(|i| i + 1).filter(|&e| e > from) else {
            self.offset += from as u64;
            return changed;
        };
        let text = String::from_utf8_lossy(&buf[from..end]);
        for e in lines(&text) {
            changed |= self.observe(&e);
            for c in translate(self.kind, &e, &mut self.patches) {
                changed |= self.take(&c);
            }
        }
        self.offset += end as u64;
        changed
    }

    pub(super) fn push(&mut self, item: Item) {
        if self.turns.is_empty() {
            // Started mid-file, inside a turn whose prompt is further back.
            let at = match &item { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms };
            self.turns.push(Turn::starting(at, String::new()));
        }
        let turn = self.turns.last_mut().unwrap();
        turn.items.push(item);
        if turn.items.len() > MAX_ITEMS {
            turn.items.remove(0);
            turn.more += 1;
        }
    }

    /// Steps still open in the current turn when it ends (stopped, or the next prompt came): done.
    pub(super) fn close_open(&mut self, why: &str) -> bool {
        let Some(turn) = self.turns.last_mut() else { return false };
        let mut changed = false;
        for item in turn.items.iter_mut() {
            if let Item::Step { verb, past, result, done, .. } = item {
                if !*done {
                    *verb = past.to_string();
                    *result = why.to_string();
                    *done = true;
                    changed = true;
                }
            }
        }
        changed
    }
}
