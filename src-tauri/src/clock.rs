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

/// Milliseconds since Cue started, on a clock that only moves forward: for how long something takes. Setting
/// the Mac's clock (or the network setting it) moves `now_ms` and never this, so every deadline and every
/// "how long since" reads it. A fake clock moves it on as it moves `now_ms`.
pub fn now_mono_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let mono = START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64;
    #[cfg(test)]
    return fake::now_mono_ms(mono);
    #[cfg(not(test))]
    mono
}

/// Look (`done`) every `every_ms` until it says yes or `limit_ms` have passed by the forward-only clock:
/// whether it said yes. The first look is at once.
pub fn within(limit_ms: u64, every_ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = now_mono_ms() + limit_ms;
    loop {
        if done() {
            return true;
        }
        if now_mono_ms() >= end {
            return false;
        }
        sleep_ms(every_ms);
    }
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
    use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
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
        /// How far the Mac's clock was set, back or forward (`set_wall`): it moves `now_ms` only.
        set_by: Arc<AtomicI64>,
    }

    impl Fake {
        /// A clock at `start_ms` that moves only when moved.
        pub fn frozen(start_ms: u64) -> Self {
            Self { ahead: Arc::default(), start: Some(start_ms), set_by: Arc::default() }
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

        /// The Mac's clock set `by_ms` (negative: back), as a person or the network does: the time of day
        /// jumps, the forward-only clock doesn't.
        pub fn set_wall(&self, by_ms: i64) {
            self.set_by.fetch_add(by_ms, Ordering::SeqCst);
        }

        fn wall_ms(&self, wall: u64) -> u64 {
            (self.start.unwrap_or(wall) + self.ahead.load(Ordering::SeqCst)).saturating_add_signed(self.set_by.load(Ordering::SeqCst))
        }

        /// A frozen clock's forward-only time starts at 0; a running one's at the process's.
        fn mono_ms(&self, mono: u64) -> u64 {
            self.start.map_or(mono, |_| 0) + self.ahead.load(Ordering::SeqCst)
        }
    }

    fn installed() -> Option<Fake> {
        INSTALLED.with(|c| c.borrow().clone())
    }

    /// The time on this thread's fake clock, or the wall's (`wall`) with none installed.
    pub(super) fn now_ms(wall: u64) -> u64 {
        installed().map_or(wall, |f| f.wall_ms(wall))
    }

    /// The forward-only time on this thread's fake clock, or the real one's (`mono`) with none installed.
    pub(super) fn now_mono_ms(mono: u64) -> u64 {
        installed().map_or(mono, |f| f.mono_ms(mono))
    }

    /// A wait on this thread's fake clock, if it has one: true when it was waited that way.
    pub(super) fn sleep_ms(ms: u64) -> bool {
        let Some(f) = installed() else { return false };
        f.advance(ms);
        std::thread::sleep(std::time::Duration::from_millis(ms / LAPSE));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;

    #[test]
    fn a_wait_on_the_fake_clock_moves_both_clocks() {
        let f = Fake::frozen(1_000_000);
        f.install();
        let (wall, mono) = (now_ms(), now_mono_ms());
        sleep_ms(400);
        assert_eq!((now_ms() - wall, now_mono_ms() - mono), (400, 400));
        Fake::uninstall();
    }

    #[test]
    fn a_deadline_holds_when_the_macs_clock_is_set_back() {
        let f = Fake::frozen(10_000_000_000);
        f.install();
        let start = now_mono_ms();
        let mut looks = 0;
        // The clock set back an hour at the first look: a deadline by the time of day would wait an hour more.
        let said = within(2_000, 25, || {
            if looks == 0 {
                f.set_wall(-3_600_000);
            }
            looks += 1;
            false
        });
        assert!(!said);
        assert!((2_000..2_100).contains(&(now_mono_ms() - start)), "ended at its limit: {} ms", now_mono_ms() - start);
        assert_eq!(now_ms(), 10_000_000_000 + now_mono_ms() - 3_600_000, "the time of day did jump");
        Fake::uninstall();
    }

    #[test]
    fn within_stops_looking_once_it_is_so() {
        let f = Fake::frozen(0);
        f.install();
        let mut looks = 0;
        assert!(within(10_000, 100, || {
            looks += 1;
            looks == 3
        }));
        assert_eq!((looks, now_mono_ms()), (3, 200), "looked at once, then every 100 ms");
        Fake::uninstall();
    }
}
