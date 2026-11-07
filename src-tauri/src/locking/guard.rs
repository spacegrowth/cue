//! A lock held: the standard guard (it derefs to what's locked) together with the record of who holds it.

use super::held::Held;
use std::sync::{Condvar, MutexGuard, RwLockReadGuard, RwLockWriteGuard};

/// A lock held, with its record. The record goes before the lock does, so the next holder's record is
/// never the one cleared.
pub struct Locked<G> {
    held: Held,
    inner: G,
}

/// A `Mutex` held (`locking::lock`).
pub type Guard<'a, T> = Locked<MutexGuard<'a, T>>;
/// An `RwLock` read (`locking::read`).
pub type ReadGuard<'a, T> = Locked<RwLockReadGuard<'a, T>>;
/// An `RwLock` written (`locking::write`).
pub type WriteGuard<'a, T> = Locked<RwLockWriteGuard<'a, T>>;

impl<G> Locked<G> {
    pub(super) fn new(held: Held, inner: G) -> Locked<G> {
        Locked { held, inner }
    }
}

impl<'a, T> Locked<MutexGuard<'a, T>> {
    /// Wait on `cv` with the lock let go meanwhile (`Condvar::wait`): the record goes with it, and comes
    /// back when the lock does. Called as `Guard::wait(g, &cv)`, not a method, so it never hides a method
    /// of what's locked (a locked `Child`'s `wait`), as `MutexGuard::map` does.
    #[track_caller]
    pub fn wait(guard: Self, cv: &Condvar) -> Self {
        Self::let_go_while(guard, |g| cv.wait(g).unwrap_or_else(|e| e.into_inner()))
    }

    /// `wait`, for at most `dur` (`Condvar::wait_timeout`); the caller looks again at what it waits for.
    #[track_caller]
    pub fn wait_timeout(guard: Self, cv: &Condvar, dur: std::time::Duration) -> Self {
        Self::let_go_while(guard, |g| cv.wait_timeout(g, dur).unwrap_or_else(|e| e.into_inner()).0)
    }

    /// The lock and its record let go while `wait` runs, both back after.
    #[track_caller]
    fn let_go_while(guard: Self, wait: impl FnOnce(MutexGuard<'a, T>) -> MutexGuard<'a, T>) -> Self {
        let Locked { held, inner } = guard;
        let (what, at) = held.which();
        drop(held);
        let inner = wait(inner);
        Locked { held: Held::new(what, at), inner }
    }
}

impl<G: std::ops::Deref> std::ops::Deref for Locked<G> {
    type Target = G::Target;
    fn deref(&self) -> &G::Target {
        &self.inner
    }
}

impl<G: std::ops::DerefMut> std::ops::DerefMut for Locked<G> {
    fn deref_mut(&mut self) -> &mut G::Target {
        &mut self.inner
    }
}

/// Shown as what's locked, as the standard guards are (an assert's message can print a locked value).
impl<G: std::ops::Deref<Target: std::fmt::Debug>> std::fmt::Debug for Locked<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.inner, f)
    }
}

impl<G: std::ops::Deref<Target: std::fmt::Display>> std::fmt::Display for Locked<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&*self.inner, f)
    }
}
