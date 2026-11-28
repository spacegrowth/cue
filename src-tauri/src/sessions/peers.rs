//! Messages between sessions in a session's chat, both ways: one it got (role "peer", `from`), and one it sent with
//! Claude Code's SendMessage (role "peer", `to`), with how that one is doing: sending, waiting, answered or failed.

use super::{Session, Sessions};

/// Who sent a message, as the chat labels it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sender {
    /// The name Cue shows for that session, else Claude's name for it, else "another agent".
    pub from: String,
    /// Its project, when Cue knows the session.
    pub project: String,
    /// Its id, when Cue knows the session.
    pub sid: String,
    /// Claude's own name for it (what a SendMessage to it would say), when the log gave one.
    pub claude: String,
}

/// Why a message it sent is "failed" when its call's result never came: its turn ended, its Claude exited, or Cue
/// restarted. A result that comes after all (the session lived through the restart) still settles it.
pub const NO_RESULT: &str = "Its session stopped before Claude Code said whether this went.";

/// Every message `s` was still sending, failed: no result will come for it now (`NO_RESULT`).
pub(super) fn unsettled(s: &mut Session) -> bool {
    let mut any = false;
    for e in s.thread.iter_mut().filter(|e| e.role == "peer" && e.sent == "sending") {
        e.sent = "failed".into();
        e.why = NO_RESULT.into();
        any = true;
    }
    any
}

/// The name Cue shows for a session, as the chat's header does: yours for it, else its folder's, else its project.
fn shown_name(s: &Session) -> String {
    if !s.name.trim().is_empty() {
        return s.name.clone();
    }
    let folder = s.origin.cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if folder.is_empty() { s.project.clone() } else { folder.to_string() }
}

impl Sessions {
    /// Who sent a message, from the process that sent it (`pid`, when Claude Code said) and Claude's name for it.
    pub fn sender(&self, pid: Option<i32>, claude: &str) -> Sender {
        let known = pid.and_then(|p| self.0.iter().find(|(_, s)| s.origin.agent_pid == Some(p)));
        match known {
            Some((sid, s)) => Sender { from: shown_name(s), project: s.project.clone(), sid: sid.clone(), claude: claude.to_string() },
            None => Sender { from: if claude.is_empty() { "another agent".into() } else { claude.to_string() }, claude: claude.to_string(), ..Default::default() },
        }
    }

    /// The session (other than `except`) that goes by `name`: its name in Cue, or Claude's name for it.
    pub fn sid_by_name(&self, name: &str, except: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let by = |f: &dyn Fn(&Session) -> bool| self.0.iter().find(|(k, s)| k.as_str() != except && f(s)).map(|(k, _)| k.clone());
        by(&|s| s.name == name).or_else(|| by(&|s| s.claude_name.as_deref() == Some(name)))
    }

    /// A message it's sending another session (its SendMessage call `send_id`, still out): in its chat, marked sending.
    pub fn note_peer_sent(&mut self, session_id: &str, to: &str, to_sid: &str, text: &str, send_id: &str, keep: usize) {
        let text = text.trim();
        let Some(s) = self.0.get_mut(session_id).filter(|_| !text.is_empty()) else { return };
        // The same call seen again (its stream read back after a restart): noted once.
        if !send_id.is_empty() && s.thread.iter().any(|e| e.send_id == send_id) {
            return;
        }
        // Pushed as it is: two the same in a row are two messages sent.
        Self::push(s, "peer", text, &[], keep);
        if let Some(e) = s.thread.last_mut() {
            e.to = to.to_string();
            e.to_sid = to_sid.to_string();
            e.sent = "sending".into();
            e.send_id = send_id.to_string();
        }
    }

    /// Its SendMessage call `send_id` came back: it went (waiting on an answer), or it didn't (`why`). Whether that
    /// call was one of these.
    pub fn peer_send_result(&mut self, session_id: &str, send_id: &str, went: bool, why: &str) -> bool {
        let Some(e) = self.0.get_mut(session_id).and_then(|s| s.thread.iter_mut().rev().find(|e| e.role == "peer" && !e.send_id.is_empty() && e.send_id == send_id)) else {
            return false;
        };
        if e.sent != "sending" && !(e.sent == "failed" && e.why == NO_RESULT) {
            return false;
        }
        if went {
            e.sent = "waiting".into();
            e.why.clear();
        } else {
            e.sent = "failed".into();
            e.why = why.to_string();
        }
        true
    }

    /// Its messages still going out, failed: their calls' results won't come (its turn ended, its Claude exited, or
    /// it's started again). Whether there were any.
    pub fn peer_sends_stopped(&mut self, session_id: &str) -> bool {
        self.0.get_mut(session_id).is_some_and(unsettled)
    }

    /// `sender` wrote to this session (at `at`): the latest message this one sent it that still waits is answered.
    pub(super) fn peer_answered(&mut self, session_id: &str, sender: &Sender, at: u64) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        // By id when both are known (two sessions can go by one name); by name only when one isn't.
        let to_them = |to: &str, to_sid: &str| {
            if !sender.sid.is_empty() && !to_sid.is_empty() {
                return to_sid == sender.sid;
            }
            to == sender.from || (!sender.claude.is_empty() && to == sender.claude)
        };
        if let Some(e) = s.thread.iter_mut().rev().find(|e| e.role == "peer" && e.sent == "waiting" && to_them(&e.to, &e.to_sid)) {
            e.sent = "answered".into();
            e.answered_ms = at;
        }
    }
}
