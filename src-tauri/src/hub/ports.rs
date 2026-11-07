//! What the hub reaches outside itself through a seam a test can stand in for: the terminals it types into
//! (terminal::keys) and where its follow-up work runs (`Later`: a watch on a typed message, a kept message
//! sent once a turn ends, a queued review). The app's are `Ports::real`; testing::World brings its own.

use crate::terminal::keys::{Keys, Terminals};
use std::sync::Arc;

/// Work the hub hands off the caller's thread, run with the hub as the app holds it.
#[cfg(test)]
pub type Job = Box<dyn FnOnce(&Arc<super::Hub>) + Send>;

/// Where follow-up work runs (Hub::later). An enum, not a trait object, so the spawn is in plain sight of
/// the inventory's checks (xtask): the job runs on a thread of its own, never under the caller's locks.
pub enum Later {
    /// A thread per job. A hub with no app (a test's) has nothing to hand the job: it's dropped.
    Threads,
    /// Held until the test runs it (testing::Deferred).
    #[cfg(test)]
    Deferred(Arc<crate::testing::Deferred>),
}

pub struct Ports {
    pub keys: Arc<dyn Keys>,
    pub later: Later,
}

impl Ports {
    pub fn real() -> Self {
        Self { keys: Arc::new(Terminals), later: Later::Threads }
    }
}
