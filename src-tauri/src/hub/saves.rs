//! Saving what the store keeps across a restart, after the store is let go. A change and its save are two
//! steps, so two changes' saves can reach the database in either order; each save therefore writes what the
//! store holds now, not what its change made, under a lock of its own per collection. Whichever save runs
//! last runs after every change before it, so the database ends as the store is: never an older state
//! written over a newer one. Lock order: a save lock, then the store (a look), then the database.
//!
//! An answered card is saved once, under its own id, and never changed after; what it can race is its session
//! being deleted (purge), so the two share a lock and the save skips a session deleted since it was answered.

use super::*;

/// One lock per saved collection (see the module's note).
#[derive(Default)]
pub(super) struct SaveLocks {
    review_queue: Mutex<()>,
    parked: Mutex<()>,
    answered: Mutex<()>,
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

    /// An answered card saved, unless its session was deleted since.
    pub(super) fn save_answered(&self, answered: super::history::Answered) {
        let _saving = crate::locking::lock(&self.saves.answered, "answered saves");
        let purged = self.read(|st| st.purged.get(answered.session_id()).copied());
        answered.save_unless_purged(purged);
    }

    /// An answered card saved by the draw its change asks for (redraw.rs `save_live`), in the same write as the
    /// live sessions: whoever answered never waits on the database for it. A hub that never draws saves it here.
    pub(super) fn save_answered_with_draw(&self, answered: super::history::Answered) {
        if !self.draws_later() {
            return self.save_answered(answered);
        }
        self.write(|st| st.unsaved.push(answered));
    }

    /// The answered cards handed to the draw (`save_answered_with_draw`), unless deleted since, written by `write`
    /// (with the live sessions, redraw.rs), under the lock a session's deletion takes.
    pub(super) fn save_handed_answers(&self, write: impl FnOnce(&[Item])) {
        let _saving = crate::locking::lock(&self.saves.answered, "answered saves");
        let answered: Vec<Item> = self.write(|st| {
            std::mem::take(&mut st.unsaved).into_iter().filter_map(|a| {
                let purged = st.purged.get(a.session_id()).copied();
                a.unless_purged(purged)
            }).collect()
        });
        write(&answered);
    }

    /// Everything saved of a session deleted (its record, chat, answers), after any save of it on its way.
    pub(super) fn delete_saved_session(&self, session_id: &str) {
        let _saving = crate::locking::lock(&self.saves.answered, "answered saves");
        crate::db::delete_session(session_id);
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
