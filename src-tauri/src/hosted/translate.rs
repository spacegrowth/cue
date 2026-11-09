//! Claude's lines (wire::Inbound) as what Cue cares about (HostedEvent). Pure: no process, no time, so
//! every case is a test over recorded lines (translate_tests.rs). It keeps the little a single line can't
//! say on its own: whether the session already said it started, whether a turn is under way, and whether
//! anything of yours started it.

use super::wire::Inbound;
use super::{Ask, AskKind, HostedEvent};
use serde_json::Value;

/// A `rate_limit_event`'s windows as the status line's `rate_limits` (utilization 0–1 → a percentage).
/// It carries `unifiedWindows` with both; an older one only the window it's about (`rateLimitType`).
/// None when it says no numbers.
fn rates(v: &Value) -> Option<HostedEvent> {
    if v.get("type").and_then(Value::as_str) != Some("rate_limit_event") {
        return None;
    }
    let info = v.get("rate_limit_info")?;
    let window = |w: &Value| -> Option<Value> {
        let pct = w.get("utilization")?.as_f64()? * 100.0;
        Some(serde_json::json!({ "used_percentage": pct, "resets_at": w.get("resetsAt").and_then(Value::as_u64) }))
    };
    let mut rl = serde_json::Map::new();
    for k in ["five_hour", "seven_day"] {
        let w = info.pointer(&format!("/unifiedWindows/{k}")).or_else(|| (info.get("rateLimitType").and_then(Value::as_str) == Some(k)).then_some(info));
        if let Some(w) = w.and_then(window) {
            rl.insert(k.to_string(), w);
        }
    }
    (!rl.is_empty()).then(|| HostedEvent::Rates(Value::Object(rl)))
}

/// A `prompt_suggestion` line: what you'd likely type next, said after a turn ends (only when there's an obvious one).
fn suggested(v: &Value) -> Option<HostedEvent> {
    if v.get("type").and_then(Value::as_str) != Some("prompt_suggestion") {
        return None;
    }
    let text = v.get("suggestion").and_then(Value::as_str)?.trim();
    (!text.is_empty()).then(|| HostedEvent::Suggested { text: text.to_string() })
}

/// Claude's tool for questions with choices, and the one a sandboxed command asks network access through.
const QUESTION_TOOL: &str = "AskUserQuestion";
const NETWORK_TOOL: &str = "SandboxNetworkAccess";

#[derive(Default)]
pub struct Translator {
    started: bool,
    in_turn: bool,
    /// A message of yours was taken since the last turn ended.
    prompted: bool,
}

impl Translator {
    /// Whether a turn is under way (from your message, or Claude's first step, to its result).
    pub fn in_turn(&self) -> bool {
        self.in_turn
    }

    pub fn step(&mut self, line: &Inbound) -> Vec<HostedEvent> {
        match line {
            Inbound::Init(v) => {
                // Claude says it's starting a turn (it repeats this line every turn) as soon as it reads a
                // message while idle: never mid-turn, where a message is folded into the turn going on.
                let mut es = self.started(v);
                if !std::mem::replace(&mut self.in_turn, true) {
                    es.push(HostedEvent::TurnStarted);
                }
                es
            }
            Inbound::System(v) => task(v).into_iter().collect(),
            Inbound::Assistant(v) => {
                self.in_turn = true;
                assistant(v)
            }
            Inbound::User(v) => self.user(v),
            Inbound::Result(v) => vec![self.turn_ended(v)],
            Inbound::Request { id, request } => vec![ask(id, request)],
            Inbound::Other(v) => rates(v).or_else(|| suggested(v)).into_iter().collect(),
            Inbound::Response(_) | Inbound::Text(_) => vec![],
        }
    }

    fn started(&mut self, v: &Value) -> Vec<HostedEvent> {
        if std::mem::replace(&mut self.started, true) {
            return vec![];
        }
        vec![HostedEvent::Started {
            session_id: text(v, "session_id"),
            model: text(v, "model"),
            slash_commands: names(v.get("slash_commands")),
            terminal_only: names(v.get("terminal_slash_commands")),
        }]
    }

    fn user(&mut self, v: &Value) -> Vec<HostedEvent> {
        let agent = parent(v);
        let content = v.pointer("/message/content").unwrap_or(&Value::Null);
        if v.get("isReplay").and_then(Value::as_bool) == Some(true) {
            self.in_turn = true;
            self.prompted = true;
            return vec![HostedEvent::Taken { text: plain_text(content) }];
        }
        blocks(content)
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .map(|b| HostedEvent::ToolResult {
                id: text(b, "tool_use_id"),
                output: b.get("content").cloned().unwrap_or(Value::Null),
                failed: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                agent: agent.clone(),
            })
            .collect()
    }

    fn turn_ended(&mut self, v: &Value) -> HostedEvent {
        let unprompted = !std::mem::replace(&mut self.prompted, false);
        self.in_turn = false;
        // A turn that failed before saying anything ("No conversation found…") has its reason in `errors`.
        let said = text(v, "result");
        let errors = || v.get("errors").and_then(Value::as_array).map(|e| e.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default();
        HostedEvent::TurnEnded {
            text: if said.is_empty() { errors() } else { said },
            failed: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            cost_usd: v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
            unprompted,
        }
    }
}

/// An assistant message: what it said and the tools it called, in order.
fn assistant(v: &Value) -> Vec<HostedEvent> {
    let agent = parent(v);
    blocks(v.pointer("/message/content").unwrap_or(&Value::Null))
        .filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("text") => Some(HostedEvent::Said { text: text(b, "text"), agent: agent.clone() }),
            Some("tool_use") => Some(HostedEvent::ToolCall {
                id: text(b, "id"),
                name: text(b, "name"),
                input: b.get("input").cloned().unwrap_or(Value::Null),
                agent: agent.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// A background agent's or task's own event (`task_started`, `task_progress`, …), kept whole.
fn task(v: &Value) -> Option<HostedEvent> {
    let what = text(v, "subtype");
    (what.starts_with("task_") || what == "background_tasks_changed").then(|| HostedEvent::Task { what, detail: v.clone() })
}

/// A request from Claude: an ask Cue shows you, or one it refuses for not knowing its kind.
fn ask(id: &str, r: &Value) -> HostedEvent {
    let subtype = text(r, "subtype");
    let tool_name = text(r, "tool_name");
    let kind = match (subtype.as_str(), tool_name.as_str()) {
        ("can_use_tool", QUESTION_TOOL) => AskKind::Question,
        ("can_use_tool", NETWORK_TOOL) => AskKind::Network,
        ("can_use_tool", _) => AskKind::Permission,
        ("elicitation", _) => AskKind::Input,
        _ => return HostedEvent::Refused { request_id: id.to_string(), subtype },
    };
    HostedEvent::Ask(Ask {
        request_id: id.to_string(),
        kind,
        tool_name,
        input: r.get("input").cloned().unwrap_or(Value::Null),
        suggestions: r.get("permission_suggestions").cloned().unwrap_or(Value::Array(vec![])),
        agent_id: text(r, "agent_id"),
        tool_use_id: text(r, "tool_use_id"),
        server: text(r, "mcp_server_name"),
        message: text(r, "message"),
        form: r.get("requested_schema").cloned().unwrap_or(Value::Null),
    })
}

/// The tool call that started the agent a line comes from; None for the session's own lines.
fn parent(v: &Value) -> Option<String> {
    v.get("parent_tool_use_id").and_then(Value::as_str).map(str::to_string)
}

fn blocks(content: &Value) -> impl Iterator<Item = &Value> {
    content.as_array().into_iter().flatten()
}

/// A message's words: its text, or its text blocks joined.
fn plain_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        _ => blocks(content).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
    }
}

/// The names in a list of names or of objects with a `name`.
fn names(list: Option<&Value>) -> Vec<String> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().or_else(|| c.get("name").and_then(Value::as_str)))
        .map(str::to_string)
        .collect()
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}
