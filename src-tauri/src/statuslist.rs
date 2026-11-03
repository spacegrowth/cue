//! The status list: each Claude Code session ends its replies with where it stands (what's done, what it's
//! on, what's left, what it waits on), and Cue shows the latest one as a ring beside the mic (ui/status.js).
//! Cue's own hook carries it (hook.rs): on each message you send, the session is handed its last list and
//! asked to end its reply with an updated one; when a turn that used tools ends without one, it's sent
//! back once to add it. A session gets it once you turn it on from its ring (each starts off), while it's on in
//! Settings → Progress (it is unless turned off there: off, no session gets it and no ring shows). A machine's sessions
//! (+ New → Machine) get it too: their hook waits a moment for Cue's answer (machines/hooks.rs), worked out here
//! from the live copy of their transcript on this Mac.

use serde_json::{json, Value};
use std::io::Write;
use std::sync::Mutex;

/// The heading that starts the list (ui/status.js finds it by the same words).
pub const MARKER: &str = "**Where we are**";

/// What a session is asked to end its reply with.
pub const SPEC: &str = "End this reply with the status list, exactly this shape, kept short:\n\
**Where we are**\n\
✓ <done item>\n\
→ <item in progress or next>\n\
○ <item still left>\n\
Waiting on you: <decision needed, or 'nothing'> (plain text, no icon)\n\
Items that can run side by side (different files, no order between them) get ∥ after their mark (○ ∥ <item>); ∥ items next to each other run together, everything else in order, top to bottom.\n\
Keep every item from the previous list (tick finished ones, add new asks) until the whole request is done.";

/// On a spoken message: the list only to answer from, no list read out at the end.
const SPOKEN: &str = "This is a spoken ask: answer from this list if it helps, no footer this time.";

/// On for this session: on in Settings (unless turned off there) and turned on for the session from its ring.
pub fn on_for(cfg: &Value, session_id: &str) -> bool {
    cfg.pointer("/status/on").and_then(Value::as_bool).unwrap_or(true)
        && cfg.pointer("/status/sessions").and_then(Value::as_array).is_some_and(|on| on.iter().any(|s| s == session_id))
}

/// The prompt hook's added context: the session's last list, then the ask for a new one (`spoken`: a voice
/// message, which gets the list to answer from but no ask). None when there's nothing to add.
pub fn on_prompt(p: &Value, spoken: bool) -> Option<String> {
    let rows = rows(p);
    let (context, wrote) = prompt_context(&rows, spoken);
    // What it costs: this context read in, and the list the last reply wrote out.
    spend(&model_of(&rows), context.as_deref().map_or(0, str::len), wrote);
    context
}

/// The added context for a session's transcript, and how long the list its last reply wrote is.
fn prompt_context(rows: &[Value], spoken: bool) -> (Option<String>, usize) {
    let last = last_list(rows);
    let wrote = last.as_deref().map_or(0, str::len);
    let carried = last.map(|l| format!("Your status list from the end of your last reply:\n{l}\n\n"));
    let context = match (spoken, carried) {
        (true, None) => None,
        (true, Some(c)) => Some(c + SPOKEN),
        (false, c) => Some(c.unwrap_or_default() + SPEC),
    };
    (context, wrote)
}

/// The stop hook's answer: a turn that used tools and ended without a list is sent back once to add it
/// (never a second time: Claude Code says `stop_hook_active` then). None lets it end.
pub fn on_stop(p: &Value) -> Option<Value> {
    let rows = rows(p);
    let reason = stop_reason(p, &rows)?;
    spend(&model_of(&rows), reason.len(), 0);
    Some(json!({ "decision": "block", "reason": reason }))
}

/// Why a turn is sent back, if it is (see on_stop).
fn stop_reason(p: &Value, rows: &[Value]) -> Option<String> {
    if p["stop_hook_active"].as_bool() == Some(true) || p["last_assistant_message"].as_str().unwrap_or("").contains(MARKER) {
        return None;
    }
    let turn = this_turn(rows);
    (used_tools(turn) && !spoken(turn)).then(|| format!("Your reply is missing the status list. {SPEC}"))
}

/// The sessions it's on for now (none while it's off in Settings): what a machine's hook waits for Cue's answer on.
pub fn sessions_on(cfg: &Value) -> Vec<String> {
    if !cfg.pointer("/status/on").and_then(Value::as_bool).unwrap_or(true) {
        return vec![];
    }
    cfg.pointer("/status/sessions").and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// A machine's session's hook answer (`p`: its hook input, its transcript the live copy on this Mac): the prompt hook's
/// added context, or a reply sent back for its list. None: nothing to add. (No voice line there: a machine's hook
/// can't read Cue's data folder, so nothing it's sent is spoken.)
pub fn remote_answer(event: &str, p: &Value) -> Option<Value> {
    match event {
        "prompt" => on_prompt(p, false).map(|c| json!({ "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": c } })),
        "stop" => on_stop(p),
        _ => None,
    }
}

/// `status.sessions` with one session turned on (or off again) from its ring: the newest 500 kept.
pub fn sessions_with(cfg: &Value, session_id: &str, on: bool) -> Value {
    let mut ids: Vec<Value> = cfg.pointer("/status/sessions").and_then(Value::as_array).cloned().unwrap_or_default();
    ids.retain(|s| s != session_id);
    if on {
        ids.push(json!(session_id));
    }
    let skip = ids.len().saturating_sub(500);
    Value::Array(ids.split_off(skip))
}

/// What the status list has cost, kept as a line per message in Cue's folder (the hook runs once per event, often
/// for several sessions at once: an appended line is never lost), totalled for Settings → Progress.
fn spent_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("status-spent.log")
}

/// The session's model (its latest reply's), for its prices.
fn model_of(rows: &[Value]) -> String {
    rows.iter().rev().find_map(|r| r["message"]["model"].as_str().filter(|m| m.starts_with("claude"))).unwrap_or_default().to_string()
}

/// Tokens from text: about four characters each (an estimate: Claude Code doesn't count the list's own part).
fn tokens(chars: usize) -> u64 {
    chars.div_ceil(4) as u64
}

/// What `read` characters added to a session and `wrote` written by it cost on `model`: the added text as new input
/// (written to its cache once), the list at the output price. Unknown model: counted at Opus's prices.
pub fn cost(model: &str, read: usize, wrote: usize) -> (u64, f64) {
    let r = crate::logs::prices::rates(model).or_else(|| crate::logs::prices::rates("claude-opus-5-5"));
    let (tin, tout) = (tokens(read), tokens(wrote));
    (tin + tout, r.map_or(0.0, |r| (tin as f64 * r.write_5m + tout as f64 * r.output) / 1e6))
}

fn spend(model: &str, read: usize, wrote: usize) {
    if read + wrote == 0 {
        return;
    }
    let (tokens, usd) = cost(model, read, wrote);
    // One write per line, so lines from hooks running at once never mix.
    let line = format!("{} {tokens} {usd:.6}\n", crate::model::now_ms());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(spent_file()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// The total so far, for Settings: { replies, tokens, usd }. Read again only when the file has grown.
pub fn spent() -> Value {
    static SEEN: Mutex<(u64, u64, u64, f64)> = Mutex::new((0, 0, 0, 0.0));
    let len = std::fs::metadata(spent_file()).map_or(0, |m| m.len());
    let mut g = crate::locking::lock(&SEEN, "what the status list has cost");
    if g.0 != len {
        let (mut n, mut tok, mut usd) = (0, 0, 0.0);
        for line in std::fs::read_to_string(spent_file()).unwrap_or_default().lines() {
            let f: Vec<&str> = line.split(' ').collect();
            if let [_, t, u] = f[..] {
                n += 1;
                tok += t.parse::<u64>().unwrap_or(0);
                usd += u.parse::<f64>().unwrap_or(0.0);
            }
        }
        *g = (len, n, tok, usd);
    }
    json!({ "replies": g.1, "tokens": g.2, "usd": g.3 })
}

/// The session's transcript, a row per line (unreadable lines skipped).
fn rows(p: &Value) -> Vec<Value> {
    let Some(path) = p["transcript_path"].as_str().filter(|s| !s.is_empty()) else { return vec![] };
    std::fs::read_to_string(path).map(|t| t.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()).unwrap_or_default()
}

/// A row's content blocks (a plain-text message as one text block).
fn blocks(row: &Value) -> Vec<Value> {
    match &row["message"]["content"] {
        Value::String(s) => vec![json!({ "type": "text", "text": s })],
        Value::Array(a) => a.clone(),
        _ => vec![],
    }
}

/// A message you typed (not a tool's result coming back).
fn is_prompt(row: &Value) -> bool {
    row["type"] == "user" && !blocks(row).iter().any(|b| b["type"] == "tool_result")
}

/// The rows since your last message.
fn this_turn(rows: &[Value]) -> &[Value] {
    let start = rows.iter().rposition(is_prompt).map_or(0, |i| i + 1);
    &rows[start..]
}

/// A turn you started by voice: the voice line Cue added to it (talk.rs) is in its transcript.
fn spoken(rows: &[Value]) -> bool {
    rows.iter().any(|r| r.to_string().contains(&crate::talk::ASK[..30]))
}

fn used_tools(rows: &[Value]) -> bool {
    rows.iter().filter(|r| r["type"] == "assistant").any(|r| blocks(r).iter().any(|b| b["type"] == "tool_use"))
}

/// The last list the session wrote: from its heading to the end of that reply.
fn last_list(rows: &[Value]) -> Option<String> {
    rows.iter().rev().filter(|r| r["type"] == "assistant").flat_map(|r| blocks(r).into_iter().rev()).find_map(|b| {
        let text = b["text"].as_str()?;
        text.find(MARKER).map(|at| text[at..].trim_end().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The added context for these rows (the decision alone: nothing is recorded).
    fn ctx(rows: &[Value], spoken: bool) -> Option<String> {
        prompt_context(rows, spoken).0
    }
    const LIST: &str = "**Where we are**\n✓ built it\n→ testing it\nWaiting on you: nothing";
    fn you(text: &str) -> Value { json!({ "type": "user", "message": { "content": text } }) }
    fn said(text: &str) -> Value { json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } }) }
    fn tool() -> Value { json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "name": "Bash" }] } }) }
    fn result() -> Value { json!({ "type": "user", "message": { "content": [{ "type": "tool_result" }] } }) }

    #[test]
    fn a_message_carries_the_last_list_and_asks_for_a_new_one() {
        let rows = [you("go"), said(&format!("Done.\n\n{LIST}")), you("next")];
        let c = ctx(&rows, false).unwrap();
        assert!(c.starts_with("Your status list from the end of your last reply:\n**Where we are**\n✓ built it"));
        assert!(c.ends_with(SPEC));
        assert_eq!(prompt_context(&rows, false).1, LIST.len(), "the list it wrote, for its cost");
        // The first message of a session: just the ask.
        assert_eq!(ctx(&[you("hi")], false).unwrap(), SPEC);
    }

    #[test]
    fn a_spoken_message_gets_the_list_to_answer_from_and_no_ask() {
        let c = ctx(&[said(LIST)], true).unwrap();
        assert!(c.contains("✓ built it") && c.ends_with(SPOKEN) && !c.contains("End this reply"));
        assert_eq!(ctx(&[you("hi")], true), None);
    }

    #[test]
    fn a_turn_that_used_tools_and_left_out_the_list_is_sent_back_once() {
        let rows = [you("fix it"), tool(), result(), said("Fixed.")];
        let mut p = json!({ "last_assistant_message": "Fixed." });
        assert!(stop_reason(&p, &rows).unwrap().contains(MARKER));
        // Not twice, not with the list there.
        p["stop_hook_active"] = json!(true);
        assert_eq!(stop_reason(&p, &rows), None);
        p["stop_hook_active"] = json!(false);
        p["last_assistant_message"] = json!(format!("Fixed.\n{LIST}"));
        assert_eq!(stop_reason(&p, &rows), None);
        // Not on a spoken one (Cue's voice line is in the turn).
        let voice = json!({ "type": "attachment", "attachment": { "type": "hook_additional_context", "content": [crate::talk::ASK] } });
        let spoken = [you("fix it"), voice, tool(), result(), said("Fixed.")];
        assert_eq!(stop_reason(&json!({ "last_assistant_message": "Fixed." }), &spoken), None);
    }

    #[test]
    fn its_cost_is_the_added_text_as_input_and_the_list_as_output() {
        let r = crate::logs::prices::rates("claude-opus-5-5").unwrap();
        let (tok, usd) = cost("claude-opus-5-5", 400, 200);
        assert_eq!(tok, 150);
        assert!((usd - (100.0 * r.write_5m + 50.0 * r.output) / 1e6).abs() < 1e-12);
        // A model Cue doesn't know: Opus's prices, not nothing.
        assert_eq!(cost("mystery", 400, 200).1, usd);
        assert_eq!(cost("claude-opus-5-5", 0, 0), (0, 0.0));
    }

    #[test]
    fn a_turn_without_tools_ends_as_it_is() {
        assert_eq!(stop_reason(&json!({ "last_assistant_message": "4" }), &[you("what's 2+2"), said("4")]), None);
    }

    #[test]
    fn each_session_is_off_until_turned_on_and_settings_off_turns_every_one_off() {
        assert!(!on_for(&json!({}), "a"), "a session starts off");
        let cfg = json!({ "status": { "sessions": ["a"] } });
        assert!(on_for(&cfg, "a") && !on_for(&cfg, "b"), "on for the session turned on, Settings on unless turned off");
        assert!(!on_for(&json!({ "status": { "on": false, "sessions": ["a"] } }), "a"), "off in Settings: off for every one");
        // A session turned on, then off again; never listed twice.
        assert_eq!(sessions_with(&cfg, "b", true), json!(["a", "b"]));
        assert_eq!(sessions_with(&cfg, "a", true), json!(["a"]));
        assert_eq!(sessions_with(&cfg, "a", false), json!([]));
        // What a machine's hook waits on: those turned on, none while it's off in Settings.
        assert_eq!(sessions_on(&cfg), vec!["a".to_string()]);
        assert!(sessions_on(&json!({ "status": { "on": false, "sessions": ["a"] } })).is_empty());
    }
}
