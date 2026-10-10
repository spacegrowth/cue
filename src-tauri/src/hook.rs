//! The hook Claude Code and Codex run (their hook protocols match), built into Cue's own binary so
//! it needs nothing installed (no Python on a fresh Mac):
//!
//!   cue hook <event> [claude|codex]
//!
//!   permission  PermissionRequest  -> ask Cue and wait; print the decision JSON
//!   stop        Stop               -> tell Cue "finished, waiting for you"
//!   prompt      UserPromptSubmit   -> tell Cue you're back (clears the waiting card)
//!   end         SessionEnd         -> tell Cue the session is gone
//!   compact     PreCompact         -> tell Cue it's compacting (Claude Code)
//!   notice      Notification       -> a prompt is showing in its terminal that never reached Cue (Claude Code)
//!
//! Never gets in the agent's way: if Cue isn't running or anything goes wrong, it exits 0 with no
//! output and the agent carries on with its normal terminal prompt.

use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::{Duration, Instant};

/// How long to wait for a restarting Cue before giving up on a pending question.
const RECONNECT_FOR: Duration = Duration::from_secs(15 * 60);
/// A Stop hook's own output starts with this; what the agent wrote after it is its follow-up.
const HOOK_MARK: &str = crate::transcript::STOP_HOOK_MARK;

/// Entry point for `cue hook …`. Always returns normally; the caller exits 0.
pub fn main(args: &[String]) {
    let event = args.first().map(String::as_str).unwrap_or("permission");
    let harness = args.get(1).map(String::as_str).unwrap_or("claude");
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(p) = serde_json::from_str::<Value>(&input) else { return };
    let _ = match event {
        "permission" => ask(&p, harness),
        // A finished turn carries its last exchange and who drives it; the rest are just the event.
        // ("notice": a permission prompt has sat in its terminal a few seconds, one Cue can't answer, a
        // sandboxed command's network access, or one that never came through Cue's permission hook.)
        "stop" | "prompt" | "end" | "compact" | "notice" => match event_for(event, &p) {
            Some(("stopped", last)) => send_event(&p, harness, "stopped", &last, Some(final_turn(&p)), &driven_by()),
            Some((name, message)) => send_event(&p, harness, name, &message, None, ""),
            None => None,
        },
        // StopFailure: the turn ended on an API error (a usage limit, an outage) instead of finishing.
        "failure" => send_failure(&p, harness),
        // The status line's input, piped here by the status line script: the plan's usage.
        "usage" => send_usage(&p),
        _ => None,
    };
}

/// Which Cue event a hook event is, and its message: the one table both the Mac's hook and a machine's
/// (machine_hooks) use. None for the ones that aren't plain events (permission, usage) or that Cue skips.
pub(crate) fn event_for(event: &str, p: &Value) -> Option<(&'static str, String)> {
    Some(match event {
        "stop" => ("stopped", s(p, "last_assistant_message")),
        "prompt" => ("active", s(p, "prompt")),
        "end" => ("ended", String::new()),
        "compact" => ("compacting", s(p, "trigger")),
        "notice" if s(p, "notification_type") == "permission_prompt" => ("terminal_ask", s(p, "message")),
        _ => return None,
    })
}

fn s(p: &Value, key: &str) -> String {
    p.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn sock() -> std::path::PathBuf {
    crate::server::socket_path()
}

/// Who drives this session when it isn't you. A tool that runs agents for another agent labels
/// each helper it starts with CUE_DRIVEN_BY; the hook runs inside the agent's process, so it sees
/// that label: the helper's finished turns are its driver's to handle, not "your turn".
fn driven_by() -> String {
    std::env::var("CUE_DRIVEN_BY").unwrap_or_default().trim().to_string()
}

fn ps(args: &[&str]) -> String {
    Command::new("/bin/ps")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The agent's own process: the nearest ancestor whose command line names the harness (Claude runs
/// as "node …/claude"). Shell wrappers that are only running this hook are skipped: tracking one
/// of those would clear the card as soon as the hook finished.
fn agent_pid(harness: &str) -> i32 {
    let parent = std::os::unix::process::parent_id() as i32;
    let mut pid = parent;
    for _ in 0..6 {
        let out = ps(&["-o", "ppid=,command=", "-p", &pid.to_string()]);
        if out.is_empty() {
            break;
        }
        let (ppid, cmd) = out.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((out.as_str(), ""));
        let ours = cmd.contains("cue-hook") || cmd.contains("Cue.app") || cmd.contains(" hook ");
        if cmd.to_lowercase().contains(harness) && !ours {
            return pid;
        }
        match ppid.trim().parse::<i32>() {
            Ok(n) if n > 1 => pid = n,
            _ => break,
        }
    }
    parent
}

/// Where this agent runs: its session, folder and transcript, and its terminal (for Go to tab and
/// typing replies).
fn origin(p: &Value, harness: &str) -> Map<String, Value> {
    let pid = agent_pid(harness);
    let tty = ps(&["-o", "tty=", "-p", &pid.to_string()]);
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let mut m = Map::new();
    m.insert("harness".into(), json!(harness));
    m.insert("session_id".into(), json!(s(p, "session_id")));
    m.insert("cwd".into(), json!(s(p, "cwd")));
    m.insert("transcript_path".into(), json!(s(p, "transcript_path")));
    m.insert("tty".into(), json!(if tty.is_empty() || tty == "??" { String::new() } else { format!("/dev/{tty}") }));
    m.insert("agent_pid".into(), json!(pid));
    m.insert("term_program".into(), json!(env("TERM_PROGRAM")));
    m.insert("iterm_session_id".into(), json!(env("ITERM_SESSION_ID")));
    m.insert("tmux_pane".into(), json!(if env("TMUX").is_empty() { String::new() } else { env("TMUX_PANE") }));
    m.insert("wezterm_pane".into(), json!(env("WEZTERM_PANE")));
    m.insert("kitty_window_id".into(), json!(env("KITTY_WINDOW_ID")));
    m.insert("kitty_listen_on".into(), json!(env("KITTY_LISTEN_ON")));
    m
}

pub(crate) fn connect(timeout: Duration) -> Option<UnixStream> {
    let s = UnixStream::connect(sock()).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    s.set_write_timeout(Some(timeout)).ok()?;
    Some(s)
}

fn send_event(p: &Value, harness: &str, event: &str, message: &str, turn: Option<Vec<Value>>, driver: &str) -> Option<()> {
    let mut s = connect(Duration::from_millis(500))?;
    let mut msg = origin(p, harness);
    msg.insert("type".into(), json!("event"));
    msg.insert("event".into(), json!(event));
    msg.insert("message".into(), json!(message));
    if let Some(id) = p.get("prompt_id").filter(|v| !v.is_null()) {
        msg.insert("prompt_id".into(), id.clone());
    }
    if let Some(t) = turn.filter(|t| !t.is_empty()) {
        msg.insert("turn".into(), Value::Array(t));
    }
    if !driver.is_empty() {
        msg.insert("driven_by".into(), json!(driver));
    }
    s.write_all(format!("{}\n", Value::Object(msg)).as_bytes()).ok()
}

fn send_failure(p: &Value, harness: &str) -> Option<()> {
    let mut s = connect(Duration::from_millis(500))?;
    let mut msg = origin(p, harness);
    let details = s_or(p, "error_details", "last_assistant_message");
    msg.insert("type".into(), json!("event"));
    msg.insert("event".into(), json!("failed"));
    msg.insert("message".into(), json!(details));
    msg.insert("error_type".into(), json!(s_or(p, "error_type", "error")));
    s.write_all(format!("{}\n", Value::Object(msg)).as_bytes()).ok()
}

fn s_or(p: &Value, a: &str, b: &str) -> String {
    let v = s(p, a);
    if v.trim().is_empty() { s(p, b) } else { v }
}

/// Runs on every status line refresh, so it only forwards what's there and never waits long.
fn send_usage(p: &Value) -> Option<()> {
    let rl = p.get("rate_limits").filter(|v| v.is_object()).cloned().unwrap_or(Value::Null);
    let cost = p.pointer("/cost/total_cost_usd").and_then(Value::as_f64);
    let sid = s(p, "session_id");
    // The session's context window as Claude Code knows it (its transcript doesn't say: 200k or 1M).
    let window = p.pointer("/context_window/context_window_size").and_then(Value::as_u64);
    if rl.is_null() && ((cost.is_none() && window.is_none()) || sid.is_empty()) {
        return None;
    }
    let mut c = connect(Duration::from_millis(200))?;
    c.write_all(format!("{}\n", json!({"type": "usage", "rate_limits": rl, "session_id": sid, "cost": cost, "window": window})).as_bytes()).ok()
}

/// Everything the agent wrote since your last prompt, as [{text, hooked}]: `hooked` once a Stop
/// hook sent it back to work (its follow-up, e.g. a checklist, isn't the answer you want first).
/// Claude Code waking it afterwards (a background task finished) starts new work: what it writes then
/// is an answer again, not more of the hook's follow-up.
fn final_turn(p: &Value) -> Vec<Value> {
    let last = s(p, "last_assistant_message").trim().to_string();
    let entries = read_tail(&s(p, "transcript_path"));
    let user_text = |e: &Value| -> Option<String> {
        match e.pointer("/message/content") {
            Some(Value::String(t)) => Some(t.clone()),
            Some(Value::Array(blocks)) if !blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) => Some(
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<String>(),
            ),
            _ => None,
        }
    };
    let flag = |e: &Value, k: &str| e.get(k).and_then(Value::as_bool).unwrap_or(false);
    let kind = |e: &Value| e.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    // The turn starts after your last real prompt (not a tool result, a meta note or a subagent's).
    let mut start = 0;
    for (i, e) in entries.iter().enumerate() {
        if kind(e) == "user" {
            if let Some(t) = user_text(e) {
                if !flag(e, "isMeta") && !flag(e, "isSidechain") && !t.starts_with('<') {
                    start = i + 1;
                }
            }
        }
    }
    let mut parts: Vec<Value> = vec![];
    let mut hooked = false;
    for e in &entries[start.min(entries.len())..] {
        if flag(e, "isSidechain") {
            continue;
        }
        match kind(e).as_str() {
            "user" => {
                let t = user_text(e).unwrap_or_default();
                if t.starts_with(HOOK_MARK) {
                    hooked = true;
                } else if crate::transcript::harness_text(&t) {
                    hooked = false;
                }
            }
            "assistant" => {
                let texts: Vec<String> = match e.pointer("/message/content") {
                    Some(Value::String(t)) => vec![t.clone()],
                    Some(Value::Array(blocks)) => blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(Value::as_str).map(String::from))
                        .collect(),
                    _ => vec![],
                };
                for t in texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                    parts.push(json!({ "text": t, "hooked": hooked }));
                }
            }
            _ => {}
        }
    }
    // The transcript can trail the hook by a moment: make sure the final message is in.
    if !last.is_empty() && parts.last().and_then(|x| x.get("text")).and_then(Value::as_str) != Some(last.as_str()) {
        parts.push(json!({ "text": last, "hooked": hooked }));
    }
    parts
}

/// The last ~2 MB of a JSONL transcript, parsed (the first, possibly partial, line is dropped).
fn read_tail(path: &str) -> Vec<Value> {
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(2_000_000);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return vec![];
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines().skip(1).filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect()
}

/// Cue's harness-neutral decision -> the PermissionRequest output (Claude and Codex share it).
pub(crate) fn to_claude(d: &Value, p: &Value) -> Value {
    let out = if d.get("behavior").and_then(Value::as_str) == Some("deny") {
        let m = d.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()).unwrap_or("Denied in Cue.");
        json!({ "behavior": "deny", "message": m })
    } else if let Some(answers) = d.get("answers").filter(|a| !a.is_null()) {
        let mut input = p.get("tool_input").and_then(Value::as_object).cloned().unwrap_or_default();
        input.insert("answers".into(), answers.clone());
        json!({ "behavior": "allow", "updatedInput": input })
    } else {
        let mut o = json!({ "behavior": "allow" });
        if d.get("behavior").and_then(Value::as_str) == Some("allow_always") {
            if let Some(perm) = d.get("permission").filter(|v| !v.is_null()) {
                o["updatedPermissions"] = json!([perm]);
            }
        }
        o
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": out } })
}

/// Ask Cue and wait for your answer. If Cue restarts mid-wait, ask again when it's back (for up
/// to 15 minutes). If Cue isn't running at all, stay out of the way.
fn ask(p: &Value, harness: &str) -> Option<()> {
    let tool = s(p, "tool_name");
    let mut msg = origin(p, harness);
    msg.insert("type".into(), json!("ask"));
    msg.insert("kind".into(), json!(if tool == "AskUserQuestion" { "question" } else { "permission" }));
    msg.insert("tool_name".into(), json!(tool));
    msg.insert("tool_input".into(), p.get("tool_input").cloned().unwrap_or(json!({})));
    msg.insert("suggestions".into(), p.get("permission_suggestions").cloned().unwrap_or(json!([])));
    let line = format!("{}\n", Value::Object(msg));
    let mut first = true;
    let mut gone_since: Option<Instant> = None;
    loop {
        let Some(mut sock) = connect(Duration::from_secs(1)) else {
            if first {
                return None; // Cue isn't running: stay out of the way
            }
            // Cue is restarting: keep the request alive, but not forever.
            let since = *gone_since.get_or_insert_with(Instant::now);
            if since.elapsed() > RECONNECT_FOR {
                return None;
            }
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        first = false;
        gone_since = None;
        if sock.write_all(line.as_bytes()).is_err() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        // Wait as long as it takes; the agent's own hook timeout is the ceiling.
        let _ = sock.set_read_timeout(None);
        for l in BufReader::new(sock).lines() {
            let Ok(l) = l else { break };
            let Ok(m) = serde_json::from_str::<Value>(&l) else { continue };
            match m.get("type").and_then(Value::as_str) {
                Some("decision") => {
                    print!("{}", to_claude(&m, p));
                    let _ = std::io::stdout().flush();
                    return Some(());
                }
                Some("cancel") | Some("error") => return None, // answered elsewhere: let the agent's own flow win
                _ => {}
            }
        }
        // The connection closed without an answer: Cue quit or restarted. Ask again when it's back.
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The shim the agents' settings point at: a plain shell script in Cue's data folder that hands
/// off to this binary. Cue rewrites it on every launch, so moving or updating the app can't break
/// the hooks, and the settings files never need to know where the app lives.
pub fn write_shim() {
    let Ok(exe) = std::env::current_exe() else { return };
    let dir = crate::server::cue_dir().join("bin");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cue-hook");
    let body = format!("#!/bin/sh\n# Written by Cue on every launch: hands the agent's hook event to Cue.\nexec \"{}\" hook \"$@\"\n", exe.display());
    if std::fs::read_to_string(&path).ok().as_deref() == Some(body.as_str()) {
        return;
    }
    let tmp = dir.join("cue-hook.tmp");
    if std::fs::write(&tmp, &body).is_ok() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_map_to_the_agents_permission_output() {
        let p = json!({ "tool_input": { "questions": [1] } });
        let allow = to_claude(&json!({ "behavior": "allow" }), &p);
        assert_eq!(allow["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
        let deny = to_claude(&json!({ "behavior": "deny" }), &p);
        assert_eq!(deny["hookSpecificOutput"]["decision"]["message"], "Denied in Cue.");
        let always = to_claude(&json!({ "behavior": "allow_always", "permission": { "type": "addRules" } }), &p);
        assert_eq!(always["hookSpecificOutput"]["decision"]["updatedPermissions"], json!([{ "type": "addRules" }]));
        let answered = to_claude(&json!({ "behavior": "allow", "answers": { "Q": "A" } }), &p);
        assert_eq!(answered["hookSpecificOutput"]["decision"]["updatedInput"], json!({ "questions": [1], "answers": { "Q": "A" } }));
    }

    #[test]
    fn the_turn_is_split_at_a_stop_hook_and_ends_with_the_final_message() {
        let dir = std::env::temp_dir().join(format!("cue-hook-t-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let lines = [
            json!({ "type": "summary" }), // the first line is dropped (it may be cut mid-way)
            json!({ "type": "user", "message": { "content": "make the panel clickable" } }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Done: panels open." }] } }),
            json!({ "type": "user", "isMeta": true, "message": { "content": "Stop hook feedback:\nchecklist" } }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Checked: all tests pass." }] } }),
        ];
        std::fs::write(&path, lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n") + "\n").unwrap();
        let p = json!({ "transcript_path": path.to_string_lossy(), "last_assistant_message": "Checked: all tests pass." });
        let t = final_turn(&p);
        assert_eq!(t, vec![json!({ "text": "Done: panels open.", "hooked": false }), json!({ "text": "Checked: all tests pass.", "hooked": true })]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_background_task_finishing_after_the_hook_starts_a_new_answer() {
        let dir = std::env::temp_dir().join(format!("cue-hook-bg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let lines = [
            json!({ "type": "summary" }),
            json!({ "type": "user", "message": { "content": "build the release" } }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "The build is running." }] } }),
            json!({ "type": "user", "isMeta": true, "message": { "content": "Stop hook feedback:\nchecklist" } }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Checked: all tests pass." }] } }),
            json!({ "type": "user", "message": { "content": "<task-notification>\n<status>completed</status>\n</task-notification>" } }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "The release is signed." }] } }),
        ];
        std::fs::write(&path, lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n") + "\n").unwrap();
        let p = json!({ "transcript_path": path.to_string_lossy(), "last_assistant_message": "The release is signed." });
        let parts: Vec<crate::model::TurnPart> = final_turn(&p).into_iter().map(|v| serde_json::from_value(v).unwrap()).collect();
        let (answer, aside) = crate::hub::split_turn("The release is signed.".into(), &parts, "answer");
        assert_eq!(answer, "The release is signed.", "the reply after the task finished is the answer");
        assert_eq!(aside, "Checked: all tests pass.", "only the checklist stays tucked away");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
