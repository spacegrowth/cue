//! What a session did, step by step, read from its Claude Code transcript for the chat's step lines
//! (the window's Active pane, and anything else that asks): the agent's words between steps, and each
//! tool call as one line (what it did, how it went). Read only while someone has the session open:
//! each feed remembers how far into the file it got and reads just the new lines, and a feed nobody
//! asked for in a couple of minutes is dropped. Nothing is saved; the transcript is the only copy.
//!
//! A step's full output or diff isn't part of the feed: `detail` fetches it when you open the step.
//!
//! Pi's session log (~/.pi/agent/sessions/…) and Codex's (~/.codex/sessions/…/rollout-*.jsonl) are read
//! the same way: each of their entries is first put in the shape Claude Code's transcript uses
//! (`from_pi`, `from_codex`), so all three share everything below.
//!
//! SHORTCUT: viewers poll (every 0.5 s while the session works) rather than being
//! pushed new steps. Fine for a few viewers, since a poll with nothing new reads no file and answers
//! with just a version; push new steps instead if many viewers watch at once.
//! SHORTCUT: only the last 4 MB of a transcript is read, so very old turns of a huge session aren't
//! there. That's past anything the chat shows; read further back on demand if that changes.

use crate::transcript::{content_blocks, cut, lines, stamp};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A first look reads this much from the end of the transcript (older turns are past what the chat shows).
const START_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TURNS: usize = 30;
/// Per turn: a very long turn keeps its newest steps and says how many came before.
const MAX_ITEMS: usize = 400;
const FORGET: Duration = Duration::from_secs(120);
/// A step's output, opened: this many lines at most (the start and, mostly, the end).
const OUT_HEAD: usize = 40;
const OUT_TAIL: usize = 160;
const LINE_CHARS: usize = 400;

#[derive(Clone, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Item {
    /// The agent's own words between steps.
    Say { text: String, at_ms: u64 },
    /// One tool call. `verb` + `subject` read as a line ("Ran" + "npm test"); `result` is how it went
    /// ("84 lines", "+6 −2", "exit 1"); `bad` when it failed; `done` once its result is in.
    Step {
        id: String,
        kind: &'static str,
        verb: String,
        subject: String,
        mono: bool,
        result: String,
        bad: bool,
        done: bool,
        at_ms: u64,
        #[serde(skip)]
        tool: String,
        #[serde(skip)]
        past: &'static str,
    },
}

/// From one prompt (yours, or a hand-over from Claude Code) to the next.
#[derive(Clone, Serialize)]
pub struct Turn {
    at_ms: u64,
    /// The message that started it (yours; "" for Claude Code's own hand-overs), shortened.
    #[serde(skip_serializing_if = "String::is_empty")]
    prompt: String,
    items: Vec<Item>,
    /// Earlier steps of this turn left out (MAX_ITEMS).
    more: usize,
}

/// What the chat's tab shows about a session, from its log: the model, how full its context is (the
/// latest reply's tokens of the model's window), and the cost so far
/// where the agent logs it (Pi; Claude Code's comes from its status line instead).
#[derive(Clone, Default, Serialize, PartialEq)]
pub struct Meta {
    model: String,
    context: u64,
    window: u64,
    cost: Option<f64>,
}

struct Feed {
    path: String,
    meta: Meta,
    /// Whose log it is (its first line says): Claude Code's, Pi's or Codex's.
    kind: Kind,
    /// Codex: each apply_patch's patch, by call id (its result doesn't repeat it; the step's size and
    /// diff come from it).
    patches: HashMap<String, String>,
    offset: u64,
    /// Started mid-file: the first (partial) line still has to be skipped.
    align: bool,
    turns: Vec<Turn>,
    version: u64,
    used: Instant,
    /// A background agent's own log (every entry is a sidechain one, and read as this feed's).
    helper: bool,
    /// Helper: when its last answer ended (its report is written, no step open); 0 while it works.
    ended_ms: u64,
    /// The background agents this session started (Claude Code's Agent tool), each read from its own log.
    helpers: Vec<Helper>,
}

/// A background agent a Claude Code session started: it runs inside the session's process, with no
/// terminal of its own, and writes its log to `<session folder>/subagents/agent-<id>.jsonl`. Nothing
/// can be sent to it from outside; it reports to its session. Here so the chat can show it at work
/// (its steps, live) under the step that started it, and the session can be marked as waiting on it.
#[derive(Serialize)]
pub struct Helper {
    /// Claude Code's agent id.
    id: String,
    /// The Delegating step that started it (its tool_use id).
    step: String,
    /// The one-line description the session gave it.
    desc: String,
    /// Its agent type ("Explore", "general-purpose"), from its meta file; "" until that's read.
    kind: String,
    model: String,
    started_ms: u64,
    /// When the session was told it finished (its task-notification); 0 until then.
    finished_ms: u64,
    /// Its own steps, from its log (the last MAX_HELPER_ITEMS; `more` says how many came before).
    items: Vec<Item>,
    more: usize,
    #[serde(skip)]
    feed: Option<Box<Feed>>,
}

/// A helper's steps shown in the chat: the newest of them.
const MAX_HELPER_ITEMS: usize = 80;
/// Helpers remembered per session (finished ones go first, oldest first).
const MAX_HELPERS: usize = 12;

impl Helper {
    /// Running: not reported finished, and its log doesn't end on an answer.
    pub fn running(&self) -> bool {
        self.finished_ms == 0 && self.feed.as_ref().is_none_or(|f| f.ended_ms == 0)
    }

    /// Read what its log added; true when its steps changed.
    fn catch_up(&mut self) -> bool {
        let Some(f) = self.feed.as_mut() else { return false };
        let before = f.version;
        f.catch_up();
        if f.version == before {
            return false;
        }
        let all: Vec<&Item> = f.turns.iter().flat_map(|t| t.items.iter()).collect();
        let skip = all.len().saturating_sub(MAX_HELPER_ITEMS);
        self.more = skip + f.turns.iter().map(|t| t.more).sum::<usize>();
        self.items = all.into_iter().skip(skip).cloned().collect();
        if self.model.is_empty() {
            self.model = f.meta.model.clone();
        }
        true
    }
}

/// Where a session's background agent writes its log, and its meta file (`<session folder>/subagents/`).
fn helper_path(session_log: &str, agent_id: &str) -> Option<std::path::PathBuf> {
    let folder = session_log.strip_suffix(".jsonl")?;
    Some(std::path::Path::new(folder).join("subagents").join(format!("agent-{agent_id}.jsonl")))
}

static FEEDS: Mutex<Option<HashMap<String, Feed>>> = Mutex::new(None);

/// The session's turns and steps, newest last. `known`: the version the caller already has; when
/// nothing changed, only the version comes back.
pub fn steps(path: &str, known: u64) -> Value {
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    feeds.retain(|_, f| f.used.elapsed() < FORGET);
    // A feed nobody asked for in a while is dropped: a fresh one starts mid-file and counts its version
    // from 1 again. Its count can land exactly on the version the caller already has, and the "nothing
    // changed" answer would leave the caller's model and steps stale forever. A fresh feed answers in
    // full, whatever its count, so the caller redraws from what's there now.
    let fresh = !feeds.contains_key(path);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    if feed.version == known && !fresh {
        return json!({ "version": feed.version });
    }
    let turns: Vec<&Turn> = feed.turns.iter().filter(|t| !t.items.is_empty()).collect();
    let meta = (!feed.meta.model.is_empty() || feed.meta.context > 0).then_some(&feed.meta);
    // Each helper with whether it's still at work (that's read from its log, not a field).
    let helpers: Vec<Value> = feed.helpers.iter().map(|h| { let mut v = serde_json::to_value(h).unwrap_or(Value::Null); v["running"] = json!(h.running()); v }).collect();
    json!({ "version": feed.version, "turns": turns, "meta": meta, "helpers": helpers })
}

/// The background agents a Claude Code session has running right now: their descriptions.
/// (The session is at its prompt while they run; Claude Code wakes it when they report.)
pub fn running_helpers(path: &str) -> Vec<String> {
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    feed.helpers.iter().filter(|h| h.running()).map(|h| h.desc.clone()).collect()
}

/// How full the session's context is (the latest reply's tokens, as a share of the model's window), 0–100.
/// The model a session last answered with ("claude-opus-5-5"), from its transcript.
pub fn model_of(path: &str) -> Option<String> {
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    Some(feed.meta.model.clone()).filter(|m| !m.is_empty())
}

/// Each Claude session's context window as its status line last said (by session id), and each model's
/// as a session on it last said. Its transcript doesn't say, and the model's name doesn't always either
/// (a 1M window without "[1m]"). Kept on disk: a status line only runs when its session does something,
/// so after Cue restarts an idle session would otherwise read as on the 200k guess (196k → 98%).
#[derive(Default, Serialize, serde::Deserialize)]
struct Windows {
    #[serde(default)]
    sessions: HashMap<String, u64>,
    #[serde(default)]
    models: HashMap<String, u64>,
}
static WINDOWS: Mutex<Option<Windows>> = Mutex::new(None);

fn windows_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("windows.json")
}

/// The windows Cue knows (read from disk the first time; tests never touch the real one).
fn with_windows<R>(f: impl FnOnce(&mut Windows) -> R) -> R {
    let mut g = WINDOWS.lock().unwrap();
    let w = g.get_or_insert_with(|| if cfg!(test) { Windows::default() } else { std::fs::read(windows_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() });
    f(w)
}

/// A session's window: what its status line said, else what one on the same model said.
fn known_window(path: &str, model: &str) -> Option<u64> {
    let sid = std::path::Path::new(path).file_stem()?.to_str()?;
    with_windows(|w| w.sessions.get(sid).or_else(|| w.models.get(model)).copied())
}

/// What Claude Code's status line says a session's window is. A feed that has it open takes it at
/// once (and counts a new version, so the window redraws its context).
pub fn set_window(session_id: &str, window: u64) {
    if window == 0 {
        return;
    }
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    let mine = |f: &Feed| f.kind == Kind::Claude && std::path::Path::new(&f.path).file_stem().and_then(|s| s.to_str()) == Some(session_id);
    let model = feeds.values().find(|f| mine(f)).map(|f| f.meta.model.clone()).unwrap_or_default();
    with_windows(|w| {
        // Old sessions' entries go now and then; the models' stay.
        if w.sessions.len() > 2000 {
            w.sessions.clear();
        }
        let new_session = w.sessions.insert(session_id.to_string(), window) != Some(window);
        let new_model = !model.is_empty() && w.models.insert(model, window) != Some(window);
        if (new_session || new_model) && !cfg!(test) {
            let tmp = windows_file().with_extension("json.tmp");
            if serde_json::to_vec(&*w).ok().is_some_and(|b| std::fs::write(&tmp, b).is_ok()) {
                let _ = std::fs::rename(&tmp, windows_file());
            }
        }
    });
    for f in feeds.values_mut() {
        if mine(f) && f.meta.window != window {
            f.meta.window = window;
            f.version += 1;
        }
    }
}

pub fn context_pct(path: &str) -> Option<u8> {
    let mut guard = FEEDS.lock().unwrap();
    let feeds = guard.get_or_insert_with(HashMap::new);
    let feed = feeds.entry(path.to_string()).or_insert_with(|| Feed::open(path, 1));
    feed.used = Instant::now();
    feed.catch_up();
    let m = &feed.meta;
    (m.window > 0).then(|| (m.context * 100 / m.window).min(100) as u8)
}

impl Feed {
    fn open(path: &str, version: u64) -> Feed {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let offset = len.saturating_sub(START_BYTES);
        Feed { path: path.to_string(), meta: Meta::default(), kind: kind_of(path), patches: HashMap::new(), offset, align: offset > 0, turns: Vec::new(), version, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new() }
    }

    /// A background agent's log, read from its start (it's small: one task).
    fn open_helper(path: &str) -> Feed {
        Feed { path: path.to_string(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: true, ended_ms: 0, helpers: Vec::new() }
    }

    /// A background agent just started (its launch result came in): remember it and open its log.
    fn add_helper(&mut self, step: &str, extra: &Value, at_ms: u64) {
        let s = |k: &str| extra.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let id = s("agentId");
        if id.is_empty() || self.helpers.iter().any(|h| h.id == id) {
            return;
        }
        let path = helper_path(&self.path, &id);
        let kind = path
            .as_ref()
            .and_then(|p| std::fs::read(p.with_extension("meta.json")).ok())
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|m| m.get("agentType").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let feed = path.and_then(|p| p.to_str().map(|p| Box::new(Feed::open_helper(p))));
        self.helpers.push(Helper { id, step: step.to_string(), desc: s("description"), kind, model: s("resolvedModel"), started_ms: at_ms, finished_ms: 0, items: Vec::new(), more: 0, feed });
        // The oldest finished ones go first; what's running is never dropped.
        while self.helpers.len() > MAX_HELPERS {
            match self.helpers.iter().position(|h| !h.running()) {
                Some(i) => { self.helpers.remove(i); }
                None => break,
            }
        }
    }

    /// Claude Code told the session a background agent finished (`<task-notification>`): mark it.
    fn helper_finished(&mut self, notification: &str, at_ms: u64) {
        let tag = |t: &str| notification.split(&format!("<{t}>")).nth(1).and_then(|r| r.split(&format!("</{t}>")).next()).map(str::trim).unwrap_or("").to_string();
        let (id, status) = (tag("task-id"), tag("status"));
        if let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) {
            if status != "running" {
                h.finished_ms = at_ms;
            }
        }
    }

    /// Read what was added since last time (whole lines only; a half-written line waits for the next call).
    fn catch_up(&mut self) {
        let mut changed = self.read_new();
        // Its background agents' logs too: their steps are part of what this session is doing.
        for h in &mut self.helpers {
            changed |= h.catch_up();
        }
        if changed {
            if self.turns.len() > MAX_TURNS {
                self.turns.drain(..self.turns.len() - MAX_TURNS);
            }
            self.version += 1;
        }
    }

    /// Read this log's new lines; true when anything shown changed.
    fn read_new(&mut self) -> bool {
        let Ok(mut file) = std::fs::File::open(&self.path) else { return false };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut changed = false;
        if len < self.offset {
            // The file was replaced: start over (and count a version, so callers redraw).
            *self = Feed::open(&self.path, self.version);
            changed = true;
        }
        if len <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return changed;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
                // A Stop hook sent it back to work: more of the same turn (the chat folds it under
                // "After a stop hook"), not a turn of its own.
                if text.starts_with(crate::transcript::STOP_HOOK_MARK) {
                    return false;
                }
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

    /// The model, context and cost from one raw log entry (each agent logs them its own way).
    /// Returns whether they changed.
    fn observe(&mut self, e: &Value) -> bool {
        let before = self.meta.clone();
        let n = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0);
        let m = &mut self.meta;
        match self.kind {
            Kind::Claude => {
                if e.get("type").and_then(Value::as_str) == Some("assistant") && (self.helper || e.get("isSidechain").and_then(Value::as_bool) != Some(true)) {
                    let msg = e.get("message").cloned().unwrap_or(Value::Null);
                    if let Some(u) = msg.get("usage") {
                        let (fresh, read, wrote) = (n(u.get("input_tokens")), n(u.get("cache_read_input_tokens")), n(u.get("cache_creation_input_tokens")));
                        let all = fresh + read + wrote;
                        if all > 0 {
                            m.context = all;
                        }
                        let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
                        if !model.is_empty() && !model.starts_with('<') {
                            m.model = model.to_string();
                        }
                        // The transcript doesn't say the window. Claude Code's status line does (for this
                        // session, or another on its model); until then 200k, or 1M once it's past that (or says so).
                        m.window = match known_window(&self.path, &m.model) {
                            Some(w) => w,
                            None if m.model.contains("[1m]") || m.context > 200_000 || m.window == 1_000_000 => 1_000_000,
                            None => 200_000,
                        };
                    }
                }
            }
            Kind::Codex => {
                let p = e.get("payload").cloned().unwrap_or(Value::Null);
                if e.get("type").and_then(Value::as_str) == Some("turn_context") {
                    if let Some(model) = p.get("model").and_then(Value::as_str) {
                        m.model = model.to_string();
                    }
                }
                if p.get("type").and_then(Value::as_str) == Some("token_count") {
                    if let Some(info) = p.get("info").filter(|i| i.is_object()) {
                        let last = info.get("last_token_usage");
                        let input = n(last.and_then(|l| l.get("input_tokens")));
                        if input > 0 {
                            m.context = input;
                        }
                        let w = n(info.get("model_context_window"));
                        if w > 0 {
                            m.window = w;
                        }
                    }
                }
            }
            Kind::Pi => {
                let msg = e.get("message").cloned().unwrap_or(Value::Null);
                if e.get("type").and_then(Value::as_str) == Some("model_change") {
                    if let Some(model) = e.get("modelId").and_then(Value::as_str) {
                        m.model = model.to_string();
                        m.window = pi_window(e.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
                    }
                }
                if msg.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(u) = msg.get("usage") {
                        let (fresh, read, wrote) = (n(u.get("input")), n(u.get("cacheRead")), n(u.get("cacheWrite")));
                        let all = fresh + read + wrote;
                        if all > 0 {
                            m.context = all;
                        }
                        if let Some(c) = u.pointer("/cost/total").and_then(Value::as_f64) {
                            m.cost = Some(m.cost.unwrap_or(0.0) + c);
                        }
                    }
                    if let Some(model) = msg.get("model").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                        if m.model != model || m.window == 0 {
                            m.window = pi_window(msg.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
                        }
                        m.model = model.to_string();
                    }
                }
            }
        }
        self.meta != before
    }

    /// One transcript entry. Returns whether anything shown changed.
    fn take(&mut self, e: &Value) -> bool {
        // A helper agent's own steps (older transcripts keep them inline) aren't this session's.
        if !self.helper && e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let at = stamp(e);
        if self.helper {
            // Its answer ended: its report is written. A new message to it (its session can send one) puts it back to work.
            match e.get("type").and_then(Value::as_str) {
                Some("assistant") if e.pointer("/message/stop_reason").and_then(Value::as_str) == Some("end_turn") => self.ended_ms = at.max(1),
                Some("user") if !content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) => self.ended_ms = 0,
                _ => {}
            }
        }
        match e.get("type").and_then(Value::as_str) {
            Some("user") => {
                let blocks = content_blocks(e);
                let results: Vec<&Value> = blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
                if !results.is_empty() {
                    let extra = e.get("toolUseResult");
                    return results.into_iter().fold(false, |any, b| self.finish(b, extra, at) || any);
                }
                if e.get("isMeta").and_then(Value::as_bool) == Some(true) || e.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
                    return false;
                }
                let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
                let text = text.trim_start();
                if text.starts_with("[Request interrupted") {
                    return self.close_open("stopped");
                }
                // A local command (/rename, /model) and its output: no turn of work.
                if text.starts_with("<command-") || text.starts_with("<local-command") {
                    return false;
                }
                self.close_open("");
                if crate::transcript::harness_text(text) {
                    self.helper_finished(text, at);
                }
                let prompt = if crate::transcript::harness_text(text) { String::new() } else { cut(text.trim(), 1200) };
                self.turns.push(Turn { at_ms: at, prompt, items: Vec::new(), more: 0 });
                false // an empty turn isn't shown until something happens in it
            }
            Some("assistant") => {
                let mut changed = false;
                for b in content_blocks(e) {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t = b.get("text").and_then(Value::as_str).unwrap_or("").trim();
                            if !t.is_empty() {
                                self.push(Item::Say { text: t.to_string(), at_ms: at });
                                changed = true;
                            }
                        }
                        Some("tool_use") => {
                            self.push(step_of(&b, at));
                            changed = true;
                        }
                        _ => {}
                    }
                }
                changed
            }
            _ => false,
        }
    }

    fn push(&mut self, item: Item) {
        if self.turns.is_empty() {
            // Started mid-file, inside a turn whose prompt is further back.
            let at = match &item { Item::Say { at_ms, .. } | Item::Step { at_ms, .. } => *at_ms };
            self.turns.push(Turn { at_ms: at, prompt: String::new(), items: Vec::new(), more: 0 });
        }
        let turn = self.turns.last_mut().unwrap();
        turn.items.push(item);
        if turn.items.len() > MAX_ITEMS {
            turn.items.remove(0);
            turn.more += 1;
        }
    }

    /// A tool call's result came in: fill in how it went.
    fn finish(&mut self, block: &Value, extra: Option<&Value>, at: u64) -> bool {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { return false };
        for turn in self.turns.iter_mut().rev().take(3) {
            for item in turn.items.iter_mut().rev() {
                if let Item::Step { id: sid, tool, verb, past, result, bad, done, .. } = item {
                    if sid == id {
                        let (r, b) = outcome(tool, block, extra);
                        *verb = past.to_string();
                        *result = r;
                        *bad = b;
                        *done = true;
                        let launched = extra.filter(|x| x.get("status").and_then(Value::as_str) == Some("async_launched")).cloned();
                        let step = sid.clone();
                        if let Some(x) = launched {
                            self.add_helper(&step, &x, at);
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Steps still open in the current turn when it ends (stopped, or the next prompt came): done.
    fn close_open(&mut self, why: &str) -> bool {
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Claude,
    Pi,
    Codex,
}

/// Whose log this is, by its first line: Pi's starts `{"type":"session","version":…}`, Codex's
/// `{"type":"session_meta",…}`; anything else is Claude Code's.
fn kind_of(path: &str) -> Kind {
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

/// One log entry as Claude Code transcript entries (none, one, or a few).
fn translate(kind: Kind, e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
    match kind {
        Kind::Claude => vec![e.clone()],
        Kind::Pi => from_pi(e),
        Kind::Codex => from_codex(e, patches),
    }
}

/// One Codex rollout entry in the shape of Claude Code's transcript: your message (its `user_message`
/// event; the instructions Codex puts in as "user" messages aren't yours), its words, commands
/// (`exec_command`, `shell`) and patches (`apply_patch`), and their results. `write_stdin` (checking on
/// a command still running) is left out.
fn from_codex(e: &Value, patches: &mut HashMap<String, String>) -> Vec<Value> {
    let ts = e.get("timestamp").cloned().unwrap_or(Value::Null);
    let p = e.get("payload").cloned().unwrap_or(Value::Null);
    let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
    match (e.get("type").and_then(Value::as_str), ptype) {
        (Some("event_msg"), "user_message") => {
            let text = p.get("message").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if text.is_empty() || text.starts_with('<') {
                return vec![];
            }
            vec![json!({ "type": "user", "timestamp": ts, "message": { "content": text } })]
        }
        (Some("response_item"), "message") if p.get("role").and_then(Value::as_str) == Some("assistant") => {
            let text: String = p.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
            if text.trim().is_empty() {
                return vec![];
            }
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": [{ "type": "text", "text": text }] } })]
        }
        (Some("response_item"), "function_call" | "custom_tool_call") => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or("");
            let id = p.get("call_id").cloned().unwrap_or(Value::Null);
            let args: Value = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
            let raw = p.get("input").and_then(Value::as_str).unwrap_or("");
            let (tool, input) = match name {
                "write_stdin" => return vec![],
                "exec_command" => ("Bash".to_string(), json!({ "command": args.get("cmd").cloned().unwrap_or(Value::Null) })),
                "shell" | "local_shell" | "container.exec" => {
                    let argv: Vec<&str> = args.get("command").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                    let cmd = if argv.len() >= 3 && argv[1] == "-lc" { argv[2].to_string() } else { argv.join(" ") };
                    ("Bash".to_string(), json!({ "command": cmd }))
                }
                "apply_patch" => {
                    if let Some(i) = id.as_str() {
                        patches.insert(i.to_string(), raw.to_string());
                    }
                    let file = raw.lines().find_map(|l| ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|m| l.strip_prefix(m))).unwrap_or("").trim().to_string();
                    ("Edit".to_string(), json!({ "file_path": file }))
                }
                // The Codex app's own tool: a script it runs (JavaScript). Often it only runs a shell
                // command (`tools.exec_command({"cmd": "…"})`): then it reads as that command.
                "exec" => match script_command(raw) {
                    Some(cmd) => ("Bash".to_string(), json!({ "command": cmd })),
                    None => ("CodexScript".to_string(), json!({ "script": raw })),
                },
                n => (n.to_string(), if args.is_null() { json!({ "input": raw }) } else { args }),
            };
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": [{ "type": "tool_use", "id": id, "name": tool, "input": input }] } })]
        }
        (Some("response_item"), "function_call_output" | "custom_tool_call_output") => {
            let id = p.get("call_id").and_then(Value::as_str).unwrap_or("").to_string();
            let text = match p.get("output") {
                Some(Value::String(t)) => t.clone(),
                Some(Value::Array(a)) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
                Some(Value::Object(o)) => o.get("output").and_then(Value::as_str).unwrap_or("").to_string(),
                _ => String::new(),
            };
            // "Process exited with code 1" / "Exit code: 1" up top, then "Output:" and what it printed.
            let code = ["exited with code ", "Exit code: "].iter().find_map(|m| text.split(m).nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>())).and_then(|c| c.parse::<i64>().ok());
            let body = text.split_once("Output:\n").map(|(_, b)| b.to_string()).unwrap_or_else(|| text.clone());
            let failed = code.is_some_and(|c| c != 0);
            let extra = match patches.get(&id) {
                Some(patch) => json!({ "structuredPatch": codex_patch(patch) }),
                None => json!({ "stdout": body }),
            };
            let shown = if failed { format!("Exit code {}\n{body}", code.unwrap_or(1)) } else { body };
            vec![json!({
                "type": "user", "timestamp": ts, "toolUseResult": extra,
                "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": shown, "is_error": failed }] },
            })]
        }
        _ => vec![],
    }
}

/// The shell command a Codex app script runs, when that's all it is: `exec_command({"cmd": "…"})`.
fn script_command(script: &str) -> Option<String> {
    if script.matches("exec_command(").count() != 1 {
        return None;
    }
    let after = &script[script.find("\"cmd\"")? + 5..];
    let quote = after.find('"')?;
    let mut strings = serde_json::Deserializer::from_str(&after[quote..]).into_iter::<String>();
    strings.next()?.ok().filter(|c| !c.trim().is_empty())
}

/// An apply_patch patch as Claude Code's structured patch: one hunk per "@@" part, its +/−/context lines.
fn codex_patch(patch: &str) -> Value {
    let mut hunks: Vec<Vec<String>> = vec![];
    for l in patch.lines() {
        if l.starts_with("*** ") {
            continue;
        }
        if l.starts_with("@@") {
            hunks.push(vec![]);
            continue;
        }
        if hunks.is_empty() {
            hunks.push(vec![]);
        }
        let line = if l.starts_with('+') || l.starts_with('-') || l.starts_with(' ') { l.to_string() } else { format!(" {l}") };
        hunks.last_mut().unwrap().push(line);
    }
    Value::Array(hunks.into_iter().filter(|h| !h.is_empty()).map(|h| json!({ "lines": h })).collect())
}

/// One Pi log entry in the shape of Claude Code's transcript (so the steps read both alike): your
/// message, its words and tool calls, a tool's result. Pi's tool names map to Claude Code's.
fn from_pi(e: &Value) -> Vec<Value> {
    if e.get("type").and_then(Value::as_str) != Some("message") {
        return vec![];
    }
    let ts = e.get("timestamp").cloned().unwrap_or(Value::Null);
    let m = e.get("message").cloned().unwrap_or(Value::Null);
    let blocks = m.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
    let text_of = |bs: &[Value]| bs.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
    match m.get("role").and_then(Value::as_str) {
        Some("user") => {
            let text = match m.get("content") {
                Some(Value::String(t)) => t.clone(),
                _ => text_of(&blocks),
            };
            // What Pi itself puts in as a "user" message (a loaded skill, a reminder) starts with a tag.
            if text.trim_start().starts_with('<') || text.trim().is_empty() {
                return vec![];
            }
            vec![json!({ "type": "user", "timestamp": ts, "message": { "content": text } })]
        }
        Some("assistant") => {
            let content: Vec<Value> = blocks
                .iter()
                .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                    Some("text") => Some(json!({ "type": "text", "text": b.get("text").cloned().unwrap_or(Value::Null) })),
                    Some("toolCall") => {
                        let args = b.get("arguments").cloned().unwrap_or(Value::Null);
                        let (name, input) = pi_tool(b.get("name").and_then(Value::as_str).unwrap_or(""), &args);
                        Some(json!({ "type": "tool_use", "id": b.get("id").cloned().unwrap_or(Value::Null), "name": name, "input": input }))
                    }
                    _ => None,
                })
                .collect();
            if content.is_empty() {
                return vec![];
            }
            vec![json!({ "type": "assistant", "timestamp": ts, "message": { "content": content } })]
        }
        Some("toolResult") => {
            let text = text_of(&blocks);
            let tool = m.get("toolName").and_then(Value::as_str).unwrap_or("");
            let details = m.get("details").cloned().unwrap_or(Value::Null);
            let extra = match tool {
                "bash" => json!({ "stdout": text }),
                "read" => json!({ "type": "text", "file": { "numLines": text.lines().count() } }),
                "edit" => json!({ "structuredPatch": pi_patch(details.get("diff").and_then(Value::as_str).unwrap_or("")) }),
                _ => Value::Null,
            };
            vec![json!({
                "type": "user", "timestamp": ts, "toolUseResult": extra,
                "message": { "content": [{ "type": "tool_result", "tool_use_id": m.get("toolCallId").cloned().unwrap_or(Value::Null), "content": text, "is_error": m.get("isError").and_then(Value::as_bool).unwrap_or(false) }] },
            })]
        }
        _ => vec![],
    }
}

/// A model's context window from Pi's own model list (`<Pi's folder>/models-store.json`), read once.
fn pi_window(provider: &str, model: &str) -> Option<u64> {
    static STORE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let store = STORE.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let dir = std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent"));
        std::fs::read_to_string(dir.join("models-store.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
    });
    let find = |models: &Value| models.as_array()?.iter().find(|x| x.get("id").and_then(Value::as_str) == Some(model))?.get("contextWindow")?.as_u64();
    store.get(provider).and_then(|p| find(p.get("models")?)).or_else(|| store.as_object()?.values().find_map(|p| find(p.get("models")?)))
}

/// A Pi tool as the Claude Code tool it matches (its arguments renamed to fit).
fn pi_tool(name: &str, args: &Value) -> (String, Value) {
    let a = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    match name {
        "bash" => ("Bash".into(), json!({ "command": a("command") })),
        "read" => ("Read".into(), json!({ "file_path": a("path") })),
        "edit" => ("Edit".into(), json!({ "file_path": a("path") })),
        "write" => ("Write".into(), json!({ "file_path": a("path") })),
        "grep" => ("Grep".into(), json!({ "pattern": a("pattern"), "path": a("path") })),
        "find" => ("Glob".into(), json!({ "pattern": a("pattern") })),
        "ask_user" => ("AskUserQuestion".into(), json!({ "questions": [{ "question": a("question") }] })),
        n => (n.to_string(), args.clone()),
    }
}

/// Pi's edit diff ("-60  old", "+61  new", " 56  same", "    ..." between parts) as Claude Code's
/// structured patch: hunks of lines starting with "-", "+" or " ", line numbers dropped.
fn pi_patch(diff: &str) -> Value {
    let mut hunks: Vec<Vec<String>> = vec![vec![]];
    for l in diff.lines() {
        if l.trim() == "..." {
            if !hunks.last().unwrap().is_empty() {
                hunks.push(vec![]);
            }
            continue;
        }
        let (op, rest) = l.split_at(l.chars().next().map(char::len_utf8).unwrap_or(0));
        let op = if op == "+" || op == "-" { op } else { " " };
        // The line number, then one space, then the line as it is (its own indentation kept).
        let body = rest.trim_start().trim_start_matches(|c: char| c.is_ascii_digit());
        hunks.last_mut().unwrap().push(format!("{op}{}", body.strip_prefix(' ').unwrap_or(body)));
    }
    Value::Array(hunks.into_iter().filter(|h| !h.is_empty()).map(|h| json!({ "lines": h })).collect())
}

/// A tool call as a step line: its kind, the verb while running and once done, and its subject.
fn step_of(b: &Value, at: u64) -> Item {
    let tool = b.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let input = b.get("input").cloned().unwrap_or(Value::Null);
    let (kind, verb, past, subject, mono) = describe(&tool, &input);
    Item::Step {
        id: b.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        kind,
        verb: verb.to_string(),
        subject,
        mono,
        result: String::new(),
        bad: false,
        done: false,
        at_ms: at,
        tool,
        past,
    }
}

fn describe(tool: &str, input: &Value) -> (&'static str, &'static str, &'static str, String, bool) {
    let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let file = |k: &str| arg(k).rsplit('/').next().unwrap_or("").to_string();
    let first = |t: String| t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
    match tool {
        "Bash" => ("run", "Running", "Ran", cut(&without_cd(&first(arg("command"))), 80), true),
        "Read" => ("read", "Reading", "Read", file("file_path"), true),
        "Edit" | "MultiEdit" => ("edit", "Editing", "Edited", file("file_path"), true),
        "Write" => ("edit", "Writing", "Wrote", file("file_path"), true),
        "NotebookEdit" => ("edit", "Editing", "Edited", file("notebook_path"), true),
        "Grep" => ("search", "Searching", "Searched", format!("“{}”", cut(&arg("pattern"), 60)), false),
        "Glob" => ("search", "Finding files", "Found files", cut(&arg("pattern"), 60), true),
        "WebSearch" => ("web", "Searching the web", "Searched the web", cut(&arg("query"), 70), false),
        "WebFetch" => ("web", "Reading", "Read", cut(arg("url").trim_start_matches("https://"), 70), true),
        "Agent" | "Task" => ("agent", "Delegating", "Delegated", cut(&arg("description"), 70), false),
        "CodexScript" => ("run", "Running a script", "Ran a script", cut(&first(arg("script")), 70), true),
        "TodoWrite" => ("todo", "Updating its to-do list", "Updated its to-do list", String::new(), false),
        "AskUserQuestion" => {
            let q = input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("");
            ("ask", "Asking you", "Asked you", cut(q, 70), false)
        }
        "Skill" => ("other", "Using", "Used", arg("skill"), false),
        t if t.starts_with("mcp__") => ("other", "Using", "Used", t.trim_start_matches("mcp__").replacen("__", ": ", 1), false),
        t => ("other", "Using", "Used", t.to_string(), false),
    }
}

/// A command without its leading `cd <folder> &&` (or `;`): the folder isn't what it ran.
fn without_cd(cmd: &str) -> String {
    let mut rest = cmd.trim();
    while let Some(after) = rest.strip_prefix("cd ") {
        let Some(i) = after.find("&&").map(|i| (i, 2)).into_iter().chain(after.find(';').map(|i| (i, 1))).min_by_key(|(i, _)| *i) else { break };
        let next = after[i.0 + i.1..].trim_start();
        if next.is_empty() {
            break;
        }
        rest = next;
    }
    rest.to_string()
}

/// A tool result's text (a string, or text blocks).
fn result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// "+6 −2" from an edit's structured patch.
fn patch_counts(patch: &Value) -> (usize, usize) {
    let mut add = 0;
    let mut del = 0;
    for hunk in patch.as_array().into_iter().flatten() {
        for l in hunk.get("lines").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if l.starts_with('+') {
                add += 1;
            } else if l.starts_with('-') {
                del += 1;
            }
        }
    }
    (add, del)
}

/// How a step went, in a few words, and whether it failed.
fn outcome(tool: &str, block: &Value, extra: Option<&Value>) -> (String, bool) {
    let text = result_text(block);
    let x = |k: &str| extra.and_then(|v| v.get(k));
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        let low = text.to_lowercase();
        if low.contains("denied") || low.contains("doesn't want to proceed") || low.contains("rejected") {
            return ("declined".into(), true);
        }
        let code_after = |mark: &str| text.split(mark).nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>()).filter(|c| !c.is_empty());
        if let Some(code) = code_after("Exit code ").or_else(|| code_after("exited with code ")) {
            return (format!("exit {code}"), true);
        }
        return ("failed".into(), true);
    }
    let r = match tool {
        "Read" => match x("type").and_then(Value::as_str) {
            Some("image") => "image".into(),
            _ => match x("file").and_then(|f| f.get("numLines")).and_then(Value::as_u64) {
                Some(n) => format!("{n} lines"),
                None => format!("{} lines", text.lines().count()),
            },
        },
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            if x("type").and_then(Value::as_str) == Some("create") {
                format!("new · {} lines", x("content").and_then(Value::as_str).unwrap_or("").lines().count())
            } else {
                match x("structuredPatch").map(patch_counts) {
                    Some((0, 0)) | None => "saved".into(),
                    Some((a, d)) => format!("+{a} −{d}"),
                }
            }
        }
        "Bash" => {
            if x("interrupted").and_then(Value::as_bool) == Some(true) {
                "stopped".into()
            } else {
                let out = x("stdout").and_then(Value::as_str).map(String::from).unwrap_or(text);
                // Its last line when that reads as a result ("Tests: 41 passed", "BUILD SUCCEEDED"); a
                // long line (a match, a log line) says less than "done".
                let last = strip_ansi(out.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or(""));
                if last.is_empty() || last.chars().count() > 32 { "done".into() } else { last }
            }
        }
        "Grep" | "Glob" => {
            let first = text.lines().next().unwrap_or("").trim();
            if first.starts_with("No ") {
                "none".into()
            } else if let Some(rest) = first.strip_prefix("Found ") {
                cut(rest.split(':').next().unwrap_or(rest).trim(), 30)
            } else {
                format!("{} results", text.lines().filter(|l| !l.trim().is_empty()).count())
            }
        }
        "Agent" | "Task" if x("status").and_then(Value::as_str) == Some("async_launched") => "in the background".into(),
        _ => "done".into(),
    };
    (r, false)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lines for an opened step: long outputs keep their start and end, with how many lines were skipped.
fn clip_lines(text: &str) -> (Vec<String>, usize) {
    let all: Vec<String> = strip_ansi(text).lines().map(|l| cut(l, LINE_CHARS)).collect();
    if all.len() <= OUT_HEAD + OUT_TAIL {
        return (all, 0);
    }
    let skipped = all.len() - OUT_HEAD - OUT_TAIL;
    let mut out = all[..OUT_HEAD].to_vec();
    out.extend_from_slice(&all[all.len() - OUT_TAIL..]);
    (out, skipped)
}

/// Opened steps: for each id, its full subject (the whole command, path, pattern), and its output
    #[test]
    fn a_stop_hooks_follow_up_stays_in_the_turn_it_continues() {
        let log = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Done: expiry fixed."}]}}
{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":"Stop hook feedback:\n[bash rules-check.sh]: Check the rules"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat rules.md"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"text","text":"1. SHAPE: fine."}]}}
{"type":"user","timestamp":"2026-10-04T10:00:30.000Z","message":{"content":"and the refresh token?"}}
"#;
        let f = feed_of(log);
        assert_eq!(f.turns.iter().map(|t| t.prompt.as_str()).collect::<Vec<_>>(), ["fix the token expiry", "and the refresh token?"], "the hook's entry opens no turn");
        let items = &f.turns[0].items;
        assert_eq!(items.len(), 3, "answer, the hook's step, its checklist: all one turn");
        assert_eq!(step(&items[1]), ("Ran", "cat rules.md", "ok", false, true));
    }

/// lines or diff. Read from the transcript on demand, never kept.
pub fn detail(path: &str, ids: &[String]) -> Value {
    let kind = kind_of(path);
    let mut patches = HashMap::new();
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(START_BYTES);
    let Ok(mut file) = std::fs::File::open(path) else { return json!({}) };
    let mut buf = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut buf).is_err() {
        return json!({});
    }
    let text = String::from_utf8_lossy(&buf);
    let mut calls: HashMap<&str, (String, Value)> = HashMap::new();
    let mut results: HashMap<&str, (Value, Value)> = HashMap::new();
    for line in text.lines() {
        let Some(id) = ids.iter().find(|id| line.contains(id.as_str())) else { continue };
        let Ok(raw) = serde_json::from_str::<Value>(line) else { continue };
        for e in translate(kind, &raw, &mut patches) {
        for b in content_blocks(&e) {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") if b.get("id").and_then(Value::as_str) == Some(id) => {
                    calls.insert(id.as_str(), (b.get("name").and_then(Value::as_str).unwrap_or("").to_string(), b.get("input").cloned().unwrap_or(Value::Null)));
                }
                Some("tool_result") if b.get("tool_use_id").and_then(Value::as_str) == Some(id) => {
                    results.insert(id.as_str(), (b.clone(), e.get("toolUseResult").cloned().unwrap_or(Value::Null)));
                }
                _ => {}
            }
        }
        }
    }
    let mut out = serde_json::Map::new();
    for id in ids {
        let Some((tool, input)) = calls.get(id.as_str()) else { continue };
        let (block, extra) = results.get(id.as_str()).cloned().unwrap_or((Value::Null, Value::Null));
        out.insert(id.clone(), one_detail(tool, input, &block, &extra));
    }
    Value::Object(out)
}

fn one_detail(tool: &str, input: &Value, block: &Value, extra: &Value) -> Value {
    let arg = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
    let full = match tool {
        "Bash" => cut(&arg("command"), 4000),
        "CodexScript" => cut(&arg("script"), 4000),
        "Read" | "Edit" | "MultiEdit" | "Write" => arg("file_path"),
        "NotebookEdit" => arg("notebook_path"),
        "Grep" => [arg("pattern"), arg("path")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  in  "),
        "Glob" => arg("pattern"),
        "WebFetch" => arg("url"),
        "WebSearch" => arg("query"),
        "Agent" | "Task" => cut(&arg("prompt"), 1200),
        _ => cut(&input.to_string(), 1200),
    };
    // An edit (not a failed one): its diff, hunk by hunk.
    let edit = matches!(tool, "Edit" | "MultiEdit" | "Write" | "NotebookEdit") && !failed;
    if edit {
        let mut diff: Vec<[String; 2]> = Vec::new();
        let mut cut_lines = 0;
        if extra.get("type").and_then(Value::as_str) == Some("create") {
            let content = extra.get("content").and_then(Value::as_str).unwrap_or("");
            cut_lines = content.lines().count().saturating_sub(OUT_TAIL);
            for l in content.lines().take(OUT_TAIL) {
                diff.push(["+".into(), cut(l, LINE_CHARS)]);
            }
        } else {
            for (n, hunk) in extra.get("structuredPatch").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if n > 0 {
                    diff.push(["…".into(), String::new()]);
                }
                for l in hunk.get("lines").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    let (op, rest) = l.split_at(l.chars().next().map(char::len_utf8).unwrap_or(0));
                    diff.push([if op == "+" || op == "-" { op.to_string() } else { " ".into() }, cut(rest, LINE_CHARS)]);
                }
            }
        }
        cut_lines += diff.len().saturating_sub(OUT_HEAD + OUT_TAIL);
        diff.truncate(OUT_HEAD + OUT_TAIL);
        // A long diff is cut at the end: `skipped` lines more.
        return json!({ "full": full, "diff": diff, "skipped": cut_lines });
    }
    // A read shows no file contents: the line already says how much it read.
    if tool == "Read" && !failed {
        return json!({ "full": full });
    }
    let output = if tool == "Bash" && !failed {
        let mut o = extra.get("stdout").and_then(Value::as_str).unwrap_or("").to_string();
        let err = extra.get("stderr").and_then(Value::as_str).unwrap_or("").trim();
        // The shell's own note about its folder isn't the command's output.
        let err: String = err.lines().filter(|l| !l.starts_with("Shell cwd was reset")).collect::<Vec<_>>().join("\n");
        if !err.trim().is_empty() {
            if !o.is_empty() {
                o.push('\n');
            }
            o.push_str(&err);
        }
        if o.trim().is_empty() { result_text(block) } else { o }
    } else {
        result_text(block)
    };
    let (out, skipped) = clip_lines(&output);
    // `gap_at`: where the skipped lines were (after the first OUT_HEAD).
    json!({ "full": full, "output": out, "skipped": skipped, "gap_at": OUT_HEAD })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_of(jsonl: &str) -> Feed {
        let mut f = Feed { path: String::new(), meta: Meta::default(), kind: Kind::Claude, patches: HashMap::new(), offset: 0, align: false, turns: Vec::new(), version: 1, used: Instant::now(), helper: false, ended_ms: 0, helpers: Vec::new() };
        for e in lines(jsonl) {
            f.take(&e);
        }
        f
    }
    fn step(item: &Item) -> (&str, &str, &str, bool, bool) {
        match item {
            Item::Step { verb, subject, result, bad, done, .. } => (verb, subject, result, *bad, *done),
            Item::Say { .. } => panic!("not a step"),
        }
    }

    const T: &str = r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"fix the token expiry"}}
{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"text","text":"Looking at it."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"..."}]},"toolUseResult":{"type":"text","file":{"filePath":"/a/src/token.ts","numLines":84}}}
{"type":"assistant","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/a/src/token.ts"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:06.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"structuredPatch":[{"lines":[" a","-b","+c","+d"]}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"npm test\n"}}]}}
{"type":"user","timestamp":"2026-10-04T10:00:20.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t3","is_error":true,"content":"Exit code 1\nTests: 3 failed"}]},"toolUseResult":"Error: Exit code 1"}
{"type":"assistant","timestamp":"2026-10-04T10:00:22.000Z","isSidechain":true,"message":{"content":[{"type":"text","text":"a helper's words"}]}}
{"type":"assistant","timestamp":"2026-10-04T10:00:25.000Z","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"npm test"}}]}}"#;

    #[test]
    fn steps_read_as_lines_with_how_they_went() {
        let f = feed_of(T);
        assert_eq!(f.turns.len(), 1);
        let items = &f.turns[0].items;
        assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking at it."));
        assert_eq!(step(&items[1]), ("Read", "token.ts", "84 lines", false, true));
        assert_eq!(step(&items[2]), ("Edited", "token.ts", "+2 −1", false, true));
        assert_eq!(step(&items[3]), ("Ran", "npm test", "exit 1", true, true));
        // Still running: the present-tense verb, no result. The helper's words aren't here.
        assert_eq!(step(&items[4]), ("Running", "npm test", "", false, false));
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn a_background_agent_shows_at_work_under_its_step_until_its_session_hears_it_finished() {
        let dir = std::env::temp_dir().join(format!("cue-steps-helper-{}", std::process::id()));
        let sub = dir.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","description":"Backend: names","toolUseId":"t1","requestShape":"background"}"#).unwrap();
        std::fs::write(sub.join("agent-a1.jsonl"), [
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"user","content":"find every name"}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:04.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"h1","name":"Grep","input":{"pattern":"ai-title"}}]}}"#,
            r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h1","content":"Found 3 files"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:06.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"h2","name":"Read","input":{"file_path":"/a/hub.rs"}}]}}"#,
        ].join("\n") + "\n").unwrap();
        let path = dir.join("s1.jsonl");
        std::fs::write(&path, [
            r#"{"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"map the names"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:02.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Backend: names","subagent_type":"Explore","prompt":"find every name"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-04T10:00:03.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1","description":"Backend: names","resolvedModel":"claude-opus-5-5"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-04T10:00:04.000Z","message":{"content":[{"type":"text","text":"An agent is on it."}]}}"#,
        ].join("\n") + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.helpers.len(), 1);
        let h = &f.helpers[0];
        assert_eq!((h.id.as_str(), h.step.as_str(), h.desc.as_str(), h.kind.as_str(), h.model.as_str()), ("a1", "t1", "Backend: names", "Explore", "claude-opus-5-5"));
        assert!(h.running());
        assert_eq!(h.items.len(), 2, "its own steps, from its own log");
        assert_eq!(step(&h.items[0]), ("Searched", "“ai-title”", "3 files", false, true));
        assert_eq!(step(&h.items[1]).0, "Reading");
        assert_eq!(step(&f.turns[0].items[0]), ("Delegated", "Backend: names", "in the background", false, true));
        assert_eq!(running_helpers(p), vec!["Backend: names".to_string()]);
        // It writes its report: it's done, even before its session hears so.
        let v = f.version;
        let mut log = std::fs::OpenOptions::new().append(true).open(sub.join("agent-a1.jsonl")).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:07.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"h2","content":"ok"}]}}"#, "\n",
            r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-04T10:00:08.000Z","message":{"model":"claude-opus-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"Names: project, name."}]}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert!(f.version > v, "its new steps count as a change");
        assert!(!f.helpers[0].running());
        assert_eq!(f.helpers[0].finished_ms, 0);
        // Its session is told: finished for good.
        let mut log = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut log, concat!(r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<summary>Agent \"Backend: names\" finished</summary>\n</task-notification>"}}"#, "\n").as_bytes()).unwrap();
        f.catch_up();
        assert_eq!(f.helpers[0].finished_ms, crate::transcript::iso_ms("2026-10-04T10:01:00.000Z").unwrap());
        assert!(running_helpers(p).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pi_log_reads_as_steps_too() {
        let log = [
            r#"{"type":"session","version":3,"id":"P1","timestamp":"2026-10-04T10:00:00.000Z","cwd":"/a"}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":"<skill name="x">loaded</skill>"}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:02.000Z","message":{"role":"user","content":[{"type":"text","text":"fix the filter"}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:03.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":"Looking."},{"type":"toolCall","id":"c1","name":"edit","arguments":{"path":"/a/src/filters.ts"}}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:04.000Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"edit","content":[{"type":"text","text":"ok"}],"isError":false,"details":{"diff":" 56   return [];\n-60 old line\n+60 new line\n+61   added\n    ...\n 90 tail"}}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:05.000Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c2","name":"bash","arguments":{"command":"npm test"}}]}}"#,
            r#"{"type":"message","timestamp":"2026-10-04T10:00:09.000Z","message":{"role":"toolResult","toolCallId":"c2","toolName":"bash","content":[{"type":"text","text":"FAIL\n\nCommand exited with code 1"}],"isError":true}}"#,
        ]
        .join("\n");
        let dir = std::env::temp_dir().join(format!("cue-steps-pi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.jsonl");
        std::fs::write(&path, log + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        assert_eq!(f.kind, Kind::Pi);
        f.catch_up();
        assert_eq!(f.turns.len(), 1, "the skill Pi loaded isn't a turn");
        assert_eq!(f.turns[0].prompt, "fix the filter");
        let items = &f.turns[0].items;
        assert!(matches!(&items[0], Item::Say { text, .. } if text == "Looking."));
        assert_eq!(step(&items[1]), ("Edited", "filters.ts", "+2 −1", false, true));
        assert_eq!(step(&items[2]), ("Ran", "npm test", "exit 1", true, true));
        let d = detail(p, &["c1".into()]);
        assert_eq!(d["c1"]["diff"][0], json!([" ", "  return [];"]));
        assert_eq!(d["c1"]["diff"][3], json!(["+", "  added"]));
        assert_eq!(d["c1"]["diff"][4], json!(["…", ""]));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_command_reads_without_its_cd() {
        assert_eq!(without_cd("cd ~/code/app && npm test"), "npm test");
        assert_eq!(without_cd("cd /a; cd b && make"), "make");
        assert_eq!(without_cd("cd /a"), "cd /a");
        assert_eq!(without_cd("npm test && cd x"), "npm test && cd x");
    }

    #[test]
    fn a_new_prompt_starts_a_turn_and_closes_what_was_open() {
        let more = format!("{T}\n{}", r#"{"type":"user","timestamp":"2026-10-04T10:01:00.000Z","message":{"content":"[Request interrupted by user]"}}
{"type":"user","timestamp":"2026-10-04T10:01:05.000Z","message":{"content":"<command-name>/rename</command-name>"}}
{"type":"user","timestamp":"2026-10-04T10:02:00.000Z","message":{"content":"now the docs"}}
{"type":"assistant","timestamp":"2026-10-04T10:02:01.000Z","message":{"content":[{"type":"text","text":"On it."}]}}"#);
        let f = feed_of(&more);
        assert_eq!(f.turns.len(), 2, "the interrupt and the /rename don't start turns");
        assert_eq!(step(&f.turns[0].items[4]), ("Ran", "npm test", "stopped", false, true));
        assert_eq!(f.turns[1].items.len(), 1);
    }

    #[test]
    fn reads_only_whole_new_lines() {
        let dir = std::env::temp_dir().join(format!("cue-steps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let first: Vec<&str> = T.lines().collect();
        // Everything up to the Edit's result, then half of the next line.
        std::fs::write(&path, format!("{}\n{}", first[..5].join("\n"), &first[5][..20])).unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        f.catch_up();
        assert_eq!(f.turns[0].items.len(), 3);
        let v = f.version;
        std::fs::write(&path, format!("{}\n", first.join("\n"))).unwrap();
        f.catch_up();
        assert_eq!(f.turns[0].items.len(), 5, "the half line is read once it's whole");
        assert!(f.version > v);
        let d = detail(p, &["t2".into(), "t3".into()]);
        assert_eq!(d["t2"]["diff"][1], json!(["-", "b"]));
        assert_eq!(d["t3"]["full"], "npm test\n");
        assert_eq!(d["t3"]["output"][1], "Tests: 3 failed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_window_claude_code_reports_sets_the_context_share() {
        let dir = std::env::temp_dir().join(format!("cue-steps-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sid = format!("win-{}", std::process::id());
        let path = dir.join(format!("{sid}.jsonl"));
        // 174k tokens on a model whose name doesn't say its window is 1M.
        std::fs::write(&path, "{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{\"content\":\"hi\"}}\n{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{\"model\":\"claude-opus-5-5\",\"content\":[{\"type\":\"text\",\"text\":\"Hello.\"}],\"usage\":{\"input_tokens\":2,\"cache_read_input_tokens\":173376,\"cache_creation_input_tokens\":710}}}\n").unwrap();
        let p = path.to_str().unwrap();
        assert_eq!(context_pct(p), Some(87), "without word from Claude Code: the 200k guess");
        let before = steps(p, 0)["version"].as_u64().unwrap();
        set_window(&sid, 1_000_000);
        assert_eq!(context_pct(p), Some(17), "Claude Code's own window, as its status line shows");
        let after = steps(p, before);
        assert!(after["version"].as_u64().unwrap() > before && after["meta"]["window"] == 1_000_000, "the window redraws: {after}");
        // Another session on that model, before its own status line has run: the model's window, not the guess.
        let other = dir.join("win-other.jsonl");
        std::fs::write(&other, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
        let path2 = dir.join(format!("{sid}-2.jsonl"));
        std::fs::write(&path2, std::fs::read_to_string(&path).unwrap().replace("claude-opus-5-5", "claude-test-model-w")).unwrap();
        assert_eq!(context_pct(path2.to_str().unwrap()), Some(87), "a model nobody has reported yet: the guess");
        set_window(&format!("{sid}-2"), 1_000_000);
        assert_eq!(context_pct(other.to_str().unwrap()), Some(17), "the window another session on its model reported");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_fresh_feed_answers_in_full_even_when_its_count_matches() {
        let dir = std::env::temp_dir().join(format!("cue-steps-fresh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let user = |text: &str| format!("{{\"type\":\"user\",\"timestamp\":\"2026-10-04T10:00:00.000Z\",\"message\":{{\"content\":\"{text}\"}}}}\n");
        let line = |model: &str| {
            format!(
                "{{\"type\":\"assistant\",\"timestamp\":\"2026-10-04T10:00:02.000Z\",\"message\":{{\"model\":\"{model}\",\"content\":[{{\"type\":\"text\",\"text\":\"Hello.\"}}],\"usage\":{{\"input_tokens\":10,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}}}}\n"
            )
        };
        let append = |path: &std::path::Path| {
            let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            std::io::Write::write_all(&mut file, format!("{}", user("switch")).as_bytes()).unwrap();
            std::io::Write::write_all(&mut file, line("claude-opus-5-5").as_bytes()).unwrap();
        };
        // While the feed lives, a model change is read and answered.
        let live = dir.join("live.jsonl");
        std::fs::write(&live, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
        let l1 = steps(live.to_str().unwrap(), 0);
        assert_eq!(l1["meta"]["model"], "claude-sonnet-5-5");
        append(&live);
        let l2 = steps(live.to_str().unwrap(), l1["version"].as_u64().unwrap());
        assert_eq!(l2["meta"]["model"], "claude-opus-5-5", "the model change is read while the feed lives");
        // A feed nobody asked for in a while is dropped: the caller still has its version, 2 here (the
        // first read). A fresh feed reads the whole file in one go and counts 2 again, so the "nothing
        // changed" answer would leave the caller on the old model. It must answer in full.
        let path = dir.join("t.jsonl");
        std::fs::write(&path, format!("{}{}", user("hi"), line("claude-sonnet-5-5"))).unwrap();
        let p = path.to_str().unwrap();
        let r1 = steps(p, 0);
        assert_eq!(r1["version"].as_u64(), Some(2));
        append(&path);
        FEEDS.lock().unwrap().get_or_insert_with(HashMap::new).remove(p);
        let r2 = steps(p, r1["version"].as_u64().unwrap());
        assert_eq!(r2["version"].as_u64(), r1["version"].as_u64());
        assert_eq!(r2["meta"]["model"], "claude-opus-5-5", "a fresh feed answers with what's there now, whatever its count");
        assert!(r2["turns"].as_array().is_some_and(|t| !t.is_empty()));
        // Once it has answered, a feed that still counts the same answers with just the version.
        let r3 = steps(p, r2["version"].as_u64().unwrap());
        assert!(r3.get("turns").is_none() && r3.get("meta").is_none(), "only the version comes back when nothing changed");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod codex_tests {
    use super::*;

    #[test]
    fn a_codex_log_reads_as_steps_too() {
        let log = [
            r#"{"timestamp":"2026-10-05T06:36:45.800Z","type":"session_meta","payload":{"session_id":"X1","cwd":"/a"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:45.816Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"AGENTS.md instructions"}]}}"#,
            r#"{"timestamp":"2026-10-05T06:36:45.831Z","type":"event_msg","payload":{"type":"user_message","message":"fix the menu"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:46.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking at the menu code."}]}}"#,
            r#"{"timestamp":"2026-10-05T06:36:47.000Z","type":"event_msg","payload":{"type":"agent_message","message":"Looking at the menu code."}}"#,
            r#"{"timestamp":"2026-10-05T06:36:48.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"cd /a && rg --files\"}","call_id":"k1"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:48.500Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k1","output":"Chunk ID: 1\nWall time: 0.03 seconds\nProcess exited with code 0\nOutput:\napp.py\nREADME.md\n"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:49.000Z","type":"response_item","payload":{"type":"function_call","name":"write_stdin","arguments":"{\"session_id\":1,\"chars\":\"\"}","call_id":"k2"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:50.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: /a/app.py\n@@\n-    old()\n+    new()\n+    more()\n*** End Patch","call_id":"k3"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:50.500Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"k3","output":"Exit code: 0\nWall time: 0.2 seconds\nOutput:\nSuccess. Updated the following files:\nM /a/app.py\n"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:51.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pytest -q\"}","call_id":"k4"}}"#,
            r#"{"timestamp":"2026-10-05T06:36:55.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"k4","output":"Chunk ID: 2\nProcess exited with code 1\nOutput:\n1 failed\n"}}"#,
        ]
        .join("\n");
        let dir = std::env::temp_dir().join(format!("cue-steps-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout.jsonl");
        std::fs::write(&path, log + "\n").unwrap();
        let p = path.to_str().unwrap();
        let mut f = Feed::open(p, 1);
        assert_eq!(f.kind, Kind::Codex);
        f.catch_up();
        assert_eq!(f.turns.len(), 1, "Codex's instructions aren't a turn of yours");
        assert_eq!(f.turns[0].prompt, "fix the menu");
        let items = &f.turns[0].items;
        let line = |i: usize| match &items[i] {
            Item::Step { verb, subject, result, bad, .. } => (verb.as_str(), subject.as_str(), result.as_str(), *bad),
            Item::Say { text, .. } => ("say", text.as_str(), "", false),
        };
        assert_eq!(items.len(), 4, "its words once (not again from agent_message), no write_stdin");
        assert_eq!(line(0), ("say", "Looking at the menu code.", "", false));
        assert_eq!(line(1), ("Ran", "rg --files", "README.md", false));
        assert_eq!(line(2), ("Edited", "app.py", "+2 −1", false));
        assert_eq!(line(3), ("Ran", "pytest -q", "exit 1", true));
        let d = detail(p, &["k3".into(), "k4".into()]);
        assert_eq!(d["k3"]["diff"][1], json!(["+", "    new()"]));
        assert_eq!(d["k4"]["output"][1], "1 failed");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod script_tests {
    #[test]
    fn a_script_that_only_runs_a_command_reads_as_that_command() {
        assert_eq!(super::script_command(r#"const r = await tools.exec_command({"cmd":"git status --short","yield_time_ms":1000}); text(r);"#).as_deref(), Some("git status --short"));
        assert_eq!(super::script_command("const m = ALL_TOOLS.filter(x => x); text(m);"), None);
    }
}
