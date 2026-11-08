//! What each message on the socket does to the hub, with the socket left out: `apply` takes a parsed
//! `ClientMsg` and says what the connection does next (`Outcome`). The socket layer (mod.rs) reads and
//! writes lines and keeps a connection open while an Ask waits or a session listens; tests drive the
//! hub through `apply` exactly as a connection would (testing::World).
//! Every function here is blocking (the store's lock, transcript reads): the socket calls it on the
//! blocking pool.

use crate::hub::{Hub, NewAsk, Reply};
use crate::logs::transcript::{self, Sender};
use crate::model::{AgentCmd, ClientMsg, Origin, TurnPart};
use crate::terminal::focus;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

/// What a connection does once its first message is applied.
pub enum Outcome {
    /// Nothing to say back: the connection ends.
    Quiet,
    /// One line back, then the connection ends.
    Say(Value),
    /// An Ask, queued as `id`: the connection stays open until `answer` comes, or the client speaks again.
    Asked { id: String, answer: oneshot::Receiver<Reply> },
    /// A prompt only its transcript can say the sender of: settled (`settle`) once it does.
    Settle(Prompt),
    /// A session that takes Cue's messages for it on this connection (Pi), as connection number `n`.
    Subscribed { session_id: String, n: u64, messages: mpsc::UnboundedReceiver<Value> },
}

/// An "active" event's prompt whose sender the transcript at `path` says, by Claude's `prompt_id`.
#[derive(Clone)]
pub struct Prompt {
    pub session_id: String,
    pub path: String,
    pub prompt_id: String,
    pub text: String,
    pub voice: bool,
}

/// `msg`, applied to the hub. `peer`: the connecting process, when the socket can say (a subscription
/// is refused to any other process than the session's own).
pub fn apply(hub: &Hub, msg: ClientMsg, peer: Option<i32>) -> Outcome {
    // A session's folder, seen before the hub records it with its project (logs::repos::first_look).
    if let ClientMsg::Ask { origin, .. } | ClientMsg::Event { origin, .. } = &msg {
        if origin.machine.is_empty() {
            crate::logs::repos::first_look(&origin.cwd);
        }
    }
    match msg {
        ClientMsg::Ask { origin, kind, tool_name, tool_input, suggestions, context, agent_id } => {
            let (id, answer) = hub.add_ask(origin, NewAsk { kind, tool_name, tool_input, suggestions, context, agent_id });
            Outcome::Asked { id, answer }
        }
        ClientMsg::Event { origin, event, message, turn, driven_by, prompt_id, error_type, voice } => {
            if event == "failed" {
                hub.failed(origin, &error_type, &message);
                return Outcome::Quiet;
            }
            event_arm(hub, origin, &event, message, turn, &driven_by, prompt_id, voice)
        }
        ClientMsg::Usage { rate_limits, session_id, cost, window, machine } => {
            usage(hub, &rate_limits, &session_id, cost, window, &machine);
            Outcome::Quiet
        }
        ClientMsg::List => Outcome::Say(hub.state()),
        ClientMsg::Probe => Outcome::Say(probe()),
        ClientMsg::Respond { id, decision } => Outcome::Say(json!({"ok": hub.respond(&id, decision)})),
        ClientMsg::Focus { id } => Outcome::Say(told(Ok(go_to(hub, &id)))),
        ClientMsg::Reply { id, text, images } => Outcome::Say(told(Ok(hub.reply(&id, &text, &images)))),
        ClientMsg::SendTo { session_id, text, images, now } => Outcome::Say(told(Ok(hub.send_to_session(&session_id, &text, &images, now)))),
        ClientMsg::Interrupt { session_id } => Outcome::Say(told(Ok(hub.interrupt(&session_id)))),
        ClientMsg::Subscribe { origin, commands } => subscribe(hub, origin, commands, peer),
        ClientMsg::Resolved { .. } | ClientMsg::Commands { .. } => Outcome::Quiet,
    }
}

/// A probe's answer: how Cue is holding up (locking::stats), and the chaos knobs it runs under.
pub fn probe() -> Value {
    let mut v = crate::locking::stats::probe();
    v["chaos"] = crate::chaos::named().into_iter().map(|(k, n)| (k.to_string(), json!(n))).collect::<serde_json::Map<_, _>>().into();
    v
}

/// How a control message went (`r`: the hub call's outcome, or why it never ran), as its client is told.
pub fn told(r: std::io::Result<Result<String, String>>) -> Value {
    let r = r.unwrap_or_else(|e| Err(e.to_string()));
    json!({"ok": r.is_ok(), "detail": r.unwrap_or_else(|e| e)})
}

fn go_to(hub: &Hub, id: &str) -> Result<String, String> {
    focus::focus(&hub.get(id).ok_or("no such item")?.origin)
}

/// The status line's report: the session's context window, the plan's limits, the session's cost.
fn usage(hub: &Hub, rate_limits: &Value, session_id: &str, cost: Option<f64>, window: Option<u64>, machine: &str) {
    if let (false, Some(size)) = (session_id.is_empty(), window) {
        crate::logs::steps::set_window(session_id, size, machine.is_empty());
    }
    // The plan's limits first: whether there are any decides if the session's cost is shown.
    if rate_limits.is_object() {
        hub.set_rates(rate_limits);
    }
    if let (false, Some(c)) = (session_id.is_empty(), cost) {
        hub.set_cost(session_id, c);
    }
}

/// A lifecycle event, told to the hub. An "active" event whose sender only the transcript can say comes
/// back as a `Prompt` to settle once it does. Claude Code's own notice of a finished background task is
/// never yours, whatever the transcript says yet: nothing to settle. (Handed over mid-turn, it carries the
/// turn's promptId: that prompt is yours.)
#[allow(clippy::too_many_arguments)]
fn event_arm(hub: &Hub, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str, prompt_id: String, voice: bool) -> Outcome {
    let harness = transcript::harness_text(&message);
    match tell_event(hub, origin, event, message.clone(), turn, driven_by, &prompt_id) {
        Some((session_id, path)) if !harness => Outcome::Settle(Prompt { session_id, path, prompt_id, text: message, voice }),
        _ => Outcome::Quiet,
    }
}

/// The event told to the hub; for an "active" one whose sender the transcript must say, its session and
/// transcript.
fn tell_event(h: &Hub, origin: Origin, event: &str, message: String, turn: Vec<TurnPart>, driven_by: &str, prompt_id: &str) -> Option<(String, String)> {
    // A session on a machine keeps its transcript there: nothing to look up here, so it's yours
    // straight away (waiting on a file that never shows let its reply land first).
    let path = if origin.machine.is_empty() { origin.transcript_path.clone() } else { String::new() };
    let sid = origin.session_id.clone();
    let driven_by = crate::leads::resolve_driver(&origin.session_id, driven_by);
    let lookup = event == "active" && !prompt_id.is_empty() && !path.is_empty();
    // Nothing to look up (Pi, Codex, a session on a machine): the prompt is yours, noted with the event itself.
    if event == "active" && !lookup && !transcript::harness_text(&message) {
        h.active_from_you(origin, message, turn, &driven_by);
        return None;
    }
    h.event(origin, event, message, turn, &driven_by);
    lookup.then_some((sid, path))
}

/// Who sent `p`, by one look at its transcript: None until it says. Handed over from Claude Code's queue,
/// only the attachment says so, so that's looked at first.
pub fn sender(p: &Prompt) -> Option<Sender> {
    if transcript::queued_from_harness(&p.path, &p.text) {
        return Some(Sender::Harness);
    }
    transcript::prompt_sender(&p.path, &p.prompt_id)
}

/// `p` settled: its sender as the transcript said, or (None: it never said) yours.
pub fn settle(h: &Hub, p: Prompt, said: Option<Sender>) {
    match said {
        Some(Sender::Harness) => {}
        Some(Sender::Peer { name, pid, body }) => h.prompt_from_peer(&p.session_id, &name, pid, &body),
        Some(Sender::Human) | None => {
            h.prompt_from_you(&p.session_id, &p.text);
            if p.voice {
                h.said_by_voice(&p.session_id, &p.text);
            }
        }
    }
}

/// An open Ask's client spoke again: `line`, a `Resolved` (it got its answer in its own terminal), or
/// anything else (it gave up). The request is finished either way.
pub fn ask_ended(h: &Hub, id: &str, line: Option<ClientMsg>) {
    match line {
        Some(ClientMsg::Resolved { by, behavior }) => {
            let by = if by.is_empty() { "terminal".to_string() } else { by };
            h.finish(id, "answered_elsewhere", format!("{behavior} in the {by}").trim());
        }
        _ => gone(h, id),
    }
}

/// An open Ask whose client stopped waiting (it hung up, or never heard it was queued).
pub fn gone(h: &Hub, id: &str) {
    h.finish(id, "gone", "agent stopped waiting");
}

fn subscribe(hub: &Hub, origin: Origin, commands: Vec<AgentCmd>, peer: Option<i32>) -> Outcome {
    let session_id = origin.session_id.clone();
    if session_id.is_empty() {
        return Outcome::Quiet;
    }
    match hub.subscribe(&origin, peer) {
        Ok((n, messages)) => {
            set_commands(hub, &session_id, &commands);
            Outcome::Subscribed { session_id, n, messages }
        }
        Err(e) => Outcome::Say(json!({"type": "error", "error": e})),
    }
}

/// A subscribed session's commands (its "/" menu), as it sent them: at subscribing, and again on Refresh.
pub fn set_commands(hub: &Hub, session_id: &str, commands: &[AgentCmd]) {
    hub.set_agent_commands(session_id, crate::commands::for_pi(commands));
}
