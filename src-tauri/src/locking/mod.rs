//! One way to take a lock, `locking::lock(&m, "name")`, so a hang names itself instead of freezing the app
//! with no word (twice now a lock taken twice on one thread reached the installed app that way):
//! - A lock taken again on the thread that holds it would wait on itself for good. It's caught before
//!   it waits: logged to locks.log with a backtrace, and in debug and test builds a panic naming it.
//! - A wait longer than two seconds is logged with who holds the lock and for how long, and the
//!   waiter's backtrace; in debug and test builds a wait of five seconds is a panic (nothing in Cue holds
//!   a lock that long: two threads each wait on the other's lock, or one held across slow work).
//! - `probe` watches the main thread and logs every lock held while it's stalled.
//!
//! A lock another thread panicked while holding is taken anyway: what's in it is whole (each change is
//! made under one lock), and refusing it would stop every later lock too.

mod held;
#[cfg(test)]
mod lint;
pub mod probe;
mod report;

use std::sync::{Condvar, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

/// How long a wait goes before it's logged, and (debug and test builds) before it's a panic.
#[derive(Clone, Copy)]
struct Patience {
    report_after: Duration,
    panic_after: Option<Duration>,
}

const PATIENCE: Patience = Patience {
    report_after: Duration::from_secs(2),
    panic_after: if cfg!(debug_assertions) { Some(Duration::from_secs(5)) } else { None },
};

/// `m` locked. `what`: the lock's name, for the log and the panic; each lock has its own.
pub fn lock<'a, T>(m: &'a Mutex<T>, what: &'static str) -> Guard<'a, T> {
    lock_with(m, what, PATIENCE)
}

fn lock_with<'a, T>(m: &'a Mutex<T>, what: &'static str, patience: Patience) -> Guard<'a, T> {
    let at = m as *const Mutex<T> as usize;
    if held::by_this_thread(at) {
        report::reentry(what);
    }
    let inner = acquire(m, what, at, patience);
    Guard { held: held::Held::new(what, at), inner }
}

/// Wait for `m`, saying so once the wait is long. Polled, so the wait can be measured and logged while it
/// lasts (a deadlock never ends, so "after" would never come); past the report in release, a plain wait.
fn acquire<'a, T>(m: &'a Mutex<T>, what: &str, at: usize, patience: Patience) -> MutexGuard<'a, T> {
    let t0 = Instant::now();
    let mut nap = Duration::from_micros(20);
    let mut reported = false;
    loop {
        match m.try_lock() {
            Ok(g) => return g,
            Err(TryLockError::Poisoned(p)) => return p.into_inner(),
            Err(TryLockError::WouldBlock) => {}
        }
        let waited = t0.elapsed();
        if !reported && waited >= patience.report_after {
            report::long_wait(what, waited, held::holder_of(at));
            reported = true;
        }
        match patience.panic_after {
            Some(limit) if waited >= limit => {
                let holder = held::holder_of(at).unwrap_or_else(|| "nobody now".into());
                panic!("cue: {what} has been held for {}s (by {holder}): a deadlock, or a lock held across slow work", limit.as_secs_f32());
            }
            None if reported => return m.lock().unwrap_or_else(|e| e.into_inner()),
            _ => {}
        }
        std::thread::sleep(nap);
        nap = (nap * 2).min(Duration::from_millis(1));
    }
}

/// A lock held, as `MutexGuard` is (it derefs to what's locked), that also keeps the record of who holds
/// it. Its record goes before the lock does, so the next holder's record is never the one cleared.
pub struct Guard<'a, T> {
    held: held::Held,
    inner: MutexGuard<'a, T>,
}

impl<'a, T> Guard<'a, T> {
    /// Wait on `cv` with the lock let go meanwhile (`Condvar::wait`): the record goes with it, and comes
    /// back when the lock does. Called as `Guard::wait(g, &cv)`, not a method, so it never hides a method
    /// of what's locked (a locked `Child`'s `wait`), as `MutexGuard::map` does.
    pub fn wait(guard: Self, cv: &Condvar) -> Guard<'a, T> {
        let Guard { held, inner } = guard;
        let (what, at) = held.which();
        drop(held);
        let inner = cv.wait(inner).unwrap_or_else(|e| e.into_inner());
        Guard { held: held::Held::new(what, at), inner }
    }
}

impl<T> std::ops::Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests;
