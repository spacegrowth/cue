//! What's left in a session: the list Cue keeps for each session you track (its Tasks tab).
//!
//! Turning it on has a small model read the session's chat (Cue's log of it: any agent, any machine) and list
//! what's left. After each of its turns the small model reads the list and only what was said since, and answers
//! with edits: done, dropped, added. Never a new list. The list is only read: to change what's left you tell the
//! session, and the next read takes it from the chat. Refresh reads the chat again with the list and its ids, so items keep who they are. What it all cost is added
//! up as it goes. (Asking a copy of the session instead, as btw does, re-reads its whole conversation at its own
//! model's price: $42 for one list of a 450k-token session whose cache had gone. The chat read by Haiku: a cent.)
//!
//! Kept in Cue's folder, tasks/<session id>.json. The agent's own tools are left alone: none of this is in its
//! context.

use crate::btw::Spent;
use crate::hub::Hub;
use crate::model::{now_ms, Exchange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The groups an item goes under, in the order the tab shows them: for you to check, to build or fix, for you to
/// decide, and a choice the agent made that waits on your OK.
pub const GROUPS: [&str; 4] = ["check", "build", "decide", "ok"];

/// The most of the chat a small model reads at once: the newest messages, each cut to MSG_CHARS.
const CHAT_CHARS: usize = 40_000;
const MSG_CHARS: usize = 4_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    /// "t1", "t2"…: stays the item's while it lives, so edits and Refresh name it.
    pub id: String,
    pub group: String,
    pub text: String,
    /// "open" | "done" | "dropped" (the chat called it off). Done and dropped ones show struck through.
    pub state: String,
    pub at_ms: u64,
    /// What it is, in a sentence or two for someone who didn't follow the chat (shown when you open the item).
    #[serde(default)]
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub items: Vec<Item>,
    /// The number the next item's id takes.
    pub next: u32,
    /// The newest message the list has read.
    pub seen_ms: u64,
    /// What the last update changed, in a few words.
    pub said: String,
    /// Why the last update failed; "" when it didn't.
    pub err: String,
    /// What keeping this list has cost so far.
    pub spent: Spent,
    pub updated_ms: u64,
    /// What it's doing now ("listing", "updating"); "" when idle. Not kept: a restart finds it idle.
    #[serde(skip_deserializing)]
    pub busy: String,
    /// What the session is about, one line: set by the first list, changed only when the work does.
    #[serde(default)]
    pub about: String,
    /// Where it stands now, a line or two, after its last turn.
    #[serde(default)]
    pub now: String,
    /// A line per turn the list read, oldest first, the newest TURNS_KEPT: what "since you looked" shows.
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

/// Tasks are on (Settings → Tasks): off, no list is made or asked for, and the window shows none of it.
pub fn on() -> bool {
    crate::config::effective().pointer("/tasks/on").and_then(Value::as_bool).unwrap_or(false)
}

static TRACKS: Mutex<Option<HashMap<String, Track>>> = Mutex::new(None);

/// Every call the lists have made, in every session, kept when a session stops being tracked (Settings shows it).
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
    let mut g = crate::locking::lock(&TOTAL, "what the lists have cost");
    let t = g.get_or_insert_with(|| std::fs::read_to_string(total_file()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default());
    t.tokens += s.tokens;
    t.usd += s.usd;
    t.calls += 1;
    if let Ok(text) = serde_json::to_string(&*t) {
        let _ = std::fs::write(total_file(), text);
    }
}

/// What every list has cost so far, and how many sessions are tracked now, for Settings.
pub fn spent() -> Value {
    let t = *crate::locking::lock(&TOTAL, "what the lists have cost")
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
    let mut g = crate::locking::lock(&TRACKS, "the lists of what's left");
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

/// Change a session's track (if it's tracked) and keep it; what `f` returns.
fn change<R>(session_id: &str, f: impl FnOnce(&mut Track) -> R) -> Option<R> {
    with(|all| {
        let t = all.get_mut(session_id)?;
        let r = f(t);
        save(session_id, t);
        Some(r)
    })
}

/// Every tracked session's list, for the window's state.
pub fn view() -> Value {
    with(|all| serde_json::to_value(&*all).unwrap_or(Value::Null))
}

pub fn is_tracked(session_id: &str) -> bool {
    with(|all| all.contains_key(session_id))
}

/// Track what's left in a session: its first list, asked for in the background.
pub fn start(hub: Arc<Hub>, session_id: &str) -> Result<(), String> {
    if !on() {
        return Err("Tasks are off: turn them on in Settings → Tasks".into());
    }
    if file(session_id).is_none() {
        return Err("not a session Cue can track".into());
    }
    hub.session_origin(session_id).ok_or("that session is gone")?;
    let fresh = with(|all| {
        if all.get(session_id).is_some_and(|t| !t.busy.is_empty()) {
            return false;
        }
        let t = all.entry(session_id.to_string()).or_insert_with(|| Track { next: 1, ..Default::default() });
        t.busy = "listing".into();
        t.err.clear();
        save(session_id, t);
        true
    });
    if !fresh {
        return Err("it's already working on the list".into());
    }
    hub.redraw();
    let sid = session_id.to_string();
    std::thread::spawn(move || {
        let r = list(&hub, &sid);
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

/// Ask for the whole list again (Refresh): the same as turning it on, with the list as it stands.
pub fn refresh(hub: Arc<Hub>, session_id: &str) -> Result<(), String> {
    if !is_tracked(session_id) {
        return Err("this session isn't tracked".into());
    }
    start(hub, session_id)
}

/// Stop tracking a session: its list goes.
pub fn stop(session_id: &str) {
    with(|all| all.remove(session_id));
    if let Some(p) = file(session_id) {
        let _ = std::fs::remove_file(p);
    }
}

/// A tracked session's turn ended: read what was said since the list last looked, in the background. While
/// it's busy, nothing: the next turn's end reads from where the list left off.
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

/// The first list, or Refresh's: the small model reads the chat (with the list so far, by id).
fn list(hub: &Hub, session_id: &str) -> Result<(), String> {
    let log = hub.chat_log(session_id);
    if log.is_empty() {
        return Err("nothing has been said in this session yet".into());
    }
    let now = with(|all| all.get(session_id).map(|t| t.items.clone())).ok_or("this session isn't tracked")?;
    let open: Vec<Item> = now.into_iter().filter(|i| i.state == "open").collect();
    let a = crate::btw::ask_small(&chat_prompt(&open, &log), session_id)?;
    spend(a.spent);
    let listed = parse_list(&a.text).ok_or_else(|| format!("its answer wasn't a list: {}", crate::logs::transcript::cut(&a.text, 200)))?;
    let seen = log.last().map(|e| e.at_ms).unwrap_or(0);
    change(session_id, |t| {
        if !listed.about.is_empty() {
            t.about = listed.about.clone();
        }
        if !listed.now.is_empty() {
            t.now = listed.now.clone();
        }
        let (added, done, dropped) = reconcile(t, listed.items, now_ms());
        t.spent.tokens += a.spent.tokens;
        t.spent.usd += a.spent.usd;
        t.seen_ms = t.seen_ms.max(seen);
        t.said = said_words(added, done, dropped);
    });
    Ok(())
}

/// After a turn: the list and what was said since it last looked, to the small model; its edits applied.
fn update(hub: &Hub, session_id: &str) -> Result<(), String> {
    let (items, seen) = with(|all| all.get(session_id).map(|t| (t.items.clone(), t.seen_ms))).ok_or("this session isn't tracked")?;
    let since: Vec<Exchange> = hub.chat_log(session_id).into_iter().filter(|e| e.at_ms > seen).collect();
    if !since.iter().any(|e| e.role == "agent") {
        return Ok(());
    }
    let open: Vec<Item> = items.into_iter().filter(|i| i.state == "open").collect();
    let about = with(|all| all.get(session_id).map(|t| t.about.clone())).unwrap_or_default();
    let a = crate::btw::ask_small(&update_prompt(&about, &open, &since), session_id)?;
    spend(a.spent);
    let edits = parse_edits(&a.text).ok_or_else(|| format!("its answer wasn't a change to the list: {}", crate::logs::transcript::cut(&a.text, 200)))?;
    let newest = since.last().map(|e| e.at_ms).unwrap_or(seen);
    // The turn's line goes at its reply's time, where "since you looked" counts from.
    let reply_at = since.iter().rev().find(|e| e.role == "agent").map(|e| e.at_ms).unwrap_or(newest);
    change(session_id, |t| {
        let (added, done, dropped) = apply(t, &edits, now_ms());
        t.spent.tokens += a.spent.tokens;
        t.spent.usd += a.spent.usd;
        t.seen_ms = newest;
        t.updated_ms = now_ms();
        t.said = if edits.said.trim().is_empty() { said_words(added, done, dropped) } else { edits.said.trim().to_string() };
        if !edits.about.is_empty() {
            t.about = edits.about.clone();
        }
        if !edits.now.is_empty() {
            t.now = edits.now.clone();
        }
        if !edits.said.trim().is_empty() {
            t.turns.push(Turn { at_ms: reply_at, line: edits.said.trim().to_string() });
            let over = t.turns.len().saturating_sub(TURNS_KEPT);
            t.turns.drain(..over);
        }
    });
    Ok(())
}

fn said_words(added: usize, done: usize, dropped: usize) -> String {
    let mut w = vec![];
    if done > 0 {
        w.push(format!("{done} done"));
    }
    if added > 0 {
        w.push(format!("{added} added"));
    }
    if dropped > 0 {
        w.push(format!("{dropped} dropped"));
    }
    if w.is_empty() { "nothing changed".into() } else { w.join(" · ") }
}

/// An item as the models read it: "t3 [build] A counter for reused sessions".
fn lines(items: &[Item]) -> String {
    if items.is_empty() {
        return "(nothing yet)".into();
    }
    items.iter().map(|i| format!("{} [{}] {}", i.id, i.group, i.text)).collect::<Vec<_>>().join("\n")
}

const GROUP_WORDS: &str = "\"check\": something the owner still has to check or try; \"build\": still to build or fix; \
\"decide\": the owner has to decide; \"ok\": the agent asked the owner a yes-or-no question and is waiting for the answer";

/// What isn't something left. Agents' reports are long and full of asides; read as a list, every one became an item
/// ("OK to leave the other edit alone", one item per part of a single question).
const LEAVE_OUT: &str = "Only list what really holds the work up or is still to do: a question the agent asked the owner and waits on, \
work the agent said is still to do, something the owner asked for that isn't done. Leave out the agent's notes, caveats and \
assumptions; choices it made and went ahead with; what it will do by itself or does only if something happens (\"if you're \
away…\"); what the owner already answered; and how-it-works details. One question with several parts is one item, not one \
per part, and never two items for the same thing in other words. Write each item as an action, never a question \
(\"Push the branch\", \"Decide whether to keep the list\"): it may be sent to the agent as \"Please finish this: …\". Few items are better than many: most sessions have 0 to 5.";

const LIST_JSON: &str = r#"{"about":"what the whole session is about, under 10 words","now":"where it stands and what it waits on, under 25 words","items":[{"id":"t3 or null","group":"check|build|decide|ok","text":"one short line","detail":"one or two plain sentences","done":false,"dropped":false}]}"#;

/// Each item's "detail": the line alone reads as shorthand to someone who wasn't following.
const DETAIL_WORDS: &str = "For each item, \"detail\": one or two plain sentences for someone who didn't follow the chat: \
what this is about and why it's left (what it waits on, or what happens once it's done). No file names or code unless needed.";

/// What the small model is asked for a whole list: from the chat, keeping the ids of the list so far.
fn chat_prompt(open: &[Item], log: &[Exchange]) -> String {
    let have = if open.is_empty() { String::new() } else { format!("The list so far, for its ids (keep the id of one that's still left; \"done\": true for one that's done now; \"id\": null for a new one; \
one you leave out goes off the list):\n{}\n\n", lines(open)) };
    format!(
        "A coding agent's session with its owner, oldest first:\n\n{}\n\n{have}\
         List what is still left to do: what the owner still has to check, what is still to build or fix, what the owner has to \
         decide, and choices or questions of the agent's that wait on the owner's OK. Leave out what's done. One short line each, \
         in plain words. {LEAVE_OUT} Groups: {GROUP_WORDS}. {DETAIL_WORDS}\n\nAnswer with only this JSON: {LIST_JSON}",
        chat(log)
    )
}

fn update_prompt(about: &str, open: &[Item], since: &[Exchange]) -> String {
    format!(
        "A coding agent's session{}. What's left in it, so far:\n{}\n\nWhat was said since, oldest first:\n\n{}\n\n\
         Update the list from these messages. Mark an item done only when the agent says it's done or the owner confirms it. \
         Drop an item the owner called off or that no longer applies. Add what the owner asked for, what the agent left for later, \
         and questions or choices of the agent's that wait on the owner; not what was finished in these messages, and not what's \
         already on the list. {LEAVE_OUT} Groups: {GROUP_WORDS}. {DETAIL_WORDS} Also say, in plain words for the owner: what the agent did in these messages, \
         under 12 words (\"turn\"); where the session stands and what it waits on, under 25 words (\"now\"); and what the whole \
         session is about, under 10 words (\"about\"){}.\n\n\
         \"left\": the ids of the items above that are still left now; one you leave out goes off the list (it was answered, \
         is done, was asked again in other words, or no longer applies).\n\n\
         Answer with only this JSON: {{\"done\":[\"t1\"],\"left\":[\"t2\"],\"add\":[{{\"group\":\"check|build|decide|ok\",\"text\":\"one short line\",\"detail\":\"one or two plain sentences\"}}],\"turn\":\"under 12 words\",\"now\":\"under 25 words\",\"about\":\"\"}}",
        if about.is_empty() { String::new() } else { format!(", about: {about}") },
        lines(open),
        chat(since),
        // A list made before it had one (or whose first answer left it out) gets one now; else only when the work moved.
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

#[derive(Debug, PartialEq)]
struct Listed {
    id: Option<String>,
    group: String,
    text: String,
    detail: String,
    done: bool,
    /// Refresh: on the list, but not something left (by the rules it's read with now).
    dropped: bool,
}

fn group_of(v: &Value) -> String {
    let g = v.get("group").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    if GROUPS.contains(&g.as_str()) { g } else { "build".into() }
}

/// A whole list as answered, with what the session is about and where it stands.
#[derive(Debug, PartialEq)]
struct ListAnswer {
    items: Vec<Listed>,
    about: String,
    now: String,
}

fn text_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

fn parse_list(text: &str) -> Option<ListAnswer> {
    let v = json_in(text)?;
    let items = v.get("items")?
            .as_array()?
            .iter()
            .filter_map(|x| {
                let text = x.get("text")?.as_str()?.trim().to_string();
                (!text.is_empty()).then(|| Listed {
                    id: x.get("id").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && *s != "null").map(str::to_string),
                    group: group_of(x),
                    text,
                    detail: text_of(x, "detail"),
                    done: x.get("done").and_then(Value::as_bool).unwrap_or(false),
                    dropped: x.get("dropped").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect();
    Some(ListAnswer { items, about: text_of(&v, "about"), now: text_of(&v, "now") })
}

#[derive(Debug, Default, PartialEq)]
struct Edits {
    done: Vec<String>,
    dropped: Vec<String>,
    /// (group, text, detail)
    add: Vec<(String, String, String)>,
    /// The open items still left; one not in it goes off the list. None: an answer without it (each id stays).
    left: Option<Vec<String>>,
    /// What this turn did, in a line.
    said: String,
    now: String,
    about: String,
}

fn parse_edits(text: &str) -> Option<Edits> {
    let v = json_in(text)?;
    let ids = |k: &str| v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    Some(Edits {
        done: ids("done"),
        dropped: ids("dropped"),
        left: v.get("left").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()),
        add: v
            .get("add")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| Some((group_of(x), x.get("text")?.as_str()?.trim().to_string(), text_of(x, "detail")))).filter(|(_, t, _)| !t.is_empty()).collect())
            .unwrap_or_default(),
        said: Some(text_of(&v, "turn")).filter(|s| !s.is_empty()).unwrap_or_else(|| text_of(&v, "said")),
        now: text_of(&v, "now"),
        about: text_of(&v, "about"),
    })
}

/// Two lines that say the same thing: the same words, case and punctuation aside.
fn same(a: &str, b: &str) -> bool {
    let k = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    k(a) == k(b)
}

fn add(t: &mut Track, group: &str, text: &str, detail: &str, at_ms: u64) -> bool {
    if t.items.iter().any(|i| same(&i.text, text)) {
        return false;
    }
    let id = format!("t{}", t.next.max(1));
    t.next = t.next.max(1) + 1;
    t.items.push(Item { id, group: group.into(), text: text.into(), state: "open".into(), at_ms, detail: detail.into() });
    true
}

/// A whole list (first, or Refresh) laid over the track: a known open item takes its new words and group, or is
/// done, or dropped as not something left; one with no id (or one Cue doesn't know) is added unless the list has
/// it. One that's over stays over, and an open one the answer leaves out is dropped: the answer is the whole list.
/// (added, done, dropped)
fn reconcile(t: &mut Track, listed: Vec<Listed>, at_ms: u64) -> (usize, usize, usize) {
    let (mut added, mut done, mut dropped) = (0, 0, 0);
    let named: Vec<String> = listed.iter().filter(|l| !l.done && !l.dropped).filter_map(|l| l.id.clone()).collect();
    let before: Vec<String> = t.items.iter().filter(|i| i.state == "open").map(|i| i.id.clone()).collect();
    for l in listed {
        match l.id.as_deref().and_then(|id| t.items.iter_mut().find(|i| i.id == id)) {
            Some(i) if i.state != "open" => {}
            Some(i) => {
                i.text = l.text;
                i.group = l.group;
                if !l.detail.is_empty() {
                    i.detail = l.detail;
                }
                if l.done {
                    i.state = "done".into();
                    done += 1;
                } else if l.dropped {
                    i.state = "dropped".into();
                    dropped += 1;
                }
            }
            None if !l.done && !l.dropped => added += add(t, &l.group, &l.text, &l.detail, at_ms) as usize,
            None => {}
        }
    }
    for i in t.items.iter_mut().filter(|i| i.state == "open" && before.contains(&i.id) && !named.contains(&i.id)) {
        i.state = "dropped".into();
        dropped += 1;
    }
    (added, done, dropped)
}

/// A turn's edits: only open items change. (added, done, dropped)
fn apply(t: &mut Track, e: &Edits, at_ms: u64) -> (usize, usize, usize) {
    let mut mark = |ids: &[String], state: &str| {
        let mut n = 0;
        for i in t.items.iter_mut().filter(|i| ids.contains(&i.id) && i.state == "open") {
            i.state = state.into();
            n += 1;
        }
        n
    };
    let done = mark(&e.done, "done");
    let mut dropped = mark(&e.dropped, "dropped");
    // The list as it stands now: what it no longer names is gone (answered, said again in other words, moot).
    if let Some(left) = &e.left {
        for i in t.items.iter_mut().filter(|i| i.state == "open" && !left.contains(&i.id)) {
            i.state = "dropped".into();
            dropped += 1;
        }
    }
    let added = e.add.iter().filter(|(g, text, detail)| add(t, g, text, detail, at_ms)).count();
    (added, done, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(items: &[(&str, &str, &str)]) -> Track {
        let mut t = Track { next: 1, ..Default::default() };
        for (g, text, state) in items {
            add(&mut t, g, text, "", 1);
            t.items.last_mut().unwrap().state = state.to_string();
        }
        t
    }

    #[test]
    fn a_first_list_is_read_from_the_answer_even_in_a_fence() {
        let a = parse_list("```json\n{\"about\":\" Order router \",\"now\":\"Tests pass\",\"items\":[{\"id\":null,\"group\":\"Check\",\"text\":\"Run the replay\",\"done\":false},{\"group\":\"odd\",\"text\":\"Build it\"},{\"text\":\"  \"}]}\n```").unwrap();
        assert_eq!((a.about.as_str(), a.now.as_str()), ("Order router", "Tests pass"));
        let l = a.items;
        assert_eq!(l.len(), 2, "an empty line is left out");
        assert_eq!((l[0].id.clone(), l[0].group.as_str()), (None, "check"));
        assert_eq!(l[1].group, "build", "a group it made up goes under build");
        assert!(parse_list("Sorry, no JSON").is_none());
    }

    #[test]
    fn refresh_keeps_ids_marks_done_adds_new_and_leaves_what_is_over() {
        let mut t = track(&[("build", "A counter", "open"), ("check", "Run the replay", "open"), ("build", "Drop the old pool", "dropped")]);
        let listed = parse_list(r#"{"items":[{"id":"t1","group":"build","text":"A counter for reused sessions","done":true},{"id":"t3","group":"build","text":"Drop the old pool","done":false},{"id":null,"group":"decide","text":"When to commit"},{"id":null,"group":"check","text":"run the replay."}]}"#).unwrap().items;
        let (added, done, dropped) = reconcile(&mut t, listed, 2);
        assert_eq!((added, done, dropped), (1, 1, 1));
        assert_eq!((t.items[0].state.as_str(), t.items[0].text.as_str()), ("done", "A counter for reused sessions"));
        assert_eq!(t.items[1].state, "dropped", "left out of the answer: gone (the answer is the whole list)");
        assert_eq!(t.items[2].state, "dropped", "over stays over");
        assert_eq!(t.items.len(), 4, "the same line again isn't added");
        assert_eq!(t.items[3].id, "t4");
    }

    #[test]
    fn refresh_drops_what_isnt_left_and_adds_no_dropped_line() {
        let mut t = track(&[("ok", "OK to leave the other edit alone", "open"), ("check", "Run the replay", "open")]);
        let listed = parse_list(r#"{"items":[{"id":"t1","group":"ok","text":"OK to leave the other edit alone","dropped":true},{"id":"t2","group":"check","text":"Run the replay"},{"id":null,"group":"ok","text":"An aside","dropped":true}]}"#).unwrap().items;
        assert_eq!(reconcile(&mut t, listed, 2), (0, 0, 1));
        assert_eq!((t.items[0].state.as_str(), t.items[1].state.as_str()), ("dropped", "open"));
        assert_eq!(t.items.len(), 2, "a new one it calls dropped isn't added");
        let ex = |role: &str, text: &str| -> Exchange { serde_json::from_value(serde_json::json!({ "role": role, "text": text, "at_ms": 1 })).unwrap() };
        let p = chat_prompt(&t.items, &[ex("agent", "Done. Note: I left the other edit alone.")]);
        assert!(p.contains("Leave out the agent's notes") && p.contains("one you leave out goes off the list"), "{p}");
        assert!(update_prompt("x", &t.items, &[ex("agent", "ok")]).contains("Leave out the agent's notes"));
    }

    #[test]
    fn each_item_keeps_its_detail_from_a_list_or_a_turn() {
        let mut t = track(&[("check", "Try it", "open")]);
        let listed = parse_list(r#"{"items":[{"id":"t1","group":"check","text":"Try it","detail":"Open the tab and tick two."},{"id":null,"group":"decide","text":"Push?","detail":"Whether to share it now."}]}"#).unwrap().items;
        reconcile(&mut t, listed, 2);
        assert_eq!((t.items[0].detail.as_str(), t.items[1].detail.as_str()), ("Open the tab and tick two.", "Whether to share it now."));
        let e = parse_edits(r#"{"left":["t1","t2"],"add":[{"group":"build","text":"X","detail":"Why X."}]}"#).unwrap();
        apply(&mut t, &e, 3);
        assert_eq!(t.items[2].detail, "Why X.");
        assert!(chat_prompt(&t.items, &[]).contains("\"detail\": one or two plain sentences"));
        assert!(chat_prompt(&t.items, &[]).contains("never a question"), "items are sent as \"Please finish this\", so no \"Push it?\"");
        let old: Item = serde_json::from_str(r#"{"id":"t1","group":"ok","text":"a","state":"open","at_ms":1}"#).unwrap();
        assert_eq!(old.detail, "", "a list saved before details reads");
    }

    #[test]
    fn a_turn_with_left_drops_what_it_no_longer_names_but_not_what_it_adds() {
        let mut t = track(&[("ok", "OK the label?", "open"), ("ok", "OK the Ask label?", "open"), ("check", "Try it", "open"), ("build", "X", "done")]);
        let e = parse_edits(r#"{"done":["t3"],"left":["t2"],"add":[{"group":"decide","text":"Push it?"}],"turn":"t"}"#).unwrap();
        assert_eq!(apply(&mut t, &e, 5), (1, 1, 1));
        let st: Vec<_> = t.items.iter().map(|i| i.state.as_str()).collect();
        assert_eq!(st, ["dropped", "open", "done", "done", "open"], "the repeat goes, the one named stays, the new one is open");
    }

    #[test]
    fn a_turns_edits_change_only_open_items() {
        let mut t = track(&[("build", "A", "open"), ("build", "B", "open"), ("check", "C", "done")]);
        let e = parse_edits(r#"{"done":["t1","t3","t9"],"dropped":["t3"],"add":[{"group":"ok","text":"Retries stay at 3"},{"group":"build","text":"a"}],"said":"Reuse confirmed"}"#).unwrap();
        assert_eq!(apply(&mut t, &e, 5), (1, 1, 0));
        assert_eq!(t.items[0].state, "done");
        assert_eq!((t.items[1].state.as_str(), t.items[2].state.as_str()), ("open", "done"), "untouched, and over stays over");
        assert_eq!(t.items[3].group, "ok");
        assert_eq!(e.said, "Reuse confirmed", "the old \"said\" still reads");
        let e = parse_edits(r#"{"done":[],"turn":"Released 0.1.74","now":"Waits on you","about":""}"#).unwrap();
        assert_eq!((e.said.as_str(), e.now.as_str(), e.about.as_str()), ("Released 0.1.74", "Waits on you", ""));
    }

    #[test]
    fn the_small_model_reads_the_newest_messages_and_the_list_by_id() {
        let ex = |role: &str, text: &str| -> Exchange { serde_json::from_value(serde_json::json!({ "role": role, "text": text, "at_ms": 1 })).unwrap() };
        let t = track(&[("build", "A counter", "open")]);
        let p = update_prompt("the order router", &t.items, &[ex("you", "the reuse works"), ex("agent", "Good.")]);
        assert!(p.contains("session, about: the order router."), "{p}");
        assert!(p.contains("only if the work has moved"), "with an about, it changes only when the work does: {p}");
        let fresh = update_prompt("", &t.items, &[ex("you", "go")]);
        assert!(fresh.contains("(\"about\").") && !fresh.contains("only if the work has moved"), "no about yet: always asked for: {fresh}");
        assert!(p.contains("t1 [build] A counter") && p.contains("Owner: the reuse works\n\nAgent: Good."), "{p}");
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
