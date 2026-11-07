//! What a long run is judged by (the soak harness, `cue-ctl probe`): how long the main thread took to
//! answer each of the probe's pings, every wait for a lock that was busy (by the lock's name), how many
//! times a lock was taken again on the thread that held it, and the app's resident memory. Kept in every
//! build: a ping a second and a busy lock are rare enough that counting them costs nothing you'd notice.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// The upper bounds (ms) of a lock wait histogram's buckets; a last one takes everything longer.
const BOUNDS_MS: [u64; 8] = [1, 5, 10, 50, 100, 500, 1000, 5000];
/// How many main-thread answer times are kept (the latest): an hour of pings, one a second.
const KEEP_PINGS: usize = 4096;

/// The waits for one lock: how many, the longest, and how they spread over BOUNDS_MS.
#[derive(Clone, Default)]
struct Waits {
    count: u64,
    max_us: u64,
    buckets: [u64; BOUNDS_MS.len() + 1],
}

impl Waits {
    fn add(&mut self, waited: Duration) {
        let us = waited.as_micros() as u64;
        self.count += 1;
        self.max_us = self.max_us.max(us);
        let i = BOUNDS_MS.iter().position(|b| us < b * 1000).unwrap_or(BOUNDS_MS.len());
        self.buckets[i] += 1;
    }

    fn json(&self) -> Value {
        let mut spread = Map::new();
        for (i, n) in self.buckets.iter().enumerate().filter(|(_, n)| **n > 0) {
            spread.insert(bucket_name(i), json!(n));
        }
        json!({"waits": self.count, "max_ms": self.max_us as f64 / 1000.0, "spread": spread})
    }
}

/// A bucket's name: "<1ms", "<5ms", … and ">=5000ms" for the last.
fn bucket_name(i: usize) -> String {
    BOUNDS_MS.get(i).map_or_else(|| format!(">={}ms", BOUNDS_MS[BOUNDS_MS.len() - 1]), |b| format!("<{b}ms"))
}

/// Plain locks: the bookkeeping of the one way to take a lock can't go through it. Each is taken alone.
static WAITS: Mutex<BTreeMap<&'static str, Waits>> = Mutex::new(BTreeMap::new());
static PINGS: Mutex<VecDeque<u64>> = Mutex::new(VecDeque::new());
static REENTRIES: AtomicU64 = AtomicU64::new(0);

/// A thread waited `waited` for lock `what`, which was busy when it asked.
pub(super) fn waited(what: &'static str, waited: Duration) {
    WAITS.lock().unwrap_or_else(|e| e.into_inner()).entry(what).or_default().add(waited);
}

/// A lock was taken again on the thread that held it.
pub(super) fn reentered() {
    REENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// The main thread answered a ping after `took`.
pub(super) fn answered(took: Duration) {
    let mut p = PINGS.lock().unwrap_or_else(|e| e.into_inner());
    if p.len() == KEEP_PINGS {
        p.pop_front();
    }
    p.push_back(took.as_micros() as u64);
}

/// The `q`th quantile (0..=1) of `sorted`, in ms; 0 for none.
fn quantile_ms(sorted: &[u64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[i] as f64 / 1000.0
}

fn main_thread() -> Value {
    let mut us: Vec<u64> = PINGS.lock().unwrap_or_else(|e| e.into_inner()).iter().copied().collect();
    us.sort_unstable();
    json!({"pings": us.len(), "p50_ms": quantile_ms(&us, 0.5), "p99_ms": quantile_ms(&us, 0.99), "max_ms": quantile_ms(&us, 1.0)})
}

/// This process's resident memory in bytes (what Activity Monitor calls Real Memory), 0 if unknown.
pub fn resident_bytes() -> u64 {
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    let got = unsafe { libc::proc_pidinfo(std::process::id() as libc::c_int, libc::PROC_PIDTASKINFO, 0, (&mut info as *mut libc::proc_taskinfo).cast(), size) };
    if got == size { info.pti_resident_size } else { 0 }
}

/// Everything counted so far: the main thread's answer times, each busy lock's waits, the reentries,
/// and resident memory now.
pub fn probe() -> Value {
    let locks: Map<String, Value> = WAITS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(k, w)| (k.to_string(), w.json())).collect();
    json!({
        "pid": std::process::id(),
        "main_thread": main_thread(),
        "locks": locks,
        "reentries": REENTRIES.load(Ordering::Relaxed),
        "resident_bytes": resident_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_fall_in_their_buckets_and_the_longest_is_kept() {
        let mut w = Waits::default();
        for ms in [0, 3, 3, 70, 1200, 9000] {
            w.add(Duration::from_millis(ms));
        }
        let j = w.json();
        assert_eq!(j["waits"], 6);
        assert_eq!(j["max_ms"], 9000.0);
        assert_eq!(j["spread"], json!({"<1ms": 1, "<5ms": 2, "<100ms": 1, "<5000ms": 1, ">=5000ms": 1}));
    }

    #[test]
    fn quantiles_of_answer_times() {
        let us: Vec<u64> = (1..=100).map(|ms| ms * 1000).collect();
        assert_eq!((quantile_ms(&us, 0.5), quantile_ms(&us, 0.99), quantile_ms(&us, 1.0)), (51.0, 99.0, 100.0));
        assert_eq!(quantile_ms(&[], 0.5), 0.0);
    }

    #[test]
    fn resident_memory_is_read() {
        assert!(resident_bytes() > 1 << 20, "a running test process holds more than a megabyte");
    }
}
