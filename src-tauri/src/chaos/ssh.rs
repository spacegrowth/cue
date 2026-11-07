//! CUE_CHAOS_SSH_DROP_EVERY: the line to every machine drops every that many seconds. The ssh processes
//! Cue runs (each machine's watcher stream, a trip under way) are ended, and for DOWN_MS after, an ssh Cue
//! starts fails at once as ssh does when it can't connect (exit 255, "Connection refused"). Only once
//! `start` has run (the app's startup, debug builds): a test's FakeSsh drops its own lines.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// How long the line stays down after each drop.
const DOWN_MS: u64 = 2_000;

/// Until when (forward-only clock) the line is down; 0: it's up.
static DOWN_UNTIL: AtomicU64 = AtomicU64::new(0);

/// The thread that drops the line, when the knob is on.
pub(super) fn start() {
    let Some(every) = super::get(super::Knob::SshDropEvery) else { return };
    let drop_line = move || loop {
        std::thread::sleep(std::time::Duration::from_secs(every));
        DOWN_UNTIL.store(crate::clock::now_mono_ms() + DOWN_MS, Ordering::SeqCst);
        end_ssh_children();
    };
    let _ = std::thread::Builder::new().name("cue chaos ssh".into()).spawn(drop_line);
}

/// Whether the line is down right now.
pub fn down() -> bool {
    crate::clock::now_mono_ms() < DOWN_UNTIL.load(Ordering::SeqCst)
}

/// What runs in ssh's place while the line is down: ssh's refusal, whatever arguments it's given.
pub fn refused() -> Command {
    let mut c = Command::new("/bin/sh");
    c.args(["-c", "echo 'ssh: connect to host port 22: Connection refused' >&2; exit 255", "ssh"]);
    c
}

/// Every ssh this process started, ended (by its pid).
fn end_ssh_children() {
    let ssh = crate::machines::ssh_program();
    let Ok(out) = Command::new("/bin/ps").args(["-axo", "pid=,ppid=,command="]).output() else { return };
    let me = std::process::id() as i32;
    for (pid, ppid, cmd) in String::from_utf8_lossy(&out.stdout).lines().filter_map(crate::machines::ps_line) {
        if ppid == me && cmd.starts_with(&*ssh.to_string_lossy()) {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_dropped_line_is_ssh_refusing_whatever_it_is_asked() {
        let out = super::refused().args(["-o", "BatchMode=yes", "-T", "box", "echo hi"]).output().unwrap();
        assert_eq!(out.status.code(), Some(255));
        assert!(String::from_utf8_lossy(&out.stderr).contains("Connection refused"), "what machines::not_taken reads as a trip never started");
        assert!(out.stdout.is_empty());
        assert!(!super::down(), "the line is up until the knob's thread drops it");
    }
}
