//! Machines over SSH, stood in for: while a `FakeSsh` lives, every `ssh` Cue runs (machines::ssh) is a script
//! that plays the machine it's pointed at. A host is what the test says it is (`host`): a few lines of sh
//! given the command Cue runs there, its input on stdin, and a home of its own ($FAKE_HOME). A host the
//! test didn't name can't be reached: ssh's own refusal, exit 255. Nothing reaches a real machine, and
//! your ~/.ssh is never read. Every call is noted (`calls`).
//!
//! Make the World first: one FakeSsh at a time, after the World's own turns.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// A machine that runs what Cue sends it, for real, in its own home on this Mac: your login shell there is
/// /bin/sh. What Cue's scripts do there (a file written, a watcher's events) happens in $FAKE_HOME.
pub const RUNS_IT: &str = r#"mkdir -p "$FAKE_HOME" && HOME="$FAKE_HOME" SHELL=/bin/sh exec /bin/sh -c "$1""#;

/// The ssh every call runs: the host is the word after -T, the command the one after it.
const SHIM: &str = r#"#!/bin/sh
fake="$(dirname "$0")"
while [ $# -gt 0 ] && [ "$1" != -T ]; do shift; done
host=$2
cmd=$3
printf '%s\t%s\n' "$host" "$(printf '%s' "$cmd" | tr '\n\t' '  ' | cut -c1-2000)" >>"$fake/calls.log"
if [ ! -f "$fake/hosts/$host" ]; then
  echo "ssh: connect to host $host port 22: Connection refused" >&2
  exit 255
fi
FAKE_HOME="$fake/home/$host" exec /bin/sh "$fake/hosts/$host" "$cmd"
"#;

static PROGRAM: Mutex<Option<PathBuf>> = Mutex::new(None);
static TURN: Mutex<()> = Mutex::new(());

pub struct FakeSsh {
    dir: PathBuf,
    _turn: MutexGuard<'static, ()>,
}

impl FakeSsh {
    pub fn new() -> Self {
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cue-ssh-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("hosts")).unwrap();
        let program = dir.join("ssh");
        std::fs::write(&program, SHIM).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        *PROGRAM.lock().unwrap_or_else(|e| e.into_inner()) = Some(program);
        FakeSsh { dir, _turn: turn }
    }

    /// The ssh Cue runs instead of the system's, while a FakeSsh lives.
    pub fn program() -> Option<PathBuf> {
        PROGRAM.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// `host` plays `script` from now on: sh, given the command Cue runs there as $1.
    pub fn host(&self, host: &str, script: &str) -> &Self {
        std::fs::write(self.dir.join("hosts").join(host), script).unwrap();
        self
    }

    /// `host` can't be reached any more.
    pub fn unplug(&self, host: &str) {
        let _ = std::fs::remove_file(self.dir.join("hosts").join(host));
    }

    /// `host`'s home, where what ran there left its files.
    pub fn home(&self, host: &str) -> PathBuf {
        self.dir.join("home").join(host)
    }

    /// A folder of the test's beside the machines, for a host's script to leave things in or wait on.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every call so far, as (host, the command run there, its newlines as spaces).
    pub fn calls(&self) -> Vec<(String, String)> {
        let log = std::fs::read_to_string(self.dir.join("calls.log")).unwrap_or_default();
        log.lines().filter_map(|l| l.split_once('\t')).map(|(h, c)| (h.to_string(), c.to_string())).collect()
    }

    /// How many calls to `host` ran a command with `part` in it.
    pub fn calls_with(&self, host: &str, part: &str) -> usize {
        self.calls().iter().filter(|(h, c)| h == host && c.contains(part)).count()
    }
}

impl Drop for FakeSsh {
    fn drop(&mut self) {
        *PROGRAM.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Wait for `what` to come true: the work behind it runs on threads and processes of its own. A panic
/// naming `what` after 10 s.
#[track_caller]
pub fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        assert!(std::time::Instant::now() < end, "waited 10 s for: {what}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
