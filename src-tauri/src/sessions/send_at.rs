//! A message Cue sends by itself later: at a time you picked ("Send at…"), or when a usage limit it stopped on resets
//! (Cue's own carry-on). Sent by the usage tick (hub/send_at.rs), so it needs nothing but Cue running.

use super::Sessions;
use serde::{Deserialize, Serialize};

/// What Cue says for a session that stopped on a usage limit mid-turn, once the limit resets.
pub const CARRY_ON: &str = "Carry on where you stopped: your usage limit reset.";

/// A carry-on goes this long after the limit resets (on the dot, it can still bounce off it), and a carry-on that
/// bounced off the limit again is tried this long after.
pub const CARRY_ON_DELAY_MS: u64 = 60_000;

/// How many carry-ons Cue sends in a row that bounce off the limit before it leaves the next one to you.
pub const CARRY_ON_TRIES: u8 = 3;

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

    /// Sessions whose message for later is due at `now`: its time came, or a minute passed since the limit it waits on
    /// reset (or, for a carry-on that bounced off the limit, since it bounced). A carry-on whose session isn't limited
    /// any more (you sent it something, or it ran) is dropped here: it carried on without it. One that failed waits for you.
    pub fn send_at_due(&mut self, now: u64) -> Vec<String> {
        let mut due = vec![];
        for (sid, s) in self.0.iter_mut() {
            // It ended a turn on its own since Cue's last carry-on: the next limit starts its tries afresh.
            if s.carry_tries > 0 && s.send_at.is_none() && (s.state == "done" || s.state == "waiting") {
                s.carry_tries = 0;
            }
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
            } else {
                let from = s.limit.as_ref().and_then(|x| if s.carry_tries > 0 { Some(x.since_ms) } else { x.resets_ms });
                if from.is_some_and(|r| r + CARRY_ON_DELAY_MS <= now) {
                    due.push(sid.clone());
                }
            }
        }
        due
    }

    /// A turn a usage limit stopped: Cue's carry-on, sent a minute after the limit resets, unless a message is set for
    /// it already. When Cue's last CARRY_ON_TRIES carry-ons all bounced off the limit, this one waits for you.
    pub fn carry_on(&mut self, session_id: &str) {
        if let Some(s) = self.0.get_mut(session_id).filter(|s| s.send_at.is_none()) {
            let failed = if s.carry_tries >= CARRY_ON_TRIES {
                format!("Still out of usage after {CARRY_ON_TRIES} tries")
            } else {
                String::new()
            };
            s.send_at = Some(SendAt { text: CARRY_ON.into(), at_ms: 0, failed });
        }
    }

    /// Cue sent a carry-on: counted, so one that bounces off the limit is tried again only so often.
    pub fn carry_on_sent(&mut self, session_id: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.carry_tries = s.carry_tries.saturating_add(1);
        }
    }

    /// It couldn't go: kept, with why, for you to send or cancel.
    pub fn send_at_failed(&mut self, session_id: &str, why: &str) {
        if let Some(l) = self.0.get_mut(session_id).and_then(|s| s.send_at.as_mut()) {
            l.failed = why.to_string();
        }
    }
}
