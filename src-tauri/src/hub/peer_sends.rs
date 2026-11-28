//! A hosted session's messages to other sessions (Claude Code's SendMessage) in its chat: noted when its call goes
//! out, marked waiting or failed when the call comes back, and answered when that session writes back
//! (sessions/peers.rs). A message to one of its own teammates, or to "main", is no other session's: not noted.

use super::*;

/// A resting session started again for another session's message, until it takes it (Store `peer_waking`).
#[derive(Clone, Debug)]
pub(crate) struct PeerWake {
    /// The process it was started again as.
    pub pid: i32,
    /// The sender's id, and its SendMessage call.
    pub from: String,
    pub id: String,
    /// What the message said.
    pub text: String,
}

/// Session `to`, woken for `w`, never took it: the message leaves its chat (it never got it), and the sender's
/// entry says it didn't go, and `why`.
pub(super) fn peer_wake_failed(st: &mut Store, to: &str, w: &PeerWake, why: &str) {
    st.sessions.drop_peer_from(to, &w.from, &w.text);
    st.sessions.peer_undelivered(&w.from, &w.id, why);
}

/// Who a SendMessage goes to, as the chat says it: Claude Code may add the session's reference after its name
/// ("cue-9e [f7c5db]").
fn recipient(to: &str) -> &str {
    let to = to.trim();
    match to.rfind(" [") {
        Some(i) if to.ends_with(']') => to[..i].trim_end(),
        _ => to,
    }
}

/// An agent id as Claude Code makes them ("a74648c287d6e1f9c"): a background agent of its own, by its id.
fn agent_id_like(to: &str) -> bool {
    to.len() == 17 && to.starts_with('a') && to[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Claude Code's why for a SendMessage that didn't go says its recipient couldn't be reached ("No agent named 'x'
/// is reachable."): no process of that name runs.
fn unreachable(why: &str) -> bool {
    why.to_lowercase().contains("reachable")
}

/// A tool result's words: its text, or its text blocks joined.
fn result_text(output: &Value) -> String {
    match output {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// Whether a SendMessage went, and why not when it didn't: Claude Code answers with JSON ("success", "message").
fn went(output: &Value, failed: bool) -> (bool, String) {
    let text = result_text(output);
    let parsed: Option<Value> = serde_json::from_str(text.trim()).ok();
    let went = parsed.as_ref().and_then(|v| v.get("success")).and_then(Value::as_bool).unwrap_or(!failed);
    if went {
        return (true, String::new());
    }
    let said = parsed.as_ref().and_then(|v| v.get("message")).and_then(Value::as_str).map(str::to_string).unwrap_or(text);
    let why = said.replace("<tool_use_error>", "").replace("</tool_use_error>", "");
    let why = why.trim();
    (false, if why.is_empty() { "It didn't go.".into() } else { why.chars().take(300).collect() })
}

impl Hub {
    /// Its own SendMessage call `id` went out: a message to another session goes in its chat, sending.
    pub(super) fn peer_send_out(&self, origin: &Origin, id: &str, input: &Value) {
        let to = recipient(input.get("to").and_then(Value::as_str).unwrap_or(""));
        let text = input.get("message").and_then(Value::as_str).unwrap_or("").trim();
        if to.is_empty() || text.is_empty() || to == "main" || to == "*" || agent_id_like(to) {
            return;
        }
        let sid = origin.session_id.as_str();
        // One of its teammates (sessions/team.rs), by its name or its id: in this session's own crew.
        let teammate = self.read(|st| st.sessions.team(sid).iter().any(|t| t.name == to || (!t.agent_id.is_empty() && t.agent_id == to) || t.call == to));
        if teammate {
            return;
        }
        // Claude's list of its sessions knows the name when Cue's record of that session hasn't caught up with it.
        let listed: Vec<String> = crate::logs::live::claude().into_iter().filter(|q| q.name == to && q.session_id != sid).map(|q| q.session_id).collect();
        self.write(|st| {
            let to_sid = st.sessions.sid_by_name(to, sid).or_else(|| listed.into_iter().find(|k| st.sessions.origin(k).is_some())).unwrap_or_default();
            st.sessions.note_peer_sent(sid, to, &to_sid, text, id, crate::config::context_keep());
        });
        self.changed();
    }

    /// Its process ended, or is started again: a message still sending never went.
    pub(super) fn peer_sends_stopped(&self, session_id: &str) {
        if self.write(|st| st.sessions.peer_sends_stopped(session_id)) {
            self.changed();
        }
    }

    /// Its own tool call `id` came back: when it was a message to another session, whether it went.
    pub(super) fn peer_send_back(&self, origin: &Origin, id: &str, output: &Value, failed: bool) {
        let (went, why) = went(output, failed);
        let sid = origin.session_id.as_str();
        // Claude Code couldn't reach it because it rests in Cue: Cue hands it over itself, off this event's path, and
        // it waits on an answer as one that went.
        // Only when that's why it didn't go: a refusal or an error stays one.
        let resting = if went || !unreachable(&why) { None } else { self.resting_recipient(sid, id) };
        if self.write(|st| st.sessions.peer_send_result(sid, id, went || resting.is_some(), &why)) {
            self.changed();
        }
        if let Some((to_sid, text)) = resting {
            let (from, id) = (sid.to_string(), id.to_string());
            self.later(0, move |h| h.hand_to_resting(&from, &id, &to_sid, &text));
        }
    }

    /// Who session `sid`'s message (SendMessage call `id`, not gone) is to, and what it says, when that's a session
    /// resting in Cue: one Cue runs itself on this Mac, a Claude one, kept (not ended or deleted), whose agent isn't
    /// running now (put to rest, or paused since its agent or Cue quit). Never one in a terminal, on another
    /// machine, at work, or a Codex or Pi session; never one Cue doesn't know, and never a name two sessions go by
    /// (which one was meant can't be told).
    fn resting_recipient(&self, sid: &str, id: &str) -> Option<(String, String)> {
        let found = self.read(|st| {
            let (to, noted, text) = st.sessions.peer_unsent(sid, id)?;
            let to_sid = st.sessions.only_sid_by_name(&to, sid)?;
            if !noted.is_empty() && noted != to_sid {
                return None;
            }
            let s = st.sessions.get(&to_sid)?;
            let o = &s.origin;
            let resting = o.hosted && o.machine.is_empty() && o.harness == "claude" && s.state != "working";
            resting.then_some((to_sid, text))
        })?;
        (!self.hosted().running(&found.0)).then_some(found)
    }

    /// Session `from`'s message `text` (its call `id`) to `to`, which rests: `to` started again and handed it, as from
    /// `from` by the name its Claude goes by. When that fails, the message says it didn't go, and why.
    pub(super) fn hand_to_resting(&self, from: &str, id: &str, to: &str, text: &str) {
        let listed = crate::logs::live::claude().into_iter().find(|q| q.session_id == from && !q.name.trim().is_empty()).map(|q| q.name);
        let sender = self.read(|st| {
            let claude = listed.or_else(|| st.sessions.claude_name(from)).unwrap_or_default();
            st.sessions.sender_of(from, &claude)
        });
        let name = if sender.claude.is_empty() { sender.from.clone() } else { sender.claude.clone() };
        let why = match self.hand_peer_to_hosted(to, &sender, &name, id, text) {
            Ok(woke) => {
                eprintln!("cue: {from}'s message to {to}, which was resting: started again and handed to it");
                // Started again for it: it has WAKE_LIMIT to take it, as any wake does (hosted.rs wake_check).
                if let Some(pid) = woke {
                    let to = to.to_string();
                    self.later(super::hosted::WAKE_LIMIT_MS, move |h| h.peer_wake_check(&to, pid));
                }
                return;
            }
            Err((true, e)) => format!("It was resting and Cue couldn't start it again: {e}"),
            Err((false, e)) => format!("It was resting and Cue couldn't hand it over: {e}"),
        };
        if self.write(|st| st.sessions.peer_undelivered(from, id, &why)) {
            self.changed();
        }
    }

    /// WAKE_LIMIT after Cue started `to` again (as process `pid`) for `from`'s message `text` (its call `id`): still
    /// not taken, and no turn under way, it didn't wake. Its process is closed, the message leaves its chat (it never
    /// got it), and the sender's says it didn't go. Taken meanwhile, ended, or started again since: nothing to do.
    /// A message of yours sent it meanwhile (waiting behind this wake) didn't go either: marked so, as `wake_check`
    /// marks one.
    pub(super) fn peer_wake_check(&self, to: &str, pid: i32) {
        let stalled = self.write(|st| {
            if st.peer_waking.get(to).is_none_or(|w| w.pid != pid) {
                return None;
            }
            let w = st.peer_waking.remove(to);
            if st.hosted_waking.get(to) == Some(&pid) {
                st.hosted_waking.remove(to);
            }
            // At work: it took it (or something else woke it to work), and its turn goes on.
            if st.sessions.state(to).as_deref() == Some("working") { None } else { w }
        });
        let Some(w) = stalled else { return };
        self.hosted().close(to);
        let secs = super::hosted::WAKE_LIMIT_MS / 1000;
        let why = format!("It was resting: Cue started it again, but it took nothing in {secs}s.");
        let yours = self.write(|st| {
            st.sessions.set_pid(to, None);
            st.sessions.team_process_ended(to, now_ms());
            peer_wake_failed(st, to, &w, &why);
            let yours = st.sessions.first_pending(to).is_some();
            if yours {
                st.sessions.no_wake_pending(to, &format!("it didn't wake: started again, it took nothing in {secs}s"));
                st.sessions.set_queued(to, None);
            }
            yours
        });
        self.changed();
        if yours {
            self.desk_feed_watch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipient_loses_its_reference_and_an_agent_id_is_known_as_one() {
        assert_eq!(recipient("cue-9e [f7c5db]"), "cue-9e");
        assert_eq!(recipient(" helper "), "helper");
        assert!(agent_id_like("a74648c287d6e1f9c"));
        assert!(!agent_id_like("api-fix"));
    }

    #[test]
    fn a_send_that_failed_says_why_in_claude_codes_words() {
        assert_eq!(went(&json!(r#"{"success":true,"message":"Message sent"}"#), false), (true, String::new()));
        assert_eq!(went(&json!([{"type": "text", "text": r#"{"success":false,"message":"No agent named 'x' is reachable."}"#}]), false), (false, "No agent named 'x' is reachable.".into()));
        assert_eq!(went(&json!("<tool_use_error>No such agent</tool_use_error>"), true), (false, "No such agent".into()));
        assert!(unreachable("No agent named 'x' is reachable.\nUse ListAgents…"));
        assert!(!unreachable("Permission to use SendMessage was denied."));
    }

    #[test]
    fn only_another_sessions_message_handed_over_whole_is_never_yours() {
        use crate::logs::transcript::handed_over_peer;
        assert!(handed_over_peer(&super::super::hosted::peer_words("lead", "hi")));
        assert!(handed_over_peer("Another Claude session sent a message:\n<cross-session-message from=\"uds:/tmp/s.sock\" from-name=\"b\">\nhi\n</cross-session-message>\n\nThis came from another Claude session — not typed by your user."));
        assert!(handed_over_peer("Another Claude session sent a message:\n<teammate-message teammate_id=\"r\">hi</teammate-message>"));
        assert!(!handed_over_peer("Another Claude session sent a message: can you check what it says?"), "yours, typed that way");
        assert!(!handed_over_peer("Another Claude session sent a message:\n<cross-session-message from-name=\"b\">\nhi\n</cross-session-message>\nand what do you make of it?"), "yours, with words of your own after it");
    }
}
