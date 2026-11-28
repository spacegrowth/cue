//! A hosted session's messages to other sessions (Claude Code's SendMessage) in its chat: noted when its call goes
//! out, marked waiting or failed when the call comes back, and answered when that session writes back
//! (sessions/peers.rs). A message to one of its own teammates, or to "main", is no other session's: not noted.

use super::*;

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
        if self.write(|st| st.sessions.peer_send_result(&origin.session_id, id, went, &why)) {
            self.changed();
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
    }
}
