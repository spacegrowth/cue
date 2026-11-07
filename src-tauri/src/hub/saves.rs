//! Saving what the store keeps across a restart, after the store is let go. A change and its save are two
//! steps, so two changes' saves can reach the database in either order; each save therefore writes what the
//! store holds now, not what its change made, under a lock of its own per collection. Whichever save runs
//! last runs after every change before it, so the database ends as the store is: never an older state
//! written over a newer one. Lock order: a save lock, then the store (a look), then the database.
//!
//! Answered cards need none of this: each is saved once, under its own id, and never changed after.

use super::*;

/// One lock per saved collection (see the module's note).
#[derive(Default)]
pub(super) struct SaveLocks {
    review_queue: Mutex<()>,
    parked: Mutex<()>,
}

impl Hub {
    /// `exec`'s queued review saved as the store has it now: queued, or not (sent, taken back, moot).
    pub(super) fn save_queued_review(&self, exec: &str) {
        let _saving = crate::locking::lock(&self.saves.review_queue, "review queue saves");
        match self.read(|st| st.review_queue.iter().find(|q| q.exec == exec).cloned()) {
            Some(q) => crate::db::queue_review(&q.exec, &q.lead, &q.text, q.at_ms),
            None => crate::db::unqueue_review(exec),
        }
    }

    /// `session_id`'s parked record saved as the store has it now: parked (as it is, star and all), or not.
    pub(super) fn save_parked(&self, session_id: &str) {
        let _saving = crate::locking::lock(&self.saves.parked, "parked saves");
        match self.read(|st| st.parked.iter().find(|p| p.session.origin.session_id == session_id).cloned()) {
            Some(p) => crate::db::park(&p),
            None => crate::db::unpark(session_id),
        }
    }
}
