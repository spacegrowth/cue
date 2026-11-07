//! What the hub reaches outside itself through a seam a test can stand in for: the terminals it types into
//! (terminal::keys), Notification Center (`Notes`), and where its follow-up work runs (`Later`: a watch on a typed message, a kept message
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

/// Notification Center: notifications posted, and taken back by id.
pub trait Notes: Send + Sync {
    fn show(&self, note: crate::notify::Note);
    fn take_back(&self, ids: Vec<String>);
}

/// The app's Notification Center (notify.rs). Test instances run with CUE_QUIET=1 so they never ping you.
struct Center(tauri::AppHandle);

impl Notes for Center {
    fn show(&self, note: crate::notify::Note) {
        if std::env::var_os("CUE_QUIET").is_none() {
            crate::notify::show(self.0.clone(), note);
        }
    }

    fn take_back(&self, ids: Vec<String>) {
        crate::notify::take_back(ids);
    }
}

/// No Notification Center (a hub with no app): nothing is posted.
struct Nowhere;

impl Notes for Nowhere {
    fn show(&self, _: crate::notify::Note) {}
    fn take_back(&self, _: Vec<String>) {}
}

pub struct Ports {
    pub keys: Arc<dyn Keys>,
    pub notes: Arc<dyn Notes>,
    pub later: Later,
}

impl Ports {
    /// The real ones; notifications only with an app to post them from.
    pub fn real(app: Option<&tauri::AppHandle>) -> Self {
        let notes: Arc<dyn Notes> = match app {
            Some(a) => Arc::new(Center(a.clone())),
            None => Arc::new(Nowhere),
        };
        Self { keys: Arc::new(Terminals), notes, later: Later::Threads }
    }
}
