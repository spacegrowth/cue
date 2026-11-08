//! Pi's RPC mode (`pi --mode rpc`): JSON lines both ways. Cue writes `prompt` (with `streamingBehavior:
//! "steer"`, so a message mid-turn is read at Pi's next step, as Claude reads one) and `abort`; Pi writes its
//! session events and the dialogs its extensions open. Read leniently: events and fields this code doesn't know
//! are left alone.
//!
//! Pi has no permission prompts of its own. Cue's Pi extension (pi/cue.ts) is its gate: hosted, it asks through
//! one of Pi's own dialogs (titled `cue:ask`, the ask as JSON, answered with the decision as JSON), so the ask
//! arrives here with everything else Pi writes: a permission or question card, as for a terminal Pi. Any other
//! extension's dialog (a choice, yes or no, a line to type) becomes a card too: a question, or a form with one
//! field.

use super::protocol::Protocol;
use super::{Answer, Ask, AskKind, HostedEvent, InputAction};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The dialogs Pi waits on an answer for; its other extension calls (a status line, a notice) ask nothing.
const DIALOGS: [&str; 4] = ["select", "confirm", "input", "editor"];
/// The title of the dialog Cue's extension asks with (its placeholder: the ask).
const CUE_ASK: &str = "cue:ask";

#[derive(Default)]
pub(super) struct Pi {
    in_turn: bool,
    /// A message of yours started or joined this turn.
    prompted: bool,
    /// What it last said, and why its last reply failed (if it did): a turn's end says them.
    last_said: String,
    failure: Option<String>,
    /// You stopped this turn: Pi ends the reply it cut off as an error ("This operation was aborted"), which is
    /// your stop, not a failure.
    stopped: bool,
    /// What its replies cost so far, in dollars, since it started.
    cost: f64,
    /// Dialogs waiting for your answer, by request id.
    dialogs: HashMap<String, Dialog>,
    /// Your messages Pi hasn't said it accepted yet, by request id.
    sent: HashMap<String, String>,
    next_request: u64,
}

impl Pi {
    fn request_id(&mut self) -> String {
        self.next_request += 1;
        format!("cue-{}", self.next_request)
    }

    fn turn_ended(&mut self) -> HostedEvent {
        let unprompted = !std::mem::replace(&mut self.prompted, false);
        self.in_turn = false;
        let failure = self.failure.take().filter(|_| !std::mem::replace(&mut self.stopped, false));
        let said = std::mem::take(&mut self.last_said);
        HostedEvent::TurnEnded { failed: failure.is_some(), text: failure.unwrap_or(said), cost_usd: self.cost, unprompted }
    }

    /// A message it finished: yours (taken), or its reply (what it said and the tools it called).
    fn message_end(&mut self, m: &Value) -> Vec<HostedEvent> {
        let content = m.get("content").unwrap_or(&Value::Null);
        match m.get("role").and_then(Value::as_str) {
            Some("user") => {
                self.prompted = true;
                vec![HostedEvent::Taken { text: text_of(content) }]
            }
            Some("assistant") => {
                self.cost += m.pointer("/usage/cost/total").and_then(Value::as_f64).unwrap_or(0.0);
                if m.get("stopReason").and_then(Value::as_str) == Some("error") {
                    self.failure = Some(str_of(m, "errorMessage").unwrap_or_else(|| "Pi's reply failed".into()));
                }
                let said = text_of(content);
                if !said.trim().is_empty() {
                    self.last_said = said;
                }
                blocks(content).filter_map(reply_part).collect()
            }
            _ => vec![],
        }
    }

    /// Pi's answer to a message of yours: accepted (it's read as Pi's events say), or turned down, with why.
    fn response(&mut self, v: &Value) -> Vec<HostedEvent> {
        let Some(text) = str_of(v, "id").and_then(|id| self.sent.remove(&id)) else { return vec![] };
        if v.get("success").and_then(Value::as_bool) != Some(false) {
            return vec![];
        }
        let why = str_of(v, "error").unwrap_or_else(|| "Pi turned it down".into());
        vec![HostedEvent::NotTaken { text, why: why.lines().next().unwrap_or_default().to_string() }]
    }

    /// An extension's dialog: a card, kept until it's answered.
    fn dialog(&mut self, v: &Value) -> Vec<HostedEvent> {
        let method = str_of(v, "method").unwrap_or_default();
        if !DIALOGS.contains(&method.as_str()) {
            return vec![];
        }
        let id = str_of(v, "id").unwrap_or_default();
        let title = str_of(v, "title").unwrap_or_else(|| "Pi asks you".into());
        if let Some(ask) = (method == "input" && title == CUE_ASK).then(|| cue_ask(&id, v)).flatten() {
            self.dialogs.insert(id, Dialog::Cue);
            return vec![HostedEvent::Ask(ask)];
        }
        let options: Vec<String> = match method.as_str() {
            "select" => v.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
            "confirm" => vec!["Yes".into(), "No".into()],
            _ => vec![],
        };
        self.dialogs.insert(id.clone(), Dialog::Other { method, options: options.clone() });
        vec![HostedEvent::Ask(dialog_ask(id, title, &options))]
    }
}

impl Protocol for Pi {
    fn opening(&mut self) -> Vec<String> {
        vec![]
    }

    fn read(&mut self, line: &str) -> (Vec<HostedEvent>, Vec<String>) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return (vec![], vec![]) };
        let events = match v.get("type").and_then(Value::as_str).unwrap_or_default() {
            "agent_start" if !self.in_turn => {
                self.in_turn = true;
                vec![HostedEvent::TurnStarted]
            }
            "message_end" => self.message_end(v.get("message").unwrap_or(&Value::Null)),
            "tool_execution_end" => vec![HostedEvent::ToolResult {
                id: str_of(&v, "toolCallId").unwrap_or_default(),
                output: v.pointer("/result/content").cloned().unwrap_or(Value::Null),
                failed: v.get("isError").and_then(Value::as_bool).unwrap_or(false),
                agent: None,
            }],
            "agent_settled" if self.in_turn => vec![self.turn_ended()],
            "extension_ui_request" => self.dialog(&v),
            "response" => self.response(&v),
            _ => vec![],
        };
        (events, vec![])
    }

    fn message(&mut self, text: &str) -> Vec<String> {
        let id = self.request_id();
        self.sent.insert(id.clone(), text.to_string());
        vec![json!({ "id": id, "type": "prompt", "message": text, "streamingBehavior": "steer" }).to_string()]
    }

    fn answer(&mut self, request_id: &str, a: &Answer) -> Result<Vec<String>, String> {
        let reply = match self.dialogs.remove(request_id).ok_or("that request isn't waiting for an answer (answered already?)")? {
            Dialog::Cue => cue_answer(request_id, a),
            Dialog::Other { method, options } => dialog_answer(request_id, &method, &options, a),
        };
        Ok(vec![reply.to_string()])
    }

    fn interrupt(&mut self) -> Vec<String> {
        self.stopped = self.in_turn;
        let id = self.request_id();
        vec![json!({ "id": id, "type": "abort" }).to_string()]
    }

    fn in_turn(&self) -> bool {
        self.in_turn
    }

    fn gone(&mut self) {
        self.dialogs.clear();
        self.sent.clear();
    }

    fn wrote(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        let id = str_of(&v, "id").unwrap_or_default();
        self.next_request = self.next_request.max(super::protocol::numbered(Some(&id)));
        match v.get("type").and_then(Value::as_str).unwrap_or_default() {
            "prompt" => {
                self.sent.insert(id, str_of(&v, "message").unwrap_or_default());
            }
            "abort" => self.stopped = self.in_turn,
            "extension_ui_response" => {
                self.dialogs.remove(&id);
            }
            _ => {}
        }
    }

    fn waits_on(&self, request_id: &str) -> bool {
        self.dialogs.contains_key(request_id)
    }
}

/// A dialog waiting for your answer.
enum Dialog {
    /// Cue's extension asking (its gate, or `ask_user`): answered with the decision.
    Cue,
    /// Another extension's: answered as its `method` takes it, from the choices it offered.
    Other { method: String, options: Vec<String> },
}

/// What Cue's extension asks, from its dialog's placeholder: a permission for a tool call, or a question.
fn cue_ask(id: &str, v: &Value) -> Option<Ask> {
    let ask: Value = serde_json::from_str(&str_of(v, "placeholder")?).ok()?;
    let kind = match str_of(&ask, "kind")?.as_str() {
        "permission" => AskKind::Permission,
        "question" => AskKind::Question,
        _ => return None,
    };
    let input = ask.get("tool_input").cloned().unwrap_or(Value::Null);
    Some(Ask { request_id: id.into(), kind, tool_name: str_of(&ask, "tool_name")?, input, suggestions: json!([]), agent_id: String::new(), tool_use_id: String::new(), server: String::new(), message: String::new(), form: Value::Null })
}

/// Your answer to Cue's extension: the decision as JSON (it reads anything else as a denial).
fn cue_answer(id: &str, a: &Answer) -> Value {
    let decision = match a {
        Answer::Decision(d) => serde_json::to_value(d).unwrap_or(Value::Null),
        _ => json!({ "behavior": "deny" }),
    };
    json!({ "type": "extension_ui_response", "id": id, "value": decision.to_string() })
}

/// A block of its reply: what it said, or a tool it called.
fn reply_part(b: &Value) -> Option<HostedEvent> {
    match b.get("type").and_then(Value::as_str) {
        Some("text") => str_of(b, "text").filter(|t| !t.trim().is_empty()).map(|text| HostedEvent::Said { text, agent: None }),
        Some("toolCall") => Some(HostedEvent::ToolCall {
            id: str_of(b, "id").unwrap_or_default(),
            name: str_of(b, "name").unwrap_or_default(),
            input: b.get("arguments").cloned().unwrap_or(Value::Null),
            agent: None,
        }),
        _ => None,
    }
}

/// A dialog as a card: a choice (or yes or no) as a question with those options, anything else as a form with
/// one line to fill in.
fn dialog_ask(id: String, title: String, options: &[String]) -> Ask {
    let (kind, tool_name, input, form) = if options.is_empty() {
        (AskKind::Input, String::new(), Value::Null, json!({ "type": "object", "required": ["value"], "properties": { "value": { "type": "string", "title": "Your answer" } } }))
    } else {
        let question = json!({ "question": title, "options": options.iter().map(|o| json!({ "label": o })).collect::<Vec<_>>(), "multiSelect": false });
        (AskKind::Question, "AskUserQuestion".to_string(), json!({ "questions": [question] }), Value::Null)
    };
    Ask { request_id: id, kind, tool_name, input, suggestions: json!([]), agent_id: String::new(), tool_use_id: String::new(), server: "Pi".into(), message: title, form }
}

/// Your answer, as the dialog `method` takes it: the option you picked, yes or no, what you typed; a dialog you
/// declined or let go is cancelled (the extension decides what that means).
fn dialog_answer(id: &str, method: &str, options: &[String], a: &Answer) -> Value {
    let cancelled = json!({ "type": "extension_ui_response", "id": id, "cancelled": true });
    match a {
        Answer::Decision(d) if d.behavior != "deny" => {
            let picked = d.answers.as_ref().and_then(Value::as_object).and_then(|m| m.values().next()).and_then(Value::as_str).map(str::to_string);
            match (method, picked) {
                ("confirm", Some(p)) => json!({ "type": "extension_ui_response", "id": id, "confirmed": p == "Yes" }),
                (_, Some(p)) if options.contains(&p) => json!({ "type": "extension_ui_response", "id": id, "value": p }),
                _ => cancelled,
            }
        }
        Answer::Input { action: InputAction::Accept, content } => match content.get("value").and_then(Value::as_str) {
            Some(typed) => json!({ "type": "extension_ui_response", "id": id, "value": typed }),
            None => cancelled,
        },
        _ => cancelled,
    }
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn blocks(content: &Value) -> impl Iterator<Item = &Value> {
    content.as_array().into_iter().flatten()
}

/// A message's words: its text, or its text blocks joined.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        _ => blocks(content).filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
    }
}
