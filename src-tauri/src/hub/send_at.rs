//! Messages Cue sends by itself later (sessions/send_at.rs): "Send at…" a time you picked, and Cue's carry-on for a
//! turn a usage limit stopped (Settings' /limits/carry_on, on by default). The usage tick sends what's due, so it
//! needs nothing but Cue running: not the session you scheduled it from, nor a terminal of yours watching.

use super::*;
use crate::sessions::send_at::SendAt;

/// How often Cue asks Claude Code whether a plan limit with no known reset is over, while a carry-on waits on it.
const PROBE_MS: u64 = 15 * 60 * 1000;

impl Hub {
    /// Keep `text` for `session_id`, sent by Cue at `at_ms` (it replaces one kept already).
    pub fn send_at(&self, session_id: &str, text: &str, at_ms: u64) -> Result<String, String> {
        if text.trim().is_empty() {
            return Err("nothing to send".into());
        }
        if at_ms <= now_ms() {
            return Err("that time has passed".into());
        }
        let later = SendAt { text: text.to_string(), at_ms, failed: String::new() };
        if !self.write(|st| st.sessions.set_send_at(session_id, Some(later))) {
            return Err("that session is gone".into());
        }
        self.changed();
        Ok("Cue sends it then".into())
    }

    /// The message for later, dropped (Cancel), or back to you to change (Edit): what it was.
    pub fn drop_send_at(&self, session_id: &str) -> Option<SendAt> {
        let l = self.write(|st| st.sessions.take_send_at(session_id));
        if l.is_some() {
            self.changed();
        }
        l
    }

    /// Send the message for later now (Send now, or a failed one tried again).
    pub fn send_at_now(&self, session_id: &str) -> Result<String, String> {
        let l = self.write(|st| st.sessions.take_send_at(session_id)).ok_or("nothing to send later")?;
        self.send_one_later(session_id, l)
    }

    /// Each tick: what's due, taken in the same step (a slow send is never sent twice), and sent off this thread.
    pub(super) fn send_at_tick(&self) {
        let due: Vec<(String, SendAt)> = self.write(|st| {
            // A session of a paused Desk (#366) isn't carried on, nor sent what you scheduled, till it resumes.
            let ids: Vec<String> = st.sessions.send_at_due(now_ms()).into_iter().filter(|sid| !crate::desk::hub::away::pause::holds(st, sid)).collect();
            let due: Vec<(String, SendAt)> =
                ids.into_iter().filter_map(|sid| st.sessions.take_send_at(&sid).map(|l| (sid, l))).collect();
            for (sid, l) in &due {
                if l.at_ms == 0 {
                    st.sessions.carry_on_sent(sid);
                }
            }
            due
        });
        for (sid, l) in due {
            self.later(0, move |h| {
                let _ = h.send_one_later(&sid, l);
            });
        }
    }

    /// Sent like anything you send it (kept in Cue if it's busy); if it can't be, it's put back with why, for you.
    fn send_one_later(&self, session_id: &str, l: SendAt) -> Result<String, String> {
        // Cue's carry on (at 0) is Cue's words, not yours (#836); a message you set for later is yours.
        if l.at_ms == 0 {
            self.cue_says(&l.text);
        }
        let sent = self.send_saved(session_id, &l.text, vec![], false);
        if let Err(e) = &sent {
            eprintln!("cue: couldn't send the message for later to {session_id}: {e}");
            self.write(|st| {
                st.sessions.set_send_at(session_id, Some(l));
                st.sessions.send_at_failed(session_id, e);
            });
        }
        self.changed();
        sent
    }

    /// A carry-on waits on a plan limit (all models) whose reset Cue doesn't know (no time in the error, no usage numbers
    /// from a status line): every PROBE_MS Cue asks the small model a one-word question. Answered, the limit is over:
    /// it's marked reset now, and the next tick sends the carry-on. A few tokens a try, only while something waits.
    pub(super) fn limit_probe_tick(&self, now: u64) {
        let due = self.write(|st| {
            let waits = st.sessions.all().iter().any(|s| {
                s.state == "limited"
                    && s.limit.as_ref().is_some_and(|l| l.scope == "all" && l.resets_ms.is_none())
                    && s.send_at.as_ref().is_some_and(|l| l.at_ms == 0 && l.failed.is_empty())
            });
            let due = waits && (st.limit_probed_ms == 0 || now.saturating_sub(st.limit_probed_ms) >= PROBE_MS);
            if due {
                st.limit_probed_ms = now;
            }
            due
        });
        if due {
            self.later(0, |h| {
                if probe::back() {
                    h.limit_over(now_ms());
                }
            });
        }
    }

    /// The plan limit is over (the probe was answered): every session waiting on it with no known reset, reset now.
    fn limit_over(&self, now: u64) {
        let lifted = self.write(|st| {
            let mut lifted = false;
            for s in st.sessions.all() {
                let Some(mut l) = s.limit.clone().filter(|l| l.scope == "all" && l.resets_ms.is_none()) else { continue };
                if s.state != "limited" {
                    continue;
                }
                l.resets_ms = Some(now);
                st.sessions.set_limit(&s.origin.session_id, Some(l));
                lifted = true;
            }
            lifted
        });
        if lifted {
            self.changed();
        }
    }

    /// A turn a usage limit stopped: Cue carries it on when the limit resets, unless something is set for it already
    /// or Settings turned that off. In `st`, as the session is marked limited.
    pub(super) fn carry_on_after_limit(st: &mut Store, session_id: &str) {
        if crate::config::flag("/limits/carry_on") {
            st.sessions.carry_on(session_id);
        }
    }
}

/// Whether Claude Code answers again: a one-word ask of the small model (nothing saved, no tools). In tests, what the
/// test says (`set`), never Claude Code itself.
pub(crate) mod probe {
    #[cfg(test)]
    static BACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Tests: whether the probe finds Claude Code back.
    #[cfg(test)]
    pub fn set(back: bool) {
        BACK.store(back, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn back() -> bool {
        #[cfg(test)]
        return BACK.load(std::sync::atomic::Ordering::Relaxed);
        #[cfg(not(test))]
        crate::btw::ask_small("Reply with the word ok.", "cue-limit-probe").is_ok()
    }
}
