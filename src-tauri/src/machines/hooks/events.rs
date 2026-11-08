//! Handing a machine's event to Cue as the Mac's hook would, its log line, and a permission question's
//! answer sent back to the machine.

use super::watch::new_session_seen;
use crate::machines::{self, Machine};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};

/// What Cue's socket gets for an event from a machine: the same message the Mac's hook sends, with the
/// session placed on that machine. None for events Cue doesn't take.
pub fn to_cue(machine: &str, ev: &Value) -> Option<Value> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let p = ev.get("input")?;
    let event = s(ev, "event");
    // From its status line: only the session's context window (the plan's limits and cost are this
    // Mac's to show, and a machine's window is its own, not its model's everywhere).
    if event == "usage" {
        let window = p.pointer("/context_window/context_window_size").and_then(Value::as_u64)?;
        return Some(json!({ "type": "usage", "session_id": s(p, "session_id"), "window": window, "machine": machine }));
    }
    let harness = Some(s(ev, "harness")).filter(|h| !h.is_empty()).unwrap_or_else(|| "claude".into());
    let mut msg = Map::new();
    msg.insert("harness".into(), json!(harness));
    msg.insert("session_id".into(), json!(s(p, "session_id")));
    msg.insert("cwd".into(), json!(s(p, "cwd")));
    msg.insert("transcript_path".into(), json!(s(p, "transcript_path")));
    let in_tmux = !s(ev, "tmux").is_empty();
    msg.insert("tmux_pane".into(), json!(if in_tmux { s(ev, "pane") } else { String::new() }));
    msg.insert("term_program".into(), json!(if in_tmux { "tmux" } else { "" }));
    msg.insert("machine".into(), json!(machine));
    let mut event_msg = |name: &str, message: String| {
        msg.insert("type".into(), json!("event"));
        msg.insert("event".into(), json!(name));
        msg.insert("message".into(), json!(message));
        if let Some(id) = p.get("prompt_id").filter(|v| !v.is_null()) {
            msg.insert("prompt_id".into(), id.clone());
        }
    };
    if let Some((name, message)) = crate::hook::event_for(&event, p) {
        event_msg(name, message);
        return Some(Value::Object(msg));
    }
    match event.as_str() {
        "failure" => {
            event_msg("failed", Some(s(p, "error_details")).filter(|d| !d.is_empty()).unwrap_or_else(|| s(p, "last_assistant_message")));
            msg.insert("error_type".into(), json!(Some(s(p, "error_type")).filter(|d| !d.is_empty()).unwrap_or_else(|| "error".into())));
        }
        "permission" => {
            let tool = s(p, "tool_name");
            msg.insert("type".into(), json!("ask"));
            msg.insert("kind".into(), json!(if tool == "AskUserQuestion" { "question" } else { "permission" }));
            msg.insert("tool_name".into(), json!(tool));
            msg.insert("tool_input".into(), p.get("tool_input").cloned().unwrap_or(json!({})));
            msg.insert("suggestions".into(), p.get("permission_suggestions").cloned().unwrap_or(json!([])));
        }
        _ => return None,
    }
    Some(Value::Object(msg))
}

/// One line per event from a machine, in machines.log in Cue's data folder (cut back past 256 KB): when
/// the machine wrote it (from its id), when Cue got it, and for which session. What to look at when a
/// machine's session seems slow to report.
fn log(machine: &str, id: &str, ev: &Value) {
    let path = crate::server::cue_dir().join("machines.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    let written: u64 = id.split('-').next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let now = crate::model::now_ms();
    let line = format!(
        "{now} {machine} {} {} written {written}s, {}s ago\n",
        ev["event"].as_str().unwrap_or("?"),
        ev["input"]["session_id"].as_str().unwrap_or("?"),
        (now / 1000).saturating_sub(written)
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub(crate) fn log_raw(machine: &str, line: &str) {
    let path = crate::server::cue_dir().join("machines.log");
    let cut: String = line.chars().take(600).collect();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(format!("{} {machine} UNREADABLE {cut}\n", crate::model::now_ms()).as_bytes());
    }
}

/// Hand an event to Cue. A permission question waits for your answer (on its own thread) and sends it
/// back to the machine; answered elsewhere, an empty answer lets the agent's own prompt stand.
pub(super) fn handle(m: &Machine, id: &str, ev: Value) {
    log(&m.name, id, &ev);
    let Some(mut msg) = to_cue(&m.name, &ev) else { return };
    let event = ev["event"].as_str().unwrap_or_default().to_string();
    if let Some(sid) = msg["session_id"].as_str().filter(|s| !s.is_empty()) {
        new_session_seen(&m.name, sid);
    }
    // The session reads its log from a live copy on this Mac (machine_logs); the hooks carry on as well.
    if let Some(remote) = msg["transcript_path"].as_str().filter(|p| !p.is_empty()).map(str::to_string) {
        // Never the machine's own path: Cue would read it on this Mac (a stat on /home can stall). No copy (it
        // couldn't start): no log for now, as for a session that has none.
        let local = if ev["event"] == "end" {
            crate::machines::logs::stop(m, &remote);
            crate::machines::logs::copy_of(m, &remote)
        } else {
            crate::machines::logs::local_copy(m, &remote)
        };
        msg["transcript_path"] = json!(local);
    }
    // A reply sent back for its status list isn't the end of the turn: Cue isn't told. Nor is anything of a session
    // cue-host runs: Cue hears the rest from it directly.
    if answer_status_list(m, id, &event, &ev, &msg) || ev["hosted"] == "1" {
        return;
    }
    if msg["type"] != "ask" {
        if let Some(mut s) = crate::hook::connect(Duration::from_millis(800)) {
            if s.write_all(format!("{msg}\n").as_bytes()).is_ok() {
                // Cue hangs up once it has taken the event in: wait for that (up to the timeout), so
                // the next line, and the machine counting as reached, come after it.
                let _ = s.shutdown(std::net::Shutdown::Write);
                let _ = std::io::Read::read(&mut s, &mut [0u8; 64]);
            }
        }
        return;
    }
    let (m, id, p) = (m.clone(), id.to_string(), ev["input"].clone());
    std::thread::spawn(move || {
        let answer = ask(&msg).map(|d| crate::hook::to_claude(&d, &p).to_string()).unwrap_or_default();
        let wrote = machines::run_with_input(&m.host, &format!(r#"umask 077; cat >"$HOME/.cue/out/{id}.tmp" && mv "$HOME/.cue/out/{id}.tmp" "$HOME/.cue/out/{id}.json""#), &answer);
        // The hook takes an answer within a fifth of a second. Still there after a few: the hook had
        // stopped waiting (Cue was out of reach too long), so the agent never got it and is asking in
        // its terminal: Cue says so, instead of showing it at work on an answer it never had.
        if !answer.is_empty() && wrote.is_ok() && !collected(&m.host, &id) {
            let _ = machines::run(&m.host, &format!(r#"rm -f "$HOME/.cue/out/{id}.json""#));
            not_delivered(msg);
        }
    });
}

/// The status list, for a session it's on for: Cue's answer for the hook waiting on the machine (it always gets one, if
/// only an empty one, so it stops waiting). Whether a reply was sent back for its list.
fn answer_status_list(m: &Machine, id: &str, event: &str, ev: &Value, msg: &Value) -> bool {
    if !((event == "prompt" || event == "stop") && ev["harness"] == "claude" && msg["session_id"].as_str().is_some_and(|sid| crate::statuslist::on_for(&crate::config::load(), sid))) {
        return false;
    }
    let mut p = ev["input"].clone();
    p["transcript_path"] = msg["transcript_path"].clone();
    let answer = crate::statuslist::remote_answer(event, &p);
    let sent_back = event == "stop" && answer.is_some();
    let (host, id, text) = (m.host.clone(), id.to_string(), answer.map(|a| a.to_string()).unwrap_or_default());
    std::thread::spawn(move || {
        let _ = machines::run_with_input(&host, &format!(r#"umask 077; cat >"$HOME/.cue/out/{id}.tmp" && mv "$HOME/.cue/out/{id}.tmp" "$HOME/.cue/out/{id}.json""#), &text);
    });
    sent_back
}

/// Did the hook on the machine take answer `id` (the file is gone) within a few seconds?
fn collected(host: &str, id: &str) -> bool {
    let end = Instant::now() + Duration::from_secs(4);
    loop {
        // An answer that can't be checked (the line dropped again) counts as taken: no false alarm.
        match machines::run(host, &format!(r#"[ -e "$HOME/.cue/out/{id}.json" ] && echo here; true"#)) {
            Ok(o) if o.trim() == "here" => {}
            _ => return true,
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Tell Cue the answer to the ask `msg` never reached its agent: it's asking in its terminal.
fn not_delivered(mut msg: Value) {
    let Some(o) = msg.as_object_mut() else { return };
    for k in ["kind", "tool_name", "tool_input", "suggestions"] {
        o.remove(k);
    }
    o.insert("type".into(), json!("event"));
    o.insert("event".into(), json!("answer_lost"));
    o.insert("message".into(), json!("Your answer from Cue didn't reach it (it had stopped waiting for Cue): it's asking in its terminal"));
    if let Some(mut s) = crate::hook::connect(Duration::from_millis(800)) {
        let _ = s.write_all(format!("{msg}\n").as_bytes());
    }
}

/// Ask Cue's socket and wait for the decision (None: answered elsewhere, or Cue went away).
fn ask(msg: &Value) -> Option<Value> {
    let mut s = crate::hook::connect(Duration::from_secs(1))?;
    s.write_all(format!("{msg}\n").as_bytes()).ok()?;
    let _ = s.set_read_timeout(None);
    for l in BufReader::new(s).lines() {
        let m: Value = serde_json::from_str(&l.ok()?).ok()?;
        match m.get("type").and_then(Value::as_str) {
            Some("decision") => return Some(m),
            Some("cancel") | Some("error") => return None,
            _ => {}
        }
    }
    None
}
