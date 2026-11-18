//! cue-host: keeps Cue's hosted agents running whether or not Cue is, and keeps everything they say until Cue
//! reads it (docs/dev/cue-agent-host.md). It knows nothing of Claude Code, Pi or Codex: it starts a command,
//! writes lines to it, and keeps the lines it writes back, numbered. What they mean is Cue's.
//!
//! - `proto`: what Cue and the host say to each other.
//! - `log`: a session's numbered records, on disk.
//! - `child`: a session's process.
//! - `server`: the sessions, and the clients attached to them.
//! - `daemon`: reaching the host, starting it if it isn't running.
//! - `tail`: a file copied to a client as it's written (a session's transcript, for Cue on another machine).

pub mod child;
pub mod daemon;
pub mod log;
pub mod proto;
pub mod server;
pub mod tail;

use std::path::PathBuf;

/// This process's open-files limit raised as far as the system lets it: macOS starts a program at 256, and a host
/// running dozens of agents (three pipes each) with Cue's own files reached it (#287: "Too many open files" when a
/// session started). macOS refuses a soft limit over OPEN_MAX (10240) even when the hard one is unlimited.
pub fn more_open_files() {
    // SAFETY: getrlimit/setrlimit only read and write the struct given.
    unsafe {
        let mut l: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut l) != 0 {
            return;
        }
        let want = l.rlim_max.min(10240);
        if l.rlim_cur < want {
            l.rlim_cur = want;
            libc::setrlimit(libc::RLIMIT_NOFILE, &l);
        }
    }
}

/// `cue-host serve|connect [--only-running]|version [--home DIR]` (Cue.app carries it; without it, Cue runs it as `cue host …`). `serve`: the program and arguments that
/// run `serve`, for `connect` to start one. Returns the exit code.
pub fn main_with(args: &[String], serve: impl Fn(&std::path::Path) -> Vec<String>) -> i32 {
    let home = args.windows(2).find(|w| w[0] == "--home").map(|w| PathBuf::from(&w[1])).unwrap_or_else(default_home);
    let done = match args.first().map(String::as_str) {
        Some("serve") => daemon::serve(&home),
        // `--only-running`: never starting one (Cue reaching an older build's host on a machine).
        Some("connect") if args.iter().any(|a| a == "--only-running") => daemon::pipe(&home, &[]),
        Some("connect") => daemon::pipe(&home, &serve(&home)),
        // Which build it is, so a copy on a machine can be checked against the one Cue carries.
        Some("version") => {
            println!("{} {}", proto::VERSION, proto::BUILD);
            Ok(())
        }
        _ => {
            eprintln!("usage: cue-host serve|connect [--only-running]|version [--home DIR]");
            return 2;
        }
    };
    match done {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cue-host: {e}");
            1
        }
    }
}

/// `~/.cue/host`: its home on a machine.
pub fn default_home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cue/host")
}
