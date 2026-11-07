//! Drawing the window after a change, coalesced. A change only marks the state as needing a draw and wakes
//! one worker; the worker draws the latest state, so a burst of changes (a hundred hook events a second)
//! costs a draw or two, not one each. A draw is everything that follows a change: the live sessions saved
//! (db::sync_live, latest wins), the snapshot built and sent, notifications for answered cards taken back,
//! and the Dock badge. It runs on the worker's thread with no lock held but the ones each step takes.
//!
//! Without a window (tests) there is nothing to send: a hub on threads leaves the next ask to build it, and
//! the World's hub (ports::Later::Deferred) runs the draw (the saves, no snapshot) as follow-up work it
//! settles, deterministically.

use super::ports::Later;
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, OnceLock, Weak};

/// The window renders each state it's sent on its own main thread: sent more often than this, a burst of
/// changes would keep that thread rendering instead of answering you. Changes meanwhile wait for the next
/// draw, which shows them all.
const DRAW_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Default)]
pub(super) struct Redraws {
    /// A change not drawn yet.
    dirty: Mutex<bool>,
    wake: Condvar,
    /// The worker, started at the first change with a window.
    worker: OnceLock<()>,
    /// How many draws ran (the coalescing's own measure).
    draws: AtomicU64,
    /// How many times the worker has started waiting for a change (where it must hold no hub).
    waits: AtomicU64,
}

impl Redraws {
    /// Marked as needing a draw; true when it wasn't already.
    fn mark(&self) -> bool {
        let mut dirty = crate::locking::lock(&self.dirty, "redraw wanted");
        let was = *dirty;
        *dirty = true;
        self.wake.notify_one();
        !was
    }

    /// The mark taken off as a draw starts: a change from here on asks for the next one.
    fn take(&self) {
        *crate::locking::lock(&self.dirty, "redraw wanted") = false;
    }

    /// Waits until a draw is wanted, and takes the mark.
    fn wait(&self) {
        self.waits.fetch_add(1, Ordering::SeqCst);
        let mut dirty = crate::locking::lock(&self.dirty, "redraw wanted");
        while !*dirty {
            dirty = crate::locking::Guard::wait(dirty, &self.wake);
        }
        *dirty = false;
    }
}

impl Hub {
    /// Something changed what the window shows: drawn soon, by the worker (see the module's note).
    /// The kept state is no longer current at once, so an ask before the draw builds it (`Hub::state`).
    pub(super) fn changed(&self) {
        self.unshown();
        match &self.ports.later {
            #[cfg(test)]
            Later::Deferred(queue) => {
                if self.redraws.mark() {
                    queue.hand_in(0, Box::new(|h| h.draw()));
                }
            }
            Later::Threads if self.app.is_none() => {}
            Later::Threads => {
                self.redraws.mark();
                self.start_drawing();
            }
        }
    }

    /// Something outside the store changed what the window shows (who leads whom): drawn the same way.
    pub fn redraw(&self) {
        self.changed();
    }

    /// The worker, started once the app holds the hub (until then, each draw runs here).
    fn start_drawing(&self) {
        if self.redraws.worker.get().is_some() {
            return;
        }
        let Some(me) = self.app.as_ref().and_then(|a| a.try_state::<Arc<Hub>>()).map(|h| Arc::downgrade(h.inner())) else {
            return self.draw();
        };
        self.draw_on_worker(me);
    }

    /// The worker started (once) for this hub, `me`.
    fn draw_on_worker(&self, me: Weak<Hub>) {
        self.redraws.worker.get_or_init(|| {
            let redraws = self.redraws.clone();
            let _ = std::thread::Builder::new().name("cue-redraw".into()).spawn(move || draw_when_wanted(redraws, me));
        });
    }

    /// One draw of the latest state (see the module's note).
    fn draw(&self) {
        self.redraws.take();
        self.redraws.draws.fetch_add(1, Ordering::SeqCst);
        self.save_live();
        // No window (the World's hub): nothing to send, and the next ask builds the state.
        match &self.app {
            Some(app) => drop(app.emit("state", &*self.snapshot())),
            None => self.unshown(),
        }
        let (decisions, answered) = self.write(|st| {
            let live: HashSet<String> = st.items.iter().map(|i| i.id.clone()).collect();
            // A card answered anywhere (Cue, the terminal): its notification taken back.
            let answered: Vec<String> = st.notified.difference(&live).cloned().collect();
            for id in &answered {
                st.notified.remove(id);
            }
            (st.items.iter().filter(|i| i.kind != "waiting").count(), answered)
        });
        if !answered.is_empty() {
            self.ports.notes.take_back(answered);
        }
        if let Some(w) = self.app.as_ref().and_then(|a| a.get_webview_window("main")) {
            let _ = w.set_badge_count(if decisions > 0 { Some(decisions as i64) } else { None });
        }
    }

    /// The live sessions and waiting cards saved as the store has them now (a relaunch shows them), the sessions
    /// that ended since the last save as their records were when they ended (Closed after a relaunch), and parked
    /// sessions that are live again (you resumed one yourself, in a terminal) no longer parked. Each draw does
    /// it, and quitting does it once more, so nothing changed since the last draw is lost. All of it to the
    /// database writer: only what changed since its last save is written, and not on this thread.
    pub fn save_live(&self) {
        // Parked records are taken off here: handed in, in turn with the other parked saves.
        self.hand_in_parked_turn(|| self.write(|st| {
            let back: Vec<String> = st.parked.iter().map(|p| p.session.origin.session_id.clone()).filter(|sid| st.sessions.origin(sid).is_some()).collect();
            st.parked.retain(|p| !back.contains(&p.session.origin.session_id));
            let waiting: Vec<Item> = st.items.iter().filter(|i| i.kind == "waiting").cloned().collect();
            // Each that ended since the last save, as it was when it ended, not as of the save before: a change and
            // the end can land within one draw. Taken with the live ones under one lock, so the two agree.
            let ended = std::mem::take(&mut st.unsaved_ends).into_iter().filter(|(_, s)| st.sessions.origin(&s.origin.session_id).is_none()).collect();
            // The answered cards handed to this draw go in the same write as the live sessions.
            let answered = Self::take_unsaved(st);
            let parked = back.into_iter().map(|sid| (sid, None)).collect();
            live_writes::Job { live: Some((st.sessions.all(), waiting)), answered, ended, parked, deletes: vec![] }
        }));
    }

    /// `save_live`, and back once all of it is in the database: quitting.
    pub fn save_live_and_wait(&self) {
        self.save_live();
        self.saves.live.flush();
    }

    /// Whether a change is drawn (and saved) by a draw that follows it: the app's worker, or a test's queue.
    pub(super) fn draws_later(&self) -> bool {
        match &self.ports.later {
            #[cfg(test)]
            Later::Deferred(_) => true,
            Later::Threads => self.app.is_some(),
        }
    }

    /// Tests: how many draws have run.
    #[cfg(test)]
    pub(crate) fn draws(&self) -> u64 {
        self.redraws.draws.load(Ordering::SeqCst)
    }
}

/// The worker: a draw each time one is wanted, at most one per DRAW_EVERY, until the hub is gone.
/// It holds the hub only while it draws.
fn draw_when_wanted(redraws: Arc<Redraws>, hub: Weak<Hub>) {
    loop {
        redraws.wait();
        let started = std::time::Instant::now();
        let Some(h) = hub.upgrade() else { return };
        h.draw();
        drop(h);
        std::thread::sleep(DRAW_EVERY.saturating_sub(started.elapsed()));
    }
}

#[cfg(test)]
#[path = "redraw_tests.rs"]
mod tests;
