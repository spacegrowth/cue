//! The time as the hub reads it, and its waits. In tests a `testing::World` can run this clock ahead of the
//! wall's (`fake`), for the threads it drives: a wait of minutes (a typing gate, a confirmation watch) then
//! takes moments. Everywhere else it is the wall clock and a real sleep.

/// Milliseconds since 1970, by this clock.
pub fn now_ms() -> u64 {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    #[cfg(test)]
    return wall + fake::ahead_ms();
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

    /// The fake clock: how far ahead of the wall it is.
    #[derive(Clone, Default)]
    pub struct Fake(Arc<AtomicU64>);

    impl Fake {
        /// This thread reads the time from this clock from now on (until `uninstall`).
        pub fn install(&self) {
            INSTALLED.with(|c| *c.borrow_mut() = Some(self.clone()));
        }

        pub fn uninstall() {
            INSTALLED.with(|c| *c.borrow_mut() = None);
        }

        /// Move the clock on by `ms`.
        pub fn advance(&self, ms: u64) {
            self.0.fetch_add(ms, Ordering::SeqCst);
        }
    }

    pub(super) fn ahead_ms() -> u64 {
        INSTALLED.with(|c| c.borrow().as_ref().map_or(0, |f| f.0.load(Ordering::SeqCst)))
    }

    /// A wait on this thread's fake clock, if it has one: true when it was waited that way.
    pub(super) fn sleep_ms(ms: u64) -> bool {
        let Some(f) = INSTALLED.with(|c| c.borrow().clone()) else { return false };
        f.advance(ms);
        std::thread::sleep(std::time::Duration::from_millis(ms / LAPSE));
        true
    }
}
