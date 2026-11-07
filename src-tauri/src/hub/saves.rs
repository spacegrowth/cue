//! Saving what the store keeps across a restart, after the store is let go. A change and its save are two
//! steps, so two changes' saves can reach the database in either order; each save therefore writes what the
//! store holds now, not what its change made, under a lock of its own per collection. Whichever save runs
//! last runs after every change before it, so the database ends as the store is: never an older state
//! written over a newer one. Lock order: a save lock, then the store (a look), then the database.
//!
//! An answered card is saved once, under its own id, and never changed after; what it can race is its session
//! being deleted (purge), so the two share a lock and the save skips a session deleted since it was answered.

use super::*;

/// One lock per saved collection (see the module's note). The answered cards' lock also guards the marks of
/// what the live sessions' last save wrote: the two are saved in one write.
#[derive(Default)]
pub(super) struct SaveLocks {
    review_queue: Mutex<()>,
    parked: Mutex<()>,
    answered: Mutex<LiveMarks>,
}

/// What the live sessions' last save wrote, as a hash of each record (and of the waiting cards), so the next
/// save writes only what changed since: every draw saves, and most draws change a session or two.
#[derive(Default)]
pub(super) struct LiveMarks {
    sessions: HashMap<String, u64>,
    waiting: Option<u64>,
}

impl LiveMarks {
    /// The sessions whose records differ from what was last saved, whether the waiting cards do, and the marks
    /// to keep once they're saved (a session no longer live is forgotten). A save that fails keeps the old
    /// marks, so the next one writes it all again.
    fn changes<'a>(&self, sessions: &'a [crate::sessions::Session], waiting: &[&Item]) -> (Vec<&'a crate::sessions::Session>, bool, LiveMarks) {
        let mut next = LiveMarks { sessions: HashMap::with_capacity(sessions.len()), waiting: Some(hash_of(&waiting)) };
        let mut write = Vec::new();
        for s in sessions {
            let mark = hash_of(s);
            if self.sessions.get(&s.origin.session_id) != Some(&mark) {
                write.push(s);
            }
            next.sessions.insert(s.origin.session_id.clone(), mark);
        }
        let waiting_changed = self.waiting != next.waiting;
        (write, waiting_changed, next)
    }
}

/// A hash of what `v` saves as.
fn hash_of(v: &impl serde::Serialize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
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

    /// The live sessions and waiting cards as the draw took them, saved as only what changed since the last save
    /// (`LiveMarks`), with the answered cards handed to the draw (`save_answered_with_draw`, unless deleted since)
    /// in the same write, under the lock a session's deletion takes.
    pub(super) fn save_live_with_answers(&self, sessions: &[crate::sessions::Session], waiting: &[&Item]) {
        let mut marks = crate::locking::lock(&self.saves.answered, "answered saves");
        let answered: Vec<Item> = self.write(|st| {
            std::mem::take(&mut st.unsaved).into_iter().filter_map(|a| {
                let purged = st.purged.get(a.session_id()).copied();
                a.unless_purged(purged)
            }).collect()
        });
        let (write, waiting_changed, next) = marks.changes(sessions, waiting);
        let live: Vec<&str> = sessions.iter().map(|s| s.origin.session_id.as_str()).collect();
        if crate::db::save_live(&crate::db::LiveSave { write: &write, live: &live, waiting: waiting_changed.then_some(waiting), answered: &answered }) {
            *marks = next;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, state: &str) -> crate::sessions::Session {
        serde_json::from_value(json!({
            "session_id": id, "harness": "claude", "cwd": "/x/proj", "project": "proj", "state": state,
            "driven_by": "", "queued": null, "since_ms": 1, "prompt": "", "segments": [], "thread": []
        }))
        .unwrap()
    }

    /// The first save writes every session; the next only the ones that changed, and the waiting cards only
    /// when they did. Marks not kept (a failed save) leave the next save writing it all again.
    #[test]
    fn a_save_writes_only_what_changed_since_the_last() {
        let ids = |w: &[&crate::sessions::Session]| w.iter().map(|s| s.origin.session_id.clone()).collect::<Vec<_>>();
        let mut marks = LiveMarks::default();
        let all = [session("a", "working"), session("b", "waiting")];
        let (write, waiting, next) = marks.changes(&all, &[]);
        assert_eq!((ids(&write), waiting), (vec!["a".to_string(), "b".to_string()], true));
        let (write, _, _) = marks.changes(&all, &[]);
        assert_eq!(write.len(), 2, "not kept: written again");
        marks = next;
        let changed = [session("a", "working"), session("b", "deciding")];
        let (write, waiting, next) = marks.changes(&changed, &[]);
        assert_eq!((ids(&write), waiting), (vec!["b".to_string()], false));
        marks = next;
        let (write, _, next) = marks.changes(&changed[..1], &[]);
        assert!(write.is_empty());
        assert_eq!(next.sessions.len(), 1, "one no longer live is forgotten");
    }
}
