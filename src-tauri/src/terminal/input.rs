//! What's typed into a built-in terminal, on its way into its pseudo-terminal. One thread per terminal
//! writes it, in the order it came, so no caller ever writes to the pty itself: a client that stopped
//! reading (a tmux client stopped with ^Z or SIGSTOP, ssh on a dead line) fills the pty's buffer, and a
//! plain write would then wait for good, holding whoever called it (once that was the window's thread).
//!
//! The caller waits without a thread of its own (an async wait on the write's answer). A write fails once
//! no bytes have gone in for `BOUND`: a big paste into a slow but reading connection keeps going.
//!
//! A write that fails (stuck, refused, or an error from the pty) latches the terminal's input: every later
//! write is refused at once until the window resumes it (`resume`), which also drops whatever was handed in
//! and hasn't gone yet. Without the latch, a write that failed could still go in later while the ones after
//! it were refused, and the shell would get the start and the end of what you typed with the middle
//! missing: half a command and its Enter.
//!
//! Why a thread and not a non-blocking descriptor: the pty's reader is a dup of the same descriptor
//! (portable-pty), so O_NONBLOCK on one would turn the reader's blocking read into a busy error too.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// How long a write may go with no bytes getting through before its caller hears it didn't go.
pub(crate) const BOUND: Duration = Duration::from_secs(2);

/// Writes waiting behind the one being written: past this the terminal is clearly not reading.
const QUEUE: usize = 64;

/// What one write call to the pty is given: progress is noted between these.
const CHUNK: usize = 1024;

const NOT_RESPONDING: &str = "its terminal isn't responding: what you type isn't being sent until you resume it";

/// One terminal's input. Cloned out of the open terminals' lock so the write happens with it released.
#[derive(Clone)]
pub(crate) struct Input {
    jobs: SyncSender<Job>,
    state: Arc<State>,
}

#[derive(Default)]
struct State {
    /// When the write in progress last got bytes through, or started (`clock`); 0 while none is in progress.
    progress_at: AtomicU64,
    /// A write didn't go: every later one is refused until `resume`.
    latched: AtomicBool,
    /// Counts resumes: a write handed in before the latest one, still waiting or part written, is dropped.
    epoch: AtomicU64,
}

struct Job {
    bytes: Vec<u8>,
    epoch: u64,
    done: oneshot::Sender<std::io::Result<()>>,
}

impl Input {
    /// `w` written to from a thread of its own (named after `sid`), until every clone of this is gone.
    pub(crate) fn new(sid: &str, w: Box<dyn Write + Send>) -> Result<Input, String> {
        let (jobs, rx) = mpsc::sync_channel::<Job>(QUEUE);
        let state = Arc::new(State::default());
        let st = state.clone();
        std::thread::Builder::new().name(format!("cue-term-input {sid}")).spawn(move || pump(w, rx, st)).map_err(|e| e.to_string())?;
        Ok(Input { jobs, state })
    }

    /// `data` into the terminal, in order after every earlier write. An error once nothing has gone in for
    /// `BOUND`, and at once while the input is latched.
    pub(crate) async fn write(&self, data: &[u8]) -> Result<(), String> {
        self.write_within(data, BOUND).await
    }

    async fn write_within(&self, data: &[u8], bound: Duration) -> Result<(), String> {
        if self.state.latched.load(Ordering::Acquire) {
            return Err(NOT_RESPONDING.into());
        }
        let (done, mut answer) = oneshot::channel();
        let job = Job { bytes: data.to_vec(), epoch: self.state.epoch.load(Ordering::Acquire), done };
        match self.jobs.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(self.latch()),
            Err(TrySendError::Disconnected(_)) => return Err("its terminal isn't open".into()),
        }
        loop {
            // Until this write would count as stuck, were nothing to go in meanwhile.
            let wait = bound.saturating_sub(self.stuck_for()).max(Duration::from_millis(10));
            match tokio::time::timeout(wait, &mut answer).await {
                Ok(Ok(Ok(()))) => return Ok(()),
                Ok(Ok(Err(e))) => {
                    self.latch();
                    return Err(e.to_string());
                }
                Ok(Err(_)) => return Err("its terminal closed".into()),
                Err(_) if self.stuck_for() >= bound => return Err(self.latch()),
                Err(_) => {}
            }
        }
    }

    /// You chose to go on (the window's Resume): what was handed in and hasn't gone is dropped, and what you
    /// type goes in again. Still stuck, the next write latches it again.
    pub(crate) fn resume(&self) {
        self.state.epoch.fetch_add(1, Ordering::AcqRel);
        self.state.latched.store(false, Ordering::Release);
    }

    fn latch(&self) -> String {
        self.state.latched.store(true, Ordering::Release);
        NOT_RESPONDING.into()
    }

    /// How long the write in progress has gone without bytes getting through (zero when none is).
    fn stuck_for(&self) -> Duration {
        match self.state.progress_at.load(Ordering::Acquire) {
            0 => Duration::ZERO,
            at => Duration::from_millis(clock().saturating_sub(at)),
        }
    }
}

/// The terminal's thread: each write in turn, its outcome to whoever is still waiting for it.
fn pump(mut w: Box<dyn Write + Send>, jobs: mpsc::Receiver<Job>, st: Arc<State>) {
    for job in jobs {
        st.progress_at.store(clock(), Ordering::Release);
        let r = write_job(&mut *w, &job, &st);
        st.progress_at.store(0, Ordering::Release);
        let _ = job.done.send(r);
    }
}

/// One write, a chunk at a time, each noted as progress; dropped (the rest of it too) once resumed past it.
fn write_job(w: &mut dyn Write, job: &Job, st: &State) -> std::io::Result<()> {
    for chunk in job.bytes.chunks(CHUNK) {
        if st.epoch.load(Ordering::Acquire) != job.epoch {
            return Err(std::io::Error::other("dropped: the terminal was resumed"));
        }
        w.write_all(chunk)?;
        st.progress_at.store(clock(), Ordering::Release);
    }
    w.flush()
}

/// Milliseconds since this process first asked, from 1 (0 means "not writing" in `progress_at`).
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

    /// A slow connection that still reads: `per_chunk` for each write call.
    struct Slow {
        per_chunk: Duration,
        got: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Slow {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(self.per_chunk);
            crate::locking::lock(&self.got, "slow writer").extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn got(g: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(crate::locking::lock(g, "got").clone()).unwrap()
    }

    #[tokio::test]
    async fn once_a_write_doesnt_go_nothing_typed_after_it_goes_until_resumed() {
        let (go, release) = mpsc::channel();
        let seen = Arc::new(Mutex::new(vec![]));
        let input = Input::new("stalled", Box::new(Stalled { release, got: seen.clone() })).unwrap();
        let bound = Duration::from_millis(300);
        let t0 = Instant::now();
        let err = input.write_within(b"rm -rf ./bu", bound).await.unwrap_err();
        let took = t0.elapsed();
        assert!(took >= bound && took < Duration::from_secs(2), "bounded: {took:?}");
        assert!(err.contains("isn't responding"), "{err}");
        // Typed while it's stuck: refused at once, not queued behind it.
        let t1 = Instant::now();
        assert!(input.write_within(b"ild", bound).await.is_err());
        assert!(t1.elapsed() < Duration::from_millis(100), "no second wait: {:?}", t1.elapsed());
        // The client reads again: what went in before the stall arrives, and nothing typed after it does,
        // not even what's typed now: no half line with the middle missing and its Enter after it.
        go.send(()).unwrap();
        drop(go);
        assert!(input.write_within(b"\r", bound).await.is_err(), "still latched after the client recovers");
        // You resume it: typing goes in again, after what went in before the stall, and nothing in between.
        input.resume();
        input.write(b"\x15echo ok\r").await.unwrap();
        assert_eq!(got(&seen), "rm -rf ./bu\x15echo ok\r");
    }

    #[tokio::test]
    async fn resuming_drops_what_was_handed_in_and_hasnt_gone() {
        let (go, release) = mpsc::channel();
        let seen = Arc::new(Mutex::new(vec![]));
        let input = Input::new("dropped", Box::new(Stalled { release, got: seen.clone() })).unwrap();
        let bound = Duration::from_millis(200);
        // Three chunks' worth, stuck on its first chunk.
        let long = "x".repeat(3 * CHUNK);
        assert!(input.write_within(long.as_bytes(), bound).await.is_err());
        input.resume();
        // Its first chunk was already on its way; the rest of it isn't sent once resumed.
        go.send(()).unwrap();
        drop(go);
        input.write(b"after").await.unwrap();
        assert_eq!(got(&seen), format!("{}after", "x".repeat(CHUNK)));
    }

    #[tokio::test]
    async fn a_big_paste_into_a_slow_connection_that_still_reads_goes_whole() {
        let seen = Arc::new(Mutex::new(vec![]));
        let input = Input::new("slow", Box::new(Slow { per_chunk: Duration::from_millis(60), got: seen.clone() })).unwrap();
        // 12 chunks at 60 ms each: well past the bound in all, never past it without progress.
        let paste = "p".repeat(12 * CHUNK);
        input.write_within(paste.as_bytes(), Duration::from_millis(300)).await.unwrap();
        assert_eq!(got(&seen), paste);
    }

    #[tokio::test]
    async fn one_callers_writes_go_in_the_order_they_were_made() {
        let seen = Arc::new(Mutex::new(vec![]));
        let (go, release) = mpsc::channel();
        drop(go); // never stalls
        let input = Input::new("order", Box::new(Stalled { release, got: seen.clone() })).unwrap();
        let typed: Vec<String> = (0..500).map(|i| format!("<{i}>")).collect();
        for k in &typed {
            input.write(k.as_bytes()).await.unwrap();
        }
        assert_eq!(got(&seen), typed.concat());
    }
}
