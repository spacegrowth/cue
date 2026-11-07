//! What the lock guards have to say, in locks.log: ~/Library/Logs/Cue/locks.log, where Console shows
//! it, or Cue's own folder when CUE_HOME points somewhere else (a test Cue keeps out of your logs).
//! Every entry also goes to stderr, with a backtrace where one names the culprit. The same report again
//! within a minute (the same lock, the same holder) is counted, not written: a lock fought over all day
//! would otherwise fill the disk with one backtrace. The log is kept under 1 MB, plus the one before.

use std::backtrace::Backtrace;
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::Thread;
use std::time::{Duration, Instant};

/// How long a report keeps its like from being written again.
const QUIET_FOR: Duration = Duration::from_secs(60);
#[cfg_attr(test, allow(dead_code))] // as `path`
/// How big locks.log gets before it becomes locks.log.1 (replacing the one there).
const CAP: u64 = 1 << 20;

#[cfg_attr(test, allow(dead_code))] // tests keep what's said in memory (below)
fn path() -> std::path::PathBuf {
    match std::env::var_os("CUE_HOME") {
        Some(_) => crate::server::cue_dir().join("locks.log"),
        None => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Logs/Cue/locks.log"),
    }
}

/// When each kind of report was last written, and how many like it were held back since.
type Recent = HashMap<String, (Instant, u32)>;

/// Whether a report of kind `key` is written at `now`: Some(how many like it were held back since the
/// last one written), or None while one was written less than QUIET_FOR ago (it's counted instead).
fn admit(recent: &mut Recent, key: &str, now: Instant) -> Option<u32> {
    match recent.get_mut(key) {
        Some((last, held_back)) if now.duration_since(*last) < QUIET_FOR => {
            *held_back += 1;
            None
        }
        _ => {
            let held_back = recent.insert(key.to_string(), (now, 0)).map_or(0, |(_, n)| n);
            Some(held_back)
        }
    }
}

/// One entry of kind `key` unless one like it was written within the minute: the line, then the backtrace
/// (`trace`, taken only when it's written) indented under it.
fn write(key: &str, line: &str, trace: impl FnOnce() -> Option<Backtrace>) {
    static RECENT: Mutex<Option<Recent>> = Mutex::new(None);
    let held_back = admit(RECENT.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new), key, Instant::now());
    let Some(held_back) = held_back else { return };
    let mut entry = format!("{} {line}\n", crate::model::now_ms());
    if held_back > 0 {
        entry.push_str(&format!("    ({held_back} more like this in the minute before, not written)\n"));
    }
    if let Some(t) = trace() {
        entry.extend(t.to_string().lines().map(|l| format!("    {l}\n")));
    }
    eprint!("{entry}");
    keep(&entry);
}

/// `entry` added to the log at `path`, which first becomes `<path>.1` when the entry would take it past `cap`.
fn append_capped(path: &std::path::Path, entry: &str, cap: u64) {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() + entry.len() as u64 > cap) {
        let mut older = path.as_os_str().to_owned();
        older.push(".1");
        let _ = std::fs::rename(path, older);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

#[cfg(not(test))]
fn keep(entry: &str) {
    append_capped(&path(), entry, CAP);
}

/// Tests read what was said from here: no test writes to your logs, whatever CUE_HOME says at the time.
#[cfg(test)]
static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(test)]
fn keep(entry: &str) {
    SAID.lock().unwrap_or_else(|e| e.into_inner()).push(entry.to_string());
}

/// Every entry written so far by this test process.
#[cfg(test)]
pub(super) fn said() -> Vec<String> {
    SAID.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// This thread is taking a lock it holds already: it would wait on itself for good. Logged with where it
/// happened; in debug and test builds it's a panic, so a test or a dev run fails on the spot.
pub(super) fn reentry(what: &str) {
    super::stats::reentered();
    let line = format!("cue: LOCK REENTRY {what} on {}", super::held::describe(&std::thread::current()));
    write(&format!("reentry {what}"), &line, || Some(Backtrace::force_capture()));
    if cfg!(debug_assertions) {
        panic!("{line}");
    }
}

/// The watchdog's report: `waiter` has waited `waited` for `what`, held by `holder` (and for how long) when
/// anyone still holds it. The waiter's backtrace follows once it has the lock (`long_wait_over`).
pub(super) fn long_wait(what: &str, waiter: &str, waited: Duration, holder: Option<(Thread, Duration)>) {
    let (by, key) = match &holder {
        Some((t, held)) => (format!("{} for {} ms", super::held::describe(t), held.as_millis()), format!("wait {what} held by {}", t.name().unwrap_or("unnamed"))),
        None => ("nobody now (let go as this was written)".to_string(), format!("wait {what}")),
    };
    let line = format!("cue: LOCK WAIT {what}: {waiter} has waited {} ms; held by {by}", waited.as_millis());
    write(&key, &line, || None);
}

/// This thread has the lock it waited on long enough to be reported: where it waited.
pub(super) fn long_wait_over(what: &str, waited: Duration) {
    let line = format!("cue: LOCK WAIT {what} over after {} ms, for {}, which waited here:", waited.as_millis(), super::held::describe(&std::thread::current()));
    write(&format!("waited {what}"), &line, || Some(Backtrace::force_capture()));
}

/// The main thread hasn't answered for `waited`; `held` is every lock held right now.
pub(super) fn stall(waited: Duration, held: &[String]) {
    let mut line = format!("cue: main thread stalled {} ms", waited.as_millis());
    for h in held {
        line.push_str(&format!("\n    holding: {h}"));
    }
    if held.is_empty() {
        line.push_str("\n    holding: no lock (the main thread is busy with something else)");
    }
    let names: Vec<&str> = held.iter().filter_map(|h| h.split(" held by ").next()).collect();
    write(&format!("stall {}", names.join(", ")), &line, || None);
}

/// The main thread answered again after a stall.
pub(super) fn stall_over(took: Duration) {
    write("stall over", &format!("cue: main thread answered again after {} ms", took.as_millis()), || None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_report_within_a_minute_is_counted_not_written() {
        let mut recent = Recent::new();
        let t0 = Instant::now();
        assert_eq!(admit(&mut recent, "wait A held by x", t0), Some(0));
        assert_eq!(admit(&mut recent, "wait A held by x", t0 + Duration::from_secs(10)), None);
        assert_eq!(admit(&mut recent, "wait A held by x", t0 + Duration::from_secs(59)), None);
        assert_eq!(admit(&mut recent, "wait B held by x", t0 + Duration::from_secs(59)), Some(0), "another lock is another report");
        assert_eq!(admit(&mut recent, "wait A held by x", t0 + Duration::from_secs(61)), Some(2), "a minute on: written, with the two held back");
        assert_eq!(admit(&mut recent, "wait A held by x", t0 + Duration::from_secs(62)), None);
    }

    #[test]
    fn the_log_becomes_the_one_before_when_full() {
        let dir = std::env::temp_dir().join(format!("cue-locks-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("Logs/locks.log");
        let older = dir.join("Logs/locks.log.1");
        append_capped(&log, "first entry\n", 20);
        append_capped(&log, "second\n", 20);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "first entry\nsecond\n");
        append_capped(&log, "third one\n", 20);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "third one\n");
        assert_eq!(std::fs::read_to_string(&older).unwrap(), "first entry\nsecond\n");
        append_capped(&log, "fourth entry\n", 20);
        assert_eq!(std::fs::read_to_string(&older).unwrap(), "third one\n", "only the one before is kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
