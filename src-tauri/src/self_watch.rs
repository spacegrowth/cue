//! Two reasons a Cue ends itself, checked every few seconds for as long as it runs:
//! - it has grown past a set memory size (CUE_MEMORY_LIMIT_MB, 4 GB unless set): it says so in memory.log in its
//!   folder (and, outside a quiet test instance, in a notification), then quits. A Cue that keeps growing would
//!   otherwise take the whole Mac down with it: two leftover test copies grew to 54 GB each and froze the Mac (#764).
//! - the run that started it is gone (CUE_EXIT_WITH=<pid>, set by tests/run.sh and the end-to-end tests): a test
//!   run cut off before it could end its private Cue no longer leaves that Cue running for days (#764).

use std::time::Duration;

/// How often both are checked.
const EVERY: Duration = Duration::from_secs(5);
/// The memory size a Cue may reach, in MB, unless CUE_MEMORY_LIMIT_MB says otherwise. The installed app uses
/// well under 1.5 GB.
const DEFAULT_LIMIT_MB: u64 = 4096;

/// Why a Cue ends itself.
#[derive(Debug, PartialEq)]
pub(crate) enum Stop {
    TooBig { used_mb: u64, limit_mb: u64 },
    LauncherGone(i32),
}

impl Stop {
    /// In plain words, for memory.log and the notification.
    pub(crate) fn said(&self) -> String {
        match self {
            Stop::TooBig { used_mb, limit_mb } => format!(
                "Cue quit itself: it was using {} of memory, over its limit of {}, and would have slowed or frozen the Mac",
                size(*used_mb),
                size(*limit_mb)
            ),
            Stop::LauncherGone(pid) => format!("this test Cue quit itself: the run that started it (process {pid}) has ended"),
        }
    }
}

/// MB in GB from 1 GB up ("54.5 GB"), else in MB ("60 MB").
fn size(mb: u64) -> String {
    if mb >= 1024 { format!("{:.1} GB", mb as f64 / 1024.0) } else { format!("{mb} MB") }
}

/// What this Cue watches for: its memory limit, and the run it ends with (when it has one).
pub(crate) struct Watch {
    pub limit_mb: u64,
    pub launcher: Option<i32>,
}

impl Watch {
    /// From CUE_MEMORY_LIMIT_MB and CUE_EXIT_WITH.
    pub(crate) fn from_env() -> Watch {
        Watch {
            limit_mb: limit_mb(std::env::var("CUE_MEMORY_LIMIT_MB").ok().as_deref()),
            launcher: std::env::var("CUE_EXIT_WITH").ok().and_then(|p| p.trim().parse().ok()).filter(|p: &i32| *p > 1),
        }
    }

    /// One check: why to stop now, given the memory used (in MB), if at all.
    pub(crate) fn check(&self, used_mb: Option<u64>) -> Option<Stop> {
        if let Some(used_mb) = used_mb.filter(|u| *u > self.limit_mb) {
            return Some(Stop::TooBig { used_mb, limit_mb: self.limit_mb });
        }
        self.launcher.filter(|p| !alive(*p)).map(Stop::LauncherGone)
    }
}

/// The limit in MB: CUE_MEMORY_LIMIT_MB's value when it is a number above 0, else the default.
fn limit_mb(set: Option<&str>) -> u64 {
    set.and_then(|v| v.trim().parse().ok()).filter(|v: &u64| *v > 0).unwrap_or(DEFAULT_LIMIT_MB)
}

/// Whether process `pid` still exists (one that exists but isn't ours counts).
pub(crate) fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks the process is there; nothing is sent.
    let r = unsafe { libc::kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// This process's memory in MB, as Activity Monitor and Force Quit count it (its footprint: what it holds in
/// memory, compressed or swapped out included).
pub(crate) fn used_mb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
        // SAFETY: a rusage_info_v2 to fill, for flavor RUSAGE_INFO_V2, of this process.
        let r = unsafe { libc::proc_pid_rusage(std::process::id() as i32, libc::RUSAGE_INFO_V2, &mut info as *mut _ as *mut libc::rusage_info_t) };
        (r == 0).then(|| info.ri_phys_footprint / (1024 * 1024))
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// The watch, on a thread of its own: at the first reason, it is said (stderr, memory.log, a notification outside a
/// quiet test instance) and `quit` runs; if Cue is still here a few seconds later, the process ends at once.
pub(crate) fn start(quit: impl Fn() + Send + 'static) {
    let watch = Watch::from_env();
    let mut ticks = 0u64;
    let run = move || loop {
        std::thread::sleep(EVERY);
        let mut used = used_mb();
        // Freed memory the allocator still holds counts in the footprint until it's handed back: big buffers let go
        // (each state built for the window is megabytes) stayed there by the gigabyte (#876). Handed back before
        // Cue is judged too big, and every so often once it's past HAND_BACK_FROM_MB.
        ticks += 1;
        if used.is_some_and(|mb| mb >= watch.limit_mb || (mb >= HAND_BACK_FROM_MB && ticks % HAND_BACK_EVERY == 0)) {
            hand_back_freed();
            used = used_mb();
        }
        let Some(stop) = watch.check(used) else { continue };
        let said = stop.said();
        eprintln!("cue: {said}");
        log(&said);
        if matches!(stop, Stop::TooBig { .. }) && std::env::var_os("CUE_QUIET").is_none() {
            let body = format!("{said}. Open Cue again to carry on; memory.log in Cue's folder has the details.");
            crate::notify::later(move || {
                crate::notify::post("cue-memory-limit", "Cue quit: too much memory", &body, "");
            });
            std::thread::sleep(Duration::from_secs(1)); // the notification out first
        }
        quit();
        std::thread::sleep(Duration::from_secs(5));
        std::process::exit(1);
    };
    let _ = std::thread::Builder::new().name("cue-self-watch".into()).spawn(run);
}

/// Past this footprint (MB), freed memory is handed back to macOS every HAND_BACK_EVERY checks.
const HAND_BACK_FROM_MB: u64 = 768;
const HAND_BACK_EVERY: u64 = 6;

/// Hands the memory Cue freed, that the allocator still holds, back to macOS: the footprint is then what Cue uses.
fn hand_back_freed() {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: libc::size_t) -> libc::size_t;
        }
        // SAFETY: every zone (null), with no goal (0): as much as it can. It only returns free pages.
        unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
    }
}

/// One line in memory.log in Cue's folder.
fn log(line: &str) {
    use std::io::Write;
    let path = crate::server::cue_dir().join("memory.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {line}", crate::model::now_ms());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_4_gb_unless_set_to_a_number() {
        assert_eq!(limit_mb(None), 4096);
        assert_eq!(limit_mb(Some("2048")), 2048);
        assert_eq!(limit_mb(Some("0")), 4096);
        assert_eq!(limit_mb(Some("lots")), 4096);
    }

    #[test]
    fn a_cue_over_its_limit_stops_and_says_how_big_it_was() {
        let w = Watch { limit_mb: 4096, launcher: None };
        assert_eq!(w.check(Some(4000)), None);
        assert_eq!(w.check(None), None);
        let stop = w.check(Some(55_766)).expect("over the limit");
        assert_eq!(stop, Stop::TooBig { used_mb: 55_766, limit_mb: 4096 });
        assert!(stop.said().contains("54.5 GB of memory, over its limit of 4.0 GB"), "{}", stop.said());
        assert!(Stop::TooBig { used_mb: 70, limit_mb: 60 }.said().contains("70 MB of memory, over its limit of 60 MB"));
    }

    #[test]
    fn this_process_memory_is_read_and_counts_what_it_holds() {
        let before = used_mb().expect("memory read");
        assert!(before > 0);
        // 256 MB written to (so really held), then read again.
        let held = vec![1u8; 256 * 1024 * 1024];
        let after = used_mb().expect("memory read");
        assert!(after >= before + 200, "before {before} MB, after {after} MB");
        drop(held);
    }

    /// The cause of #764: a test run ended without ending its private Cue (it was killed, its EXIT trap never ran),
    /// and that Cue ran on for days. With CUE_EXIT_WITH, it stops once that run is gone.
    #[test]
    fn a_test_cue_stops_once_the_run_that_started_it_is_gone() {
        let mut run = std::process::Command::new("sleep").arg("30").spawn().expect("a stand-in run");
        let w = Watch { limit_mb: 4096, launcher: Some(run.id() as i32) };
        assert_eq!(w.check(Some(100)), None, "the run is still going");
        run.kill().unwrap();
        run.wait().unwrap();
        assert_eq!(w.check(Some(100)), Some(Stop::LauncherGone(run.id() as i32)));
    }

    #[test]
    fn a_cue_started_by_hand_has_no_run_to_end_with() {
        assert!(alive(std::process::id() as i32));
        assert!(alive(1), "launchd: there, though not ours");
    }
}
