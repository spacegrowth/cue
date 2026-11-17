//! A session's child: started in a process group of its own (so ending it reaches everything it started), its
//! output and error output read line by line on threads of their own, and its exit waited for. Each goes to the
//! function it was started with; the child's input is written to directly.
//!
//! Closing it closes its input (an agent finishes and exits), then, if it's still there after a grace, ends its
//! group. When it exits by itself while something it started still holds its output open (a background command),
//! that group is ended too: the session is over when the child is, not when its last child lets go.
//!
//! On macOS the child is started as its own "responsible" process (as Terminal does for what runs in it), so what
//! it starts is checked against its own permissions, not Cue's: a test run opening many browsers would otherwise
//! have macOS re-check Cue's signature for each, and that can stall the window server.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the child gets to exit on its own once its input is closed, then once asked to end.
pub const CLOSE_GRACE: Duration = Duration::from_secs(3);
pub const END_GRACE: Duration = Duration::from_secs(2);
/// How long its output may stay open after the child exited (its last lines on their way) before whatever of its
/// group holds it is ended.
const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// What a child says: a line of output or of error output, then (once, last) its exit.
pub enum Said {
    Out(String),
    Err(String),
    Exited(Option<i32>),
}

pub struct Child {
    pub pid: i32,
    input: Mutex<Option<File>>,
    gone: Arc<AtomicBool>,
}

impl Child {
    /// `argv` in `cwd` with exactly `env`; everything it says goes to `said`, from threads of its own, ending with
    /// `Exited` once its output is read to the end.
    pub fn start(argv: &[String], env: &[(String, String)], cwd: &str, said: impl Fn(Said) + Send + Sync + 'static) -> std::io::Result<Child> {
        if argv.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "nothing to run"));
        }
        let Spawned { pid, input, output, errors } = spawn(argv, env, cwd)?;
        let (input, output, errors) = (Some(input), Some(output), Some(errors));
        let said = Arc::new(said);
        let gone = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let errors = errors.map(|e| {
            let said = said.clone();
            std::thread::spawn(move || lines(e, |l| said(Said::Err(l))))
        });
        let exit = {
            let ended = ended.clone();
            std::thread::spawn(move || {
                let code = wait(pid);
                // Its output still open a moment after it exited: something it started holds it. That group is ended
                // so the session can end (only while the output is open: an empty group could be another's by now).
                if !wait_for(&ended, DRAIN_GRACE) {
                    signal(pid, libc::SIGTERM);
                    if !wait_for(&ended, END_GRACE) {
                        signal(pid, libc::SIGKILL);
                    }
                }
                code
            })
        };
        {
            let (said, gone) = (said.clone(), gone.clone());
            std::thread::spawn(move || {
                if let Some(o) = output {
                    lines(o, |l| said(Said::Out(l)));
                }
                ended.store(true, Ordering::SeqCst);
                let code = exit.join().ok().flatten();
                if let Some(e) = errors {
                    let _ = e.join();
                }
                gone.store(true, Ordering::SeqCst);
                said(Said::Exited(code));
            });
        }
        Ok(Child { pid, input: Mutex::new(input), gone })
    }

    /// A line to its input.
    pub fn write(&self, line: &str) -> std::io::Result<()> {
        let mut input = self.input.lock().unwrap_or_else(|e| e.into_inner());
        let w = input.as_mut().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "its input is closed"))?;
        w.write_all(line.as_bytes())?;
        w.write_all(b"\n")?;
        w.flush()
    }

    pub fn gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    /// Close its input so it finishes and exits; if it hasn't after a grace, end its group. Returns once it's gone
    /// (or was ended). Closing twice does nothing more.
    pub fn close(&self) {
        self.input.lock().unwrap_or_else(|e| e.into_inner()).take();
        if wait_for(&self.gone, CLOSE_GRACE) {
            return;
        }
        signal(self.pid, libc::SIGTERM);
        if !wait_for(&self.gone, END_GRACE) {
            signal(self.pid, libc::SIGKILL);
            wait_for(&self.gone, END_GRACE);
        }
    }
}

/// A started child: its pid and its input, output and error output.
struct Spawned {
    pid: i32,
    input: File,
    output: File,
    errors: File,
}

/// `argv` in `cwd` with exactly `env`, in a process group of its own, its three streams piped.
#[cfg(not(target_os = "macos"))]
fn spawn(argv: &[String], env: &[(String, String)], cwd: &str) -> std::io::Result<Spawned> {
    use std::os::fd::OwnedFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..]).current_dir(cwd).env_clear().envs(env.iter().map(|(k, v)| (k, v))).process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn()?;
    let piped = |o: Option<OwnedFd>| o.map(File::from).ok_or_else(|| std::io::Error::other("a stream wasn't piped"));
    Ok(Spawned {
        pid: child.id() as i32,
        input: piped(child.stdin.take().map(OwnedFd::from))?,
        output: piped(child.stdout.take().map(OwnedFd::from))?,
        errors: piped(child.stderr.take().map(OwnedFd::from))?,
    })
    // `child` is dropped without waiting: `wait` reaps it by its pid.
}

/// The same through posix_spawn, with the child made responsible for itself (what Rust's Command can't ask for).
#[cfg(target_os = "macos")]
fn spawn(argv: &[String], env: &[(String, String)], cwd: &str) -> std::io::Result<Spawned> {
    use std::ffi::CString;
    use std::os::fd::{FromRawFd, OwnedFd};
    extern "C" {
        // libSystem (macOS 10.14+): the child is its own responsible process, not charged to whoever started it.
        fn responsibility_spawnattrs_setdisclaim(attr: *mut libc::posix_spawnattr_t, disclaim: libc::c_int) -> libc::c_int;
    }
    let cstr = |s: &str| CString::new(s).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a NUL in what to run"));
    let check = |r: libc::c_int| if r == 0 { Ok(()) } else { Err(std::io::Error::from_raw_os_error(r)) };
    let program = cstr(&find(&argv[0], env, cwd))?;
    let args = argv.iter().map(|a| cstr(a)).collect::<std::io::Result<Vec<_>>>()?;
    let vars = env.iter().map(|(k, v)| cstr(&format!("{k}={v}"))).collect::<std::io::Result<Vec<_>>>()?;
    let dir = cstr(cwd)?;
    let mut arg_ptrs: Vec<*mut libc::c_char> = args.iter().map(|a| a.as_ptr() as *mut _).collect();
    arg_ptrs.push(std::ptr::null_mut());
    let mut var_ptrs: Vec<*mut libc::c_char> = vars.iter().map(|a| a.as_ptr() as *mut _).collect();
    var_ptrs.push(std::ptr::null_mut());
    // Each pipe as (read end, write end), both closed on exec; the child's end is dup2'd onto 0, 1 or 2.
    let pipe = || -> std::io::Result<(OwnedFd, OwnedFd)> {
        let mut fds = [0; 2];
        // SAFETY: pipe fills two descriptors we then own; fcntl only sets a flag on them.
        unsafe {
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC);
            Ok((OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])))
        }
    };
    let (in_r, in_w) = pipe()?;
    let (out_r, out_w) = pipe()?;
    let (err_r, err_w) = pipe()?;
    use std::os::fd::AsRawFd;
    let mut pid: libc::pid_t = 0;
    // SAFETY: the attributes and file actions are initialised before use and destroyed after; every pointer handed
    // to posix_spawn points into a CString or Vec alive until it returns.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::mem::zeroed();
        let mut acts: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        check(libc::posix_spawnattr_init(&mut attr))?;
        if let Err(e) = check(libc::posix_spawn_file_actions_init(&mut acts)) {
            libc::posix_spawnattr_destroy(&mut attr);
            return Err(e);
        }
        let r = (|| {
            // A group of its own; nothing open but its three streams; signals as a fresh process has them
            // (SIGPIPE back to default, as Rust's Command does).
            let flags = libc::POSIX_SPAWN_SETPGROUP | libc::POSIX_SPAWN_CLOEXEC_DEFAULT | libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK;
            check(libc::posix_spawnattr_setflags(&mut attr, flags as libc::c_short))?;
            check(libc::posix_spawnattr_setpgroup(&mut attr, 0))?;
            let mut sigs: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut sigs);
            check(libc::posix_spawnattr_setsigmask(&mut attr, &sigs))?;
            libc::sigaddset(&mut sigs, libc::SIGPIPE);
            check(libc::posix_spawnattr_setsigdefault(&mut attr, &sigs))?;
            // Best effort: on a macOS that refuses it, the child is simply charged to us as before.
            let _ = responsibility_spawnattrs_setdisclaim(&mut attr, 1);
            check(libc::posix_spawn_file_actions_adddup2(&mut acts, in_r.as_raw_fd(), 0))?;
            check(libc::posix_spawn_file_actions_adddup2(&mut acts, out_w.as_raw_fd(), 1))?;
            check(libc::posix_spawn_file_actions_adddup2(&mut acts, err_w.as_raw_fd(), 2))?;
            check(libc::posix_spawn_file_actions_addchdir_np(&mut acts, dir.as_ptr()))?;
            check(libc::posix_spawn(&mut pid, program.as_ptr(), &acts, &attr, arg_ptrs.as_ptr(), var_ptrs.as_ptr()))
        })();
        libc::posix_spawn_file_actions_destroy(&mut acts);
        libc::posix_spawnattr_destroy(&mut attr);
        r?;
    }
    drop((in_r, out_w, err_w));
    Ok(Spawned { pid, input: File::from(in_w), output: File::from(out_r), errors: File::from(err_r) })
}

/// Where `program` is: as given when it has a `/` (a relative one taken from `cwd`), else the first executable of
/// that name on the child's own PATH (or ours when it has none), as Command looks it up.
#[cfg(target_os = "macos")]
fn find(program: &str, env: &[(String, String)], cwd: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    if program.contains('/') {
        return if program.starts_with('/') { program.to_string() } else { format!("{cwd}/{program}") };
    }
    let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).or_else(|| std::env::var("PATH").ok()).unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| std::path::Path::new(d).join(program))
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

/// Wait for `pid` to exit: its exit code, or None when a signal ended it.
fn wait(pid: i32) -> Option<i32> {
    let mut status = 0;
    loop {
        // SAFETY: waitpid only writes the status.
        let r = unsafe { libc::waitpid(pid, &mut status, 0) };
        if r == pid {
            return libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));
        }
        if r < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return None;
        }
    }
}

/// Each line of `r`, as text (anything that isn't UTF-8 made so), to `each`, until it ends.
fn lines(r: impl Read, mut each: impl FnMut(String)) {
    let mut reader = BufReader::new(r);
    let mut buf = Vec::new();
    while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
        while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            buf.pop();
        }
        each(String::from_utf8_lossy(&buf).into_owned());
        buf.clear();
    }
}

/// `sig` to the process group `pgid` leads.
fn signal(pgid: i32, sig: i32) {
    // SAFETY: killpg only sends a signal.
    unsafe {
        libc::killpg(pgid, sig);
    }
}

/// Wait (on the wall clock: a bound on a real process) up to `limit` for `flag`.
fn wait_for(flag: &AtomicBool, limit: Duration) -> bool {
    let end = Instant::now() + limit;
    while !flag.load(Ordering::SeqCst) {
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    extern "C" {
        fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
    }

    /// A child is responsible for itself, not charged to the host that started it.
    #[test]
    fn a_child_is_its_own_responsible_process() {
        let child = Child::start(&["sleep".into(), "5".into()], &[("PATH".into(), "/bin:/usr/bin".into())], "/", |_| {}).unwrap();
        // SAFETY: only asks about a pid.
        let responsible = unsafe { responsibility_get_pid_responsible_for_pid(child.pid) };
        child.close();
        assert_eq!(responsible, child.pid);
    }
}
