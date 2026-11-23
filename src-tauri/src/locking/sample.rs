//! A stalled main thread is mostly the window's page busy in its own process (WebKit's WebContent): #746's
//! stall showed only the main thread waiting on it (`displayDidRefresh`), not what the page was doing. So
//! a stall also takes a short `/usr/bin/sample` of the main window's page process and keeps it next to
//! locks.log as `stall-<ms>.sample.txt`, `<ms>` the stall's time as its log line has it.
//!
//! The process is the one WebKit names for Cue's own web view (`_webProcessIdentifier`, asked when its page
//! finishes loading, as the main thread can't be asked during a stall), and it's sampled only while macOS
//! still counts it as this app's (the same responsible process as Cue's): never one found by its name.
//! One sample at a time; the newest `KEEP` are kept.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// How long each sample runs, in seconds.
const SECONDS: u32 = 2;
/// How many samples the folder keeps.
const KEEP: usize = 10;
const PREFIX: &str = "stall-";
const SUFFIX: &str = ".sample.txt";

/// The main window's page process, 0 until its page has loaded.
static PAGE: AtomicI32 = AtomicI32::new(0);
/// A sample is running.
static BUSY: AtomicBool = AtomicBool::new(false);

/// The main window's page process is `pid` (WebKit's own word for it; 0 when it has none).
pub fn page_process(pid: i32) {
    PAGE.store(pid, Ordering::SeqCst);
}

/// The main thread has stalled at `at_ms`: a sample of the page process starts, into `dir`. Its file, or
/// None when none started (no page process known, it's no longer this app's, or one is running already).
pub(super) fn on_stall(dir: &Path, at_ms: u64) -> Option<PathBuf> {
    let pid = PAGE.load(Ordering::SeqCst);
    if pid <= 0 || !ours(pid) || BUSY.swap(true, Ordering::SeqCst) {
        return None;
    }
    let file = dir.join(format!("{PREFIX}{at_ms}{SUFFIX}"));
    let (dir, out) = (dir.to_path_buf(), file.clone());
    let started = std::thread::Builder::new().name("cue stall sample".into()).spawn(move || {
        take(pid, &dir, &out);
        BUSY.store(false, Ordering::SeqCst);
    });
    if started.is_err() {
        BUSY.store(false, Ordering::SeqCst);
        return None;
    }
    Some(file)
}

/// `pid` sampled into `out`, then the folder trimmed to the newest `KEEP` samples.
fn take(pid: i32, dir: &Path, out: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let ran = std::process::Command::new("/usr/bin/sample")
        .arg(pid.to_string())
        .arg(SECONDS.to_string())
        .arg("-mayDie")
        .arg("-file")
        .arg(out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !ran.as_ref().is_ok_and(|s| s.success()) {
        eprintln!("cue: the stall's sample of the page process {pid} didn't finish ({ran:?})");
    }
    trim(dir, KEEP);
}

/// The oldest samples in `dir` removed, `keep` left. Only Cue's own sample files are looked at.
fn trim(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut samples: Vec<(u64, PathBuf)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?;
            let ms = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.parse().ok()?;
            Some((ms, p))
        })
        .collect();
    samples.sort();
    let over = samples.len().saturating_sub(keep);
    for (_, p) in samples.into_iter().take(over) {
        let _ = std::fs::remove_file(p);
    }
}

/// `pid` is still this app's: macOS holds the same process responsible for it as for Cue (a web view's page
/// process is its app's), so a page process gone and its number reused by another app isn't sampled.
#[cfg(target_os = "macos")]
fn ours(pid: i32) -> bool {
    extern "C" {
        fn responsibility_get_pid_responsible_for_pid(pid: i32) -> i32;
    }
    let me = std::process::id() as i32;
    let (theirs, mine) = unsafe { (responsibility_get_pid_responsible_for_pid(pid), responsibility_get_pid_responsible_for_pid(me)) };
    theirs > 0 && theirs == mine
}
#[cfg(not(target_os = "macos"))]
fn ours(_: i32) -> bool {
    false
}

/// The web view's page process (WebKit's `_webProcessIdentifier`), 0 when it has none or WebKit can't say.
#[cfg(target_os = "macos")]
pub fn pid_of(wk: *mut std::ffi::c_void) -> i32 {
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel};
    let wk = wk as *mut AnyObject;
    if wk.is_null() {
        return 0;
    }
    unsafe {
        let has: bool = msg_send![&*wk, respondsToSelector: sel!(_webProcessIdentifier)];
        if !has {
            return 0;
        }
        msg_send![&*wk, _webProcessIdentifier]
    }
}

#[cfg(test)]
pub(super) fn wait_idle() {
    while BUSY.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_samples_are_kept() {
        let dir = std::env::temp_dir().join(format!("cue-stall-trim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for ms in [5, 1, 4, 2, 3] {
            std::fs::write(dir.join(format!("{PREFIX}{ms}{SUFFIX}")), "x").unwrap();
        }
        std::fs::write(dir.join("locks.log"), "kept").unwrap();
        trim(&dir, 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["locks.log", "stall-4.sample.txt", "stall-5.sample.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_process_not_of_this_app_is_not_sampled() {
        assert!(!ours(1), "launchd isn't Cue's");
        assert!(!ours(i32::MAX), "no such process");
    }
}
