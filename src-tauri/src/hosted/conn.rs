//! How Cue reaches a cue-host (host_link.rs): its socket on this Mac, or `cue-host connect` on another machine over
//! SSH, whose input and output carry the same lines to the host's socket there. Either way a `Conn`: lines to write,
//! lines to read, and a way to end it.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One connection to a host.
pub struct Conn {
    pub read: Box<dyn Read + Send>,
    pub write: Box<dyn Write + Send>,
    /// Ends it (its reader then sees the end).
    pub end: Box<dyn Fn() + Send + Sync>,
}

/// Where a host is, and how a connection to it is made (again, after one drops).
pub enum Dial {
    /// The host in `home` on this Mac. `serve` starts one when none runs; empty: never started (an older host Cue
    /// only lets finish).
    Local { home: PathBuf, serve: Vec<String> },
    /// The host on a machine: `home` there (for saying so), and the ssh command that runs `connect` there (which
    /// starts the host if none runs), made anew for each connection. `once`: an older build's host Cue only lets
    /// finish, not reached again once its connection drops.
    Machine { home: PathBuf, command: Box<dyn Fn() -> Command + Send + Sync>, once: bool },
}

impl Dial {
    pub fn home(&self) -> &std::path::Path {
        match self {
            Dial::Local { home, .. } | Dial::Machine { home, .. } => home,
        }
    }

    /// Whether a connection that drops is made again: not to an older host on this Mac.
    pub fn comes_back(&self) -> bool {
        match self {
            Dial::Local { serve, .. } => !serve.is_empty(),
            Dial::Machine { once, .. } => !once,
        }
    }

    pub fn open(&self) -> std::io::Result<Conn> {
        match self {
            Dial::Local { home, serve } => {
                // A host Cue only lets finish is reached where it is, never started.
                let s = if serve.is_empty() { UnixStream::connect(cue_host::daemon::socket(home))? } else { cue_host::daemon::connect(home, serve)? };
                let (read, write, ender) = (s.try_clone()?, s.try_clone()?, s);
                Ok(Conn { read: Box::new(read), write: Box::new(write), end: Box::new(move || drop(ender.shutdown(std::net::Shutdown::Both))) })
            }
            Dial::Machine { home, command, .. } => machine(home, command()),
        }
    }
}

/// `connect` on a machine, as ssh runs it: its output is the host's lines, its input goes to the host. What ssh or the
/// host say on their error output is logged (why a machine can't be reached, say). Ended by a signal to ssh, never
/// once it has exited (its pid could be another process's by then).
fn machine(home: &std::path::Path, mut c: Command) -> std::io::Result<Conn> {
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (Some(write), Some(read), Some(err)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        return Err(std::io::Error::other("ssh gave no input or output"));
    };
    let where_ = home.display().to_string();
    std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(Result::ok) {
            eprintln!("cue: cue-host at {where_}: {l}");
        }
    });
    let pid = child.id() as i32;
    let gone = Arc::new(AtomicBool::new(false));
    let waited = gone.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        waited.store(true, Ordering::SeqCst);
    });
    let end = Box::new(move || {
        if !gone.load(Ordering::SeqCst) {
            // SAFETY: a signal to the ssh this connection started, which hasn't been waited for yet.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    });
    Ok(Conn { read: Box::new(read), write: Box::new(write), end })
}
