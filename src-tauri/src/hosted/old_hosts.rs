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
    /// On another machine: its ssh host and the build it is (its folder and program there are named by it).
    pub machine: Option<(String, String)>,
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
            Ok((link, h)) => found.push(OldHost { link, pid: h.pid, legacy, machine: None }),
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

/// The host ended by a signal, then its folder cleared (on a machine, its program too).
pub(super) fn stop(h: &OldHost) {
    h.link.shut();
    match &h.machine {
        None => {
            end_process(h.pid);
            clear(h.link.home(), h.legacy);
        }
        Some((host, build)) => {
            if let Err(e) = crate::machines::run(host, &stop_there(h.pid, build)) {
                eprintln!("cue: couldn't stop the older cue-host on {host}: {e}");
            }
        }
    }
}

/// What ends an older build's host on a machine (by its pid, asked first, then ended) and clears its folder and its
/// program there. Never a pid that isn't a process of yours, nor one past an ordinary pid's range.
fn stop_there(pid: i32, build: &str) -> String {
    let b = build.chars().filter(char::is_ascii_hexdigit).collect::<String>();
    let kill = if pid > 1 { format!("kill {pid} 2>/dev/null; i=0; while kill -0 {pid} 2>/dev/null && [ $i -lt 20 ]; do sleep 0.1; i=$((i+1)); done; kill -9 {pid} 2>/dev/null; ") } else { String::new() };
    format!("{kill}rm -rf \"$HOME/.cue/host/b-{b}\" \"$HOME/.cue/bin/cue-host-{b}\"; true")
}

/// The hosts of other builds on machine `host` (`m_name`, for saying so) that still run: each reached through its own
/// program there (`connect --only-running`, never starting one). A build's folder whose host doesn't answer is
/// cleared with its program; so is a program whose build has no folder.
pub(super) fn find_there(host: &str, m_name: &str, current: &str) -> Vec<OldHost> {
    let listed = crate::machines::run(host, "cd \"$HOME/.cue\" 2>/dev/null || exit 0; ls -d host/b-* bin/cue-host-* 2>/dev/null; true").unwrap_or_default();
    let builds_of = |prefix: &str| -> Vec<String> { listed.lines().filter_map(|l| l.trim().strip_prefix(prefix)).filter(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_hexdigit()) && *b != current).map(str::to_string).collect() };
    let (homes, programs) = (builds_of("host/b-"), builds_of("bin/cue-host-"));
    let mut found = vec![];
    for b in programs.iter().filter(|b| !homes.contains(b)) {
        let _ = crate::machines::run(host, &format!("rm -f \"$HOME/.cue/bin/cue-host-{b}\""));
    }
    for b in homes {
        let script = format!("exec \"$HOME/.cue/bin/cue-host-{b}\" connect --only-running --home \"$HOME/.cue/host/b-{b}\"");
        let ssh = host.to_string();
        let dial = super::conn::Dial::Machine { home: PathBuf::from(format!("{m_name}:~/.cue/host/b-{b}")), command: Box::new(move || crate::machines::direct_command(&ssh, &script)), once: true };
        match HostLink::over(dial).and_then(|link| hello(&link).map(|h| (link, h))) {
            Ok((link, h)) => found.push(OldHost { link, pid: h.pid, legacy: false, machine: Some((host.to_string(), b)) }),
            // Not reached: cleared, unless its host still runs (the connection may have failed, not the host).
            Err(e) => {
                let clear = format!("p=$(cat \"$HOME/.cue/host/b-{b}/host.pid\" 2>/dev/null); if [ -n \"$p\" ] && kill -0 \"$p\" 2>/dev/null; then echo \"still running\"; else {}; fi", stop_there(0, &b));
                eprintln!("cue: an older cue-host on {m_name} ({b}) didn't answer ({e}): {}", crate::machines::run(host, &clear).map(|o| if o.contains("still running") { "it still runs, left alone" } else { "cleared" }).unwrap_or("left alone"));
            }
        }
    }
    found
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
