//! Messages Cue sends by itself later (sessions/send_at.rs): "Send at…" a time you picked, and Cue's carry-on for a
//! turn a usage limit stopped (Settings' /limits/carry_on, on by default). The usage tick sends what's due, so it
//! needs nothing but Cue running: not the session you scheduled it from, nor a terminal of yours watching.

use super::*;
use crate::sessions::send_at::SendAt;

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
            let ids = st.sessions.send_at_due(now_ms());
            ids.into_iter().filter_map(|sid| st.sessions.take_send_at(&sid).map(|l| (sid, l))).collect()
        });
        for (sid, l) in due {
            self.later(0, move |h| {
                let _ = h.send_one_later(&sid, l);
            });
        }
    }

    /// Sent like anything you send it (kept in Cue if it's busy); if it can't be, it's put back with why, for you.
    fn send_one_later(&self, session_id: &str, l: SendAt) -> Result<String, String> {
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

    /// A turn a usage limit stopped: Cue carries it on when the limit resets, unless something is set for it already
    /// or Settings turned that off. In `st`, as the session is marked limited.
    pub(super) fn carry_on_after_limit(st: &mut Store, session_id: &str) {
        if crate::config::flag("/limits/carry_on") {
            st.sessions.carry_on(session_id);
        }
    }
}
