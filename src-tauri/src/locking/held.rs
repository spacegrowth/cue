//! Who holds which lock. Each thread knows the locks it holds itself (to catch one taken again on the
//! same thread before it hangs), and one table says who holds each lock right now, for the thread
//! waiting on it and for the main-thread probe to name. A lock is known by its address: two locks may
//! share a name (a lock per session, say), never an address.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::Thread;
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

/// Every lock taken through `locking::lock` and not yet let go, by address. A plain lock: the bookkeeping
/// of the one way to take a lock can't go through it.
static HOLDERS: Mutex<Option<HashMap<usize, Holder>>> = Mutex::new(None);

fn holders<R>(f: impl FnOnce(&mut HashMap<usize, Holder>) -> R) -> R {
    f(HOLDERS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// A thread as a log line names it: its name (tokio's and Tauri's threads have one) and its id.
pub(super) fn describe(t: &Thread) -> String {
    format!("{} {:?}", t.name().unwrap_or("unnamed"), t.id())
}

/// Whether this thread holds the lock at `at` already.
pub(super) fn by_this_thread(at: usize) -> bool {
    MINE.try_with(|m| m.borrow().contains(&at)).unwrap_or(false)
}

/// Who holds the lock at `at`, and for how long so far: "<thread> for <ms> ms".
pub(super) fn holder_of(at: usize) -> Option<String> {
    holders(|h| h.get(&at).map(|h| format!("{} for {} ms", describe(&h.thread), h.since.elapsed().as_millis())))
}

/// Every lock held right now, longest-held first: "<name> held by <thread> for <ms> ms".
pub fn all() -> Vec<String> {
    let mut now: Vec<(Duration, String)> = holders(|h| {
        h.values().map(|h| (h.since.elapsed(), format!("{} held by {} for {} ms", h.what, describe(&h.thread), h.since.elapsed().as_millis()))).collect()
    });
    now.sort_by(|a, b| b.0.cmp(&a.0));
    now.into_iter().map(|(_, line)| line).collect()
}

/// The record of one lock held by this thread, kept while its guard lives. Dropped before the lock
/// itself is let go, so the next holder's record never gets cleared by this one's.
pub(super) struct Held {
    what: &'static str,
    at: usize,
}

impl Held {
    pub(super) fn new(what: &'static str, at: usize) -> Held {
        let _ = MINE.try_with(|m| m.borrow_mut().push(at));
        holders(|h| h.insert(at, Holder { what, thread: std::thread::current(), since: Instant::now() }));
        Held { what, at }
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
        holders(|h| h.remove(&self.at));
    }
}
