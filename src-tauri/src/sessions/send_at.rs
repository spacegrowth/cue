//! A message Cue sends by itself later: at a time you picked ("Send at…"), or when a usage limit it stopped on resets
//! (Cue's own carry-on). Sent by the usage tick (hub/send_at.rs), so it needs nothing but Cue running.

use super::Sessions;
use serde::{Deserialize, Serialize};

/// What Cue says for a session that stopped on a usage limit mid-turn, once the limit resets.
pub const CARRY_ON: &str = "Carry on where you stopped: your usage limit reset.";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SendAt {
    pub text: String,
    /// When it goes (ms since the epoch); 0 = when the usage limit it stopped on resets (Cue's carry-on).
    #[serde(default)]
    pub at_ms: u64,
    /// Why it couldn't go when it was due ("" = it hasn't failed): it waits for you then (Send now, Cancel).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub failed: String,
}

impl Sessions {
    /// Its message for later, set or (None) dropped. False when the session is gone.
    pub fn set_send_at(&mut self, session_id: &str, later: Option<SendAt>) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        s.send_at = later;
        true
    }

    pub fn take_send_at(&mut self, session_id: &str) -> Option<SendAt> {
        self.0.get_mut(session_id)?.send_at.take()
    }

    /// Sessions whose message for later is due at `now`: its time came, or the limit it waits on reset. A carry-on whose
    /// session isn't limited any more (you sent it something, or it ran) is dropped here: it carried on without it.
    /// One that failed waits for you.
    pub fn send_at_due(&mut self, now: u64) -> Vec<String> {
        let mut due = vec![];
        for (sid, s) in self.0.iter_mut() {
            let Some(l) = &s.send_at else { continue };
            if !l.failed.is_empty() {
                continue;
            }
            if l.at_ms > 0 {
                if now >= l.at_ms {
                    due.push(sid.clone());
                }
            } else if s.state != "limited" {
                s.send_at = None;
            } else if s.limit.as_ref().and_then(|x| x.resets_ms).is_some_and(|r| r <= now) {
                due.push(sid.clone());
            }
        }
        due
    }

    /// A turn a usage limit stopped: Cue's carry-on, sent when the limit resets, unless a message is set for it already.
    pub fn carry_on(&mut self, session_id: &str) {
        if let Some(s) = self.0.get_mut(session_id).filter(|s| s.send_at.is_none()) {
            s.send_at = Some(SendAt { text: CARRY_ON.into(), at_ms: 0, failed: String::new() });
        }
    }

    /// It couldn't go: kept, with why, for you to send or cancel.
    pub fn send_at_failed(&mut self, session_id: &str, why: &str) {
        if let Some(l) = self.0.get_mut(session_id).and_then(|s| s.send_at.as_mut()) {
            l.failed = why.to_string();
        }
    }
}
