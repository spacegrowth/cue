//! The window's state as last built: what `get_state` (and the socket's `list`) hand out without building
//! one, so asking for the state never waits on the store, a transcript or git. Building it is the redraw
//! worker's (redraw.rs), never the window's thread.
//!
//! Some of what it shows changes with time alone (a lead busy for a few seconds after a review, a session
//! that just ended kept off the live list): nothing in the store changes then, so nobody would build it
//! again. Each build says when its next such change is (`NextChange`), and one timer per hub builds it
//! again at that moment (`Shown::rebuild_at`).

use serde_json::Value;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// The last snapshot built, if it's current, and the newest point in the store's order (`Store::shown_seq`)
/// it accounts for: two built at once on two threads land in either order, and the older one must never
/// replace the newer.
///
/// SHORTCUT: the order is the store's only. A build reads what's outside the store (transcripts, git, leads,
/// settings, the db) before it takes its number under the store's lock, so of two builds at once, the one
/// with the later number can carry the older outside reads, and it's the one kept until the next change or
/// redraw. Closed by Phase 2's caches (stability-plan §2.4): the outside facts
/// become values each with its own version, read in the same step as the store.
#[derive(Default)]
pub(super) struct Shown {
    kept: RwLock<(u64, Option<Arc<Value>>)>,
    /// The timer's thread, told each new time to build again (None: no time-based change ahead). Started
    /// by the first `rebuild_at`; it ends with this.
    timer: OnceLock<Sender<Option<Instant>>>,
}

impl Shown {
    /// The last snapshot, if one is current. Held for no longer than it takes to clone an `Arc`.
    pub(super) fn get(&self) -> Option<Arc<Value>> {
        crate::locking::read(&self.kept, "shown snapshot").1.clone()
    }

    /// `snap`, taken from the store at `seq`: kept unless a later one (or a later `forget`) is already in.
    /// Whether it was kept.
    pub(super) fn put(&self, seq: u64, snap: Arc<Value>) -> bool {
        let mut cur = crate::locking::write(&self.kept, "shown snapshot");
        let newer = seq > cur.0;
        if newer {
            *cur = (seq, Some(snap));
        }
        newer
    }

    /// The store changed after `seq` and nobody built the new state (no window to send it to): nothing taken
    /// at or before `seq` is current any more, and the next ask builds it.
    pub(super) fn forget(&self, seq: u64) {
        let mut cur = crate::locking::write(&self.kept, "shown snapshot");
        *cur = (cur.0.max(seq), None);
    }

    /// The kept snapshot changes with time alone `at` (ms since the epoch; None: it doesn't): `rebuild` runs
    /// then, once, on the timer's thread. Each call replaces the time set before, since the latest build
    /// knows every change still ahead. `rebuild` is asked for only the first time, to start the thread.
    pub(super) fn rebuild_at(&self, at: Option<u64>, rebuild: impl FnOnce() -> Box<dyn Fn() + Send>) {
        let timer = self.timer.get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            let run = rebuild();
            let _ = std::thread::Builder::new().name("cue-snapshot-timer".into()).spawn(move || wait_and_rebuild(rx, run));
            tx
        });
        let due = at.map(|at| Instant::now() + Duration::from_millis(at.saturating_sub(crate::model::now_ms())));
        let _ = timer.send(due);
    }
}

/// The timer: waits for the time it was last given, then runs `rebuild` (which gives it the next time).
fn wait_and_rebuild(times: Receiver<Option<Instant>>, rebuild: Box<dyn Fn() + Send>) {
    let mut due: Option<Instant> = None;
    loop {
        let next = match due {
            None => times.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => times.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match next {
            Ok(at) => due = at,
            Err(RecvTimeoutError::Timeout) => {
                due = None;
                rebuild();
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The earliest moment (ms since the epoch) at which something a snapshot shows changes with time alone:
/// each rule that compares the clock with a threshold says when it would flip.
#[derive(Default, Debug, PartialEq)]
pub(super) struct NextChange(Option<u64>);

impl NextChange {
    /// A rule flips `at`; kept when it's still ahead of `now` and the earliest so far.
    pub(super) fn at(&mut self, at: u64, now: u64) {
        if at > now {
            self.0 = Some(self.0.map_or(at, |c| c.min(at)));
        }
    }

    pub(super) fn when(&self) -> Option<u64> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The timer runs the rebuild at the time it was last given, not before; a new time replaces the old one,
    /// and None calls it off.
    #[test]
    fn a_time_based_change_is_rebuilt_at_its_time_and_not_before() {
        let s = Shown::default();
        let (ran, runs) = std::sync::mpsc::channel();
        let at = |ms: u64| Some(crate::model::now_ms() + ms);
        let t0 = Instant::now();
        s.rebuild_at(at(300), move || Box::new(move || { let _ = ran.send(Instant::now()); }));
        assert!(runs.recv_timeout(Duration::from_millis(200)).is_err(), "not before its time");
        let fired = runs.recv_timeout(Duration::from_millis(1000)).expect("at its time");
        assert!(fired - t0 >= Duration::from_millis(299) && fired - t0 < Duration::from_millis(700), "{:?}", fired - t0);
        assert!(runs.recv_timeout(Duration::from_millis(300)).is_err(), "once");
        // A later build moves it: the first time passes with nothing, the new one fires.
        let t1 = Instant::now();
        s.rebuild_at(at(200), || unreachable!("the thread is running already"));
        s.rebuild_at(at(500), || unreachable!());
        assert!(runs.recv_timeout(Duration::from_millis(400)).is_err(), "the replaced time doesn't fire");
        let fired = runs.recv_timeout(Duration::from_millis(1000)).expect("the new time fires");
        assert!(fired - t1 >= Duration::from_millis(499), "{:?}", fired - t1);
        // Nothing time-based ahead any more: called off.
        s.rebuild_at(at(150), || unreachable!());
        s.rebuild_at(None, || unreachable!());
        assert!(runs.recv_timeout(Duration::from_millis(400)).is_err(), "called off");
    }

    /// The hub's rules together: a session that just ended shows as live again JUST_ENDED_MS after, and a
    /// lead settling after a review is free REVIEW_SETTLE_MS after it; the earlier one is next.
    #[test]
    fn the_hub_says_its_next_time_based_change() {
        let mut st = super::super::Store::default();
        let t = 1_000_000;
        assert_eq!(super::super::Hub::next_change(&st, t).when(), None, "nothing time-based");
        st.just_ended.insert("S".into(), t);
        assert_eq!(super::super::Hub::next_change(&st, t + 1).when(), Some(t + super::super::sessions::JUST_ENDED_MS));
        st.review_last.insert("L".into(), t + 5_000);
        let both = super::super::Hub::next_change(&st, t + 6_000).when();
        assert_eq!(both, Some((t + super::super::sessions::JUST_ENDED_MS).min(t + 5_000 + super::super::reviews::REVIEW_SETTLE_MS)));
    }

    #[test]
    fn the_next_change_is_the_earliest_still_ahead() {
        let mut n = NextChange::default();
        assert_eq!(n.when(), None);
        n.at(900, 1000);
        assert_eq!(n.when(), None, "already past");
        n.at(5000, 1000);
        n.at(3000, 1000);
        n.at(4000, 1000);
        assert_eq!(n.when(), Some(3000));
    }

    #[test]
    fn an_older_snapshot_landing_late_never_replaces_a_newer_one() {
        let s = Shown::default();
        assert!(s.get().is_none());
        s.put(2, Arc::new(json!("newer")));
        s.put(1, Arc::new(json!("older")));
        assert_eq!(*s.get().unwrap(), json!("newer"));
        s.put(3, Arc::new(json!("newest")));
        assert_eq!(*s.get().unwrap(), json!("newest"));
        // Changed at 5: one built from before that, landing now, isn't current; one built after is.
        s.forget(5);
        assert!(s.get().is_none());
        s.put(4, Arc::new(json!("stale")));
        assert!(s.get().is_none(), "built before the change");
        s.put(6, Arc::new(json!("rebuilt")));
        assert_eq!(*s.get().unwrap(), json!("rebuilt"));
    }

    /// A hub call that holds the store (a slow one, or one stuck): the window still gets its state at once,
    /// and after a change, the state that has it.
    #[test]
    fn the_state_comes_back_while_the_store_is_held_and_is_the_latest_after_a_change() {
        use crate::model::Origin;
        use std::sync::mpsc::channel;
        let dir = std::env::temp_dir().join(format!("cue-shown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Arc::new(super::super::Hub::new(None));
        assert!(hub.kept_state().is_none(), "nothing built yet: get_state builds it on the pool");
        assert!(hub.state()["items"].as_array().unwrap().is_empty(), "the first ask builds it");
        assert!(hub.kept_state().is_some(), "and then it's kept: get_state answers without the pool");
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        hub.event(origin, "stopped", "Here's the fix.".into(), vec![], "");
        let after = hub.state();
        assert_eq!(after["items"].as_array().unwrap().len(), 1, "the change is in the next one");
        let (held_tx, held) = channel();
        let (go, release) = channel::<()>();
        let h = hub.clone();
        let holder = std::thread::spawn(move || h.hold_store(held_tx, release));
        held.recv().unwrap();
        let before = crate::model::now_ms();
        let (h, (tx, rx)) = (hub.clone(), channel());
        std::thread::spawn(move || tx.send(h.kept_state().expect("kept")));
        let got = rx.recv_timeout(std::time::Duration::from_millis(500)).expect("handed out without waiting for the store");
        go.send(()).unwrap();
        holder.join().unwrap();
        assert_eq!(got["items"], after["items"]);
        assert!(got["now_ms"].as_u64().unwrap() >= before, "its clock reads now, not when it was built");
    }
}
