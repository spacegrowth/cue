//! The live sessions, answered cards, ended sessions and deletions, written to cue.db by one writer, in the
//! order they're handed in. A draw (redraw.rs) hands in what it took from the store and goes on: it never
//! waits on the database, and neither does an answer, whose card rides along with the next draw's save. With
//! the app running the writer is a thread of its own, and work handed in while it writes waits in one job:
//! the newest live sessions replace older ones not yet written (each save writes the store as it is, so the
//! latest is all that counts), the rest is kept in order. Without the app (tests, the World) the writing is
//! done where it's handed in, so each test reads the database right after.
//!
//! The writer keeps what the last save wrote (`LiveMarks`) and writes only the sessions that changed since,
//! and which sessions were deleted, so no card of theirs answered before then is saved after.

use super::saves::LiveMarks;
use crate::model::Item;
use crate::sessions::Session;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// What's to be written, as one job.
#[derive(Default)]
pub(super) struct Job {
    /// The live sessions and the pending finished-turn cards, as the store had them (None: nothing new).
    pub live: Option<(Vec<Session>, Vec<Item>)>,
    /// Answered cards, each saved once (sessions deleted since already left out).
    pub answered: Vec<Item>,
    /// Sessions that ended, each as it was when it did, and when.
    pub ended: Vec<(u64, Session)>,
    /// Sessions deleted, and when: everything saved of them goes, after anything of theirs handed in before,
    /// and a card of theirs answered before then that's handed in after is never saved.
    pub deletes: Vec<(String, u64)>,
}

impl Job {
    fn is_empty(&self) -> bool {
        self.live.is_none() && self.answered.is_empty() && self.ended.is_empty() && self.deletes.is_empty()
    }

    /// `later` folded in: its live sessions are the newer, everything else follows what's here.
    fn then(&mut self, later: Job) {
        if later.live.is_some() {
            self.live = later.live;
        }
        self.answered.extend(later.answered);
        self.ended.extend(later.ended);
        self.deletes.extend(later.deletes);
    }
}

/// What the last save wrote, and the sessions deleted (by id, when).
#[derive(Default)]
struct Written {
    marks: LiveMarks,
    deleted: std::collections::HashMap<String, u64>,
}

#[derive(Default)]
struct Queue {
    job: Job,
    /// The writer is writing a job taken from here.
    writing: bool,
}

#[derive(Default)]
pub(super) struct LiveWrites {
    queue: Mutex<Queue>,
    /// Wakes the writer (work handed in) and whoever waits for it to be done (`flush`).
    wake: Condvar,
    /// What the writer keeps: only it takes this (or, without a writer thread, whoever writes).
    written: Mutex<Written>,
    worker: OnceLock<()>,
}

impl LiveWrites {
    /// `job` written: by the writer thread when `threaded` (started at the first), here otherwise.
    pub(super) fn hand(self: &Arc<Self>, job: Job, threaded: bool) {
        if job.is_empty() {
            return;
        }
        if !threaded {
            return self.write(job);
        }
        self.worker.get_or_init(|| {
            let me = self.clone();
            let _ = std::thread::Builder::new().name("cue-db-writer".into()).spawn(move || me.run());
        });
        crate::locking::lock(&self.queue, "db writes").job.then(job);
        self.wake.notify_all();
    }

    /// Returns once everything handed in so far is written (quitting: nothing changed is lost).
    pub(super) fn flush(&self) {
        let mut q = crate::locking::lock(&self.queue, "db writes");
        while q.writing || !q.job.is_empty() {
            q = crate::locking::Guard::wait(q, &self.wake);
        }
    }

    /// The writer: each job as it comes, until the app goes.
    fn run(&self) {
        loop {
            let job = {
                let mut q = crate::locking::lock(&self.queue, "db writes");
                while q.job.is_empty() {
                    q = crate::locking::Guard::wait(q, &self.wake);
                }
                q.writing = true;
                std::mem::take(&mut q.job)
            };
            self.write(job);
            crate::locking::lock(&self.queue, "db writes").writing = false;
            self.wake.notify_all();
        }
    }

    /// One job to the database, in its order: the live sessions (what changed of them) and the answered cards
    /// in one write, then the ended ones (not live again since), then the deletions.
    fn write(&self, job: Job) {
        let mut w = crate::locking::lock(&self.written, "db writer's marks");
        let Job { live, mut answered, ended, deletes } = job;
        answered.retain(|it| w.deleted.get(&it.origin.session_id).is_none_or(|at| it.resolved_ms.unwrap_or(0) > *at));
        let live_ids: Vec<String> = live.iter().flat_map(|(s, _)| s.iter().map(|s| s.origin.session_id.clone())).collect();
        match &live {
            Some((sessions, waiting)) => w.marks = save_live(&w.marks, sessions, waiting, &answered),
            None => answered.iter().for_each(crate::db::save_answered),
        }
        let ended: Vec<(u64, Session)> = ended.into_iter().filter(|(_, s)| !live_ids.contains(&s.origin.session_id)).collect();
        crate::db::save_ended(&ended);
        for (sid, at) in deletes {
            crate::db::delete_session(&sid);
            w.deleted.insert(sid, at);
        }
    }
}

/// The live sessions that changed since `marks` and the waiting cards if they did, with `answered`, in one
/// write: the marks to keep after (the old ones if the write failed, so the next writes it all again).
fn save_live(marks: &LiveMarks, sessions: &[Session], waiting: &[Item], answered: &[Item]) -> LiveMarks {
    let waiting: Vec<&Item> = waiting.iter().collect();
    let (write, waiting_changed, next) = marks.changes(sessions, &waiting);
    let ids: Vec<&str> = sessions.iter().map(|s| s.origin.session_id.as_str()).collect();
    let saved = crate::db::save_live(&crate::db::LiveSave { write: &write, live: &ids, waiting: waiting_changed.then_some(&waiting), answered });
    if saved { next } else { marks.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answered(id: &str, sid: &str, at: u64) -> Item {
        serde_json::from_value(json!({
            "id": id, "kind": "permission", "session_id": sid, "harness": "claude", "cwd": "/x", "project": "x", "tool_name": "Bash",
            "tool_input": {}, "suggestions": null, "context": [], "message": "", "created_ms": at - 1, "status": "answered",
            "outcome": "allowed", "resolved_ms": at,
        }))
        .unwrap()
    }

    /// On the writer's thread, in the order handed in: a deleted session's card answered before its deletion is
    /// never saved, even handed in after it; one answered after is; `flush` returns once it's all written.
    #[test]
    fn the_writer_writes_in_order_and_never_brings_back_a_deleted_sessions_answer() {
        let dir = std::env::temp_dir().join(format!("cue-writer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let w = Arc::new(LiveWrites::default());
        let job = |answered: Vec<Item>, deletes: Vec<(String, u64)>| Job { answered, deletes, ..Default::default() };
        w.hand(job(vec![answered("a", "s", 10), answered("k", "other", 10)], vec![]), true);
        w.hand(job(vec![], vec![("s".into(), 20)]), true);
        w.hand(job(vec![answered("late", "s", 15), answered("after", "s", 30)], vec![]), true);
        w.flush();
        let ids: Vec<String> = crate::db::history(50).into_iter().map(|i| i.id).collect();
        assert!(ids.contains(&"k".to_string()) && ids.contains(&"after".to_string()), "{ids:?}");
        assert!(!ids.contains(&"a".to_string()) && !ids.contains(&"late".to_string()), "{ids:?}");
        std::env::remove_var("CUE_HOME");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn work_handed_in_meanwhile_is_one_job_with_the_newest_live_sessions() {
        let mut j = Job { live: Some((vec![], vec![answered("w1", "s", 1)])), answered: vec![answered("a", "s", 1)], ..Default::default() };
        j.then(Job { live: Some((vec![], vec![])), answered: vec![answered("b", "s", 2)], ..Default::default() });
        j.then(Job { deletes: vec![("s".into(), 3)], ..Default::default() });
        assert_eq!(j.live.as_ref().map(|(_, w)| w.len()), Some(0), "the newer live sessions");
        assert_eq!(j.answered.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(j.deletes.len(), 1);
        assert!(!j.is_empty() && Job::default().is_empty());
    }
}
