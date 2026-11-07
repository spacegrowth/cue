//! The time as the hub reads it, and its waits. In tests a `testing::World` can run this clock ahead of the
//! wall's (`fake`), for the threads it drives: a wait of minutes (a typing gate, a confirmation watch) then
//! takes moments. Everywhere else it is the wall clock and a real sleep.
//!
//! Read through here: the hub's time and waits, the settings' (config::effective), typing's pause and paste
//! watch, ending an agent, and Move to Cue's waits. Left on the wall on purpose: a bound on a real process or
//! thread (focus's `capped`, the built-in terminal's join and write bounds, the quit grace), the app's own
//! tokio tickers (a World ticks the hub itself), and the redraw timer thread (hub/shown.rs, which takes its
//! due time from this clock). Not yet: file ages read through `elapsed()` in machines/hooks.rs and
//! logs/steps.rs, and the action runner's waits (actions/run.rs).

/// Milliseconds since 1970, by this clock.
pub fn now_ms() -> u64 {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    #[cfg(test)]
    return fake::now_ms(wall);
    #[cfg(not(test))]
    wall
}

/// Wait `ms` by this clock.
pub fn sleep_ms(ms: u64) {
    #[cfg(test)]
    if fake::sleep_ms(ms) {
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// A clock a test runs ahead of the wall's, shared by the threads it's installed on (each thread that
/// should see it installs it: `Fake::install`). Time only moves forward. A wait on it moves it on by the
/// whole wait and waits a twentieth of that for real, so another thread still gets to run meanwhile.
/// `Fake::frozen` starts at a given time and moves only when moved: what a fixture that must come out
/// the same every time is built on.
#[cfg(test)]
pub mod fake {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// How much faster than the wall a wait on the fake clock goes.
    const LAPSE: u64 = 20;

    thread_local! {
        static INSTALLED: RefCell<Option<Fake>> = const { RefCell::new(None) };
    }

    /// The fake clock: how far it has been moved on, from the wall's time or (frozen) from its start.
    #[derive(Clone, Default)]
    pub struct Fake {
        ahead: Arc<AtomicU64>,
        start: Option<u64>,
    }

    impl Fake {
        /// A clock at `start_ms` that moves only when moved.
        pub fn frozen(start_ms: u64) -> Self {
            Self { ahead: Arc::default(), start: Some(start_ms) }
        }

        /// This thread reads the time from this clock from now on (until `uninstall`).
        pub fn install(&self) {
            INSTALLED.with(|c| *c.borrow_mut() = Some(self.clone()));
        }

        pub fn uninstall() {
            INSTALLED.with(|c| *c.borrow_mut() = None);
        }

        /// Move the clock on by `ms`.
        pub fn advance(&self, ms: u64) {
            self.ahead.fetch_add(ms, Ordering::SeqCst);
        }
    }

    /// The time on this thread's fake clock, or the wall's (`wall`) with none installed.
    pub(super) fn now_ms(wall: u64) -> u64 {
        INSTALLED.with(|c| c.borrow().as_ref().map_or(wall, |f| f.start.unwrap_or(wall) + f.ahead.load(Ordering::SeqCst)))
    }

    /// A wait on this thread's fake clock, if it has one: true when it was waited that way.
    pub(super) fn sleep_ms(ms: u64) -> bool {
        let Some(f) = INSTALLED.with(|c| c.borrow().clone()) else { return false };
        f.advance(ms);
        std::thread::sleep(std::time::Duration::from_millis(ms / LAPSE));
        true
    }
}
