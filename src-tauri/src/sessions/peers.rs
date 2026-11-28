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

    /// The one session (other than `except`) that goes by `name`, by its name in Cue or Claude's name for it: None when
    /// none does, or more than one (then which one was meant can't be told).
    pub fn only_sid_by_name(&self, name: &str, except: &str) -> Option<String> {
        let name = name.trim();
        let mut by = self.0.iter().filter(|(k, s)| k.as_str() != except && !name.is_empty() && (s.name == name || s.claude_name.as_deref() == Some(name)));
        let (sid, _) = by.next()?;
        by.next().is_none().then(|| sid.clone())
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

    /// Its message sent with SendMessage call `send_id` that hasn't gone yet (still sending, or failed for want of a
    /// result): who it's to (Claude's name for it, and Cue's id for it when known), and what it said.
    pub fn peer_unsent(&self, session_id: &str, send_id: &str) -> Option<(String, String, String)> {
        let e = self.0.get(session_id)?.thread.iter().rev().find(|e| e.role == "peer" && !e.send_id.is_empty() && e.send_id == send_id)?;
        let open = e.sent == "sending" || (e.sent == "failed" && e.why == NO_RESULT);
        open.then(|| (e.to.clone(), e.to_sid.clone(), e.text.clone()))
    }

    /// Its message (call `send_id`) that Cue took over to hand to a resting session, so waiting, didn't reach it after
    /// all: failed, and `why`. Whether it was one still waiting.
    pub fn peer_undelivered(&mut self, session_id: &str, send_id: &str, why: &str) -> bool {
        let Some(e) = self.0.get_mut(session_id).and_then(|s| s.thread.iter_mut().rev().find(|e| e.role == "peer" && !e.send_id.is_empty() && e.send_id == send_id)) else {
            return false;
        };
        if e.sent != "waiting" {
            return false;
        }
        e.sent = "failed".into();
        e.why = why.to_string();
        true
    }

    /// The latest message from session `from_sid` saying `text` in session `sid`'s chat, taken out: Cue handed it
    /// over, but it never took it.
    pub fn drop_peer_from(&mut self, sid: &str, from_sid: &str, text: &str) {
        let Some(s) = self.0.get_mut(sid) else { return };
        if let Some(i) = s.thread.iter().rposition(|e| e.role == "peer" && e.to.is_empty() && e.from_sid == from_sid && e.text == text.trim()) {
            s.thread.remove(i);
        }
    }

    /// Session `sid` as the sender of a message: the name Cue shows for it, its project, its id, and `claude`
    /// (Claude's name for it).
    pub fn sender_of(&self, sid: &str, claude: &str) -> Sender {
        match self.0.get(sid) {
            Some(s) => Sender { from: shown_name(s), project: s.project.clone(), sid: sid.to_string(), claude: claude.to_string() },
            None => Sender { from: claude.to_string(), claude: claude.to_string(), ..Default::default() },
        }
    }

    /// The name session `sid`'s Claude goes by for SendMessage, when Cue knows it (`claude_name`).
    pub fn claude_name(&self, sid: &str) -> Option<String> {
        self.0.get(sid).and_then(|s| s.claude_name.clone()).filter(|n| !n.trim().is_empty())
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
