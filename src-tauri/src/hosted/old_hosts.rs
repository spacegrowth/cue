//! Hosts of another build than Cue's own (docs/dev/cue-agent-host.md §12): each build of cue-host lives in a folder
//! of its own under the hosts' root (`b-<build>`; the root itself for the first host, from before builds were told),
//! so a Cue with fixed host code never sends a new session to an older host. An older one is let finish what it runs,
//! then stopped by a signal (nothing is asked of its code, so a broken one can't stay) and its folder cleared.

use super::host_link::HostLink;
use cue_host::proto::Hello;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a host asked to end (SIGTERM) gets before it's ended (SIGKILL).
const END_GRACE: Duration = Duration::from_secs(2);

/// An older host Cue is connected to while it finishes what it runs.
pub(super) struct OldHost {
    pub link: Arc<HostLink>,
    pub pid: i32,
    /// The first host's, in the root itself (its files sit beside the newer hosts' folders).
    pub legacy: bool,
}

/// Hosts under `root` other than the one in `current` that still answer; any that don't are gone (their agents went
/// with them), and their folders are cleared.
pub(super) fn find(root: &Path, current: &Path) -> Vec<OldHost> {
    let mut homes: Vec<(PathBuf, bool)> = vec![(root.to_path_buf(), true)];
    if let Ok(dirs) = std::fs::read_dir(root) {
        homes.extend(dirs.flatten().map(|d| d.path()).filter(|p| p.is_dir() && p != current && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("b-"))).map(|p| (p, false)));
    }
    let mut found = vec![];
    for (home, legacy) in homes {
        if !cue_host::daemon::socket(&home).exists() && legacy {
            continue;
        }
        match HostLink::existing(home.clone()).and_then(|link| hello(&link).map(|h| (link, h))) {
            Ok((link, h)) => found.push(OldHost { link, pid: h.pid, legacy }),
            Err(_) => clear(&home, legacy),
        }
    }
    found
}

pub(super) fn hello(link: &HostLink) -> Result<Hello, String> {
    serde_json::from_value(link.request(json!({ "op": "hello", "version": cue_host::proto::VERSION }))?).map_err(|e| e.to_string())
}

/// Whether the host on `link` still runs any agent.
pub(super) fn busy(link: &HostLink) -> bool {
    hello(link).map(|hello| hello.sessions.iter().any(|s| s.running)).unwrap_or(false)
}

/// The host ended by a signal, then its folder cleared.
pub(super) fn stop(h: &OldHost) {
    h.link.shut();
    end_process(h.pid);
    clear(h.link.home(), h.legacy);
}

/// Process `pid` asked to end, then ended. Never this process (a host can run inside it: the tests'), nor nothing.
pub(super) fn end_process(pid: i32) {
    if pid <= 0 || pid == std::process::id() as i32 {
        return;
    }
    // SAFETY: plain signals to one process id; signal 0 only asks whether it's still there.
    unsafe {
        libc::kill(pid, libc::SIGTERM);
        let until = Instant::now() + END_GRACE;
        while libc::kill(pid, 0) == 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        if libc::kill(pid, 0) == 0 {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

/// What a host kept, dropped: a build's folder whole; for the first host, its own files in the root (the newer hosts'
/// folders beside them stay).
fn clear(home: &Path, legacy: bool) {
    if !legacy {
        let _ = std::fs::remove_dir_all(home);
        return;
    }
    let _ = std::fs::remove_dir_all(home.join("sessions"));
    for f in ["host.sock", "host.pid", "host.lock", "host.log"] {
        let _ = std::fs::remove_file(home.join(f));
    }
}
