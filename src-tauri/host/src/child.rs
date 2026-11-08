//! A session's child: started in a process group of its own (so ending it reaches everything it started), its
//! output and error output read line by line on threads of their own, and its exit waited for. Each goes to the
//! function it was started with; the child's input is written to directly.
//!
//! Closing it closes its input (an agent finishes and exits), then, if it's still there after a grace, ends its
//! group. When it exits by itself while something it started still holds its output open (a background command),
//! that group is ended too: the session is over when the child is, not when its last child lets go.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{ChildStdin, Command, Stdio};
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
    input: Mutex<Option<ChildStdin>>,
    gone: Arc<AtomicBool>,
}

impl Child {
    /// `argv` in `cwd` with exactly `env`; everything it says goes to `said`, from threads of its own, ending with
    /// `Exited` once its output is read to the end.
    pub fn start(argv: &[String], env: &[(String, String)], cwd: &str, said: impl Fn(Said) + Send + Sync + 'static) -> std::io::Result<Child> {
        let (program, args) = argv.split_first().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "nothing to run"))?;
        let mut c = Command::new(program);
        c.args(args).current_dir(cwd).env_clear().envs(env.iter().map(|(k, v)| (k, v))).process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn()?;
        let pid = child.id() as i32;
        let (input, output, errors) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
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
                let code = child.wait().ok().and_then(|s| s.code());
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
