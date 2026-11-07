//! The soak's verdict: what one run measured (written as JSON by src-tauri/src/testing/soak and by
//! tests/e2e/soak.js, in the shape `Run` reads) judged by the detectors below, and the one-page report
//! (`page`). One place for the thresholds, whichever run measured.
//!
//! `cargo run -p xtask -- soak-report <run.json>…` prints the page and fails on any detector's failure.

mod page;

pub use page::page;
use serde_json::Value;

/// The window must answer within this (ms), every time it's asked.
pub const ANSWER_MS: f64 = 500.0;
/// No wait for a lock may last longer (ms).
pub const LOCK_WAIT_MS: f64 = 1000.0;
/// After warm-up, resident memory and the snapshot's size may not grow past this many times what they were.
pub const GROWTH: f64 = 2.0;
/// A hook hears a decision you gave within this (ms).
pub const DECISION_MS: f64 = 10_000.0;

/// One run's measurements.
pub struct Run(pub Value);

impl Run {
    pub fn read(path: &std::path::Path) -> Result<Run, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map(Run).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn name(&self) -> &str {
        self.0["run"].as_str().unwrap_or("?")
    }

    pub(crate) fn num(&self, ptr: &str) -> f64 {
        self.0.pointer(ptr).and_then(Value::as_f64).unwrap_or(0.0)
    }

    /// A list of numbers at `ptr`.
    pub(crate) fn samples(&self, ptr: &str) -> Vec<f64> {
        self.0.pointer(ptr).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
    }

    /// A curve at `ptr`: [seconds, value] pairs.
    pub(crate) fn curve(&self, ptr: &str) -> Vec<(f64, f64)> {
        let pair = |p: &Value| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?));
        self.0.pointer(ptr).and_then(Value::as_array).map(|a| a.iter().filter_map(pair).collect()).unwrap_or_default()
    }

    /// The strings at `ptr`.
    pub(crate) fn lines(&self, ptr: &str) -> Vec<String> {
        self.0.pointer(ptr).and_then(Value::as_array).map(|a| a.iter().filter_map(|l| l.as_str().map(String::from)).collect()).unwrap_or_default()
    }

    /// Every busy lock's waits, as the app's probe counted them: (name, waits, longest ms, spread).
    pub(crate) fn locks(&self) -> Vec<(String, u64, f64, Value)> {
        let Some(locks) = self.0.pointer("/probe/locks").and_then(Value::as_object) else { return Vec::new() };
        let mut out: Vec<_> = locks.iter().map(|(k, v)| (k.clone(), v["waits"].as_u64().unwrap_or(0), v["max_ms"].as_f64().unwrap_or(0.0), v["spread"].clone())).collect();
        out.sort_by(|a, b| b.2.total_cmp(&a.2));
        out
    }

    /// Every detector's failures, in words; none is a pass.
    pub fn failures(&self) -> Vec<String> {
        let detectors: [fn(&Run) -> Vec<String>; 7] = [answers, locks, reentries, growth, decisions, panics, deliveries];
        detectors.iter().flat_map(|d| d(self)).collect()
    }
}

/// The window answered late: a sample over ANSWER_MS, from the run's own polls or the app's main-thread probe.
fn answers(r: &Run) -> Vec<String> {
    let mut out = Vec::new();
    let late: Vec<f64> = r.samples("/answer_ms").into_iter().filter(|ms| *ms > ANSWER_MS).collect();
    if !late.is_empty() {
        let worst = late.iter().copied().fold(0.0, f64::max);
        out.push(format!("{} answered late {} times (worst {worst:.0} ms, limit {ANSWER_MS:.0})", r.0["answer_what"].as_str().unwrap_or("the window"), late.len()));
    }
    let pinged = r.num("/probe/main_thread/max_ms");
    if pinged > ANSWER_MS {
        out.push(format!("the main thread took {pinged:.0} ms to answer a probe ping (limit {ANSWER_MS:.0})"));
    }
    out
}

fn locks(r: &Run) -> Vec<String> {
    r.locks().into_iter().filter(|l| l.2 > LOCK_WAIT_MS).map(|(name, waits, max, _)| format!("lock \"{name}\": a wait of {max:.0} ms (limit {LOCK_WAIT_MS:.0}; {waits} waits)")).collect()
}

fn reentries(r: &Run) -> Vec<String> {
    let counted = r.num("/probe/reentries") as u64;
    let logged = r.lines("/log").iter().filter(|l| l.contains("LOCK REENTRY")).count() as u64;
    let n = counted.max(logged);
    if n > 0 { vec![format!("a lock was taken again on the thread holding it ({n} times)")] } else { Vec::new() }
}

/// The value at the end of warm-up (the first sample from then on), and the most it reached after.
fn after_warmup(curve: &[(f64, f64)], warmup_s: f64) -> Option<(f64, f64)> {
    let mut later = curve.iter().filter(|(t, _)| *t >= warmup_s);
    let base = later.next()?.1;
    Some((base, later.map(|(_, v)| *v).fold(base, f64::max)))
}

fn growth(r: &Run) -> Vec<String> {
    let warmup = r.num("/warmup_s");
    let mut out = Vec::new();
    for (ptr, what) in [("/memory", "resident memory"), ("/snapshot", "the window's state")] {
        if let Some((base, most)) = after_warmup(&r.curve(ptr), warmup) {
            if base > 0.0 && most > GROWTH * base {
                out.push(format!("{what} grew {:.1}× after warm-up ({base:.0} → {most:.0} bytes, limit {GROWTH}×)", most / base));
            }
        }
    }
    out
}

fn decisions(r: &Run) -> Vec<String> {
    let mut out = Vec::new();
    let late: Vec<f64> = r.samples("/decisions_ms").into_iter().filter(|ms| *ms > DECISION_MS).collect();
    if !late.is_empty() {
        out.push(format!("a hook waited past {DECISION_MS:.0} ms for a decision already given ({} times)", late.len()));
    }
    let lost = r.num("/undecided") as u64;
    if lost > 0 {
        out.push(format!("{lost} hooks never heard a decision that was given"));
    }
    out
}

fn panics(r: &Run) -> Vec<String> {
    r.lines("/panics").into_iter().take(5).map(|p| format!("panic: {p}")).collect()
}

fn deliveries(r: &Run) -> Vec<String> {
    let failed = r.num("/events/failed") as u64;
    if failed > 0 { vec![format!("{failed} hook events never reached Cue's socket")] } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(v: Value) -> Run {
        Run(v)
    }

    #[test]
    fn a_clean_run_passes() {
        let r = run(json!({"run": "rust", "warmup_s": 60, "answer_ms": [3, 12, 40], "memory": [[0, 10], [60, 50], [600, 90]], "snapshot": [[60, 1000], [600, 1500]],
            "probe": {"locks": {"store": {"waits": 4, "max_ms": 80.0}}, "reentries": 0, "main_thread": {"max_ms": 20}}, "decisions_ms": [100, 900], "events": {"failed": 0}}));
        assert_eq!(r.failures(), Vec::<String>::new());
    }

    #[test]
    fn each_detector_names_what_failed() {
        let r = run(json!({"run": "e2e", "warmup_s": 60, "answer_what": "executeScript", "answer_ms": [3, 700], "memory": [[0, 1], [60, 50], [600, 101]], "snapshot": [[60, 1000], [600, 1500]],
            "probe": {"locks": {"DB": {"waits": 2, "max_ms": 1500.0}}, "reentries": 1, "main_thread": {"max_ms": 900}}, "decisions_ms": [12_000], "undecided": 1,
            "panics": ["thread 'x' panicked at hub/mod.rs"], "events": {"failed": 3}}));
        let f = r.failures().join("\n");
        for part in ["executeScript answered late 1 times", "main thread took 900", "lock \"DB\"", "taken again", "resident memory grew 2.0×", "past 10000 ms", "1 hooks never", "panic: thread", "3 hook events"] {
            assert!(f.contains(part), "{part} in:\n{f}");
        }
        assert!(!f.contains("window's state"), "the snapshot grew 1.5×, inside the limit:\n{f}");
    }

    #[test]
    fn growth_is_measured_from_the_end_of_warm_up() {
        assert_eq!(after_warmup(&[(0.0, 1.0), (30.0, 5.0), (60.0, 40.0), (90.0, 60.0), (120.0, 50.0)], 60.0), Some((40.0, 60.0)));
        assert_eq!(after_warmup(&[(0.0, 1.0)], 60.0), None, "a run too short to be past warm-up isn't judged");
    }
}
