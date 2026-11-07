//! Who holds which lock, and who waits on it. Each thread knows the locks it holds itself (to catch one
//! taken again on the same thread before it hangs); one table says who holds each lock right now, and
//! another who has been waiting for one since when, for the watchdog and the main-thread probe to name.
//! A lock is known by its address: two locks may share a name (a lock per session, say), never an address.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::{Thread, ThreadId};
use std::time::{Duration, Instant};

thread_local! {
    /// The locks this thread holds, innermost last.
    static MINE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Who holds a lock and since when.
struct Holder {
    what: &'static str,
    thread: Thread,
    since: Instant,
}

/// A thread waiting for a lock: since when, when its wait is worth a report, and whether it's had one.
struct Waiter {
    what: &'static str,
    at: usize,
    thread: Thread,
    since: Instant,
    report_after: Duration,
    reported: bool,
}

/// The two tables. Plain locks: the bookkeeping of the one way to take a lock can't go through it.
/// Each is taken alone, never with the other held. Holders are by lock and thread: a lock read by
/// several threads at once has a holder each.
static HOLDERS: Mutex<Option<HashMap<(usize, ThreadId), Holder>>> = Mutex::new(None);
static WAITERS: Mutex<Option<HashMap<u64, Waiter>>> = Mutex::new(None);

fn table<K, V, R>(t: &Mutex<Option<HashMap<K, V>>>, f: impl FnOnce(&mut HashMap<K, V>) -> R) -> R {
    f(t.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// A thread as a log line names it: its name (tokio's and Tauri's threads have one) and its id.
pub(super) fn describe(t: &Thread) -> String {
    format!("{} {:?}", t.name().unwrap_or("unnamed"), t.id())
}

/// Whether this thread holds the lock at `at` already.
pub(super) fn by_this_thread(at: usize) -> bool {
    MINE.try_with(|m| m.borrow().contains(&at)).unwrap_or(false)
}

/// The thread that holds the lock at `at` (the longest, when several read it), and for how long.
pub(super) fn holder_of(at: usize) -> Option<(Thread, Duration)> {
    table(&HOLDERS, |h| h.iter().filter(|(k, _)| k.0 == at).map(|(_, h)| (h.thread.clone(), h.since.elapsed())).max_by_key(|(_, d)| *d))
}

/// Every lock held right now, longest-held first: "<name> held by <thread> for <ms> ms".
pub fn all() -> Vec<String> {
    let mut now: Vec<(Duration, String)> = table(&HOLDERS, |h| {
        h.values().map(|h| (h.since.elapsed(), format!("{} held by {} for {} ms", h.what, describe(&h.thread), h.since.elapsed().as_millis()))).collect()
    });
    now.sort_by(|a, b| b.0.cmp(&a.0));
    now.into_iter().map(|(_, line)| line).collect()
}

/// The record of one lock held by this thread, kept while its guard lives. Dropped before the lock
/// itself is let go, so the next holder's record never gets cleared by this one's. How long it was held,
/// and where it was taken, are counted as it goes (`stats::held`).
pub(super) struct Held {
    what: &'static str,
    at: usize,
    site: &'static std::panic::Location<'static>,
    since: Instant,
}

impl Held {
    #[track_caller]
    pub(super) fn new(what: &'static str, at: usize) -> Held {
        let _ = MINE.try_with(|m| m.borrow_mut().push(at));
        let thread = std::thread::current();
        let since = Instant::now();
        table(&HOLDERS, |h| h.insert((at, thread.id()), Holder { what, thread, since }));
        Held { what, at, site: std::panic::Location::caller(), since }
    }

    /// The lock this is the record of: its name and address.
    pub(super) fn which(&self) -> (&'static str, usize) {
        (self.what, self.at)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = MINE.try_with(|m| {
            let mut m = m.borrow_mut();
            if let Some(i) = m.iter().rposition(|a| *a == self.at) {
                m.remove(i);
            }
        });
        // On the thread that took it: a guard can't move to another.
        table(&HOLDERS, |h| h.remove(&(self.at, std::thread::current().id())));
        super::stats::held(self.what, self.site, self.since.elapsed());
    }
}

/// This thread's wait for a lock, in the waiters' table while it lasts.
pub(super) struct Waiting {
    id: u64,
}

impl Waiting {
    pub(super) fn new(what: &'static str, at: usize, report_after: Duration) -> Waiting {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let w = Waiter { what, at, thread: std::thread::current(), since: Instant::now(), report_after, reported: false };
        table(&WAITERS, |t| t.insert(id, w));
        Waiting { id }
    }

    /// The wait is over: how long it was, when it was long enough to be reported.
    pub(super) fn over(self) -> Option<Duration> {
        table(&WAITERS, |t| t.get(&self.id).filter(|w| w.reported).map(|w| w.since.elapsed()))
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        table(&WAITERS, |t| t.remove(&self.id));
    }
}

/// A wait past its report time.
pub(super) struct Overdue {
    pub what: &'static str,
    pub at: usize,
    pub waiter: String,
    pub waited: Duration,
}

/// Whether any thread is waiting for a lock.
pub(super) fn anyone_waiting() -> bool {
    table(&WAITERS, |t| !t.is_empty())
}

/// The waits past their report time that haven't been reported, marked reported now.
pub(super) fn overdue() -> Vec<Overdue> {
    table(&WAITERS, |t| {
        t.values_mut()
            .filter(|w| !w.reported && w.since.elapsed() >= w.report_after)
            .map(|w| {
                w.reported = true;
                Overdue { what: w.what, at: w.at, waiter: describe(&w.thread), waited: w.since.elapsed() }
            })
            .collect()
    })
}
