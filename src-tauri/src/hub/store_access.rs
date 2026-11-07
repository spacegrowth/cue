//! The one way into the hub's store: `read` and `write` run a closure with it held and let it go when the
//! closure returns. No guard leaves this file, so nothing can hold the store across a call to the hub, a
//! file, the database or a terminal: what needs those decides under `write`, then acts once it returns.
//! This is also the one file to change if the store moves to a thread of its own (an actor: plan §2.5).

use super::{Hub, Store};
use std::sync::Mutex;

/// The hub's store, locked only here.
pub(super) struct StoreCell(Mutex<Store>);

impl StoreCell {
    pub(super) fn new(st: Store) -> Self {
        Self(Mutex::new(st))
    }
}

impl Hub {
    /// `f` with the store held, to look. A panic on another thread while it held the store poisons the
    /// lock; what's in it is still whole (each change is made under one lock), so Cue goes on with it rather
    /// than panicking at every look from then on (locking::lock).
    pub(super) fn read<R>(&self, f: impl FnOnce(&Store) -> R) -> R {
        let st = crate::locking::lock(&self.store.0, "the hub's store");
        f(&st)
    }

    /// `f` with the store held, to change it.
    pub(super) fn write<R>(&self, f: impl FnOnce(&mut Store) -> R) -> R {
        let mut st = crate::locking::lock(&self.store.0, "the hub's store");
        f(&mut st)
    }
}

impl Hub {
    /// The store, locked, for hub/activity.rs until it moves to `read`/`write`.
    pub(super) fn st(&self) -> crate::locking::Guard<'_, Store> {
        crate::locking::lock(&self.store.0, "the hub's store")
    }
}

#[cfg(test)]
impl Hub {
    /// Tests: the store held, as a slow or stuck hub call holds it: `held` hears once it is, and it's let go
    /// when `release` hears (or hangs up).
    pub(crate) fn hold_store(&self, held: std::sync::mpsc::Sender<()>, release: std::sync::mpsc::Receiver<()>) {
        self.read(|_| {
            let _ = held.send(());
            let _ = release.recv();
        });
    }
}
