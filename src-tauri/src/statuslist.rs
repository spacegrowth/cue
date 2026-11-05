//! The status list: each Claude Code session ends its replies with where it stands (what's done, what it's
//! on, what's left, what it waits on), and Cue shows the latest one as a ring beside the mic (ui/status.js).
//! Cue's own hook carries it (hook.rs): on each message you send, the session is handed its last list and
//! asked to end its reply with an updated one; when a turn that used tools ends without one, it's sent
//! back once to add it. A session gets it once you turn it on from its ring (each starts off), while it's on in
//! Settings → Progress (it is unless turned off there: off, no session gets it and no ring shows). A machine's sessions
//! (+ New → Machine) get it too: their hook waits a moment for Cue's answer (machines/hooks.rs), worked out here
//! from the live copy of their transcript on this Mac.

use serde_json::{json, Value};
use std::collections::HashMap;
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

/// The prompt hook's added context: the ask for a new list, after the session's last list when a compaction has
/// dropped it (`spoken`: a voice message, which gets that list to answer from but no ask). None when there's nothing
/// to add.
pub fn on_prompt(p: &Value, spoken: bool) -> Option<String> {
    let rows = rows(p);
    let (context, wrote) = prompt_context(&rows, spoken);
    // What it costs: this context read in, and the list the last reply wrote out.
    spend(p["session_id"].as_str().unwrap_or_default(), &model_of(&rows), context.as_deref().map_or(0, str::len), wrote);
    context
}

/// The added context for a session's transcript, and how long the list its last reply wrote is. The last list is
/// carried over only when the conversation was compacted after it: until then it's still there, ending that reply.
fn prompt_context(rows: &[Value], spoken: bool) -> (Option<String>, usize) {
    let last = last_list(rows);
    let wrote = last.as_ref().map_or(0, |(_, l)| l.len());
    let carried = last
        .filter(|(at, _)| rows[*at..].iter().any(is_compaction))
        .map(|(_, l)| format!("Your status list from the end of your last reply:\n{l}\n\n"));
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
    spend(p["session_id"].as_str().unwrap_or_default(), &model_of(&rows), reason.len(), 0);
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
/// for several sessions at once: an appended line is never lost): `<ms> <tokens> <usd> <session>`, totalled for
/// Settings → Progress and per session for its open list. (Lines from before the session was written have three
/// fields: they count in the total only.)
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

fn spend(session_id: &str, model: &str, read: usize, wrote: usize) {
    if read + wrote == 0 {
        return;
    }
    let (tokens, usd) = cost(model, read, wrote);
    // One write per line, so lines from hooks running at once never mix.
    let line = format!("{} {tokens} {usd:.6} {session_id}\n", crate::model::now_ms());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(spent_file()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// What a stretch of the log adds up to: replies (lines), tokens, dollars.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Tally {
    replies: u64,
    tokens: u64,
    usd: f64,
}
impl Tally {
    fn add(&mut self, tokens: u64, usd: f64) {
        self.replies += 1;
        self.tokens += tokens;
        self.usd += usd;
    }
    fn json(&self) -> Value {
        json!({ "replies": self.replies, "tokens": self.tokens, "usd": self.usd })
    }
}

/// The log's lines added up: the total, and each session's own (lines that name one).
fn tally(log: &str) -> (Tally, HashMap<String, Tally>) {
    let (mut all, mut by) = (Tally::default(), HashMap::<String, Tally>::new());
    for line in log.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let (Some(t), Some(u)) = (f.get(1).and_then(|t| t.parse().ok()), f.get(2).and_then(|u| u.parse().ok())) else { continue };
        all.add(t, u);
        if let Some(sid) = f.get(3).filter(|s| !s.is_empty()) {
            by.entry((*sid).to_string()).or_default().add(t, u);
        }
    }
    (all, by)
}

/// The total so far, for Settings, and each session's (for its open list): { replies, tokens, usd, sessions: { id: {
/// replies, tokens, usd } } }. Read again only when the file has grown.
pub fn spent() -> Value {
    static SEEN: Mutex<Option<(u64, Value)>> = Mutex::new(None);
    let len = std::fs::metadata(spent_file()).map_or(0, |m| m.len());
    let mut g = crate::locking::lock(&SEEN, "what the status list has cost");
    if g.as_ref().map(|(l, _)| *l) != Some(len) {
        let (all, by) = tally(&std::fs::read_to_string(spent_file()).unwrap_or_default());
        let mut v = all.json();
        v["sessions"] = by.into_iter().map(|(sid, t)| (sid, t.json())).collect::<serde_json::Map<_, _>>().into();
        *g = Some((len, v));
    }
    g.as_ref().map_or(Value::Null, |(_, v)| v.clone())
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

/// The last list the session wrote: its row, and the list from its heading to the end of that reply.
fn last_list(rows: &[Value]) -> Option<(usize, String)> {
    rows.iter().enumerate().rev().filter(|(_, r)| r["type"] == "assistant").find_map(|(i, r)| {
        blocks(r).into_iter().rev().find_map(|b| {
            let text = b["text"].as_str()?;
            text.find(MARKER).map(|at| (i, text[at..].trim_end().to_string()))
        })
    })
}

/// Where Claude Code compacted the conversation: what came before it is no longer there as written.
fn is_compaction(row: &Value) -> bool {
    row["type"] == "system" && row["subtype"] == "compact_boundary"
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
    fn compaction() -> Value { json!({ "type": "system", "subtype": "compact_boundary" }) }

    #[test]
    fn spent_adds_up_the_total_and_each_session() {
        let (all, by) = tally("1 100 0.010000 a\n2 50 0.005000 b\n3 10 0.001000\n4 20 0.002000 a\nbroken\n");
        assert_eq!((all.replies, all.tokens), (4, 180), "every line in the total, old ones without a session too");
        assert!((all.usd - 0.018).abs() < 1e-9);
        assert_eq!((by["a"].replies, by["a"].tokens), (2, 120), "a's own two");
        assert_eq!(by["b"].replies, 1);
        assert_eq!(by.len(), 2, "a line without a session is in no session's");
    }
    fn tool() -> Value { json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "name": "Bash" }] } }) }
    fn result() -> Value { json!({ "type": "user", "message": { "content": [{ "type": "tool_result" }] } }) }

    #[test]
    fn a_message_asks_for_a_new_list_and_carries_the_last_only_after_a_compaction() {
        let rows = [you("go"), said(&format!("Done.\n\n{LIST}")), you("next")];
        assert_eq!(ctx(&rows, false).unwrap(), SPEC, "the last list is still in the conversation: just the ask");
        assert_eq!(prompt_context(&rows, false).1, LIST.len(), "the list it wrote, for its cost");
        let compacted = [you("go"), said(&format!("Done.\n\n{LIST}")), compaction(), you("next")];
        let c = ctx(&compacted, false).unwrap();
        assert!(c.starts_with("Your status list from the end of your last reply:\n**Where we are**\n✓ built it"));
        assert!(c.ends_with(SPEC));
        let since = [you("go"), compaction(), said(&format!("Done.\n\n{LIST}")), you("next")];
        assert_eq!(ctx(&since, false).unwrap(), SPEC, "a list written after the compaction is still there");
        // The first message of a session: just the ask.
        assert_eq!(ctx(&[you("hi")], false).unwrap(), SPEC);
    }

    #[test]
    fn a_spoken_message_gets_the_list_to_answer_from_and_no_ask() {
        assert_eq!(ctx(&[said(LIST)], true), None, "still in the conversation: nothing to add");
        let c = ctx(&[said(LIST), compaction()], true).unwrap();
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
