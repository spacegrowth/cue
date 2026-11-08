//! One hosted agent (Claude Code, Pi): started from a `Spec`, it reports everything as `HostedEvent`s to the
//! function it was started with, on a thread of its own, until it exits. Your messages, answers and interrupts
//! go to its input. What its lines mean, and how Cue's are spelled, is its `Driver`'s (driver.rs); this is the
//! process, run as Cue's own child (cue-host runs it otherwise: hosts.rs).
//!
//! Its process runs in a group of its own: closing the session closes its input (the agent finishes and exits),
//! then, if it's still there after a grace, ends the group, so nothing it started outlives it. When the agent
//! exits by itself while something it started still holds its output open (a background command), that
//! leftover is ended too: the session is over when the agent is, not when its last child lets go.

use super::driver::{Driver, Sink};
use super::{launch, Answer, HostedEvent, Spec};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long the agent gets to finish and exit on its own once its input is closed, then once asked to end.
const CLOSE_GRACE: Duration = Duration::from_secs(3);
const END_GRACE: Duration = Duration::from_secs(2);
/// How long its output may stay open after the agent itself exited (its last lines still on their way) before
/// whatever of its group holds it is ended.
const DRAIN_GRACE: Duration = Duration::from_secs(1);
/// How long an agent that opens its conversation itself (Codex) gets to say it's open before starting fails.
pub(super) const OPEN_GRACE: Duration = Duration::from_secs(30);

pub struct Session {
    shared: Arc<Shared>,
}

struct Shared {
    pgid: i32,
    /// Its lines made sense of, and its events reported.
    driver: Driver,
    /// None once the session is closed.
    input: Mutex<Option<ChildStdin>>,
    /// Its output has ended.
    ended: AtomicBool,
    /// Its output has ended and its exit is known: the session is over.
    gone: AtomicBool,
}

impl Session {
    /// Start the agent for `spec`; everything it does goes to `sink`, ending with `Exited`.
    pub fn start(spec: &Spec, sink: impl Fn(&str, HostedEvent) + Send + Sync + 'static) -> Result<Session, String> {
        let name = spec.agent.name();
        let mut child = launch::command(spec)?.spawn().map_err(|e| format!("couldn't start {name}: {e}"))?;
        let (Some(input), Some(output), Some(errors)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            return Err(format!("{name} started without its input and output"));
        };
        let sink: Sink = Arc::new(sink);
        let shared = Arc::new(Shared { pgid: child.id() as i32, driver: Driver::new(spec, sink), input: Mutex::new(Some(input)), ended: AtomicBool::new(false), gone: AtomicBool::new(false) });
        let errors = keep_last_words(shared.clone(), errors);
        let exit = wait_for_exit(shared.clone(), child);
        let reader = shared.clone();
        std::thread::spawn(move || reader.read(output, exit, errors));
        let session = Session { shared };
        session.shared.write_lines(&session.shared.driver.opening())?;
        wait_open(&session.shared.driver, || session.gone()).inspect_err(|_| session.close())?;
        Ok(session)
    }

    /// The conversation's id: the one it was started under, or the one the agent picked.
    pub fn session_id(&self) -> String {
        self.shared.driver.session_id()
    }

    /// Where the agent keeps the conversation, when it says so.
    pub fn transcript(&self) -> Option<String> {
        self.shared.driver.transcript()
    }

    /// Its process (the leader of its process group): what the processes it starts descend from.
    pub fn pid(&self) -> i32 {
        self.shared.pgid
    }

    /// Your message. Taken at once when the agent is idle, at its next step when it's busy (`Taken` says when).
    pub fn send(&self, text: &str) -> Result<(), String> {
        self.shared.write_lines(&self.shared.driver.message(text))
    }

    /// Your answer to the request `request_id` (an `Ask`'s).
    pub fn answer(&self, request_id: &str, a: &Answer) -> Result<(), String> {
        self.shared.write_lines(&self.shared.driver.answer(request_id, a)?)
    }

    /// Stop what it's doing: the running tool stops and the turn ends.
    pub fn interrupt(&self) -> Result<(), String> {
        self.shared.write_lines(&self.shared.driver.interrupt())
    }

    /// Whether its process has exited.
    pub fn gone(&self) -> bool {
        self.shared.gone.load(Ordering::SeqCst)
    }

    /// Close its input so the agent finishes and exits; if it hasn't after a grace, end its process group.
    /// Returns once it's gone (or was ended). Closing twice does nothing more.
    pub fn close(&self) {
        crate::locking::lock(&self.shared.input, "hosted session: input").take();
        if self.shared.wait_gone(CLOSE_GRACE) {
            return;
        }
        self.shared.signal(libc::SIGTERM);
        if !self.shared.wait_gone(END_GRACE) {
            self.shared.signal(libc::SIGKILL);
            self.shared.wait_gone(END_GRACE);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

impl Shared {
    /// Its output, line by line, until it ends; then its exit, after its error output is read to the end.
    fn read(&self, output: impl Read, exit: JoinHandle<Option<i32>>, errors: JoinHandle<()>) {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            for r in self.driver.read(&line) {
                let _ = self.write(&r);
            }
        }
        self.ended.store(true, Ordering::SeqCst);
        let code = exit.join().ok().flatten();
        let _ = errors.join();
        self.gone.store(true, Ordering::SeqCst);
        self.driver.exited(code);
    }

    fn write_lines(&self, lines: &[String]) -> Result<(), String> {
        lines.iter().try_for_each(|l| self.write(l))
    }

    fn write(&self, line: &str) -> Result<(), String> {
        let mut input = crate::locking::lock(&self.input, "hosted session: input");
        let Some(w) = input.as_mut() else { return Err("the session is closed".into()) };
        w.write_all(line.as_bytes()).and_then(|_| w.write_all(b"\n")).and_then(|_| w.flush()).map_err(|e| format!("{} isn't taking input: {e}", self.driver.name))
    }

    fn wait_gone(&self, limit: Duration) -> bool {
        wait_until(&self.gone, limit)
    }

    /// `sig` to its whole process group (the agent, and anything it started that's still in it).
    fn signal(&self, sig: i32) {
        // SAFETY: killpg only sends a signal; the group is the one this session's process leads.
        unsafe {
            libc::killpg(self.pgid, sig);
        }
    }
}

/// The agent's own exit, waited for on a thread of its own: its code. If its output is still open a moment later,
/// something it started holds it; that group is ended so the session can end. (Only while the output is open:
/// a group with nothing left in it could be another's by now.)
fn wait_for_exit(shared: Arc<Shared>, mut child: Child) -> JoinHandle<Option<i32>> {
    std::thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        if !wait_until(&shared.ended, DRAIN_GRACE) {
            shared.signal(libc::SIGTERM);
            if !wait_until(&shared.ended, END_GRACE) {
                shared.signal(libc::SIGKILL);
            }
        }
        code
    })
}

/// Wait (on the wall clock: a bound on a real process) up to `limit` for `flag`.
fn wait_until(flag: &AtomicBool, limit: Duration) -> bool {
    let end = Instant::now() + limit;
    while !flag.load(Ordering::SeqCst) {
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    true
}

/// Its error output read to the end on a thread of its own, its last lines kept.
fn keep_last_words(shared: Arc<Shared>, errors: impl Read + Send + 'static) -> JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(errors).lines().map_while(Result::ok) {
            shared.driver.error_line(line);
        }
    })
}

/// Until its conversation is open, for an agent that opens it itself (Codex): so its id is known when starting
/// returns. Err with what it said, once it's `gone` or OPEN_GRACE has gone by.
pub(super) fn wait_open(driver: &Driver, gone: impl Fn() -> bool) -> Result<(), String> {
    let end = Instant::now() + OPEN_GRACE;
    while !driver.ready() {
        if gone() || Instant::now() >= end {
            let words = driver.last_words();
            return Err(format!("{} didn't open the conversation{}", driver.name, if words.is_empty() { String::new() } else { format!(": {words}") }));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}
