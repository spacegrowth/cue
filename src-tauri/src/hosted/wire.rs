//! The lines a hosted session's Claude reads and writes (Claude Code's stream-json, the protocol its SDK
//! speaks): one JSON object per line, each way. Read leniently: a line of a type this code doesn't know, or
//! fields it doesn't know, are kept whole rather than refused, so a newer Claude Code only adds what Cue
//! ignores. Written exactly: these are the only places that spell the lines Claude reads.

use super::{Answer, InputAction};
use serde_json::{json, Value};

/// A line from Claude, by type.
#[derive(Clone, Debug, PartialEq)]
pub enum Inbound {
    /// `system` / `init`: the session is up (repeated at the start of every turn).
    Init(Value),
    /// Any other `system` line (task events, thinking estimates, …).
    System(Value),
    Assistant(Value),
    /// A user message: one of yours echoed back as it's taken, or tool results.
    User(Value),
    /// A turn's end.
    Result(Value),
    /// Claude asking the host something (`request_id`, and the request itself).
    Request { id: String, request: Value },
    /// Claude answering a request of Cue's (setup, interrupt).
    Response(Value),
    /// JSON of a type this code doesn't know.
    Other(Value),
    /// Not JSON at all (a shell's greeting before Claude started, say).
    Text(String),
}

pub fn parse(line: &str) -> Inbound {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Inbound::Text(line.to_string()) };
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
    match kind.as_str() {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => Inbound::Init(v),
        "system" => Inbound::System(v),
        "assistant" => Inbound::Assistant(v),
        "user" => Inbound::User(v),
        "result" => Inbound::Result(v),
        "control_request" => {
            let id = v.get("request_id").and_then(Value::as_str).unwrap_or_default().to_string();
            Inbound::Request { id, request: v.get("request").cloned().unwrap_or(Value::Null) }
        }
        "control_response" => Inbound::Response(v),
        _ => Inbound::Other(v),
    }
}

/// Your message.
pub fn user_message(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
}

/// The setup request every host sends first (its reply lists commands, models and more).
pub fn initialize(request_id: &str) -> String {
    request(request_id, json!({ "subtype": "initialize" }))
}

/// Stop what it's doing: the running tool is stopped and the turn ends.
pub fn interrupt(request_id: &str) -> String {
    request(request_id, json!({ "subtype": "interrupt" }))
}

/// A request of Cue's (`body`: its `subtype` and fields), Claude's answer coming back as a `control_response` under
/// `request_id` (see `reply`).
pub fn request(request_id: &str, body: Value) -> String {
    json!({ "type": "control_request", "request_id": request_id, "request": body }).to_string()
}

/// `line`, when it's a `control_response`: as `reply` reads it.
pub fn parse_reply(line: &str) -> Option<(String, Result<Value, String>)> {
    if !line.contains("\"control_response\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    (v.get("type").and_then(Value::as_str) == Some("control_response")).then(|| reply(&v)).flatten()
}

/// A `control_response` line's request id and what it said: its `response` (`{}` when it has none), or its error.
pub fn reply(v: &Value) -> Option<(String, Result<Value, String>)> {
    let r = v.get("response")?;
    let id = r.get("request_id")?.as_str()?.to_string();
    let said = match r.get("subtype").and_then(Value::as_str) {
        Some("success") => Ok(r.get("response").cloned().unwrap_or_else(|| json!({}))),
        _ => Err(r.get("error").and_then(Value::as_str).unwrap_or("it refused, saying nothing").to_string()),
    };
    Some((id, said))
}

/// The answer to request `request_id`. `tool_input` is the asking tool call's input: an allow always carries
/// the input it runs with, unchanged unless the answer changes it (a question's answers).
pub fn answer(request_id: &str, a: &Answer, tool_input: &Value) -> String {
    let body = match a {
        Answer::Decision(d) => {
            let mut body = crate::hook::decision(&serde_json::to_value(d).unwrap_or_default(), tool_input);
            if body["behavior"] == "allow" && body.get("updatedInput").is_none() {
                body["updatedInput"] = tool_input.clone();
            }
            body
        }
        Answer::Input { action, content } => json!({ "action": action_name(*action), "content": content }),
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": body } }).to_string()
}

/// Request `request_id` refused: what Claude gets for a request Cue can't answer.
pub fn refuse(request_id: &str, why: &str) -> String {
    json!({ "type": "control_response", "response": { "subtype": "error", "request_id": request_id, "error": why } }).to_string()
}

fn action_name(a: InputAction) -> &'static str {
    match a {
        InputAction::Accept => "accept",
        InputAction::Decline => "decline",
        InputAction::Cancel => "cancel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Decision;

    fn response(line: &str) -> Value {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!((v["type"].as_str(), v["response"]["request_id"].as_str()), (Some("control_response"), Some("r1")));
        v["response"]["response"].clone()
    }

    fn decided(behavior: &str) -> Decision {
        Decision { behavior: behavior.into(), ..Default::default() }
    }

    #[test]
    fn an_allow_carries_the_input_it_runs_with() {
        let input = json!({ "command": "ls" });
        assert_eq!(response(&answer("r1", &Answer::Decision(decided("allow")), &input)), json!({ "behavior": "allow", "updatedInput": input }));
    }

    #[test]
    fn a_deny_says_why_and_always_allow_carries_the_rule() {
        let input = json!({ "command": "ls" });
        let deny = Decision { message: Some("not now".into()), ..decided("deny") };
        assert_eq!(response(&answer("r1", &Answer::Decision(deny), &input)), json!({ "behavior": "deny", "message": "not now" }));
        let rule = json!({ "type": "addRules", "rules": [{ "toolName": "Bash" }] });
        let always = Decision { permission: Some(rule.clone()), ..decided("allow_always") };
        let r = response(&answer("r1", &Answer::Decision(always), &input));
        assert_eq!((r["updatedPermissions"].clone(), r["updatedInput"].clone()), (json!([rule]), input));
    }

    #[test]
    fn a_questions_answers_go_back_inside_its_input() {
        let input = json!({ "questions": [{ "question": "Red or blue?" }] });
        let d = Decision { answers: Some(json!({ "Red or blue?": "Blue" })), ..decided("allow") };
        let r = response(&answer("r1", &Answer::Decision(d), &input));
        assert_eq!(r["updatedInput"]["answers"], json!({ "Red or blue?": "Blue" }));
        assert_eq!(r["updatedInput"]["questions"], input["questions"]);
    }

    #[test]
    fn a_form_is_answered_with_its_action_and_content() {
        let a = Answer::Input { action: InputAction::Accept, content: json!({ "color": "blue" }) };
        assert_eq!(response(&answer("r1", &a, &Value::Null)), json!({ "action": "accept", "content": { "color": "blue" } }));
        let d = Answer::Input { action: InputAction::Decline, content: Value::Null };
        assert_eq!(response(&answer("r1", &d, &Value::Null))["action"], "decline");
    }

    #[test]
    fn requests_and_refusals_are_spelled_as_claude_reads_them() {
        let i: Value = serde_json::from_str(&interrupt("cue-2")).unwrap();
        assert_eq!(i, json!({ "type": "control_request", "request_id": "cue-2", "request": { "subtype": "interrupt" } }));
        let r: Value = serde_json::from_str(&refuse("r1", "no")).unwrap();
        assert_eq!(r["response"], json!({ "subtype": "error", "request_id": "r1", "error": "no" }));
        let u: Value = serde_json::from_str(&user_message("hi")).unwrap();
        assert_eq!(u, json!({ "type": "user", "message": { "role": "user", "content": "hi" } }));
    }
}
