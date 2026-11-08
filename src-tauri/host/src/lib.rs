//! cue-host: keeps Cue's hosted agents running whether or not Cue is, and keeps everything they say until Cue
//! reads it (docs/dev/cue-agent-host.md). It knows nothing of Claude Code, Pi or Codex: it starts a command,
//! writes lines to it, and keeps the lines it writes back, numbered. What they mean is Cue's.
//!
//! - `proto`: what Cue and the host say to each other.
//! - `log`: a session's numbered records, on disk.
//! - `child`: a session's process.
//! - `server`: the sessions, and the clients attached to them.
//! - `daemon`: reaching the host, starting it if it isn't running.

pub mod child;
pub mod daemon;
pub mod log;
pub mod proto;
pub mod server;

use std::path::PathBuf;

/// `cue-host serve|connect [--only-running]|version [--home DIR]` (Cue runs it as `cue host …`). `serve`: the program and arguments that
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
