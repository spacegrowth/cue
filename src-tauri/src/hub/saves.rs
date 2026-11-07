//! Saving what the store keeps across a restart, after the store is let go. A change and its save are two
//! steps, so two changes' saves can reach the database in either order; each save therefore writes what the
//! store holds now, not what its change made, under a lock of its own per collection. Whichever save runs
//! last runs after every change before it, so the database ends as the store is: never an older state
//! written over a newer one. Lock order: a save lock, then the store (a look), then the database.
//!
//! The live sessions, answered cards and deletions go through one writer instead (live_writes.rs), in order:
//! an answered card is saved once, under its own id, and never changed after; what it can race is its session
//! being deleted (purge), which the writer takes after anything handed in before it, and a card answered
//! after its session was deleted is left out as it's handed in.

use super::*;

/// One lock per saved collection (see the module's note), and the writer of the live sessions and answers.
#[derive(Default)]
pub(super) struct SaveLocks {
    review_queue: Mutex<()>,
    parked: Mutex<()>,
    pub(super) live: Arc<super::live_writes::LiveWrites>,
}

/// What the live sessions' last save wrote, as a hash of each record (and of the waiting cards), so the next
/// save writes only what changed since: every draw saves, and most draws change a session or two.
#[derive(Default, Clone)]
pub(super) struct LiveMarks {
    sessions: HashMap<String, u64>,
    waiting: Option<u64>,
}

impl LiveMarks {
    /// The sessions whose records differ from what was last saved, whether the waiting cards do, and the marks
    /// to keep once they're saved (a session no longer live is forgotten). A save that fails keeps the old
    /// marks, so the next one writes it all again.
    pub(super) fn changes<'a>(&self, sessions: &'a [crate::sessions::Session], waiting: &[&Item]) -> (Vec<&'a crate::sessions::Session>, bool, LiveMarks) {
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

    /// `session_id`'s saved row is gone (deleted): it's written in full the next time it's live.
    pub(super) fn forget(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
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
        let purged = self.read(|st| st.purged.get(answered.session_id()).copied());
        let answered = answered.unless_purged(purged);
        self.hand_writes(super::live_writes::Job { answered: answered.into_iter().collect(), ..Default::default() });
    }

    /// An answered card saved by the draw its change asks for (redraw.rs `save_live`), in the same write as the
    /// live sessions: whoever answered never waits on the database for it. A hub that never draws saves it here.
    pub(super) fn save_answered_with_draw(&self, answered: super::history::Answered) {
        if !self.draws_later() {
            return self.save_answered(answered);
        }
        self.write(|st| st.unsaved.push(answered));
    }

    /// The answered cards handed to the draw (`save_answered_with_draw`), but not those whose session was deleted
    /// since: taken for the draw's save.
    pub(super) fn take_unsaved(st: &mut Store) -> Vec<Item> {
        std::mem::take(&mut st.unsaved).into_iter().filter_map(|a| {
            let purged = st.purged.get(a.session_id()).copied();
            a.unless_purged(purged)
        }).collect()
    }

    /// `job` to the database writer: its thread when the app runs, else written here (live_writes.rs).
    pub(super) fn hand_writes(&self, job: super::live_writes::Job) {
        self.saves.live.hand(job, self.app.is_some());
    }

    /// Everything saved of a session deleted (its record, chat, answers), after any save of it on its way.
    pub(super) fn delete_saved_session(&self, session_id: &str) {
        let at = self.read(|st| st.purged.get(session_id).copied()).unwrap_or_else(now_ms);
        self.hand_writes(super::live_writes::Job { deletes: vec![(session_id.to_string(), at)], ..Default::default() });
    }

    /// `session_id`'s parked record saved as the store has it now: parked (as it is, star and all), or not, by
    /// the database writer.
    pub(super) fn save_parked(&self, session_id: &str) {
        self.hand_in_parked_turn(|| {
            let p = self.read(|st| st.parked.iter().find(|p| p.session.origin.session_id == session_id).cloned());
            super::live_writes::Job { parked: vec![(session_id.to_string(), p)], ..Default::default() }
        });
    }

    /// The job `make` builds (reading the store's parked records), handed to the writer before any other parked
    /// save can read the store: so saves of the same record are handed in, and land, in the order it changed.
    pub(super) fn hand_in_parked_turn(&self, make: impl FnOnce() -> super::live_writes::Job) {
        let _saving = crate::locking::lock(&self.saves.parked, "parked saves");
        self.hand_writes(make());
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
