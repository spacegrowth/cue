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

/// The windows Cue knows, read from disk the first time (tests never touch the real one). Call it with no
/// lock held: a feed's read step does (`Plan::fetch`), before its lines are put in under the feed's lock.
pub(super) fn load_windows() {
    if crate::locking::lock(&WINDOWS, "WINDOWS").is_some() {
        return;
    }
    let read = if cfg!(test) { Windows::default() } else { std::fs::read(windows_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() };
    crate::locking::lock(&WINDOWS, "WINDOWS").get_or_insert(read);
}

fn with_windows<R>(f: impl FnOnce(&mut Windows) -> R) -> R {
    load_windows();
    f(crate::locking::lock(&WINDOWS, "WINDOWS").get_or_insert_with(Windows::default))
}

/// What a status line said a session's window is (and its model's, `model` when it stands for it), kept on
/// disk when it's news. The file is written with WINDOWS let go, newest first: an older copy never lands
/// over a newer one.
pub(super) fn remember_window(session_id: &str, window: u64, model: Option<&str>) {
    static SAVED: Mutex<u64> = Mutex::new(0);
    static COPIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let copy = with_windows(|w| {
        // Old sessions' entries go now and then; the models' stay.
        if w.sessions.len() > 2000 {
            w.sessions.clear();
        }
        let new_session = w.sessions.insert(session_id.to_string(), window) != Some(window);
        let new_model = model.filter(|m| !m.is_empty()).is_some_and(|m| w.models.insert(m.to_string(), window) != Some(window));
        let n = COPIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        (new_session || new_model).then(|| serde_json::to_vec(&*w).ok().map(|b| (n, b))).flatten()
    });
    let Some((n, bytes)) = copy.filter(|_| !cfg!(test)) else { return };
    // Only the file's own writers wait here (nothing reading a session does).
    let mut saved = crate::locking::lock(&SAVED, "windows file");
    if n > *saved {
        let tmp = windows_file().with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, windows_file()).is_ok() {
            *saved = n;
        }
    }
}

/// A session's window: what its status line said, else what one on the same model said.
pub(super) fn known_window(path: &str, model: &str) -> Option<u64> {
    let sid = std::path::Path::new(path).file_stem()?.to_str()?;
    // Only what's loaded (`load_windows`): this runs under a feed's lock, where no file is read.
    let windows = crate::locking::lock(&WINDOWS, "WINDOWS");
    windows.as_ref().and_then(|w| w.sessions.get(sid).or_else(|| w.models.get(model)).copied())
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
    if model.is_empty() {
        return 0;
    }
    // Tests never read the turns history: the point is what the test set (test_auto), else none.
    if let Some(at) = test_auto::get(model) {
        return at;
    }
    auto_compact_seen(model)
}

/// auto_compact_at from the turns history, kept for 10 minutes a model.
fn auto_compact_seen(model: &str) -> u64 {
    static SEEN: Mutex<Option<HashMap<String, (Instant, u64)>>> = Mutex::new(None);
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

/// Tests: where Claude Code lately compacted sessions on a model on its own, as a test sets it (the turns
/// history isn't read in tests).
#[cfg(test)]
pub(crate) mod test_auto {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static AT: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

    pub(crate) fn set(model: &str, at: u64) {
        AT.lock().unwrap().get_or_insert_with(HashMap::new).insert(model.to_string(), at);
    }

    pub(super) fn get(model: &str) -> Option<u64> {
        Some(AT.lock().unwrap().as_ref().and_then(|m| m.get(model).copied()).unwrap_or(0))
    }
}

/// Outside tests: nothing set, the turns history says.
#[cfg(not(test))]
mod test_auto {
    pub(super) fn get(_model: &str) -> Option<u64> {
        None
    }
}
