//! The window's state as last built: what `get_state` (and the socket's `list`) hand out without building
//! one, so asking for the state never waits on the store, a transcript or git. Building it is the work
//! of whoever changed something (`Hub::redraw`, `Hub::changed`), on that thread, never the window's.

use serde_json::Value;
use std::sync::{Arc, RwLock};

/// The last snapshot built, if it's current, and the newest point in the store's order (`Store::shown_seq`)
/// it accounts for: two built at once on two threads land in either order, and the older one must never
/// replace the newer.
#[derive(Default)]
pub(super) struct Shown(RwLock<(u64, Option<Arc<Value>>)>);

impl Shown {
    /// The last snapshot, if one is current. Held for no longer than it takes to clone an `Arc`.
    pub(super) fn get(&self) -> Option<Arc<Value>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).1.clone()
    }

    /// `snap`, taken from the store at `seq`: kept unless a later one (or a later `forget`) is already in.
    pub(super) fn put(&self, seq: u64, snap: Arc<Value>) {
        let mut cur = self.0.write().unwrap_or_else(|e| e.into_inner());
        if seq > cur.0 {
            *cur = (seq, Some(snap));
        }
    }

    /// The store changed after `seq` and nobody built the new state (no window to send it to): nothing taken
    /// at or before `seq` is current any more, and the next ask builds it.
    pub(super) fn forget(&self, seq: u64) {
        let mut cur = self.0.write().unwrap_or_else(|e| e.into_inner());
        *cur = (cur.0.max(seq), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(hub.state()["items"].as_array().unwrap().is_empty(), "the first ask builds it");
        let origin = Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        hub.event(origin, "stopped", "Here's the fix.".into(), vec![], "");
        let after = hub.state();
        assert_eq!(after["items"].as_array().unwrap().len(), 1, "the change is in the next one");
        let (held_tx, held) = channel();
        let (go, release) = channel::<()>();
        let h = hub.clone();
        let holder = std::thread::spawn(move || {
            let _st = h.st();
            held_tx.send(()).unwrap();
            let _ = release.recv();
        });
        held.recv().unwrap();
        let before = crate::model::now_ms();
        let (h, (tx, rx)) = (hub.clone(), channel());
        std::thread::spawn(move || tx.send(h.state()));
        let got = rx.recv_timeout(std::time::Duration::from_millis(500)).expect("handed out without waiting for the store");
        go.send(()).unwrap();
        holder.join().unwrap();
        assert_eq!(got["items"], after["items"]);
        assert!(got["now_ms"].as_u64().unwrap() >= before, "its clock reads now, not when it was built");
    }
}
