//! What a session is about: the About tab Cue keeps for each session you follow.
//!
//! Turning it on has a small model read the session's chat (Cue's log of it: any agent, any machine) and say what
//! the session is about, where it stands and, in a few sentences, what it's done. After each of its turns the small
//! model reads only what was said since and adds a line for the turn. Refresh reads the chat again. What it all cost
//! is added up as it goes. (Asking a copy of the session instead, as btw does, re-reads its whole conversation at its
//! own model's price: $42 for a 450k-token session whose cache had gone. The chat read by Haiku: a cent.)
//!
//! Kept in Cue's folder, tasks/<session id>.json (the name is from when this was a list of what's left; the window's
//! state and the settings still say `tasks`). The agent's own tools are left alone: none of this is in its context.

use crate::btw::Spent;
use crate::hub::Hub;
use crate::model::{now_ms, Exchange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The most of the chat a small model reads at once: the newest messages, each cut to MSG_CHARS.
const CHAT_CHARS: usize = 40_000;
const MSG_CHARS: usize = 4_000;

/// A followed session. A file written when it also kept a list reads too: the list is left out.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// The newest message it has read.
    #[serde(default)]
    pub seen_ms: u64,
    /// Why the last read failed; "" when it didn't.
    #[serde(default)]
    pub err: String,
    /// What following this session has cost so far.
    #[serde(default)]
    pub spent: Spent,
    #[serde(default)]
    pub updated_ms: u64,
    /// What it's doing now ("listing", "updating"); "" when idle. Not kept: a restart finds it idle.
    #[serde(skip_deserializing)]
    pub busy: String,
    /// What the session is about, one line: set by the first read, changed only when the work does.
    #[serde(default)]
    pub about: String,
    /// Where it stands now, a line or two, after its last turn.
    #[serde(default)]
    pub now: String,
    /// The session in a few plain sentences (what it's for, what's done, what's open).
    #[serde(default)]
    pub summary: String,
    /// A line per turn it read, oldest first, the newest TURNS_KEPT: what "since you looked" shows.
    #[serde(default)]
    pub turns: Vec<Turn>,
}

/// One turn, in a line: when it ended (its reply's time) and what it did.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub at_ms: u64,
    pub line: String,
}

const TURNS_KEPT: usize = 60;

/// The small model, asked `prompt` about session `session_id` (btw::ask_small). In tests, the one a test stands
/// in (`small::stand_in`), never Claude Code itself.
fn ask_small(prompt: &str, session_id: &str) -> Result<crate::btw::Answer, String> {
    #[cfg(test)]
    return small::ask(prompt, session_id);
    #[cfg(not(test))]
    crate::btw::ask_small(prompt, session_id)
}

/// Tests: the small model as a test says it answers, and the tracks forgotten between tests.
#[cfg(test)]
pub mod small {
    use std::sync::{Arc, Mutex};

    type Model = Arc<dyn Fn(&str, &str) -> Result<crate::btw::Answer, String> + Send + Sync>;
    static MODEL: Mutex<Option<Model>> = Mutex::new(None);

    /// `model` answers every ask from now on (the prompt and the session's id in, its answer out).
    pub fn stand_in(model: impl Fn(&str, &str) -> Result<crate::btw::Answer, String> + Send + Sync + 'static) {
        *crate::locking::lock(&MODEL, "the small model's stand-in") = Some(Arc::new(model));
    }

    pub(super) fn ask(prompt: &str, session_id: &str) -> Result<crate::btw::Answer, String> {
        let model = crate::locking::lock(&MODEL, "the small model's stand-in").clone().ok_or("no small model in tests")?;
        model(prompt, session_id)
    }

    /// No stand-in, and every track and total forgotten: the next read is from this test's folder.
    pub fn reset() {
        *crate::locking::lock(&MODEL, "the small model's stand-in") = None;
        *crate::locking::lock(&super::TRACKS, "what each session is about") = None;
        *crate::locking::lock(&super::TOTAL, "what About has cost") = None;
    }
}

/// About is on (Settings → Summaries): off, nothing is read or asked for, and the window shows none of it.
pub fn on() -> bool {
    crate::config::effective().pointer("/tasks/on").and_then(Value::as_bool).unwrap_or(false)
}

static TRACKS: Mutex<Option<HashMap<String, Track>>> = Mutex::new(None);

/// Every call it has made, in every session, kept when a session stops being followed (Settings shows it).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Total {
    pub tokens: u64,
    pub usd: f64,
    pub calls: u64,
}

static TOTAL: Mutex<Option<Total>> = Mutex::new(None);

fn total_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("tasks-spent.json")
}

/// One more call's cost on the running total, kept in Cue's folder.
fn spend(s: Spent) {
    let mut g = crate::locking::lock(&TOTAL, "what About has cost");
    let t = g.get_or_insert_with(|| std::fs::read_to_string(total_file()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default());
    t.tokens += s.tokens;
    t.usd += s.usd;
    t.calls += 1;
    if let Ok(text) = serde_json::to_string(&*t) {
        let _ = std::fs::write(total_file(), text);
    }
}

/// What it has cost so far, and how many sessions are followed now, for Settings.
pub fn spent() -> Value {
    let t = *crate::locking::lock(&TOTAL, "what About has cost")
        .get_or_insert_with(|| std::fs::read_to_string(total_file()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default());
    let tracked = with(|all| all.len());
    serde_json::json!({ "tokens": t.tokens, "usd": t.usd, "calls": t.calls, "tracked": tracked })
}

fn dir() -> std::path::PathBuf {
    crate::server::cue_dir().join("tasks")
}

/// Ids are Cue's own (`session_id` from a session), but a file name takes only the characters ids use.
fn file(session_id: &str) -> Option<std::path::PathBuf> {
    (!session_id.is_empty() && session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| dir().join(format!("{session_id}.json")))
}

/// The tracks, read from Cue's folder the first time.
fn with<R>(f: impl FnOnce(&mut HashMap<String, Track>) -> R) -> R {
    let mut g = crate::locking::lock(&TRACKS, "what each session is about");
    let all = g.get_or_insert_with(|| {
        let mut m = HashMap::new();
        for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
            let p = e.path();
            let (Some(sid), Ok(text)) = (p.file_stem().and_then(|s| s.to_str()).map(str::to_string), std::fs::read_to_string(&p)) else { continue };
            if let Ok(t) = serde_json::from_str::<Track>(&text) {
                m.insert(sid, t);
            }
        }
        m
    });
    f(all)
}

fn save(session_id: &str, t: &Track) {
    let Some(path) = file(session_id) else { return };
    let _ = std::fs::create_dir_all(dir());
    if let Ok(text) = serde_json::to_string_pretty(t) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Change a session's track (if it's followed) and keep it; what `f` returns.
fn change<R>(session_id: &str, f: impl FnOnce(&mut Track) -> R) -> Option<R> {
    with(|all| {
        let t = all.get_mut(session_id)?;
        let r = f(t);
        save(session_id, t);
        Some(r)
    })
}

/// Every followed session's About, for the window's state.
pub fn view() -> Value {
    with(|all| serde_json::to_value(&*all).unwrap_or(Value::Null))
}

pub fn is_tracked(session_id: &str) -> bool {
    with(|all| all.contains_key(session_id))
}

/// Follow a session: its first read, asked for in the background.
pub fn start(hub: Arc<Hub>, session_id: &str) -> Result<(), String> {
    if !on() {
        return Err("About is off: turn it on in Settings → Summaries".into());
    }
    if file(session_id).is_none() {
        return Err("not a session Cue can follow".into());
    }
    hub.session_origin(session_id).ok_or("that session is gone")?;
    let fresh = with(|all| {
        if all.get(session_id).is_some_and(|t| !t.busy.is_empty()) {
            return false;
        }
        let t = all.entry(session_id.to_string()).or_default();
        t.busy = "listing".into();
        t.err.clear();
        save(session_id, t);
        true
    });
    if !fresh {
        return Err("it's already reading the chat".into());
    }
    hub.redraw();
    let sid = session_id.to_string();
    std::thread::spawn(move || {
        let r = read_all(&hub, &sid);
        change(&sid, |t| {
            t.busy.clear();
            t.updated_ms = now_ms();
            if let Err(e) = r {
                t.err = e;
            }
        });
        hub.redraw();
    });
    Ok(())
}

/// Read the whole chat again (Refresh).
pub fn refresh(hub: Arc<Hub>, session_id: &str) -> Result<(), String> {
    if !is_tracked(session_id) {
        return Err("this session isn't followed".into());
    }
    start(hub, session_id)
}

/// Stop following a session: what it kept goes.
pub fn stop(session_id: &str) {
    with(|all| all.remove(session_id));
    if let Some(p) = file(session_id) {
        let _ = std::fs::remove_file(p);
    }
}

/// A followed session's turn ended: read what was said since it last looked, in the background. While it's busy,
/// nothing: the next turn's end reads from where it left off.
pub fn turn_ended(hub: Arc<Hub>, session_id: &str) {
    let go = with(|all| match all.get_mut(session_id) {
        Some(t) if t.busy.is_empty() => {
            t.busy = "updating".into();
            true
        }
        _ => false,
    });
    if !go {
        return;
    }
    hub.redraw();
    let sid = session_id.to_string();
    std::thread::spawn(move || {
        let r = update(&hub, &sid);
        change(&sid, |t| {
            t.busy.clear();
            if let Err(e) = r {
                t.err = e;
            } else {
                t.err.clear();
            }
        });
        hub.redraw();
    });
}

/// The first read, or Refresh's: the small model reads the chat.
fn read_all(hub: &Hub, session_id: &str) -> Result<(), String> {
    let log = hub.chat_log(session_id);
    if log.is_empty() {
        return Err("nothing has been said in this session yet".into());
    }
    let a = ask_small(&chat_prompt(&log), session_id)?;
    spend(a.spent);
    let r = parse(&a.text).ok_or_else(|| format!("its answer wasn't what was asked: {}", crate::logs::transcript::cut(&a.text, 200)))?;
    let seen = log.last().map(|e| e.at_ms).unwrap_or(0);
    change(session_id, |t| {
        r.lay_on(t);
        t.spent.tokens += a.spent.tokens;
        t.spent.usd += a.spent.usd;
        t.seen_ms = t.seen_ms.max(seen);
    });
    Ok(())
}

/// After a turn: what was said since it last looked, to the small model; a line for the turn.
fn update(hub: &Hub, session_id: &str) -> Result<(), String> {
    let (seen, about, summary) = with(|all| all.get(session_id).map(|t| (t.seen_ms, t.about.clone(), t.summary.clone()))).ok_or("this session isn't followed")?;
    let since: Vec<Exchange> = hub.chat_log(session_id).into_iter().filter(|e| e.at_ms > seen).collect();
    if !since.iter().any(|e| e.role == "agent") {
        return Ok(());
    }
    let a = ask_small(&update_prompt(&about, &summary, &since), session_id)?;
    spend(a.spent);
    let r = parse(&a.text).ok_or_else(|| format!("its answer wasn't what was asked: {}", crate::logs::transcript::cut(&a.text, 200)))?;
    let newest = since.last().map(|e| e.at_ms).unwrap_or(seen);
    // The turn's line goes at its reply's time, where "since you looked" counts from.
    let reply_at = since.iter().rev().find(|e| e.role == "agent").map(|e| e.at_ms).unwrap_or(newest);
    change(session_id, |t| {
        r.lay_on(t);
        t.spent.tokens += a.spent.tokens;
        t.spent.usd += a.spent.usd;
        t.seen_ms = newest;
        t.updated_ms = now_ms();
        if !r.turn.is_empty() {
            t.turns.push(Turn { at_ms: reply_at, line: r.turn.clone() });
            let over = t.turns.len().saturating_sub(TURNS_KEPT);
            t.turns.drain(..over);
        }
    });
    Ok(())
}

/// The About tab's few sentences under About: the one line alone doesn't say what the session did or where it's going.
const SUMMARY_WORDS: &str = "\"summary\": the whole session in 2 to 4 plain sentences for someone who didn't follow it: what \
it's for, what's been done, and what's still open. No file names or code unless needed.";

/// What the small model is asked from the whole chat.
fn chat_prompt(log: &[Exchange]) -> String {
    format!(
        "A coding agent's session with its owner, oldest first:\n\n{}\n\n\
         Say, in plain words for the owner: what the whole session is about, under 10 words (\"about\"); where it stands and \
         what it waits on, under 25 words (\"now\"). {SUMMARY_WORDS}\n\n\
         Answer with only this JSON: {{\"about\":\"under 10 words\",\"now\":\"under 25 words\",\"summary\":\"2 to 4 plain sentences\"}}",
        chat(log)
    )
}

fn update_prompt(about: &str, summary: &str, since: &[Exchange]) -> String {
    format!(
        "A coding agent's session{}.{} What was said since, oldest first:\n\n{}\n\n\
         Say, in plain words for the owner: what the agent did in these messages, under 12 words (\"turn\"); where the session \
         stands and what it waits on, under 25 words (\"now\"); and what the whole session is about, under 10 words (\"about\"){}. \
         {SUMMARY_WORDS} Rewrite it with these messages in.\n\n\
         Answer with only this JSON: {{\"turn\":\"under 12 words\",\"now\":\"under 25 words\",\"about\":\"\",\"summary\":\"2 to 4 plain sentences\"}}",
        if about.is_empty() { String::new() } else { format!(", about: {about}") },
        if summary.is_empty() { String::new() } else { format!(" In short so far: {summary}") },
        chat(since),
        // One made before it had an about (or whose first answer left it out) gets one now; else only when the work moved.
        if about.is_empty() { "" } else { ", only if the work has moved to something else, else leave it empty" },
    )
}

/// The messages as the small model reads them, the newest kept when they're long.
fn chat(log: &[Exchange]) -> String {
    let who = |r: &str| match r { "agent" => "Agent", "you" => "Owner", _ => "Another session" };
    let mut msgs: Vec<String> = vec![];
    let mut total = 0;
    for e in log.iter().rev() {
        let m = format!("{}: {}", who(&e.role), crate::logs::transcript::cut(e.text.trim(), MSG_CHARS));
        if total + m.len() > CHAT_CHARS && !msgs.is_empty() {
            msgs.push("(earlier messages left out)".into());
            break;
        }
        total += m.len();
        msgs.push(m);
    }
    msgs.reverse();
    msgs.join("\n\n")
}

/// The JSON object in a model's answer (it may wrap it in a fence or a word).
fn json_in(text: &str) -> Option<Value> {
    let (a, b) = (text.find('{')?, text.rfind('}')?);
    serde_json::from_str(text.get(a..=b)?).ok()
}

/// An answer: what the turn did (after a turn only), where it stands, what it's about, the session in short.
#[derive(Debug, Default, PartialEq)]
struct Read {
    turn: String,
    now: String,
    about: String,
    summary: String,
}

impl Read {
    /// What it said onto the track; what it left empty stays as it was.
    fn lay_on(&self, t: &mut Track) {
        for (to, from) in [(&mut t.about, &self.about), (&mut t.now, &self.now), (&mut t.summary, &self.summary)] {
            if !from.is_empty() {
                *to = from.clone();
            }
        }
    }
}

fn parse(text: &str) -> Option<Read> {
    let v = json_in(text)?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    Some(Read { turn: s("turn"), now: s("now"), about: s("about"), summary: s("summary") })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(role: &str, text: &str) -> Exchange {
        serde_json::from_value(serde_json::json!({ "role": role, "text": text, "at_ms": 1 })).unwrap()
    }

    #[test]
    fn an_answer_is_read_even_in_a_fence_and_empty_parts_leave_what_was_there() {
        let r = parse("```json\n{\"about\":\" Order router \",\"now\":\"Tests pass\",\"summary\":\"It did X.\"}\n```").unwrap();
        assert_eq!((r.about.as_str(), r.now.as_str(), r.summary.as_str(), r.turn.as_str()), ("Order router", "Tests pass", "It did X.", ""));
        assert!(parse("Sorry, no JSON").is_none());
        let mut t = Track { about: "Old".into(), now: "Was".into(), ..Default::default() };
        parse(r#"{"turn":"Released 0.1.74","now":"Waits on you","about":""}"#).unwrap().lay_on(&mut t);
        assert_eq!((t.about.as_str(), t.now.as_str()), ("Old", "Waits on you"));
    }

    #[test]
    fn a_file_from_when_it_kept_a_list_still_reads() {
        let t: Track = serde_json::from_str(r#"{"items":[{"id":"t1","group":"ok","text":"a","state":"open","at_ms":1}],"next":2,"seen_ms":5,"said":"1 added","err":"","spent":{"tokens":3,"usd":0.1},"updated_ms":6,"about":"A","turns":[{"at_ms":4,"line":"Did it"}]}"#).unwrap();
        assert_eq!((t.seen_ms, t.about.as_str(), t.turns.len()), (5, "A", 1));
        assert!(!serde_json::to_string(&t).unwrap().contains("items"));
    }

    #[test]
    fn the_small_model_reads_the_newest_messages() {
        let p = update_prompt("the order router", "It moved the router onto one session.", &[ex("you", "the reuse works"), ex("agent", "Good.")]);
        assert!(p.contains("session, about: the order router. In short so far: It moved the router onto one session."), "{p}");
        assert!(p.contains("Rewrite it with these messages in") && p.contains("\"turn\":"), "{p}");
        assert!(p.contains("only if the work has moved"), "with an about, it changes only when the work does: {p}");
        assert!(p.contains("Owner: the reuse works\n\nAgent: Good."), "{p}");
        let fresh = update_prompt("", "", &[ex("you", "go")]);
        assert!(fresh.contains("(\"about\").") && !fresh.contains("only if the work has moved"), "no about yet: always asked for: {fresh}");
        assert!(chat_prompt(&[]).contains("2 to 4 plain sentences") && !chat_prompt(&[]).contains("items"));
        let big = "x".repeat(MSG_CHARS);
        let many: Vec<_> = (0..30).map(|_| ex("agent", &big)).chain([ex("agent", "the newest")]).collect();
        let c = chat(&many);
        assert!(c.starts_with("(earlier messages left out)") && c.ends_with("Agent: the newest"));
    }

    #[test]
    fn only_a_session_id_names_a_file() {
        assert!(file("abc-123_x").is_some());
        assert!(file("../x").is_none() && file("").is_none() && file("a/b").is_none());
    }
}
