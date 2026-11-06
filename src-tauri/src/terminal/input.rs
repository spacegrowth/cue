//! What's typed into a built-in terminal, on its way into its pseudo-terminal. One thread per terminal
//! writes it, in the order it came, so no caller ever writes to the pty itself: a client that stopped
//! reading (a tmux client stopped with ^Z or SIGSTOP, ssh on a dead line) fills the pty's buffer, and a
//! plain write would then wait for good, holding whoever called it (once that was the window's thread).
//!
//! A write that isn't taken within `BOUND` comes back as an error instead. While the terminal's thread
//! has been stuck that long, the next write is refused at once rather than queued behind it, so typing
//! into a stalled terminal costs nothing and nothing typed is reordered: what was refused never went in,
//! what went in goes in order (a write that timed out still goes, if the client reads again).
//!
//! Why a thread and not a non-blocking descriptor: the pty's reader is a dup of the same descriptor
//! (portable-pty), so O_NONBLOCK on one would turn the reader's blocking read into a busy error too.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// How long one write may take before its caller hears it didn't go.
pub(crate) const BOUND: Duration = Duration::from_secs(2);

/// Writes waiting behind the one being written: past this the terminal is clearly not reading.
const QUEUE: usize = 64;

/// One terminal's input. Cloned out of the open terminals' lock so the write happens with it released.
#[derive(Clone)]
pub(crate) struct Input {
    jobs: SyncSender<Job>,
    /// When the write in progress started (`clock`), 0 while none is.
    busy_since: Arc<AtomicU64>,
}

struct Job {
    bytes: Vec<u8>,
    done: mpsc::Sender<std::io::Result<()>>,
}

impl Input {
    /// `w` written to from a thread of its own (named after `sid`), until every clone of this is gone.
    pub(crate) fn new(sid: &str, w: Box<dyn Write + Send>) -> Result<Input, String> {
        let (jobs, rx) = mpsc::sync_channel::<Job>(QUEUE);
        let busy_since = Arc::new(AtomicU64::new(0));
        let busy = busy_since.clone();
        std::thread::Builder::new().name(format!("cue-term-input {sid}")).spawn(move || pump(w, rx, busy)).map_err(|e| e.to_string())?;
        Ok(Input { jobs, busy_since })
    }

    /// `data` into the terminal, in order after every earlier write; an error after `BOUND` at most.
    pub(crate) fn write(&self, data: &[u8]) -> Result<(), String> {
        self.write_within(data, BOUND)
    }

    fn write_within(&self, data: &[u8], bound: Duration) -> Result<(), String> {
        if self.stuck_for() >= bound {
            return Err(stalled(self.stuck_for()));
        }
        let (done, answer) = mpsc::channel();
        match self.jobs.try_send(Job { bytes: data.to_vec(), done }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(stalled(self.stuck_for())),
            Err(TrySendError::Disconnected(_)) => return Err("its terminal isn't open".into()),
        }
        match answer.recv_timeout(bound) {
            Ok(r) => r.map_err(|e| e.to_string()),
            Err(RecvTimeoutError::Timeout) => Err(stalled(self.stuck_for().max(bound))),
            Err(RecvTimeoutError::Disconnected) => Err("its terminal closed".into()),
        }
    }

    /// How long the write in progress has been going (zero when none is).
    fn stuck_for(&self) -> Duration {
        match self.busy_since.load(Ordering::Acquire) {
            0 => Duration::ZERO,
            since => Duration::from_millis(clock().saturating_sub(since)),
        }
    }
}

fn stalled(for_: Duration) -> String {
    format!("its terminal isn't taking input (nothing read for {:.1} s): its client may be stopped or its connection gone", for_.as_secs_f32())
}

/// The terminal's thread: each write in turn, its outcome to whoever is still waiting for it.
fn pump(mut w: Box<dyn Write + Send>, jobs: mpsc::Receiver<Job>, busy: Arc<AtomicU64>) {
    for job in jobs {
        busy.store(clock(), Ordering::Release);
        let r = w.write_all(&job.bytes).and_then(|_| w.flush());
        busy.store(0, Ordering::Release);
        let _ = job.done.send(r);
    }
}

/// Milliseconds since this process first asked, from 1 (0 means "not writing" in `busy_since`).
fn clock() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A client that stops reading: every write waits until `release` gets a message (or is dropped).
    /// What it was given, in order, lands in `got`.
    pub(crate) struct Stalled {
        pub(crate) release: mpsc::Receiver<()>,
        pub(crate) got: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Stalled {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let _ = self.release.recv();
            crate::locking::lock(&self.got, "stalled writer").extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_to_a_terminal_that_stopped_reading_fails_within_its_bound_and_the_next_fails_at_once() {
        let (go, release) = mpsc::channel();
        let got = Arc::new(Mutex::new(vec![]));
        let input = Input::new("stalled", Box::new(Stalled { release, got: got.clone() })).unwrap();
        let t0 = Instant::now();
        let err = input.write_within(b"a", Duration::from_millis(300)).unwrap_err();
        let took = t0.elapsed();
        assert!(took >= Duration::from_millis(300) && took < Duration::from_secs(2), "bounded: {took:?}");
        assert!(err.contains("isn't taking input"), "{err}");
        // Still stuck: refused at once, not queued behind it.
        let t1 = Instant::now();
        assert!(input.write_within(b"b", Duration::from_millis(300)).is_err());
        assert!(t1.elapsed() < Duration::from_millis(100), "no second wait: {:?}", t1.elapsed());
        // The client reads again: the first write goes, the refused one never does, and writing works again.
        go.send(()).unwrap();
        drop(go);
        input.write(b"c").unwrap();
        assert_eq!(*got.lock().unwrap(), b"ac".to_vec());
    }

    #[test]
    fn one_callers_writes_go_in_the_order_they_were_made() {
        let got = Arc::new(Mutex::new(vec![]));
        let (go, release) = mpsc::channel();
        drop(go); // never stalls
        let input = Input::new("order", Box::new(Stalled { release, got: got.clone() })).unwrap();
        let typed: Vec<String> = (0..500).map(|i| format!("<{i}>")).collect();
        for k in &typed {
            input.write(k.as_bytes()).unwrap();
        }
        assert_eq!(String::from_utf8(got.lock().unwrap().clone()).unwrap(), typed.concat());
    }
}
