//! What a soak run counts and samples, and the JSON it's judged from (xtask::soak reads that shape): the
//! window's answer times, memory and the window's state over time, each decision's trip to its hook, and
//! every panic on any thread.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Counts, by what they count.
#[derive(Default)]
pub struct Counts {
    pub events_sent: AtomicU64,
    pub events_failed: AtomicU64,
    pub typing_sent: AtomicU64,
    pub typing_failed: AtomicU64,
    pub polls: AtomicU64,
    pub asks: AtomicU64,
    pub machine_drops: AtomicU64,
}

pub fn add(n: &AtomicU64) {
    n.fetch_add(1, Ordering::Relaxed);
}

fn get(n: &AtomicU64) -> u64 {
    n.load(Ordering::Relaxed)
}

pub struct Tally {
    pub counts: Counts,
    start: Instant,
    answer_ms: Mutex<Vec<f64>>,
    decisions_ms: Mutex<Vec<f64>>,
    /// Decisions given and not yet heard by their hooks, by ask id: since when.
    given: Mutex<HashMap<String, Instant>>,
    memory: Mutex<Vec<(f64, f64)>>,
    snapshot: Mutex<Vec<(f64, f64)>>,
    /// Why typing was refused, and how often.
    refused: Mutex<HashMap<String, u64>>,
}

impl Tally {
    pub fn new() -> Tally {
        Tally { counts: Counts::default(), start: Instant::now(), answer_ms: Mutex::default(), decisions_ms: Mutex::default(), given: Mutex::default(), memory: Mutex::default(), snapshot: Mutex::default(), refused: Mutex::default() }
    }

    /// Seconds since the run started.
    pub fn at(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// The window (a `list`) answered in `ms`.
    pub fn answered(&self, ms: f64) {
        crate::locking::lock(&self.answer_ms, "soak answers").push(ms);
    }

    /// Resident memory and the window's state's size, now.
    pub fn sample(&self, memory: u64, snapshot: usize) {
        let t = self.at();
        crate::locking::lock(&self.memory, "soak memory").push((t, memory as f64));
        crate::locking::lock(&self.snapshot, "soak snapshot").push((t, snapshot as f64));
    }

    /// A message you typed was refused, saying `why`.
    pub fn refused(&self, why: &str) {
        add(&self.counts.typing_failed);
        *crate::locking::lock(&self.refused, "soak refusals").entry(why.to_string()).or_default() += 1;
    }

    /// The decision for ask `id` was given now.
    pub fn given(&self, id: &str) {
        crate::locking::lock(&self.given, "soak given").insert(id.to_string(), Instant::now());
    }

    /// The decision for ask `id` was refused (its card was gone): there's nothing for its hook to hear.
    pub fn not_given(&self, id: &str) {
        crate::locking::lock(&self.given, "soak given").remove(id);
    }

    /// Ask `id`'s hook heard its decision.
    pub fn heard(&self, id: &str) {
        if let Some(t) = crate::locking::lock(&self.given, "soak given").remove(id) {
            crate::locking::lock(&self.decisions_ms, "soak decisions").push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// The run as xtask::soak judges it: `about` (its name, length, sessions, warm-up, chaos) with what was
    /// measured, `probe` (the app's own counts) and `panics`.
    pub fn json(&self, mut about: Value, probe: Value, panics: Vec<String>) -> Value {
        let c = &self.counts;
        let measured = json!({
            "seconds": self.at(),
            "answer_ms": *crate::locking::lock(&self.answer_ms, "soak answers"),
            "decisions_ms": *crate::locking::lock(&self.decisions_ms, "soak decisions"),
            "undecided": crate::locking::lock(&self.given, "soak given").len(),
            "memory": *crate::locking::lock(&self.memory, "soak memory"),
            "snapshot": *crate::locking::lock(&self.snapshot, "soak snapshot"),
            "events": {"sent": get(&c.events_sent), "failed": get(&c.events_failed)},
            "typing": {"sent": get(&c.typing_sent), "failed": get(&c.typing_failed), "refused": *crate::locking::lock(&self.refused, "soak refusals")},
            "polls": get(&c.polls), "asks": get(&c.asks), "machine_drops": get(&c.machine_drops),
            "probe": probe, "panics": panics,
        });
        about.as_object_mut().expect("an object").extend(measured.as_object().expect("an object").clone());
        about
    }
}

/// Every panic on any thread while the run lasts, as its message and place.
pub mod panics {
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Count panics from now on (and still print them as usual).
    pub fn catch() {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
            SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(format!("thread '{thread}' {info}"));
            before(info);
        }));
    }

    /// The panics seen, and the usual panic printing back.
    pub fn seen() -> Vec<String> {
        let _ = std::panic::take_hook();
        SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
