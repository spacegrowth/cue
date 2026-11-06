//! One way to take a lock, `locking::lock(&m, "name")` (`read`/`write` for an `RwLock`), so a hang names
//! itself instead of freezing the app with no word (twice now a lock taken twice on one thread reached the
//! installed app that way):
//! - A lock taken again on the thread that holds it would wait on itself for good. It's caught before
//!   it waits: logged to locks.log with a backtrace, and in debug and test builds a panic naming it.
//! - A wait longer than two seconds is logged by the watchdog with who holds the lock and for how long,
//!   then by the waiter with its backtrace once it has the lock; in debug and test builds a wait of five
//!   seconds is a panic (nothing in Cue holds a lock that long: two threads each wait on the other's
//!   lock, or one held across slow work).
//! - `probe` watches the main thread and logs every lock held while it's stalled.
//!
//! A lock another thread panicked while holding is taken anyway: what's in it is whole (each change is
//! made under one lock), and refusing it would stop every later lock too.

mod guard;
mod held;
#[cfg(test)]
mod lint;
#[cfg(test)]
mod source;
pub mod probe;
mod report;
mod watchdog;

pub use guard::{Guard, Locked, ReadGuard, WriteGuard};

use std::sync::{Mutex, RwLock, TryLockError, TryLockResult};
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

/// `rw` read, as `lock`.
pub fn read<'a, T>(rw: &'a RwLock<T>, what: &'static str) -> ReadGuard<'a, T> {
    take(what, address(rw), PATIENCE, || taken(rw.try_read()), || rw.read().unwrap_or_else(|e| e.into_inner()))
}

/// `rw` written, as `lock`.
pub fn write<'a, T>(rw: &'a RwLock<T>, what: &'static str) -> WriteGuard<'a, T> {
    take(what, address(rw), PATIENCE, || taken(rw.try_write()), || rw.write().unwrap_or_else(|e| e.into_inner()))
}

fn lock_with<'a, T>(m: &'a Mutex<T>, what: &'static str, patience: Patience) -> Guard<'a, T> {
    take(what, address(m), patience, || taken(m.try_lock()), || m.lock().unwrap_or_else(|e| e.into_inner()))
}

fn address<L>(lock: &L) -> usize {
    lock as *const L as usize
}

/// A try at a lock: held, or taken anyway when a panicking thread left it poisoned; None when it's busy.
fn taken<G>(r: TryLockResult<G>) -> Option<G> {
    match r {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// The lock at `at`, refused first when this thread holds it already, and recorded as held.
fn take<G>(what: &'static str, at: usize, patience: Patience, try_take: impl Fn() -> Option<G>, wait: impl FnOnce() -> G) -> Locked<G> {
    if held::by_this_thread(at) {
        report::reentry(what);
    }
    let inner = acquire(what, at, patience, try_take, wait);
    Locked::new(held::Held::new(what, at), inner)
}

/// Free: taken at once. Busy: this thread joins the waiters' table, where the watchdog reports it past
/// its report time, and waits in the kernel's queue; once it has the lock after such a report, it says
/// where it was with its own backtrace.
fn acquire<G>(what: &'static str, at: usize, patience: Patience, try_take: impl Fn() -> Option<G>, wait: impl FnOnce() -> G) -> G {
    if let Some(g) = try_take() {
        return g;
    }
    let waiting = held::Waiting::new(what, at, patience.report_after);
    watchdog::wake();
    let g = match patience.panic_after {
        None => wait(),
        Some(limit) => poll_until(what, at, limit, &try_take),
    };
    if let Some(waited) = waiting.over() {
        report::long_wait_over(what, waited);
    }
    g
}

/// Debug and test builds: past `limit` the wait is a panic naming the lock and its holder, and only the
/// waiting thread can raise it (in the test or command that waits), so it tries the lock in naps rather
/// than block on it. Unfair to it, but only a lock held all but continuously for that long starves it,
/// and that's the bug either way.
fn poll_until<G>(what: &str, at: usize, limit: Duration, try_take: &impl Fn() -> Option<G>) -> G {
    let t0 = Instant::now();
    let mut nap = Duration::from_micros(20);
    loop {
        if let Some(g) = try_take() {
            return g;
        }
        if t0.elapsed() >= limit {
            let holder = held::holder_of(at).map_or_else(|| "nobody now".to_string(), |(t, d)| format!("{} for {} ms", held::describe(&t), d.as_millis()));
            panic!("cue: {what} has been held for {}s (by {holder}): a deadlock, or a lock held across slow work", limit.as_secs_f32());
        }
        std::thread::sleep(nap);
        nap = (nap * 2).min(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests;
