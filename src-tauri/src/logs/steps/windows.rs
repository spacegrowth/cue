//! Each session's and model's context window as Claude Code's status line said (kept on disk), what a
//! waiting card's insights need from a session, and where Claude Code lately compacted on its own.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Each Claude session's context window as its status line last said (by session id), and each model's
/// as a session on it last said. Its transcript doesn't say, and the model's name doesn't always either
/// (a 1M window without "[1m]"). Kept on disk: a status line only runs when its session does something,
/// so after Cue restarts an idle session would otherwise read as on the 200k guess (196k → 98%).
#[derive(Default, Serialize, serde::Deserialize)]
pub(super) struct Windows {
    #[serde(default)]
    pub(super) sessions: HashMap<String, u64>,
    #[serde(default)]
    pub(super) models: HashMap<String, u64>,
}
static WINDOWS: Mutex<Option<Windows>> = Mutex::new(None);

pub(super) fn windows_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("windows.json")
}

/// The windows Cue knows (read from disk the first time; tests never touch the real one).
pub(super) fn with_windows<R>(f: impl FnOnce(&mut Windows) -> R) -> R {
    let mut g = crate::locking::lock(&WINDOWS, "WINDOWS");
    let w = g.get_or_insert_with(|| if cfg!(test) { Windows::default() } else { std::fs::read(windows_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() });
    f(w)
}

/// A session's window: what its status line said, else what one on the same model said.
pub(super) fn known_window(path: &str, model: &str) -> Option<u64> {
    let sid = std::path::Path::new(path).file_stem()?.to_str()?;
    with_windows(|w| w.sessions.get(sid).or_else(|| w.models.get(model)).copied())
}

/// What a waiting card's insights need from a session's log.
#[derive(Serialize)]
pub struct InsightState {
    /// When its cached context runs out (0 when its log doesn't say it caches).
    pub until_ms: u64,
    /// Its context now, the context it started with, and its model's window (0 when its status line
    /// hasn't said; insights.js takes 200k then).
    pub tokens: u64,
    pub start: u64,
    pub window: u64,
    /// When its latest reply came (a compaction since then makes the context count stale until its next reply).
    pub last_ms: u64,
    /// How many warm replies a cold one costs (prices::cold_x).
    pub cold_x: u64,
    /// What that cold reply's re-write costs at list prices, in cents (prices::cold_cents).
    pub cold_cents: u64,
    /// Where Claude Code has lately compacted sessions on its model on its own (auto_compact_at).
    pub auto_at: u64,
}

/// How many of the latest auto-compactions on a model auto_compact_at looks at, and the fewest it says anything from.
const AUTO_SEEN: u32 = 10;
const AUTO_MIN: usize = 2;
/// Where Claude Code has lately compacted sessions on `model` on its own: the least context of its last
/// AUTO_SEEN auto-compactions (from the turns history), 0 below AUTO_MIN. Not a fixed point: its trigger
/// moves (one model's sessions went from about 967k to anywhere between 206k and 474k between versions),
/// so the least seen lately is the honest warning. Asked again every 10 minutes, as the history is read.
pub(super) fn auto_compact_at(model: &str) -> u64 {
    static SEEN: Mutex<Option<HashMap<String, (Instant, u64)>>> = Mutex::new(None);
    if model.is_empty() || cfg!(test) {
        return 0;
    }
    let mut guard = crate::locking::lock(&SEEN, "AUTO_SEEN");
    let seen = guard.get_or_insert_with(HashMap::new);
    if let Some(&(_, v)) = seen.get(model).filter(|(at, _)| at.elapsed() < Duration::from_secs(600)) {
        return v;
    }
    let all = crate::db::auto_compacts(model, AUTO_SEEN);
    let v = if all.len() >= AUTO_MIN { all.into_iter().min().unwrap_or(0) } else { 0 };
    seen.insert(model.to_string(), (Instant::now(), v));
    v
}
