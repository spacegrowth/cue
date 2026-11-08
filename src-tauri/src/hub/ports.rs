//! What the hub reaches outside itself through a seam a test can stand in for: the terminals it types into
//! (terminal::keys), where it starts a session's agent (terminal::launch), the sessions it hosts itself
//! (hosted::running), Notification Center (`Notes`), the
//! window's events beside its redraws (`Window`), and where its follow-up work runs (`Later`: a watch on a typed message, a kept message sent once a turn ends, a
//! queued review). The app's are `Ports::real`; testing::World brings its own.

use crate::hosted::{Hosted, Running};
use crate::terminal::keys::{Keys, Terminals};
use crate::terminal::launch::{Launcher, Tabs};
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

/// The window, for what the hub tells it beside the state it redraws: an image copy's progress ("upload").
pub trait Window: Send + Sync {
    fn tell(&self, event: &str, payload: serde_json::Value);
}

/// The app's window: a Tauri event.
struct AppWindow(tauri::AppHandle);

impl Window for AppWindow {
    fn tell(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.0.emit(event, payload);
    }
}

/// No window (a hub with no app): nobody to tell.
struct NoWindow;

impl Window for NoWindow {
    fn tell(&self, _: &str, _: serde_json::Value) {}
}

/// No Notification Center (a hub with no app): nothing is posted.
struct Nowhere;

impl Notes for Nowhere {
    fn show(&self, _: crate::notify::Note) {}
    fn take_back(&self, _: Vec<String>) {}
}

pub struct Ports {
    pub keys: Arc<dyn Keys>,
    pub launcher: Arc<dyn Launcher>,
    pub hosted: Arc<dyn Hosted>,
    pub notes: Arc<dyn Notes>,
    pub window: Arc<dyn Window>,
    pub later: Later,
}

impl Ports {
    /// The real ones; notifications and window events only with an app to send them from.
    pub fn real(app: Option<&tauri::AppHandle>) -> Self {
        let (notes, window): (Arc<dyn Notes>, Arc<dyn Window>) = match app {
            Some(a) => (Arc::new(Center(a.clone())), Arc::new(AppWindow(a.clone()))),
            None => (Arc::new(Nowhere), Arc::new(NoWindow)),
        };
        Self { keys: Arc::new(Terminals), launcher: Arc::new(Tabs), hosted: Arc::new(Running::default()), notes, window, later: Later::Threads }
    }
}
