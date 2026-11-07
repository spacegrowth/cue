//! What the hub reaches outside itself through a seam a test can stand in for: the terminals it types into
//! (terminal::keys) and where its follow-up work runs (`Later`: a watch on a typed message, a kept message
//! sent once a turn ends, a queued review). The app's are `Ports::real`; testing::World brings its own.

use super::Hub;
use crate::terminal::keys::{Keys, Terminals};
use std::sync::Arc;

/// Work the hub hands off the caller's thread, run with the hub as the app holds it.
pub type Job = Box<dyn FnOnce(&Arc<Hub>) + Send>;

pub trait Later: Send + Sync {
    /// Run `job` after `after_ms`, off this thread. `me`: the hub as the app holds it, None with no app.
    fn run(&self, me: Option<Arc<Hub>>, after_ms: u64, job: Job);
}

/// A thread per job. A hub with no app (a test's) has nothing to hand the job: it's dropped.
pub struct Threads;

impl Later for Threads {
    fn run(&self, me: Option<Arc<Hub>>, after_ms: u64, job: Job) {
        let Some(me) = me else { return };
        std::thread::spawn(move || {
            crate::clock::sleep_ms(after_ms);
            job(&me);
        });
    }
}

pub struct Ports {
    pub keys: Arc<dyn Keys>,
    pub later: Arc<dyn Later>,
}

impl Ports {
    pub fn real() -> Self {
        Self { keys: Arc::new(Terminals), later: Arc::new(Threads) }
    }
}
