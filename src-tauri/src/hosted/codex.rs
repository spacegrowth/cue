//! Codex's app server (`codex app-server`): JSON-RPC 2.0 over JSON lines, the channel Codex's own apps use. Cue
//! sets it up (`initialize`), opens the conversation (`thread/start`, or `thread/resume` by its id), then sends
//! each message as `turn/start`, or `turn/steer` while a turn runs (read at its next step), and stops with
//! `turn/interrupt`. Codex picks a new conversation's id: the session is known by it once `thread/start` answers.
//!
//! Its requests are cards: a command to run and a file change to make (permissions), more access, a question,
//! an MCP server's form. A request of a kind Cue doesn't know is refused at once, so Codex never waits on Cue.
//! Read leniently: notifications and fields this code doesn't know are left alone.

use super::protocol::Protocol;
use super::{Answer, Ask, AskKind, HostedEvent, InputAction, Spec, Start};
use serde_json::{json, Value};
use std::collections::HashMap;

/// A request Codex is waiting on: how its answer is spelled.
enum Waiting {
    Command { decisions: Vec<Value>, amendment: Option<Value> },
    FileChange,
    Permissions { asked: Value },
    /// Question ids, by the question's words (an answer comes back keyed by its words).
    Questions(HashMap<String, String>),
    Form,
}

pub(super) struct Codex {
    /// What opens the conversation once Codex is set up.
    pub(super) open: Value,
    thread: Option<String>,
    transcript: Option<String>,
    turn: Option<String>,
    /// A message of yours started or joined this turn.
    prompted: bool,
    last_said: String,
    /// The file changes Codex is about to make, by item id: its file-change request names only the item.
    changes: HashMap<String, Value>,
    waiting: HashMap<String, Waiting>,
    /// Your messages sent before the conversation was open, sent once it is.
    held: Vec<String>,
    /// Requests of Cue's waiting for an answer: what each was.
    asked: HashMap<u64, Asked>,
    next_request: u64,
    stopped: bool,
}

enum Asked {
    Setup,
    Open,
    Message(String),
    Other,
}

impl Codex {
    pub(super) fn new(spec: &Spec) -> Codex {
        let mut params = serde_json::Map::new();
        params.insert("cwd".into(), json!(spec.cwd));
        let (approval, sandbox) = permissions(&spec.permissions);
        if let Some(a) = approval {
            params.insert("approvalPolicy".into(), json!(a));
        }
        if let Some(s) = sandbox {
            params.insert("sandbox".into(), json!(s));
        }
        if let Some(m) = spec.model.as_deref().filter(|m| !m.is_empty()) {
            params.insert("model".into(), json!(m));
        }
        let open = match &spec.start {
            Start::New(_) => json!({ "method": "thread/start", "params": params }),
            Start::Resume(id) => {
                params.insert("threadId".into(), json!(id));
                json!({ "method": "thread/resume", "params": params })
            }
        };
        Codex { open, thread: None, transcript: None, turn: None, prompted: false, last_said: String::new(), changes: HashMap::new(), waiting: HashMap::new(), held: vec![], asked: HashMap::new(), next_request: 0, stopped: false }
    }

    fn request(&mut self, method: &str, params: Value, what: Asked) -> String {
        self.next_request += 1;
        self.asked.insert(self.next_request, what);
        json!({ "jsonrpc": "2.0", "id": self.next_request, "method": method, "params": params }).to_string()
    }

    /// Your message as Codex takes it now: a new turn, or more for the one running.
    fn send(&mut self, thread: &str, text: &str) -> String {
        let input = json!([{ "type": "text", "text": text }]);
        match self.turn.clone() {
            Some(turn) => self.request("turn/steer", json!({ "threadId": thread, "expectedTurnId": turn, "input": input }), Asked::Message(text.into())),
            None => self.request("turn/start", json!({ "threadId": thread, "input": input }), Asked::Message(text.into())),
        }
    }

    /// Codex's answer to a request of Cue's.
    fn answered(&mut self, v: &Value) -> (Vec<HostedEvent>, Vec<String>) {
        let Some(what) = v.get("id").and_then(Value::as_u64).and_then(|id| self.asked.remove(&id)) else { return (vec![], vec![]) };
        let error = v.pointer("/error/message").and_then(Value::as_str).map(str::to_string);
        match (what, error) {
            (Asked::Setup, None) => {
                let open = self.open.clone();
                let line = self.request(open["method"].as_str().unwrap_or_default(), open["params"].clone(), Asked::Open);
                (vec![], vec![json!({ "jsonrpc": "2.0", "method": "initialized" }).to_string(), line])
            }
            (Asked::Open, None) => {
                let thread = v.pointer("/result/thread/id").and_then(Value::as_str).unwrap_or_default().to_string();
                self.transcript = v.pointer("/result/thread/path").and_then(Value::as_str).map(str::to_string);
                self.thread = Some(thread.clone());
                let held = std::mem::take(&mut self.held);
                (vec![], held.iter().map(|t| self.send(&thread, t)).collect())
            }
            (Asked::Message(text), Some(why)) => (vec![HostedEvent::NotTaken { text, why }], vec![]),
            (Asked::Setup | Asked::Open, Some(why)) => (vec![HostedEvent::TurnEnded { text: format!("Codex couldn't open the conversation: {why}"), failed: true, cost_usd: 0.0, unprompted: true }], vec![]),
            _ => (vec![], vec![]),
        }
    }

    fn notified(&mut self, method: &str, p: &Value) -> Vec<HostedEvent> {
        match method {
            "turn/started" => {
                self.turn = p.pointer("/turn/id").and_then(Value::as_str).map(str::to_string);
                vec![HostedEvent::TurnStarted]
            }
            "item/started" => self.item_started(p.get("item").unwrap_or(&Value::Null)),
            "item/completed" => self.item_completed(p.get("item").unwrap_or(&Value::Null)),
            "turn/completed" => vec![self.turn_completed(p.get("turn").unwrap_or(&Value::Null))],
            _ => vec![],
        }
    }

    fn item_started(&mut self, item: &Value) -> Vec<HostedEvent> {
        let id = str_of(item, "id");
        match item.get("type").and_then(Value::as_str) {
            Some("commandExecution") => vec![HostedEvent::ToolCall { id, name: "Bash".into(), input: json!({ "command": shell_command(item) }), agent: None }],
            Some("fileChange") => {
                let changes = item.get("changes").cloned().unwrap_or(Value::Null);
                self.changes.insert(id.clone(), changes.clone());
                vec![HostedEvent::ToolCall { id, name: "apply_patch".into(), input: patch_input(&changes), agent: None }]
            }
            Some("mcpToolCall") => vec![HostedEvent::ToolCall { id, name: format!("{}.{}", str_of(item, "server"), str_of(item, "tool")), input: item.get("arguments").cloned().unwrap_or(Value::Null), agent: None }],
            _ => vec![],
        }
    }

    fn item_completed(&mut self, item: &Value) -> Vec<HostedEvent> {
        let id = str_of(item, "id");
        let failed = matches!(item.get("status").and_then(Value::as_str), Some("failed" | "declined"));
        match item.get("type").and_then(Value::as_str) {
            Some("userMessage") => {
                self.prompted = true;
                let text = item.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
                vec![HostedEvent::Taken { text }]
            }
            Some("agentMessage") => {
                let text = str_of(item, "text");
                if text.trim().is_empty() {
                    return vec![];
                }
                self.last_said = text.clone();
                vec![HostedEvent::Said { text, agent: None }]
            }
            Some("commandExecution") => vec![HostedEvent::ToolResult { id, output: item.get("aggregatedOutput").cloned().unwrap_or(Value::Null), failed, agent: None }],
            Some("fileChange") => {
                self.changes.remove(&id);
                vec![HostedEvent::ToolResult { id, output: json!(str_of(item, "status")), failed, agent: None }]
            }
            Some("mcpToolCall") => vec![HostedEvent::ToolResult { id, output: item.get("result").cloned().unwrap_or(Value::Null), failed, agent: None }],
            _ => vec![],
        }
    }

    fn turn_completed(&mut self, turn: &Value) -> HostedEvent {
        self.turn = None;
        let unprompted = !std::mem::replace(&mut self.prompted, false);
        let stopped = std::mem::replace(&mut self.stopped, false);
        let said = std::mem::take(&mut self.last_said);
        let failed = turn.get("status").and_then(Value::as_str) == Some("failed") && !stopped;
        let why = turn.pointer("/error/message").and_then(Value::as_str).map(str::to_string);
        HostedEvent::TurnEnded { text: if failed { why.unwrap_or_else(|| "Codex's turn failed".into()) } else { said }, failed, cost_usd: 0.0, unprompted }
    }

    /// A request of Codex's: a card, kept until it's answered; refused at once if it isn't one Cue knows.
    fn asked(&mut self, id: &Value, method: &str, p: &Value) -> (Vec<HostedEvent>, Vec<String>) {
        let key = id.to_string();
        let item = str_of(p, "itemId");
        let ask = |kind, tool_name: &str, input: Value| Ask { request_id: key.clone(), kind, tool_name: tool_name.into(), input, suggestions: json!([]), agent_id: String::new(), tool_use_id: item.clone(), server: String::new(), message: String::new(), form: Value::Null };
        let (waiting, a) = match method {
            "item/commandExecution/requestApproval" => {
                let decisions = p.get("availableDecisions").and_then(Value::as_array).cloned().unwrap_or_default();
                let amendment = p.get("proposedExecpolicyAmendment").cloned().filter(|a| !a.is_null());
                let mut input = json!({ "command": shell_command(p) });
                if let Some(r) = p.get("reason").and_then(Value::as_str) {
                    input["description"] = json!(r);
                }
                (Waiting::Command { decisions, amendment }, ask(AskKind::Permission, "Bash", input))
            }
            "item/fileChange/requestApproval" => {
                let changes = self.changes.get(&item).cloned().unwrap_or(Value::Null);
                (Waiting::FileChange, ask(AskKind::Permission, "apply_patch", patch_input(&changes)))
            }
            "item/permissions/requestApproval" => {
                let asked = p.get("permissions").cloned().unwrap_or(Value::Null);
                (Waiting::Permissions { asked: asked.clone() }, ask(AskKind::Permission, "Permissions", json!({ "permissions": asked, "description": str_of(p, "reason") })))
            }
            "item/tool/requestUserInput" => {
                let qs = p.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
                let ids = qs.iter().map(|q| (str_of(q, "question"), str_of(q, "id"))).collect();
                let questions: Vec<Value> = qs.iter().map(|q| json!({ "question": str_of(q, "question"), "header": str_of(q, "header"), "options": q.get("options").cloned().unwrap_or(json!([])), "multiSelect": false })).collect();
                (Waiting::Questions(ids), ask(AskKind::Question, "AskUserQuestion", json!({ "questions": questions })))
            }
            "mcpServer/elicitation/request" if p.get("requestedSchema").is_some() => {
                let mut a = ask(AskKind::Input, "", Value::Null);
                a.server = str_of(p, "serverName");
                a.message = str_of(p, "message");
                a.form = p.get("requestedSchema").cloned().unwrap_or(Value::Null);
                (Waiting::Form, a)
            }
            _ => {
                let refusal = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("Cue can't answer a “{method}” request") } });
                return (vec![], vec![refusal.to_string()]);
            }
        };
        self.waiting.insert(key, waiting);
        (vec![HostedEvent::Ask(a)], vec![])
    }
}

impl Protocol for Codex {
    fn opening(&mut self) -> Vec<String> {
        vec![self.request("initialize", json!({ "clientInfo": { "name": "cue", "title": "Cue", "version": env!("CARGO_PKG_VERSION") } }), Asked::Setup)]
    }

    fn read(&mut self, line: &str) -> (Vec<HostedEvent>, Vec<String>) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return (vec![], vec![]) };
        match (v.get("id"), v.get("method").and_then(Value::as_str)) {
            (Some(id), Some(method)) => self.asked(&id.clone(), method, v.get("params").unwrap_or(&Value::Null)),
            (None, Some(method)) => (self.notified(method, v.get("params").unwrap_or(&Value::Null)), vec![]),
            (Some(_), None) => self.answered(&v),
            (None, None) => (vec![], vec![]),
        }
    }

    fn message(&mut self, text: &str) -> Vec<String> {
        match self.thread.clone() {
            Some(thread) => vec![self.send(&thread, text)],
            None => {
                self.held.push(text.to_string());
                vec![]
            }
        }
    }

    fn answer(&mut self, request_id: &str, a: &Answer) -> Result<Vec<String>, String> {
        let waiting = self.waiting.remove(request_id).ok_or("that request isn't waiting for an answer (answered already?)")?;
        let id: Value = serde_json::from_str(request_id).unwrap_or(json!(request_id));
        let result = answer_for(waiting, a);
        let mut lines = vec![json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()];
        // "Do this instead": Codex takes no words with a decline, so they follow as more for its turn.
        if let (Answer::Decision(d), Some(thread)) = (a, self.thread.clone()) {
            if let Some(instead) = d.message.as_deref().filter(|m| d.behavior == "deny" && !m.trim().is_empty()) {
                lines.push(self.send(&thread, instead));
            }
        }
        Ok(lines)
    }

    fn interrupt(&mut self) -> Vec<String> {
        let (Some(thread), Some(turn)) = (self.thread.clone(), self.turn.clone()) else { return vec![] };
        self.stopped = true;
        vec![self.request("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Asked::Other)]
    }

    fn in_turn(&self) -> bool {
        self.turn.is_some()
    }

    fn gone(&mut self) {
        self.waiting.clear();
        self.asked.clear();
    }

    fn wrote(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        match (v.get("id"), v.get("method").and_then(Value::as_str)) {
            // A request of Cue's: what it was, so Codex's answer to it is read as it was then.
            (Some(id), Some(method)) => {
                let Some(n) = id.as_u64() else { return };
                self.next_request = self.next_request.max(n);
                let what = match method {
                    "initialize" => Asked::Setup,
                    "thread/start" | "thread/resume" => Asked::Open,
                    "turn/start" | "turn/steer" => Asked::Message(v.pointer("/params/input/0/text").and_then(Value::as_str).unwrap_or_default().to_string()),
                    "turn/interrupt" => {
                        self.stopped = true;
                        Asked::Other
                    }
                    _ => Asked::Other,
                };
                self.asked.insert(n, what);
            }
            // Cue's answer to a request of Codex's.
            (Some(id), None) => {
                self.waiting.remove(&id.to_string());
            }
            _ => {}
        }
    }

    fn waits_on(&self, request_id: &str) -> bool {
        self.waiting.contains_key(request_id)
    }

    fn session_id(&self) -> Option<String> {
        self.thread.clone()
    }

    fn ready(&self) -> bool {
        self.thread.is_some()
    }

    fn transcript(&self) -> Option<String> {
        self.transcript.clone()
    }
}

/// + New's permission choice as Codex's approval policy and sandbox; none: as Codex's own config has them (a
/// resume). Ask asks before any command Codex doesn't know to be safe, whatever your config's default, as Ask does
/// for Claude Code.
fn permissions(perm: &str) -> (Option<&'static str>, Option<&'static str>) {
    match perm {
        "ask" => (Some("untrusted"), None),
        "edits" => (None, Some("workspace-write")),
        "skip" => (Some("never"), Some("danger-full-access")),
        _ => (None, None),
    }
}

/// Your answer as the request takes it.
fn answer_for(waiting: Waiting, a: &Answer) -> Value {
    let decision = match a {
        Answer::Decision(d) => d.behavior.as_str(),
        Answer::Input { action: InputAction::Accept, .. } => "allow",
        Answer::Input { .. } => "deny",
    };
    let allow = decision != "deny";
    match waiting {
        Waiting::Command { decisions, amendment } => {
            let offered = |d: &str| decisions.is_empty() || decisions.iter().any(|x| x.as_str() == Some(d));
            let always = match amendment {
                Some(a) if decision == "allow_always" => json!({ "acceptWithExecpolicyAmendment": { "execpolicy_amendment": a } }),
                _ if decision == "allow_always" => json!("acceptForSession"),
                _ => Value::Null,
            };
            let d = if !always.is_null() { always } else if allow { json!("accept") } else if offered("decline") { json!("decline") } else { json!("cancel") };
            json!({ "decision": d })
        }
        Waiting::FileChange => json!({ "decision": match decision { "allow_always" => "acceptForSession", "deny" => "decline", _ => "accept" } }),
        Waiting::Permissions { asked } => json!({ "permissions": if allow { asked } else { json!({}) }, "scope": "turn" }),
        Waiting::Questions(ids) => {
            let picked = match a {
                Answer::Decision(d) => d.answers.clone().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            let answers: serde_json::Map<String, Value> = ids.iter().filter_map(|(q, id)| picked.get(q).and_then(Value::as_str).map(|ans| (id.clone(), json!({ "answers": [ans] })))).collect();
            json!({ "answers": answers })
        }
        Waiting::Form => match a {
            Answer::Input { action, content } => json!({ "action": match action { InputAction::Accept => "accept", InputAction::Decline => "decline", InputAction::Cancel => "cancel" }, "content": content }),
            _ => json!({ "action": if allow { "accept" } else { "decline" } }),
        },
    }
}

/// A command as you'd type it: Codex runs each through `zsh -lc '…'`; the words inside are what it runs.
fn shell_command(v: &Value) -> String {
    if let Some(c) = v.get("commandActions").and_then(Value::as_array).and_then(|a| a.first()).and_then(|a| a.get("command")).and_then(Value::as_str) {
        return c.to_string();
    }
    let c = str_of(v, "command");
    for shell in ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc "] {
        if let Some(rest) = c.strip_prefix(shell) {
            return rest.trim_matches('\'').to_string();
        }
    }
    c
}

/// A file change as a card's input: the first file it touches, and the change to each.
fn patch_input(changes: &Value) -> Value {
    let list: Vec<&Value> = changes.as_array().into_iter().flatten().collect();
    let first = list.first().map(|c| str_of(c, "path")).unwrap_or_default();
    let patch = list.iter().map(|c| format!("--- {}\n{}", str_of(c, "path"), str_of(c, "diff"))).collect::<Vec<_>>().join("\n");
    json!({ "file_path": first, "files": list.iter().map(|c| str_of(c, "path")).collect::<Vec<_>>(), "patch": patch })
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}
