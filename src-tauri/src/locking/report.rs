//! What the lock guards have to say, in locks.log: ~/Library/Logs/Cue/locks.log, where Console shows
//! it, or Cue's own folder when CUE_HOME points somewhere else (a test Cue keeps out of your logs).
//! Every line also goes to stderr. Each is written with a backtrace where one names the culprit.

use std::backtrace::Backtrace;
use std::time::Duration;

#[cfg_attr(test, allow(dead_code))] // tests keep what's said in memory (below)
fn path() -> std::path::PathBuf {
    match std::env::var_os("CUE_HOME") {
        Some(_) => crate::server::cue_dir().join("locks.log"),
        None => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Logs/Cue/locks.log"),
    }
}

/// One entry: the line, then the backtrace (when there is one) indented under it.
fn write(line: &str, trace: Option<Backtrace>) {
    let at = crate::model::now_ms();
    let mut entry = format!("{at} {line}\n");
    if let Some(t) = trace {
        entry.extend(t.to_string().lines().map(|l| format!("    {l}\n")));
    }
    eprint!("{entry}");
    keep(&entry);
}

#[cfg(not(test))]
fn keep(entry: &str) {
    use std::io::Write;
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

/// Tests read what was said from here: no test writes to your logs, whatever CUE_HOME says at the time.
#[cfg(test)]
static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

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
    let line = format!("cue: LOCK REENTRY {what} on {}", super::held::describe(&std::thread::current()));
    write(&line, Some(Backtrace::force_capture()));
    if cfg!(debug_assertions) {
        panic!("{line}");
    }
}

/// A lock this thread has waited on for `waited`, held by `holder` ("<thread> for <ms> ms") when known.
pub(super) fn long_wait(what: &str, waited: Duration, holder: Option<String>) {
    let holder = holder.unwrap_or_else(|| "nobody now (let go as this was written)".into());
    let line = format!("cue: LOCK WAIT {what}: {} has waited {} ms; held by {holder}", super::held::describe(&std::thread::current()), waited.as_millis());
    write(&line, Some(Backtrace::force_capture()));
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
    write(&line, None);
}

/// The main thread answered again after a stall.
pub(super) fn stall_over(took: Duration) {
    write(&format!("cue: main thread answered again after {} ms", took.as_millis()), None);
}
