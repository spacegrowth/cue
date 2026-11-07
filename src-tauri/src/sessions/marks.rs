//! Your own marks on a session: put off for later, starred, and "tell me" when its turn is done.

use super::Sessions;

impl Sessions {
    /// Put off for later, or back (`on` false). True if that changed anything.
    pub fn set_later(&mut self, session_id: &str, on: bool) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        if (s.later_ms > 0) == on {
            return false;
        }
        s.later_ms = if on { crate::model::now_ms() } else { 0 };
        true
    }

    /// Star it, or unstar it (`on` false). True if that changed anything.
    /// A star carried over to a session's new record (Move to Cue), with when it was first starred.
    pub fn restore_star(&mut self, session_id: &str, starred_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.starred_ms = starred_ms;
        }
    }

    pub fn set_starred(&mut self, session_id: &str, on: bool) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        if (s.starred_ms > 0) == on {
            return false;
        }
        s.starred_ms = if on { crate::model::now_ms() } else { 0 };
        true
    }

    /// "Tell me" on this turn, or off (`on` false). True if that changed anything.
    pub fn set_watch(&mut self, session_id: &str, on: bool) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        if (s.watch_ms > 0) == on {
            return false;
        }
        s.watch_ms = if on { crate::model::now_ms() } else { 0 };
        true
    }

    /// Its turn ended: whether you asked to be told (and the ask is spent).
    pub fn take_watch(&mut self, session_id: &str) -> bool {
        self.0.get_mut(session_id).is_some_and(|s| std::mem::take(&mut s.watch_ms) > 0)
    }

    /// The turn you asked about ended, after `took` ms: its chat says so until the next one starts.
    pub fn set_told(&mut self, session_id: &str, took: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.told_ms = took.max(1);
        }
    }

    pub fn is_later(&self, session_id: &str) -> bool {
        self.0.get(session_id).is_some_and(|s| s.later_ms > 0)
    }
}
