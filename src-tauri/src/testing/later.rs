//! The hub's follow-up work (hub::ports::Later), held until the World runs it: at once when it's due now
//! (`World::settle`), or as the World's clock reaches it (`World::advance`). Nothing runs on a thread of
//! its own, so nothing outlives the test or races it.

use crate::hub::ports::{Job, Later};
use crate::hub::Hub;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Deferred {
    /// Each job with when it's due and the order it was handed in (two due at once run in that order).
    jobs: Mutex<Vec<(u64, u64, Job)>>,
    handed: std::sync::atomic::AtomicU64,
}

impl Later for Deferred {
    fn run(&self, _me: Option<Arc<Hub>>, after_ms: u64, job: Job) {
        let n = self.handed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::locking::lock(&self.jobs, "deferred jobs").push((crate::clock::now_ms() + after_ms, n, job));
    }
}

impl Deferred {
    /// When the next job is due, if there is one.
    pub fn next_due(&self) -> Option<u64> {
        crate::locking::lock(&self.jobs, "deferred jobs").iter().map(|(due, ..)| *due).min()
    }

    /// The earliest job due by `now`, taken off the queue.
    pub fn take_due(&self, now: u64) -> Option<Job> {
        let mut jobs = crate::locking::lock(&self.jobs, "deferred jobs");
        let i = jobs.iter().enumerate().filter(|(_, (due, ..))| *due <= now).min_by_key(|(_, (due, n, _))| (*due, *n)).map(|(i, _)| i)?;
        Some(jobs.remove(i).2)
    }

    pub fn len(&self) -> usize {
        crate::locking::lock(&self.jobs, "deferred jobs").len()
    }
}
