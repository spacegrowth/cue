//! The lock watchdog: one thread that reports every wait for a lock gone past its report time, from the
//! waiters' table, naming the lock, the waiter and the holder. The waiter itself just waits (in the
//! kernel's queue, fair to it), so it can't say anything while it does; and a deadlocked waiter never
//! would. The thread starts with the first wait for a lock and sleeps while nobody waits.

use std::sync::OnceLock;
use std::time::Duration;

/// How often waits are looked over while any thread waits.
const TICK: Duration = Duration::from_millis(100);

/// A thread waits for a lock now: the watchdog looks.
pub(super) fn wake() {
    static THREAD: OnceLock<Option<std::thread::Thread>> = OnceLock::new();
    let watchdog = THREAD.get_or_init(|| std::thread::Builder::new().name("cue lock watchdog".into()).spawn(run).ok().map(|h| h.thread().clone()));
    if let Some(t) = watchdog {
        t.unpark();
    }
}

fn run() {
    loop {
        if !super::held::anyone_waiting() {
            std::thread::park();
            continue;
        }
        std::thread::sleep(TICK);
        for o in super::held::overdue() {
            super::report::long_wait(o.what, &o.waiter, o.waited, super::held::holder_of(o.at));
        }
    }
}
