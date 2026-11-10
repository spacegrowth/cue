//! Reaching the host, starting it when it isn't running. The host's home (Cue's data folder `host/` on this Mac,
//! `~/.cue/host` on a machine) holds its socket, its pid, its lock and its log.
//!
//! `connect` connects to the socket; when nothing answers it takes the lock, checks again (another may have just
//! started one), and starts `serve` on its own: in a session of its own, its input closed and its output to
//! `host.log`, so nothing that started it (Cue, an SSH session) waits on it or takes it down with it. Then it
//! waits for the socket.

use crate::server::{Host, IDLE};
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a host just started gets to put its socket up.
const STARTING: Duration = Duration::from_secs(5);

pub fn socket(home: &Path) -> PathBuf {
    home.join("host.sock")
}

/// Its home, made if new and kept to this user (0700).
fn make_home(home: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(home)?;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))
}

/// A connection to the host in `home`, starting it first (with `serve`: the program and arguments that run
/// `serve` for `home`) when none runs.
pub fn connect(home: &Path, serve: &[String]) -> std::io::Result<UnixStream> {
    if let Ok(s) = UnixStream::connect(socket(home)) {
        return Ok(s);
    }
    make_home(home)?;
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(home.join("host.lock"))?;
    // SAFETY: flock on a file this process holds open; released when it's closed.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if let Ok(s) = UnixStream::connect(socket(home)) {
        return Ok(s);
    }
    start(home, serve)?;
    let until = Instant::now() + STARTING;
    loop {
        match UnixStream::connect(socket(home)) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= until => return Err(std::io::Error::new(e.kind(), format!("the host didn't start (see {}): {e}", home.join("host.log").display()))),
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// `serve`, on its own: a session of its own (setsid), its input closed, its output to host.log.
fn start(home: &Path, serve: &[String]) -> std::io::Result<()> {
    let (program, args) = serve.split_first().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no host program"))?;
    let log = OpenOptions::new().create(true).append(true).open(home.join("host.log"))?;
    let mut c = Command::new(program);
    c.args(args).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    // SAFETY: only async-signal-safe calls between fork and exec. Forked once more so the host isn't this process's
    // child (nothing here waits for it, and it never becomes a zombie of whoever started it), then a session of its
    // own so it outlives the terminal or SSH session that started it.
    unsafe {
        c.pre_exec(|| {
            match libc::fork() {
                -1 => return Err(std::io::Error::last_os_error()),
                0 => {}
                _ => libc::_exit(0),
            }
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = c.spawn()?;
    // The one forked in between exits at once.
    let _ = child.wait();
    Ok(())
}

/// The host in `home`, serving until it's been idle for a while. Refuses to run beside another one that answers.
pub fn serve(home: &Path) -> std::io::Result<()> {
    make_home(home)?;
    let sock = socket(home);
    if UnixStream::connect(&sock).is_ok() {
        return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "a host is already running here"));
    }
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock)?;
    std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
    std::fs::write(home.join("host.pid"), std::process::id().to_string())?;
    eprintln!("cue-host {}: serving {} (pid {})", crate::proto::VERSION, sock.display(), std::process::id());
    let host = Host::new(home, crate::log::LIMIT);
    host.serve(listener, &sock, IDLE);
    eprintln!("cue-host: idle, exiting");
    Ok(())
}

/// `connect` for a client that talks through this process's input and output (over SSH): bytes copied both ways
/// until either side ends.
pub fn pipe(home: &Path, serve: &[String]) -> std::io::Result<()> {
    // No `serve`: only a host already running is reached (an older one, let finish what it runs).
    let s = if serve.is_empty() { UnixStream::connect(socket(home))? } else { connect(home, serve)? };
    let mut to_host = s.try_clone()?;
    let mut from_host = s;
    std::thread::spawn(move || {
        let _ = pump(&mut std::io::stdin().lock(), &mut to_host);
        let _ = to_host.shutdown(std::net::Shutdown::Write);
    });
    pump(&mut from_host, &mut std::io::stdout().lock())
}

/// Bytes from `from` to `to` as they come, until `from` ends. A plain read and write, never `std::io::copy`: on Linux
/// that splices a socket into a pipe, and the kernel holds the pipe's lock while it waits for the socket, so sshd,
/// reading the other end of that pipe on its one thread, waits on the lock and stops passing your input on as well:
/// nothing moves either way.
fn pump(from: &mut impl std::io::Read, to: &mut impl std::io::Write) -> std::io::Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        to.flush()?;
    }
}
