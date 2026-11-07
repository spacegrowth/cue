//! What this Mac says that Settings shows, asked on a thread of its own and kept: whether Cue opens at login
//! (SMAppService, an XPC round trip to macOS's service manager that can take seconds), whether tmux is
//! installed, and which terminal app there is. `config::effective` reads them from here, so building the
//! window's state never waits on macOS. Asked at launch, every minute, when Settings opens and when you
//! flip Open at login.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    /// Cue is a login item (on, or waiting for you to allow it in System Settings).
    pub login: bool,
    pub tmux: bool,
    /// "iTerm" or "Terminal".
    pub terminal: &'static str,
}

/// Until macOS has answered: nothing on.
const UNASKED: Facts = Facts { login: false, tmux: false, terminal: "Terminal" };

/// Facts kept from the last time they were asked, and whether an ask is out now (one at a time: an ask that
/// hangs isn't joined by another every minute).
pub struct Kept {
    facts: RwLock<Option<Facts>>,
    asking: AtomicBool,
}

impl Kept {
    pub const fn new() -> Self {
        Kept { facts: RwLock::new(None), asking: AtomicBool::new(false) }
    }

    /// The facts as last asked, from memory.
    pub fn get(&self) -> Facts {
        crate::locking::read(&self.facts, "what macOS says").unwrap_or(UNASKED)
    }

    /// Ask again on a thread of its own, unless an ask is out already; `changed` runs when the answer differs.
    pub fn refresh_soon(&'static self, ask: fn() -> Facts, changed: fn()) {
        if self.asking.swap(true, Ordering::AcqRel) {
            return;
        }
        std::thread::spawn(move || {
            let now = ask();
            let was = std::mem::replace(&mut *crate::locking::write(&self.facts, "what macOS says"), Some(now));
            self.asking.store(false, Ordering::Release);
            if was != Some(now) {
                changed();
            }
        });
    }
}

static KEPT: Kept = Kept::new();

/// What this Mac says, from memory (all off until it first answers).
pub fn facts() -> Facts {
    KEPT.get()
}

/// Ask macOS again, off this thread.
pub fn refresh_soon() {
    KEPT.refresh_soon(ask, super::changed);
}

fn ask() -> Facts {
    use crate::terminal::focus;
    Facts { login: super::login::enabled(), tmux: focus::tmux_installed(), terminal: focus::terminal_name() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn counted() -> Facts {
        ASKED.fetch_add(1, Ordering::SeqCst);
        Facts { login: true, tmux: true, terminal: "iTerm" }
    }

    fn until(what: &str, done: impl Fn() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "{what}");
            std::thread::yield_now();
        }
    }

    #[test]
    fn reads_come_from_memory_and_an_ask_refreshes_them() {
        static K: Kept = Kept::new();
        assert_eq!(K.get(), UNASKED, "nothing on before macOS answers");
        K.refresh_soon(counted, || {});
        until("the ask finished", || K.get().login);
        let asked = ASKED.load(Ordering::SeqCst);
        for _ in 0..100 {
            assert_eq!(K.get(), Facts { login: true, tmux: true, terminal: "iTerm" });
        }
        assert_eq!(ASKED.load(Ordering::SeqCst), asked, "a read never asks macOS");
    }

    static RELEASE: AtomicBool = AtomicBool::new(false);
    static STALLED_ASKS: AtomicUsize = AtomicUsize::new(0);
    fn stalled() -> Facts {
        STALLED_ASKS.fetch_add(1, Ordering::SeqCst);
        until("the test let the stalled ask go", || RELEASE.load(Ordering::SeqCst));
        Facts { login: true, tmux: false, terminal: "Terminal" }
    }

    #[test]
    fn an_ask_that_hangs_holds_no_read_up_and_isnt_joined_by_another() {
        static K: Kept = Kept::new();
        K.refresh_soon(stalled, || {});
        until("the ask started", || STALLED_ASKS.load(Ordering::SeqCst) == 1);
        let start = Instant::now();
        assert_eq!(K.get(), UNASKED, "the last answer (none yet) while macOS is slow");
        assert!(start.elapsed() < Duration::from_millis(100), "a read doesn't wait on the ask");
        K.refresh_soon(stalled, || {});
        RELEASE.store(true, Ordering::SeqCst);
        until("the answer came", || K.get().login);
        assert_eq!(STALLED_ASKS.load(Ordering::SeqCst), 1, "no second ask while one was out");
    }
}
