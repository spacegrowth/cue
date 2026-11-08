//! One hosted agent (Claude Code, Pi): started from a `Spec`, it reports everything as `HostedEvent`s to the
//! function it was started with, on a thread of its own, until it exits. Your messages, answers and interrupts
//! go to its input. What its lines mean, and how Cue's are spelled, is its `Protocol`'s (protocol.rs); this is
//! the process.
//!
//! Its process runs in a group of its own: closing the session closes its input (the agent finishes and exits),
//! then, if it's still there after a grace, ends the group, so nothing it started outlives it. When the agent
//! exits by itself while something it started still holds its output open (a background command), that
//! leftover is ended too: the session is over when the agent is, not when its last child lets go.

use super::protocol::Protocol;
use super::{launch, Answer, HostedEvent, Spec};
use std::collections::VecDeque;
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
/// How much of its error output is kept, for saying why it exited.
const LAST_WORDS: usize = 20;

type Sink = Arc<dyn Fn(HostedEvent) + Send + Sync>;

pub struct Session {
    shared: Arc<Shared>,
}

struct Shared {
    pgid: i32,
    /// Which agent, for saying so.
    name: &'static str,
    /// None once the session is closed.
    input: Mutex<Option<ChildStdin>>,
    /// What its lines mean. Never held while writing to it.
    protocol: Mutex<Box<dyn Protocol>>,
    last_words: Mutex<VecDeque<String>>,
    /// Its output has ended.
    ended: AtomicBool,
    /// Its output has ended and its exit is known: the session is over.
    gone: AtomicBool,
}

impl Session {
    /// Start the agent for `spec`; everything it does goes to `sink`, ending with `Exited`.
    pub fn start(spec: &Spec, sink: impl Fn(HostedEvent) + Send + Sync + 'static) -> Result<Session, String> {
        let name = spec.agent.name();
        let mut child = launch::command(spec)?.spawn().map_err(|e| format!("couldn't start {name}: {e}"))?;
        let (Some(input), Some(output), Some(errors)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            return Err(format!("{name} started without its input and output"));
        };
        let shared = Arc::new(Shared {
            pgid: child.id() as i32,
            name,
            input: Mutex::new(Some(input)),
            protocol: Mutex::new(spec.agent.protocol()),
            last_words: Mutex::new(VecDeque::new()),
            ended: AtomicBool::new(false),
            gone: AtomicBool::new(false),
        });
        let errors = keep_last_words(shared.clone(), errors);
        let exit = wait_for_exit(shared.clone(), child);
        let sink: Sink = Arc::new(sink);
        let reader = shared.clone();
        std::thread::spawn(move || reader.read(output, exit, errors, &sink));
        let session = Session { shared };
        for line in session.shared.speak(|p| p.opening()) {
            session.shared.write(&line)?;
        }
        Ok(session)
    }

    /// Its process (the leader of its process group): what the processes it starts descend from.
    pub fn pid(&self) -> i32 {
        self.shared.pgid
    }

    /// Your message. Taken at once when the agent is idle, at its next step when it's busy (`Taken` says when).
    pub fn send(&self, text: &str) -> Result<(), String> {
        let line = self.shared.speak(|p| p.message(text));
        self.shared.write(&line)
    }

    /// Your answer to the request `request_id` (an `Ask`'s).
    pub fn answer(&self, request_id: &str, a: &Answer) -> Result<(), String> {
        let line = self.shared.speak(|p| p.answer(request_id, a))?;
        self.shared.write(&line)
    }

    /// Stop what it's doing: the running tool stops and the turn ends.
    pub fn interrupt(&self) -> Result<(), String> {
        let line = self.shared.speak(|p| p.interrupt());
        self.shared.write(&line)
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
    fn read(&self, output: impl Read, exit: JoinHandle<Option<i32>>, errors: JoinHandle<()>, sink: &Sink) {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            let (events, replies) = self.speak(|p| p.read(&line));
            for r in replies {
                let _ = self.write(&r);
            }
            for e in events {
                sink(e);
            }
        }
        self.ended.store(true, Ordering::SeqCst);
        let code = exit.join().ok().flatten();
        let _ = errors.join();
        self.gone.store(true, Ordering::SeqCst);
        let during_turn = self.speak(|p| {
            p.gone();
            p.in_turn()
        });
        let last_words = crate::locking::lock(&self.last_words, "hosted session: last words").iter().cloned().collect::<Vec<_>>().join("\n");
        sink(HostedEvent::Exited { code, during_turn, last_words });
    }

    /// `f` with its protocol, let go before anything is written.
    fn speak<R>(&self, f: impl FnOnce(&mut dyn Protocol) -> R) -> R {
        f(crate::locking::lock(&self.protocol, "hosted session: protocol").as_mut())
    }

    fn write(&self, line: &str) -> Result<(), String> {
        let mut input = crate::locking::lock(&self.input, "hosted session: input");
        let Some(w) = input.as_mut() else { return Err("the session is closed".into()) };
        w.write_all(line.as_bytes()).and_then(|_| w.write_all(b"\n")).and_then(|_| w.flush()).map_err(|e| format!("{} isn't taking input: {e}", self.name))
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

/// Its error output read to the end on a thread of its own, the last LAST_WORDS lines kept.
fn keep_last_words(shared: Arc<Shared>, errors: impl Read + Send + 'static) -> JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(errors).lines().map_while(Result::ok) {
            let mut kept = crate::locking::lock(&shared.last_words, "hosted session: last words");
            if kept.len() == LAST_WORDS {
                kept.pop_front();
            }
            kept.push_back(line);
        }
    })
}
