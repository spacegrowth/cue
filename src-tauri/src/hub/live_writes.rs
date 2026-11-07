//! The live sessions, answered cards, ended sessions and deletions, written to cue.db by one writer, in the
//! order they're handed in. A draw (redraw.rs) hands in what it took from the store and goes on: it never
//! waits on the database, and neither does an answer, whose card rides along with the next draw's save. With
//! the app running the writer is a thread of its own, and work handed in while it writes waits its turn, in
//! the order handed in: the newest live sessions replace older ones not yet written (each save writes the
//! store as it is, so the latest is all that counts), and the rest stays where it was handed in, so a session
//! deleted then resumed is saved again, and one saved then deleted stays deleted. Without the app (tests, the
//! World), or if the writer's thread couldn't start, the writing is done where it's handed in.
//!
//! The writer keeps what the last save wrote (`LiveMarks`) and writes only the sessions that changed since,
//! and which sessions were deleted, so nothing of theirs from before then (an answered card, the record of
//! its end) is saved after.

use super::saves::LiveMarks;
use crate::model::Item;
use crate::sessions::Session;
use std::collections::VecDeque;
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
}

/// What the last save wrote, and the sessions deleted (by id, when).
#[derive(Default)]
struct Written {
    marks: LiveMarks,
    deleted: std::collections::HashMap<String, u64>,
}

/// The jobs not written yet, in the order handed in.
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// The writer is writing jobs taken from here.
    writing: bool,
}

impl Queue {
    /// `job` after the others; its live sessions, when it has them, replace the older ones not yet written.
    fn push(&mut self, job: Job) {
        if job.live.is_some() {
            self.jobs.iter_mut().for_each(|j| j.live = None);
            self.jobs.retain(|j| !j.is_empty());
        }
        self.jobs.push_back(job);
    }
}

/// How the writer's thread is started (a test's can refuse, as the system can).
type Spawn = fn(Box<dyn FnOnce() + Send>) -> std::io::Result<()>;

fn spawn_writer(run: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    std::thread::Builder::new().name("cue-db-writer".into()).spawn(run).map(drop)
}

pub(super) struct LiveWrites {
    queue: Mutex<Queue>,
    /// Wakes the writer (work handed in) and whoever waits for it to be done (`flush`).
    wake: Condvar,
    /// What the writer keeps: only it takes this (or, without a writer thread, whoever writes).
    written: Mutex<Written>,
    /// Whether the writer's thread runs: started at the first threaded hand-in, once.
    worker: OnceLock<bool>,
    spawn: Spawn,
}

impl Default for LiveWrites {
    fn default() -> Self {
        Self { queue: Default::default(), wake: Default::default(), written: Default::default(), worker: Default::default(), spawn: spawn_writer }
    }
}

impl LiveWrites {
    /// `job` written: by the writer thread when `threaded` (started at the first), here otherwise, and here
    /// too if the thread couldn't start (nothing waits on a writer that isn't there).
    pub(super) fn hand(self: &Arc<Self>, job: Job, threaded: bool) {
        if job.is_empty() {
            return;
        }
        if !threaded || !self.writer_runs() {
            return self.write(job);
        }
        crate::locking::lock(&self.queue, "db writes").push(job);
        self.wake.notify_all();
    }

    /// The writer's thread, started if it isn't yet: whether it runs.
    fn writer_runs(self: &Arc<Self>) -> bool {
        *self.worker.get_or_init(|| {
            let me = self.clone();
            let started = (self.spawn)(Box::new(move || me.run()));
            if let Err(e) = &started {
                eprintln!("cue: the database writer couldn't start ({e}): saves are written where they're made");
            }
            started.is_ok()
        })
    }

    /// Returns once everything handed in so far is written (quitting: nothing changed is lost).
    pub(super) fn flush(&self) {
        let mut q = crate::locking::lock(&self.queue, "db writes");
        while q.writing || !q.jobs.is_empty() {
            q = crate::locking::Guard::wait(q, &self.wake);
        }
    }

    /// The writer: the jobs waiting, in order, as they come, until the app goes.
    fn run(&self) {
        loop {
            let jobs = {
                let mut q = crate::locking::lock(&self.queue, "db writes");
                while q.jobs.is_empty() {
                    q = crate::locking::Guard::wait(q, &self.wake);
                }
                q.writing = true;
                std::mem::take(&mut q.jobs)
            };
            jobs.into_iter().for_each(|job| self.write(job));
            crate::locking::lock(&self.queue, "db writes").writing = false;
            self.wake.notify_all();
        }
    }

    /// One job to the database, in its order: the live sessions (what changed of them) and the answered cards
    /// in one write, then the ended ones (not live again, nor deleted, since), then the deletions.
    fn write(&self, job: Job) {
        let mut w = crate::locking::lock(&self.written, "db writer's marks");
        let Job { live, mut answered, ended, deletes } = job;
        answered.retain(|it| w.deleted.get(&it.origin.session_id).is_none_or(|at| it.resolved_ms.unwrap_or(0) > *at));
        let live_ids: Vec<String> = live.iter().flat_map(|(s, _)| s.iter().map(|s| s.origin.session_id.clone())).collect();
        match &live {
            Some((sessions, waiting)) => w.marks = save_live(&w.marks, sessions, waiting, &answered),
            None => answered.iter().for_each(crate::db::save_answered),
        }
        // Not live again since, and not deleted since it ended.
        let ended: Vec<(u64, Session)> = ended
            .into_iter()
            .filter(|(at, s)| !live_ids.contains(&s.origin.session_id) && w.deleted.get(&s.origin.session_id).is_none_or(|del| at > del))
            .collect();
        crate::db::save_ended(&ended);
        for (sid, at) in deletes {
            crate::db::delete_session(&sid);
            // Its row is gone: live again (resumed), it's written whole, not judged unchanged.
            w.marks.forget(&sid);
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

    fn session(id: &str) -> Session {
        serde_json::from_value(json!({
            "session_id": id, "harness": "claude", "cwd": "/x/proj", "project": "proj", "state": "working",
            "driven_by": "", "queued": null, "since_ms": 1, "prompt": "", "segments": [], "thread": []
        }))
        .unwrap()
    }

    /// A database of the test's own while it runs (CUE_HOME and the database's turn held).
    struct Db(std::path::PathBuf, #[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

    impl Db {
        fn new(name: &str) -> Self {
            let guard = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!("cue-writer-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("CUE_HOME", &dir);
            crate::db::reset();
            Db(dir, guard)
        }
    }

    impl Drop for Db {
        fn drop(&mut self) {
            std::env::remove_var("CUE_HOME");
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn live(sids: &[&str]) -> Job {
        Job { live: Some((sids.iter().map(|s| session(s)).collect(), vec![])), ..Default::default() }
    }

    fn deletes(sid: &str, at: u64) -> Job {
        Job { deletes: vec![(sid.into(), at)], ..Default::default() }
    }

    /// On the writer's thread, in the order handed in: a deleted session's card answered before its deletion is
    /// never saved, even handed in after it; one answered after is; `flush` returns once it's all written.
    #[test]
    fn the_writer_writes_in_order_and_never_brings_back_a_deleted_sessions_answer() {
        let _db = Db::new("order");
        let w = Arc::new(LiveWrites::default());
        let job = |answered: Vec<Item>, deletes: Vec<(String, u64)>| Job { answered, deletes, ..Default::default() };
        w.hand(job(vec![answered("a", "s", 10), answered("k", "other", 10)], vec![]), true);
        w.hand(job(vec![], vec![("s".into(), 20)]), true);
        w.hand(job(vec![answered("late", "s", 15), answered("after", "s", 30)], vec![]), true);
        w.flush();
        let ids: Vec<String> = crate::db::history(50).into_iter().map(|i| i.id).collect();
        assert!(ids.contains(&"k".to_string()) && ids.contains(&"after".to_string()), "{ids:?}");
        assert!(!ids.contains(&"a".to_string()) && !ids.contains(&"late".to_string()), "{ids:?}");
    }

    /// Handed in while the writer is busy: a session deleted, then live again (resumed, its record unchanged),
    /// is saved live, as the order says, not deleted after it.
    #[test]
    fn a_session_deleted_then_live_again_while_the_writer_waits_is_saved_live() {
        let _db = Db::new("resumed");
        let w = Arc::new(LiveWrites::default());
        w.hand(live(&["s"]), true);
        w.flush();
        let (held_tx, held) = std::sync::mpsc::channel();
        let (release_tx, release) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || crate::db::hold(held_tx, release));
        held.recv().unwrap();
        w.hand(Job { answered: vec![answered("x", "other", 5)], ..Default::default() }, true);
        w.hand(deletes("s", 10), true);
        w.hand(live(&["s"]), true);
        release_tx.send(()).unwrap();
        holder.join().unwrap();
        w.flush();
        assert_eq!(crate::db::live().0.iter().map(|s| s.origin.session_id.as_str()).collect::<Vec<_>>(), ["s"]);
    }

    /// A session that ended before it was deleted, its end handed in after the deletion: not saved as ended.
    #[test]
    fn an_end_handed_in_after_its_sessions_deletion_isnt_saved() {
        let _db = Db::new("ended");
        let w = Arc::new(LiveWrites::default());
        w.hand(deletes("s", 20), false);
        w.hand(Job { ended: vec![(15, session("s")), (25, session("t"))], ..Default::default() }, false);
        assert!(crate::db::ended_session("s").is_none(), "deleted after it ended");
        assert!(crate::db::ended_session("t").is_some());
    }

    /// The writer's thread can't start: each job is written where it's handed in, and `flush` doesn't wait
    /// on a writer that isn't there.
    #[test]
    fn without_a_writer_thread_jobs_are_written_where_handed_in_and_flush_returns() {
        let _db = Db::new("nothread");
        let w = Arc::new(LiveWrites { spawn: |_| Err(std::io::Error::other("no threads left")), ..Default::default() });
        w.hand(Job { answered: vec![answered("a", "s", 10)], ..Default::default() }, true);
        let (done_tx, done) = std::sync::mpsc::channel();
        let flushing = w.clone();
        std::thread::spawn(move || {
            flushing.flush();
            let _ = done_tx.send(());
        });
        assert!(done.recv_timeout(std::time::Duration::from_secs(5)).is_ok(), "flush waited on a writer that never started");
        assert!(crate::db::history(50).iter().any(|i| i.id == "a"));
    }

    /// Work waiting for the writer: in the order handed in, the newest live sessions replacing older ones.
    #[test]
    fn work_waiting_keeps_its_order_with_only_the_newest_live_sessions() {
        let mut q = Queue::default();
        q.push(Job { live: Some((vec![], vec![answered("w1", "s", 1)])), answered: vec![answered("a", "s", 1)], ..Default::default() });
        q.push(deletes("s", 3));
        q.push(live(&["s"]));
        let shape: Vec<(bool, usize, usize)> = q.jobs.iter().map(|j| (j.live.is_some(), j.answered.len(), j.deletes.len())).collect();
        assert_eq!(shape, [(false, 1, 0), (false, 0, 1), (true, 0, 0)], "the older live sessions dropped, the delete before the newer");
        q.push(live(&[]));
        assert_eq!(q.jobs.len(), 3, "a job left with nothing goes");
        assert!(Job::default().is_empty());
    }
}
