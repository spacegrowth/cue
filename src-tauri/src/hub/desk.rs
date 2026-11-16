//! The Desk in the hub (desk/ has its records and words): each project's items and decisions in the store, what
//! its lead and the holders say at their turns' ends, what the window does to an item, and `state.desk`. The lead
//! itself (picking it, starting it, what it's told before each message) is desk_lead.rs.
//! A change is made under the store's lock and leaves `Effects`: its saves and messages, done once it's let go.

use super::*;
use crate::desk::parse::{self, Line};
use crate::desk::{DeskDecision, DeskItem, Holder};
use crate::sessions::Session;
use super::desk_members::{ask_first, join, may_route, UseAsk};

/// What a change to the Desk leaves to do with the store let go.
#[derive(Default)]
pub(super) struct Effects {
    /// Items to save as they are now, and items dropped.
    pub(super) items: Vec<String>,
    pub(super) dropped: Vec<String>,
    pub(super) decisions: Vec<String>,
    /// Messages to sessions: (session, text).
    pub(super) sends: Vec<(String, String)>,
    /// A lead's questions for you: (its session, its project, question, choices).
    pub(super) asks: Vec<(String, String, String, Vec<String>)>,
    /// New items for their project's lead to route: (project, item), told together a moment later.
    pub(super) news: Vec<(String, String)>,
    /// Items a lead gave to a new session (`-> new`): each started a moment later (desk_lead.rs).
    pub(super) starts: Vec<NewStart>,
    /// Sessions that joined a Desk: (project, session), to save.
    pub(super) joined: Vec<(String, String)>,
    /// Routes to sessions not on the Desk: you're asked first, in the lead's name (`desk_ask`).
    pub(super) uses: Vec<UseAsk>,
    /// Items just said DONE: the files their check card shows, read a moment later (desk_checks.rs).
    pub(super) checked: Vec<String>,
}


/// Item `id` of `project` to a new session, as the lead (`by`, `by_sid`) asked, with its why.
pub(super) struct NewStart {
    pub(super) project: String,
    pub(super) id: String,
    pub(super) why: String,
    pub(super) by: String,
    pub(super) by_sid: String,
}

/// How long new items wait for others before their lead is told of them, in one message.
const NEWS_WAIT_MS: u64 = 1000;

pub(super) fn secs(ms: u64) -> u64 {
    ms / 1000
}

/// What a session goes by on the Desk: the name you gave it, Claude's, else its folder's.
pub(super) fn shown_name(s: &Session) -> String {
    if !s.name.is_empty() {
        return s.name.clone();
    }
    match s.claude_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => crate::model::folder_of(&s.origin.cwd),
    }
}

/// A crew holder's (lead session, agent id).
pub(super) fn crew_parts(h: &Holder) -> Option<(&str, &str)> {
    (h.kind == "crew" && !h.lead_sid.is_empty()).then_some((h.lead_sid.as_str(), h.sid.as_str()))
}

/// The sessions `project`'s items may go to: the project's own, live, oldest first.
pub(super) fn routable<'a>(st: &'a Store, project: &str) -> Vec<&'a Session> {
    let mut ours: Vec<&Session> = st.sessions.iter().filter(|s| s.project == project).collect();
    ours.sort_by_key(|s| s.since_ms);
    ours
}

/// Live session `sid` as a holder: a lead when it has a crew.
fn holder_of(st: &Store, sid: &str) -> Option<Holder> {
    st.sessions.get(sid).map(|s| Holder { kind: if s.team.is_empty() { "session".into() } else { "lead".into() }, sid: sid.to_string(), name: shown_name(s), lead_sid: String::new() })
}

/// The session of `project` a ROUTE names (`to`): by name (yours or Claude's), by "sid:<start of its id>" or the
/// start of its id (six characters or more), by folder, else a crew member by the name its lead gave it.
fn resolve(st: &Store, project: &str, to: &str) -> Option<Holder> {
    let want = to.trim().trim_start_matches('@').to_lowercase();
    let id = want.strip_prefix("sid:").unwrap_or(&want).trim().to_string();
    let all = routable(st, project);
    let session = |s: &Session| holder_of(st, &s.origin.session_id).unwrap_or_default();
    if let Some(s) = all.iter().find(|s| s.name.to_lowercase() == want || s.claude_name.as_deref().is_some_and(|n| n.to_lowercase() == want)) {
        return Some(session(s));
    }
    if id.len() >= 6 {
        if let Some(s) = all.iter().find(|s| s.origin.session_id.to_lowercase().starts_with(&id)) {
            return Some(session(s));
        }
    }
    if let Some(s) = all.iter().find(|s| crate::model::folder_of(&s.origin.cwd).to_lowercase() == want) {
        return Some(session(s));
    }
    all.iter().find_map(|s| {
        let t = s.team.iter().find(|t| t.name.to_lowercase() == want)?;
        let agent = if t.agent_id.is_empty() { &t.call } else { &t.agent_id };
        Some(Holder { kind: "crew".into(), sid: agent.clone(), name: t.name.clone(), lead_sid: s.origin.session_id.clone() })
    })
}

/// Holder `h` at work: its session mid-turn (or asking, or driven by its agents), or for a crew member, that teammate.
fn at_work(st: &Store, h: &Holder) -> bool {
    match crew_parts(h) {
        Some((lead, agent)) => st.sessions.get(lead).is_some_and(|s| s.team.iter().any(|t| (t.agent_id == agent || t.call == agent) && t.working())),
        None => st.sessions.get(&h.sid).is_some_and(|s| matches!(s.state.as_str(), "working" | "deciding" | "agent")),
    }
}

/// An open ask (not a "your turn" card) of the holder's: for a crew member, its own.
fn ask_of<'a>(st: &'a Store, h: &Holder) -> Option<&'a Item> {
    let agent = crew_parts(h).map(|(_, a)| a);
    st.items.iter().find(|i| {
        i.status == "pending" && i.kind != "waiting" && i.origin.session_id == h.to() && agent.is_none_or(|a| i.agent_id == a)
    })
}

/// An ask as an item shows it: { item_id, sid, text, choices }.
fn ask_view(i: &Item) -> Value {
    let q = i.tool_input.pointer("/questions/0");
    let text = q.and_then(|q| q["question"].as_str()).map(str::to_string).unwrap_or_else(|| super::summary(i));
    let choices: Vec<String> = q.and_then(|q| q["options"].as_array()).map(|o| o.iter().filter_map(|x| x["label"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    json!({ "item_id": i.id, "sid": i.origin.session_id, "text": text, "choices": choices })
}

/// The first line of a session's last words.
fn last_words(s: &Session) -> String {
    s.thread.iter().rev().find(|e| e.role == "agent").and_then(|e| e.text.lines().map(str::trim).find(|l| !l.is_empty())).unwrap_or("").chars().take(160).collect()
}

/// `it` as the window shows it: its holder as it is now (name, lead or not), waiting on you when its holder is,
/// with its holder's latest and files.
pub(super) fn shown(st: &Store, it: &DeskItem) -> DeskItem {
    let mut out = it.clone();
    let Some(h) = it.holder.as_ref() else { return out };
    let mut h = h.clone();
    if let Some((lead, agent)) = crew_parts(&h) {
        if let Some(t) = st.sessions.get(lead).and_then(|s| s.team.iter().find(|t| t.agent_id == agent || t.call == agent)) {
            h.name = t.name.clone();
            out.latest = t.step.clone();
            out.files = inside(t.files.iter());
        }
    } else if let Some(s) = st.sessions.get(&h.sid) {
        h.name = shown_name(s);
        h.kind = if s.team.is_empty() { "session".into() } else { "lead".into() };
        out.latest = if s.doing.is_empty() { last_words(s) } else { s.doing.clone() };
        // Given to a teammate of its (ROUTE: … -> me | crew: <name>): that teammate's own latest and files, once it's
        // there (none till then: no guess by time).
        if it.crew.is_empty() {
            out.files = session_files(st, s, it);
        } else if let Some(t) = crew_named(s, &it.crew) {
            out.crew = t.name.clone();
            if !t.step.is_empty() {
                out.latest = t.step.clone();
            }
            out.files = inside(t.files.iter());
        } else {
            out.files = Vec::new();
        }
    }
    // Only the item at work: a queued one's holder asks about the item before it.
    if it.state == "working" {
        if let Some(a) = ask_of(st, &h) {
            out.state = "needs_you".into();
            out.ask = Some(ask_view(a));
        }
    }
    out.holder = Some(h);
    out
}

/// Lead `s`'s teammate named `name` (any case): the latest started, when it named more than one so.
fn crew_named<'a>(s: &'a Session, name: &str) -> Option<&'a crate::sessions::team::Teammate> {
    s.team.iter().filter(|t| t.name.eq_ignore_ascii_case(name.trim())).max_by_key(|t| t.started_ms)
}

/// Of `files`, those inside the folder they were changed in (kept relative to it; one outside is absolute), each once.
fn inside<'a>(files: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files.filter(|f| !f.is_empty() && !f.starts_with('/') && !f.starts_with('~') && !std::path::Path::new(f.as_str()).components().any(|c| c == std::path::Component::ParentDir)) {
        if !out.contains(f) {
            out.push(f.clone());
        }
    }
    out
}

/// The files changed for `it` by its holder `s` (a plain session or a lead): its crew's own files, of members started
/// after the item went to it and before its next item did, that hold no item of their own. Its own turns' changes
/// aren't counted: git sees the whole folder, other sessions' edits too. None when another of its items was at work
/// then as well (whose files they are can't be told), or the item has no start for this holder.
fn session_files(st: &Store, s: &Session, it: &DeskItem) -> Vec<String> {
    inside(session_mates(st, s, it).into_iter().flat_map(|t| t.files.iter()))
}

/// The crew members of holder `s` whose files are `it`'s (`session_files`).
fn session_mates<'a>(st: &Store, s: &'a Session, it: &DeskItem) -> Vec<&'a crate::sessions::team::Teammate> {
    let sid = &s.origin.session_id;
    let Some((from, until)) = held_window(st, sid, it) else { return Vec::new() };
    let holds = |t: &crate::sessions::team::Teammate| {
        st.desk.items.iter().any(|o| {
            // One named for an item not done (an old item's name may be given again).
            let named = !o.crew.is_empty() && o.state != "done" && o.crew.eq_ignore_ascii_case(&t.name);
            o.holder.as_ref().is_some_and(|h| crew_parts(h).is_some_and(|(_, a)| a == t.agent_id || a == t.call) || (h.sid == *sid && named))
        })
    };
    s.team.iter().filter(|t| t.started_ms >= from && t.started_ms < until && !holds(t)).collect()
}

/// When `it` was session `sid`'s to work on (ms): from when it went to it until its next item did (u64::MAX while
/// none has). None when another of its items was at work then as well (whose work is whose can't be told), or the
/// item has no start for `sid`.
pub(super) fn held_window(st: &Store, sid: &str, it: &DeskItem) -> Option<(u64, u64)> {
    let (_, from) = it.held_from.as_ref().filter(|(by, _)| by == sid)?;
    let mut until = u64::MAX;
    for o in st.desk.items.iter().filter(|o| o.id != it.id) {
        let Some((by, at)) = &o.held_from else { continue };
        if by != sid {
            continue;
        }
        if at > from {
            until = until.min(*at);
        } else if o.state == "working" && o.holder.as_ref().is_some_and(|h| h.sid == sid) {
            return None;
        }
    }
    Some((*from, until))
}

/// Where `it`'s files (as `shown` gives them) were changed: the machine, and each folder with its files in it (paths
/// relative to that folder), for Looks right's commit (desk_commit.rs).
pub(super) fn item_sources(st: &Store, it: &DeskItem) -> (String, Vec<(String, Vec<String>)>) {
    let Some(h) = it.holder.as_ref() else { return (String::new(), Vec::new()) };
    let lead = crew_parts(h).map_or(h.sid.as_str(), |(l, _)| l);
    let Some(s) = st.sessions.get(lead) else { return (String::new(), Vec::new()) };
    let mates: Vec<&crate::sessions::team::Teammate> = if let Some((_, agent)) = crew_parts(h) {
        s.team.iter().filter(|t| t.agent_id == agent || t.call == agent).take(1).collect()
    } else if it.crew.is_empty() {
        session_mates(st, s, it)
    } else {
        crew_named(s, &it.crew).into_iter().collect()
    };
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for t in mates {
        let folder = t.folder(&s.origin.cwd).to_string();
        let files = inside(t.files.iter());
        match out.iter_mut().find(|(f, _)| *f == folder) {
            Some((_, have)) => {
                for f in files {
                    if !have.contains(&f) {
                        have.push(f);
                    }
                }
            }
            None if !files.is_empty() => out.push((folder, files)),
            None => {}
        }
    }
    (s.origin.machine.clone(), out)
}

/// Items queued after `id` start now: each to its holder, with no reason (why it was queued isn't why it starts).
fn release(st: &mut Store, id: &str, now: u64, fx: &mut Effects) {
    for it in st.desk.items.iter_mut().filter(|i| i.after.as_deref() == Some(id) && i.state == "queued") {
        it.after = None;
        it.state = "working".into();
        it.updated = secs(now);
        if let Some(h) = &it.holder {
            fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "")));
        }
        fx.items.push(it.id.clone());
    }
}

/// Whether item `from` is `id`, or waits on it: directly, or through the items it waits on.
fn waits_on(st: &Store, from: &str, id: &str) -> bool {
    let mut at = Some(from.to_string());
    // Bounded by the item count: a chain longer than that has a loop of its own.
    for _ in 0..=st.desk.items.len() {
        let Some(a) = at else { return false };
        if a == id {
            return true;
        }
        at = st.desk.item(&a).and_then(|i| i.after.clone());
    }
    false
}

/// A decision noted on `project`'s Desk: one that can be undone.
#[allow(clippy::too_many_arguments)]
pub(super) fn decide(st: &mut Store, project: &str, item: Option<String>, by: &str, sid: &str, text: &str, why: &str, now: u64, fx: &mut Effects) {
    let id = st.desk.decision_id(now);
    st.desk.decisions.push(DeskDecision { id: id.clone(), project: project.into(), item_id: item, by: by.into(), sid: sid.into(), text: text.into(), why: why.into(), at: secs(now), undo: "can".into(), call: false });
    fx.decisions.push(id);
}

/// A judgment call (a DECIDED line) noted on `project`'s Desk: `decide`, shown in its Decisions list.
#[allow(clippy::too_many_arguments)]
fn decide_call(st: &mut Store, project: &str, item: Option<String>, by: &str, sid: &str, text: &str, why: &str, now: u64, fx: &mut Effects) {
    decide(st, project, item, by, sid, text, why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.call = true;
    }
}

/// Why item `id` can't wait on item `a`, as the lead is told ("#a, which …"); None when it can. Not after an item
/// that's unknown or over, nor one that (through those it waits on) waits on this one: both would wait forever.
/// The item then starts now.
fn after_refused(st: &Store, a: &str, id: &str) -> Option<String> {
    if a == id {
        return Some(format!("#{a}, itself"));
    }
    if waits_on(st, a, id) {
        return Some(format!("#{a}, which waits on it"));
    }
    match st.desk.item(a) {
        None => Some(format!("#{a}, which isn't an item")),
        Some(w) if matches!(w.state.as_str(), "to_check" | "done") => Some(format!("#{a}, which is already done")),
        Some(_) => None,
    }
}

/// Who the lead is (its name and session), for what it decides.
pub(super) struct By<'a> {
    pub(super) name: &'a str,
    pub(super) sid: &'a str,
}

/// Item `id` to the session named `to` ("new": one Cue starts for it), now or after another item, as the lead asked.
#[allow(clippy::too_many_arguments)]
pub(super) fn route(st: &mut Store, project: &str, by: &By, id: &str, to: &str, after: Option<String>, why: &str, now: u64, fx: &mut Effects) {
    let fresh = to.eq_ignore_ascii_case("new") || to.eq_ignore_ascii_case("me");
    let holder = if fresh { None } else { resolve(st, project, to) };
    if fresh || holder.as_ref().is_some_and(|h| h.sid == by.sid && h.lead_sid.is_empty()) {
        return start_fresh(st, project, by, id, why, fx);
    }
    // A session not on the Desk: you're asked first, and the item stays.
    if let Some(h) = holder.as_ref().filter(|h| !may_route(st, project, h, id)) {
        return ask_first(st, &UseAsk { lead: by.sid.into(), project: project.into(), sid: h.to().into(), name: h.name.clone(), item: id.into(), to: to.into(), after, why: why.into() }, fx);
    }
    let refused = after.as_deref().and_then(|a| after_refused(st, a, id)).unwrap_or_default();
    let waits = after.filter(|_| refused.is_empty());
    // Its holder on it now (mid-turn, or asking you): a route to it again is a turn repeating itself. Not on it (its
    // turn cut off by a crash or a quit, the item still "working"): the route sends it again.
    let busy = holder.as_ref().is_some_and(|h| at_work(st, h) || ask_of(st, h).is_some());
    // Done: not the lead's to route again. With you to check: routing it reopens it (back to work, sent again).
    let Some(it) = st.desk.item_mut(id).filter(|i| i.project == project && i.state != "done") else { return };
    let reopen = it.state == "to_check";
    let Some(h) = holder else { return no_such_session(it, to, now, fx) };
    let before = it.holder.clone();
    let same = before.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid);
    // The same route said again (a later turn repeating itself): nothing to do.
    let unchanged = match &waits {
        Some(a) => it.state == "queued" && it.after.as_deref() == Some(a.as_str()),
        None => it.state == "working" && busy,
    };
    if same && unchanged {
        // Though an `after` refused is still news to the lead (it meant this one to wait).
        if !refused.is_empty() && it.after_refused != refused {
            it.after_refused = refused;
            fx.items.push(it.id.clone());
        }
        return;
    }
    if let Some(note) = reopen.then(|| reopen_refusal(it, before.as_ref(), same && waits.is_none(), why)).flatten() {
        if it.reopen_refused != note {
            it.reopen_refused = note;
            it.updated = secs(now);
            fx.items.push(it.id.clone());
        }
        return;
    }
    let was_working = it.state == "working";
    // Moved from a holder at work on it: that one stops.
    if let Some(b) = before.filter(|_| !same && it.state == "working") {
        fx.sends.push(moved_words(&b, it, &h));
    }
    fx.items.push(it.id.clone());
    held_by(it, &h, refused, waits.clone(), why, now);
    if waits.is_none() {
        fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, why)));
    }
    let text = routed_text(id, &h.name, waits.as_deref(), reopen && same, same && was_working);
    decide(st, project, Some(id.to_string()), by.name, by.sid, &text, why, now, fx);
}

/// `it` held by `h` from now, at work (or queued after `waits`), as the lead routed it.
fn held_by(it: &mut DeskItem, h: &Holder, refused: String, waits: Option<String>, why: &str, now: u64) {
    it.updated = secs(now);
    it.route_failed.clear();
    it.reopen_refused.clear();
    it.after_refused = refused;
    it.holder = Some(h.clone());
    it.crew.clear();
    it.after_why = why.to_string();
    it.done_at = None;
    it.state = if waits.is_some() { "queued" } else { "working" }.into();
    it.after = waits;
}

/// What a route did, as its decision says it: queued, reopened, sent again or routed.
fn routed_text(id: &str, name: &str, waits: Option<&str>, reopened: bool, again: bool) -> String {
    match waits {
        Some(a) => format!("Queued #{id} after #{a} for {name}"),
        None if reopened => format!("Reopened #{id} for {name}"),
        None if again => format!("Sent #{id} to {name} again"),
        None => format!("Routed #{id} to {name}"),
    }
}

/// The lead routed `it` to `to`, a name no session of the project goes by: it stays as it is, and the lead is told.
fn no_such_session(it: &mut DeskItem, to: &str, now: u64, fx: &mut Effects) {
    it.updated = secs(now);
    it.route_failed = to.to_string();
    fx.items.push(it.id.clone());
}

/// No session fits ("new"), or the lead naming itself, which only hands work out: a new session of the project takes
/// item `id` (desk_start_new). Only one nobody else holds, so the same line said again doesn't start another.
fn start_fresh(st: &Store, project: &str, by: &By, id: &str, why: &str, fx: &mut Effects) {
    let free = |i: &DeskItem| i.project == project && matches!(i.state.as_str(), "new" | "queued" | "working") && i.holder.as_ref().is_none_or(|h| h.sid == by.sid);
    if st.desk.item(id).is_some_and(free) {
        fx.starts.push(NewStart { project: project.into(), id: id.into(), why: why.into(), by: by.name.into(), by_sid: by.sid.into() });
    }
}

/// Why item `it`, with you to check, isn't reopened by a route to its holder `before` (`to_it`: to that holder, now):
/// only its holder takes it back (sending it elsewhere, or after another, is yours), and only for a new reason (the
/// route it was given, said again by a later turn, isn't a reopen). None: it's reopened.
fn reopen_refusal(it: &DeskItem, before: Option<&Holder>, to_it: bool, why: &str) -> Option<String> {
    let held = before.map(|b| b.name.clone()).unwrap_or_default();
    let id = &it.id;
    if !to_it {
        Some(format!("it's with the user to check; Cue reopens it only for {held}, now (ROUTE: #{id} -> {held} | why: what's left)"))
    } else if why.trim().eq_ignore_ascii_case(it.after_why.trim()) {
        Some(format!("that ROUTE is the one {held} was given before; to send it back, give a new why (what's left to do)"))
    } else {
        None
    }
}

/// What holder `b`, at work on `it`, is told when it moves to `to`: (its session, the words).
fn moved_words(b: &Holder, it: &DeskItem, to: &Holder) -> (String, String) {
    let crew = if b.kind == "crew" { format!("For your teammate {}: ", b.name) } else { String::new() };
    (b.to().to_string(), format!("{crew}Desk item #{} ({}) has moved to {}: stop work on it.", it.id, it.title, to.name))
}

/// You gave item `id` to live session `sid` (of its project): it holds it now, at work on it, and is sent it.
/// Noted as your decision (nothing to undo: give it to another).
fn give(st: &mut Store, id: &str, sid: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    give_by(st, id, sid, ("You", "", ""), now, fx)
}

/// `give`, as `by` (name, session, why) decided: the lead, for a session started for it.
pub(super) fn give_by(st: &mut Store, id: &str, sid: &str, by: (&str, &str, &str), now: u64, fx: &mut Effects) -> Result<(), String> {
    let h = holder_of(st, sid).ok_or("that session isn't open")?;
    let project = st.sessions.get(sid).map(|s| s.project.clone()).unwrap_or_default();
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    if it.project != project {
        return Err(format!("{} isn't a session of {}", h.name, it.project));
    }
    if it.state == "done" {
        return Err("it's done".into());
    }
    if let Some(b) = it.holder.clone().filter(|b| it.state == "working" && !(b.sid == h.sid && b.lead_sid.is_empty())) {
        fx.sends.push(moved_words(&b, it, &h));
    }
    it.holder = Some(h.clone());
    it.crew.clear();
    it.state = "working".into();
    it.after = None;
    it.after_why.clear();
    it.route_failed.clear();
    it.after_refused.clear();
    it.done_at = None;
    it.updated = secs(now);
    it.after_why = by.2.to_string();
    fx.sends.push((sid.to_string(), crate::desk::handed_words(it, by.2)));
    fx.items.push(id.to_string());
    let text = if by.1.is_empty() { format!("Gave #{id} to {}", h.name) } else { format!("Started {} for #{id}", h.name) };
    // Given an item, by you or as a new session for it: on the Desk from now.
    join(st, &project, sid, fx);
    decide(st, &project, Some(id.to_string()), by.0, by.1, &text, by.2, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
    Ok(())
}

/// The first "#id" in `text` that's an item of `project`.
fn item_named(st: &Store, project: &str, text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter_map(|w| w.strip_prefix('#')).map(str::to_ascii_lowercase).find(|id| st.desk.item(id).is_some_and(|i| i.project == project))
}

/// An ITEM: line said by session `by` (named `from`): a new item of `project`, or the one of that kind and title
/// not done yet, with you to check too (the line said again). The item's id, and whether it's new.
#[allow(clippy::too_many_arguments)]
fn item_said(st: &mut Store, project: &str, kind: &str, title: &str, done_when: &str, from: &str, now: u64, fx: &mut Effects) -> (String, bool) {
    let same_kind = kind_of(kind);
    let open = |i: &&DeskItem| i.project == project && i.kind == same_kind && i.state != "done" && i.title.trim().eq_ignore_ascii_case(title.trim());
    if let Some(open) = st.desk.items.iter().find(open) {
        return (open.id.clone(), false);
    }
    let it = new_item(st, project, kind, title, done_when, from, now);
    let id = it.id.clone();
    fx.items.push(id.clone());
    st.desk.items.push(it);
    (id, true)
}

/// Lead `lsid` of `project` said `text` at its turn's end: its ITEM, ROUTE, ASK and DECIDED lines.
fn lead_said(st: &mut Store, project: &str, lsid: &str, text: &str, now: u64, fx: &mut Effects) {
    let name = st.sessions.get(lsid).map(shown_name).or_else(|| st.desk.lead.get(project).map(|l| l.name.clone())).unwrap_or_default();
    let by = By { name: &name, sid: lsid };
    let mut last_new: Option<String> = None;
    for line in parse::lines(text) {
        match line {
            Line::Item { kind, title, done_when, .. } => last_new = Some(item_said(st, project, &kind, &title, &done_when, &name, now, fx).0),
            Line::Route { id, to, after, why, kind, .. } => {
                // An old tag ("#d7k2") names the item by its number from now.
                let id = if id == "new" { last_new.clone() } else { Some(st.desk.canon(&id)) };
                let after = if after.as_deref() == Some("new") { last_new.clone() } else { after.map(|a| st.desk.canon(&a)) };
                if let Some(id) = id {
                    // The lead says what it is as it routes it (before, so whoever takes it is told).
                    if let (Some(k), Some(it)) = (kind, st.desk.item_mut(&id).filter(|i| i.project == project)) {
                        it.kind = k;
                        fx.items.push(id.clone());
                    }
                    route(st, project, &by, &id, &to, after, &why, now, fx);
                }
            }
            Line::Ask { question, choices } => fx.asks.push((lsid.to_string(), project.to_string(), question, choices)),
            Line::Context { who, what, why } if what == "keep" => keep(st, project, &by, &who, &why, now, fx),
            Line::Done { ids } => lead_done(st, project, lsid, &ids, fx),
            Line::Context { .. } | Line::Designs { .. } => {}
        }
    }
    for (what, why) in crate::sessions::crew_log::calls_in(text) {
        let item = item_named(st, project, &what).or_else(|| item_named(st, project, &why));
        decide_call(st, project, item, &name, lsid, &what, &why, now, fx);
    }
}

/// The lead said DONE on `ids`. Its DONE sends to check only what it holds itself at work (holder_said); for any
/// other item Cue tells it, in one short message shown in its chat, why the item stays as it is ("#69 not closed:
/// it waits on your check"), so nothing it says is dropped without a word. Once while the item stays as it is.
fn lead_done(st: &mut Store, project: &str, lsid: &str, ids: &[String], fx: &mut Effects) {
    let mut notes = Vec::new();
    for id in ids {
        let id = st.desk.canon(id);
        let Some(it) = st.desk.item(&id).filter(|i| i.project == project) else {
            notes.push(format!("#{id} not closed: this Desk has no item #{id}"));
            continue;
        };
        let mine = it.holder.as_ref().is_some_and(|h| h.is(lsid));
        if mine && matches!(it.state.as_str(), "working" | "queued") {
            continue;
        }
        // Told once while it stays as it is: a change to it (its state, a check, a route) lets the next DONE be told.
        let seen = format!("{}@{}", it.state, it.updated);
        if st.desk.done_told.get(&id) == Some(&seen) {
            continue;
        }
        let holder = it.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        let why = match it.state.as_str() {
            "to_check" => "it waits on your check".to_string(),
            "done" => "it's done already".to_string(),
            "working" | "queued" => format!("{holder} has it; its own DONE sends it to your check"),
            _ => "no one has worked on it yet".to_string(),
        };
        st.desk.done_told.insert(id.clone(), seen);
        notes.push(format!("#{id} not closed: {why}"));
    }
    if !notes.is_empty() {
        notes.push(DONE_NOTE_FOR_LEAD.to_string());
        fx.sends.push((lsid.to_string(), notes.join("\n")));
    }
}

/// The last line of Cue's note on a lead's DONE it couldn't act on: for the lead, not shown in its chat
/// (ui/desk_lead.js deskDoneNotes).
const DONE_NOTE_FOR_LEAD: &str = "Only the person closes an item that waits on their check (Looks right). Don't say DONE for these again.";

/// The lead said to keep session `who`'s context ("me": its own) as it is: Cue asks it for no hand-off note for the
/// idle stretch it's in (cache_tick's Desk rule), until it next works; said while it works, for the idle stretch
/// after this turn (the turn's end stamps it: `keep_from_now`). A decision of the lead's, which can be undone.
#[allow(clippy::too_many_arguments)]
fn keep(st: &mut Store, project: &str, by: &By, who: &str, why: &str, now: u64, fx: &mut Effects) {
    let sid = if who.eq_ignore_ascii_case("me") {
        Some(by.sid.to_string())
    } else {
        // A session, not one of a lead's crew: a crew member's context is its lead's.
        resolve(st, project, who).filter(|h| h.kind != "crew").map(|h| h.sid)
    };
    let Some(sid) = sid else { return };
    let name = if sid == by.sid { "its own".to_string() } else { st.sessions.get(&sid).map(|s| format!("{}'s", shown_name(s))).unwrap_or_default() };
    decide(st, project, None, by.name, by.sid, &format!("Kept {name} context as it is"), why, now, fx);
    let id = fx.decisions.last().cloned().unwrap_or_default();
    // The lead's own turn is ending as it says so: its idle stretch starts now.
    let working = sid != by.sid && st.sessions.get(&sid).is_some_and(|s| s.state == "working");
    st.desk.keep.insert(sid, (now, id, working));
}

/// Session `sid` (not its project's lead) said `text`: its ITEM: lines are new items of its project, from it, which
/// the lead is told of to route. A session on an item (at work on it, or it's to check) keeps its own fixes and next
/// steps in that item: an ITEM: line from it without a `why:` (how it's separate) isn't added, and it's told so.
fn session_items(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
    let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))).filter(|(p, _)| !p.is_empty()) else { return };
    let told = st.desk.lead_sid(&project).is_some();
    let on = |i: &&DeskItem| matches!(i.state.as_str(), "working" | "to_check") && i.holder.as_ref().is_some_and(|h| h.is(sid));
    let mine = st.desk.items.iter().filter(on).max_by_key(|i| i.updated).map(|i| i.id.clone());
    let mut kept = Vec::new();
    for line in parse::lines(text) {
        let Line::Item { kind, title, done_when, why } = line else { continue };
        if let Some(mine) = mine.as_ref().filter(|_| why.trim().is_empty()) {
            kept.push(format!("Not added as a new item: {}. Its fix or next step stays in #{mine}, the item you're on: do it there, or say it in your reply.", title.trim()));
            continue;
        }
        let (id, fresh) = item_said(st, &project, &kind, &title, &done_when, &name, now, fx);
        if fresh {
            if let Some(it) = st.desk.item_mut(&id) {
                it.why = why.trim().to_string();
            }
        }
        if fresh && told {
            fx.news.push((project.clone(), id));
        }
    }
    if !kept.is_empty() {
        kept.push(SEPARATE_ITEM_WORDS.into());
        fx.sends.push((sid.to_string(), kept.join("\n")));
    }
}

/// Session `sid` said where items' designs are (DESIGNS: #id <link>): each item of its project gets that link, a newer
/// one replacing the last (designs moved).
fn designs_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
    let Some(project) = st.sessions.get(sid).map(|s| s.project.clone()).filter(|p| !p.is_empty()).or_else(|| st.desk.lead_of(sid)) else { return };
    for line in parse::lines(text) {
        let Line::Designs { id, url } = line else { continue };
        let id = st.desk.canon(&id);
        let Some(it) = st.desk.item_mut(&id).filter(|i| i.project == project && i.designs != url) else { continue };
        it.designs = url;
        fx.items.push(id);
    }
}

/// How a session on an item adds one that's truly separate (said after an ITEM: line Cue didn't add).
const SEPARATE_ITEM_WORDS: &str = "Only something truly separate from it is a new item, with why in one line: ITEM: <bug|feature|task> | <title> | done when: <…> | why: <how it's separate>";

/// Holder `sid` said `text` at its turn's end: items it says are done go to you to check (and what was queued after
/// them starts), and (unless it's the lead, whose DECIDED lines are the lead's) its DECIDED lines are decisions on
/// the item it holds.
fn holder_said(st: &mut Store, sid: &str, text: &str, decided: bool, now: u64, fx: &mut Effects) {
    let mut finished: Option<String> = None;
    // What this turn made, for the Desk to link to: its web links, and its files when the session is on this Mac.
    let here = st.sessions.get(sid).is_some_and(|s| s.origin.machine.is_empty());
    for line in parse::lines(text) {
        let Line::Done { ids } = line else { continue };
        for id in ids {
            let id = st.desk.canon(&id);
            let Some(it) = st.desk.item_mut(&id).filter(|i| i.holder.as_ref().is_some_and(|h| h.is(sid)) && matches!(i.state.as_str(), "working" | "queued")) else { continue };
            it.made = parse::made_in(text, here);
            let by = if it.crew.is_empty() { it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default() } else { it.crew.clone() };
            it.checks.push(crate::desk::model::DeskCheck { at: now, by, ..Default::default() });
            fx.checked.push(id.clone());
            it.state = "to_check".into();
            it.after = None;
            it.updated = secs(now);
            fx.items.push(id.clone());
            release(st, &id, now, fx);
            finished = Some(id);
        }
    }
    let calls = crate::sessions::crew_log::calls_in(text);
    if calls.is_empty() || !decided {
        return;
    }
    // The item it's at: the one this turn said DONE (not the next, which that just started), else the one it holds
    // that changed last, at work before done.
    let held = |i: &&DeskItem| i.holder.as_ref().is_some_and(|h| h.is(sid)) && i.state != "done";
    let at = |i: &&DeskItem| finished.as_ref().map_or(i.state == "working", |f| &i.id == f);
    let Some((id, project, by)) = st.desk.items.iter().filter(held).max_by_key(|i| (at(i), i.updated)).map(|i| (i.id.clone(), i.project.clone(), i.holder.as_ref().map(|h| if h.sid == sid { h.name.clone() } else { String::new() }).unwrap_or_default())) else { return };
    let by = if by.is_empty() { st.sessions.get(sid).map(shown_name).unwrap_or_default() } else { by };
    for (what, why) in calls {
        decide_call(st, &project, Some(id.clone()), &by, sid, &what, &why, now, fx);
    }
}

/// An item's kind as kept: "bug" | "feature" | "task", or "" (one you added: the lead says what it is as it routes
/// it); anything else is a task.
fn kind_of(kind: &str) -> &str {
    if matches!(kind, "bug" | "feature" | "task" | "") { kind } else { "task" }
}

pub(super) fn new_item(st: &Store, project: &str, kind: &str, title: &str, done_when: &str, from: &str, now: u64) -> DeskItem {
    let kind = kind_of(kind);
    DeskItem {
        id: st.desk.new_id(),
        project: project.into(),
        kind: kind.into(),
        title: title.trim().into(),
        done_when: done_when.trim().into(),
        state: "new".into(),
        created: secs(now),
        updated: secs(now),
        from: from.into(),
        ..Default::default()
    }
}

/// Whether `line` starts as one of the Desk's lines would (ITEM:, ROUTE:, ASK:, DONE:, CONTEXT:, DECIDED:), after list marks
/// and bold: a cheap look before parsing.
fn desk_line_start(line: &str) -> bool {
    let l = line.trim().trim_start_matches(['-', '*', '+', '`', ' ']);
    let Some((word, _)) = l.split_once(':') else { return false };
    let word = word.trim_end_matches(['*', '`']).trim();
    matches!(word, "ITEM" | "ROUTE" | "ASK" | "DONE" | "CONTEXT" | "DESIGNS") || word.eq_ignore_ascii_case("DECIDED")
}

/// What you did to item `id` ("looks_right" | "send_back" | "start_now" | "put_before" | "drop"), in the store.
fn act_on(st: &mut Store, id: &str, act: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let busy = st.desk.item(id).and_then(|i| i.holder.as_ref()).filter(|h| at_work(st, h)).map(|h| h.name.clone());
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    let holder = it.holder.clone();
    match act {
        "looks_right" => {
            it.state = "done".into();
            it.after = None;
            it.done_at = Some(secs(now));
            release(st, id, now, &mut fx);
        }
        "send_back" => {
            let h = holder.ok_or("nobody holds it yet")?;
            let words = text.trim().to_string();
            if words.is_empty() {
                return Err("Say what isn't right first: type it in the box, then Send back".into());
            }
            (it.state, it.done_at) = ("working".into(), None);
            // Kept on the DONE it answers, so its card says why (the pictures, added by ipc desk_item_act, counted).
            if let Some(c) = it.checks.last_mut() {
                let (why, pics) = words.split_once("\nPictures with it").unwrap_or((&words, ""));
                (c.back, c.back_pics) = (why.trim().to_string(), pics.matches('`').count() / 2);
            }
            let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
            fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({}) isn't done: {words}\nIts fixes stay in #{id}; don't add new items for them. Say DONE: #{id} when it is.", it.title)));
        }
        // Nobody holds it yet: a new session of its project takes it (desk_start_new).
        "start_now" if holder.is_none() => {
            let s = NewStart { project: it.project.clone(), id: id.into(), why: String::new(), by: "You".into(), by_sid: String::new() };
            fx.starts.push(s);
        }
        // Restart (an item at work whose holder isn't on it: cut off by a crash or a quit): sent to it again.
        "start_now" => {
            let h = holder.ok_or("nobody holds it yet")?;
            if let Some(name) = busy.filter(|_| it.state == "working") {
                return Err(format!("{name} is at work on it now"));
            }
            it.after = None;
            it.state = "working".into();
            fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "")));
        }
        "put_before" => put_before(st, id, holder.as_ref(), now, &mut fx)?,
        "drop" => {
            let gone = st.desk.items.iter().position(|i| i.id == id).map(|p| st.desk.items.remove(p));
            if let Some(h) = gone.as_ref().and_then(|g| g.holder.as_ref().filter(|_| g.state == "working")) {
                fx.sends.push((h.to().to_string(), format!("Desk item #{id} was dropped: stop work on it.")));
            }
            fx.dropped.push(id.to_string());
            release(st, id, now, &mut fx);
            return Ok(fx);
        }
        _ => return Err(format!("unknown act: {act}")),
    }
    if let Some(it) = st.desk.item_mut(id) {
        it.updated = secs(now);
    }
    fx.items.push(id.to_string());
    Ok(fx)
}

/// Item `id` put before the one it waits on, in the queue: it takes that one's place (starting now when that one
/// waited on nothing), that one waits on it, and what waited on `id` waits on that one instead.
fn put_before(st: &mut Store, id: &str, holder: Option<&Holder>, now: u64, fx: &mut Effects) -> Result<(), String> {
    let (other, why) = st.desk.item(id).and_then(|it| it.after.clone().map(|a| (a, it.after_why.clone()))).ok_or("it isn't waiting on another item")?;
    // What that one waits on, if it's still to be done: this one waits on it in its place.
    let open = |a: &String| st.desk.item(a).is_some_and(|w| !matches!(w.state.as_str(), "to_check" | "done"));
    let ahead = st.desk.item(&other).and_then(|o| o.after.clone()).filter(open);
    let o = st.desk.item_mut(&other).ok_or("the item it waited on is gone")?;
    if o.state == "working" && o.holder.is_some() {
        // Already at work: only stopping it would put this first.
        return Err("the item it waits on is already being worked on".into());
    }
    let mut place = None;
    if o.holder.is_some() && !matches!(o.state.as_str(), "to_check" | "done") {
        place = ahead.map(|a| (a, o.after_why.clone()));
        o.after = Some(id.to_string());
        o.after_why = why;
        o.state = "queued".into();
        o.updated = secs(now);
        // Those queued after `id` stay behind both.
        for w in st.desk.items.iter_mut().filter(|w| w.id != other && w.after.as_deref() == Some(id)) {
            w.after = Some(other.clone());
            w.updated = secs(now);
            fx.items.push(w.id.clone());
        }
    }
    fx.items.push(other);
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    match place {
        Some((a, a_why)) => {
            it.after = Some(a);
            it.after_why = a_why;
            it.state = "queued".into();
        }
        None => {
            it.after = None;
            it.state = "working".into();
            fx.sends.extend(holder.map(|h| (h.to().to_string(), crate::desk::handed_words(it, ""))));
        }
    }
    Ok(())
}

impl Hub {
    /// The Desk as it was saved (with_ports).
    pub(super) fn desk_restore(st: &mut Store) {
        let (items, decisions, leads) = crate::db::desk_load(now_ms());
        st.desk.items = items;
        st.desk.decisions = decisions;
        // Items from before numbering get their numbers once, saved under them.
        let changed = st.desk.number_old();
        crate::db::desk_put_numbered(&st.desk, &changed);
        st.desk.lead = leads.into_iter().map(|(p, sid, name)| (p, crate::desk::Lead { sid, name })).collect();
        st.desk.members.clear();
        for (p, sid) in crate::db::desk_members() {
            st.desk.join(&p, &sid);
        }
    }

    /// Session `sid` (going by `name`) leads `project`'s Desk from now: kept and saved.
    pub(crate) fn desk_put_lead(&self, project: &str, sid: &str, name: &str) {
        self.desk_lead_to(project, Some(crate::desk::Lead { sid: sid.to_string(), name: name.to_string() }));
    }

    /// `project`'s lead put back as it was before a change of lead that failed (`before`: None when it had none).
    pub(crate) fn desk_put_back_lead(&self, project: &str, before: Option<crate::desk::Lead>) {
        self.desk_lead_to(project, before);
    }

    /// `project`'s lead from now (`lead`, or none), kept and saved.
    fn desk_lead_to(&self, project: &str, lead: Option<crate::desk::Lead>) {
        {
            let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
            // A lead is on its Desk, and stays on it once another runs it.
            let joined = self.write(|st| match lead.clone() {
                Some(l) => {
                    let joined = st.desk.join(project, &l.sid);
                    st.desk.lead.insert(project.to_string(), l);
                    joined
                }
                None => {
                    st.desk.lead.remove(project);
                    false
                }
            });
            match &lead {
                Some(l) => {
                    crate::db::desk_put_lead(project, &l.sid, &l.name, now_ms());
                    if joined {
                        crate::db::desk_put_member(project, &l.sid, now_ms());
                    }
                }
                None => crate::db::desk_drop_lead(project),
            }
        }
        self.changed();
    }

    /// The lead has been told of these failed routes (item, name) with a message that reached it: each is cleared,
    /// unless a newer one failed since.
    pub(crate) fn desk_told(&self, failed: &[(String, String)]) {
        if failed.is_empty() {
            return;
        }
        let items = self.write(|st| {
            let mut items = vec![];
            // Each (item, what it was told of): a failed route's name, or a refused `after`.
            for (id, note) in failed {
                let Some(it) = st.desk.item_mut(id) else { continue };
                if &it.route_failed == note {
                    it.route_failed.clear();
                    items.push(id.clone());
                } else if &it.after_refused == note {
                    it.after_refused.clear();
                    items.push(id.clone());
                } else if &it.reopen_refused == note {
                    it.reopen_refused.clear();
                    items.push(id.clone());
                }
            }
            items
        });
        self.desk_done(Effects { items, ..Default::default() });
    }

    /// An item of yours on `project`'s Desk. Held by live session `holder` (of the project) when given: at work on
    /// it, and sent nothing (it's already its work). Else new, and the lead (if any) told of it to route, with
    /// others added within a moment in the same message.
    /// Pictures you attached are kept with the item (uploads.rs `save_kept`) and named to whoever takes it.
    pub(crate) fn desk_add_with(&self, project: &str, kind: &str, title: &str, done_when: &str, holder: Option<&str>, images: &[crate::model::Upload]) -> Result<DeskItem, String> {
        if project.trim().is_empty() || title.trim().is_empty() {
            return Err("an item needs a project and a title".into());
        }
        let images: Vec<String> = crate::uploads::save_kept(images)?.into_iter().map(|s| s.path).collect();
        let it = self.write(|st| -> Result<(DeskItem, Effects), String> {
            let mut it = new_item(st, project, kind, title, done_when, "you", now_ms());
            it.images = images.clone();
            let mut fx = Effects { items: vec![it.id.clone()], ..Default::default() };
            match holder {
                Some(sid) => {
                    let h = holder_of(st, sid).ok_or("that session isn't open")?;
                    if st.sessions.get(sid).is_some_and(|s| s.project != project) {
                        return Err(format!("{} isn't a session of {project}", h.name));
                    }
                    it.holder = Some(h);
                    it.state = "working".into();
                    join(st, project, sid, &mut fx);
                }
                None if st.desk.lead_sid(project).is_some() => fx.news.push((project.to_string(), it.id.clone())),
                None => {}
            }
            st.desk.items.push(it.clone());
            Ok((it, fx))
        })?;
        self.desk_done(it.1);
        Ok(it.0)
    }

    /// The same with no pictures, as the tests add.
    #[cfg(test)]
    pub(crate) fn desk_add(&self, project: &str, kind: &str, title: &str, done_when: &str, holder: Option<&str>) -> Result<DeskItem, String> {
        self.desk_add_with(project, kind, title, done_when, holder, &[])
    }

    /// You gave item `id` to session `sid`: see `give`. The item as it is now.
    pub(crate) fn desk_give(&self, id: &str, sid: &str) -> Result<DeskItem, String> {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            give(st, id, sid, now, &mut fx).map(|_| fx)
        })?;
        let it = self.read(|st| st.desk.item(id).map(|i| shown(st, i)));
        self.desk_done(fx);
        it.ok_or_else(|| "no such item".into())
    }

    /// What you did to an item: "looks_right" (done), "send_back" (to its holder again, with `text`), "start_now"
    /// (not waiting on another any more), "put_before" (ahead of the item it waited on), "drop".
    pub(crate) fn desk_item_act(&self, id: &str, act: &str, text: &str) -> Result<DeskItem, String> {
        let now = now_ms();
        let fx = self.write(|st| act_on(st, id, act, text, now))?;
        let it = self.read(|st| st.desk.item(id).map(|i| shown(st, i)));
        self.desk_done(fx);
        Ok(it.unwrap_or_else(|| DeskItem { id: id.to_string(), state: "dropped".into(), ..Default::default() }))
    }

    /// Undo this: whoever made decision `id` is told to undo it (through the normal send path: kept in Cue while
    /// it's busy), and the decision is marked so. What Cue did.
    pub(crate) fn desk_undo(&self, id: &str) -> Result<String, String> {
        let d = self.write(|st| -> Result<DeskDecision, String> {
            let d = st.desk.decisions.iter_mut().find(|d| d.id == id).ok_or("no such decision")?;
            match d.undo.as_str() {
                "can" => {}
                "asked" => return Err("already asked to undo it".into()),
                _ => return Err("this one can't be undone from here".into()),
            }
            d.undo = "asked".into();
            Ok(d.clone())
        })?;
        self.desk_done(Effects { decisions: vec![id.to_string()], ..Default::default() });
        let sent = self.send_to_session(&d.sid, &crate::desk::undo_words(&d), &[], false);
        if sent.is_ok() {
            // A keep undone: Cue's own rule holds again for that session.
            self.write(|st| st.desk.keep.retain(|_, k| k.1 != d.id));
        }
        match sent {
            Ok(said) if super::team::kept(&said) => Ok(format!("Cue tells {} to undo this when its turn ends", d.by)),
            Ok(_) => Ok(format!("Asked {} to undo this", d.by)),
            Err(e) => {
                self.write(|st| st.desk.decisions.iter_mut().filter(|x| x.id == id).for_each(|x| x.undo = "can".into()));
                self.desk_done(Effects { decisions: vec![id.to_string()], ..Default::default() });
                Err(e)
            }
        }
    }

    /// Session `sid`'s turn ended, saying `text`: a lead's lines (ITEM, ROUTE, ASK, DECIDED, and DONE for what it
    /// keeps), any session's ITEM lines, and a holder's (DONE, DECIDED) acted on.
    pub(super) fn desk_turn_ended(&self, sid: &str, text: &str) {
        self.keep_from_now(sid);
        // Most turns say none of the Desk's lines: then the store isn't taken at all.
        if !text.lines().any(desk_line_start) {
            return;
        }
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            designs_said(st, sid, text, &mut fx);
            let lead = st.desk.lead_of(sid);
            match &lead {
                Some(project) => lead_said(st, project, sid, text, now, &mut fx),
                None => session_items(st, sid, text, now, &mut fx),
            }
            if st.desk.items.iter().any(|i| i.holder.as_ref().is_some_and(|h| h.is(sid))) {
                holder_said(st, sid, text, lead.is_none(), now, &mut fx);
            }
            fx
        });
        self.desk_done(fx);
    }

    /// Session `sid`'s turn ended: a keep the lead said while it worked holds from now, for the idle stretch this
    /// starts (until its next reply). Only read unless there's one.
    fn keep_from_now(&self, sid: &str) {
        if !self.read(|st| st.desk.keep.get(sid).is_some_and(|k| k.2)) {
            return;
        }
        let now = now_ms();
        self.write(|st| {
            if let Some(k) = st.desk.keep.get_mut(sid).filter(|k| k.2) {
                (k.0, k.2) = (now, false);
            }
        });
    }

    /// Cue asked session `sid` for a hand-off note (`why`: its context, and the reason: its cache ending, over the
    /// Desk's mark, its compacting failed), by the Desk's rules (cache_tick): a step by Cue, nothing to undo, on each
    /// item it holds (on its project's Desk when it holds none).
    pub(super) fn desk_cue_asked(&self, sid: &str, why: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))) else { return fx };
            let held: Vec<String> = st.desk.items.iter().filter(|i| i.project == project && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| i.id.clone()).collect();
            let on: Vec<Option<String>> = if held.is_empty() { vec![None] } else { held.into_iter().map(Some).collect() };
            for item in on {
                decide(st, &project, item, "Cue", "", &format!("Asked {name} for a hand-off note"), why, now, &mut fx);
                if let Some(d) = st.desk.decisions.last_mut() {
                    d.undo = "none".into();
                }
            }
            fx
        });
        self.desk_done(fx);
    }

    /// A lead asks you something: a question card of its session's in the Inbox, marked as its project's Desk's.
    /// Your answer goes back to it as a message (`desk_answered`).
    /// `using`: a route waiting on your yes (`UseAsk`), kept on the card for `desk_answered`.
    pub(super) fn desk_ask(&self, lsid: &str, project: &str, question: &str, choices: &[String], using: Option<&UseAsk>) {
        let Some(origin) = self.read(|st| st.sessions.origin(lsid)) else { return };
        let options: Vec<Value> = choices.iter().map(|c| json!({ "label": c, "description": "" })).collect();
        let mut input = json!({ "questions": [{ "question": question, "header": "Desk", "options": options, "multiSelect": false }] });
        if let Some(u) = using {
            input["desk_use"] = json!(u);
        }
        let ask = NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() };
        // Nobody waits on the card: its answer is sent to the lead as a message (`desk_answered`).
        let (id, _) = self.add_ask(origin, ask);
        self.write(|st| {
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.desk = true;
                it.project = project.to_string();
                // Answered, the lead is at its prompt (not at work) until the answer reaches it.
                it.at_prompt = true;
            }
        });
        self.changed();
    }

    /// Card `id` answered with `d`: a lead's ASK is filed and its answer sent to the lead. False for any other.
    pub(super) fn desk_answered(&self, id: &str, d: &Decision) -> bool {
        let Some((sid, question, using)) = self.read(|st| {
            st.items.iter().find(|i| i.id == id && i.desk).map(|i| {
                let using = serde_json::from_value::<UseAsk>(i.tool_input["desk_use"].clone()).ok();
                (i.origin.session_id.clone(), i.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string(), using)
            })
        }) else { return false };
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        let answer = answer.or_else(|| d.message.clone()).unwrap_or_else(|| if d.behavior == "deny" { "no".into() } else { "yes".into() });
        self.write(|st| st.waiters.remove(id));
        self.finish(id, "answered", &format!("answered: {answer}"));
        let mut words = format!("Your ASK: {question}\nThe answer: {answer}");
        if let Some(u) = using {
            words = self.desk_use_answered(&u, d.behavior != "deny", &answer);
        }
        self.later(0, move |h| {
            if let Err(e) = h.send_to_session(&sid, &words, &[], false) {
                eprintln!("cue: desk: couldn't send the lead its answer: {e}");
            }
        });
        true
    }

    /// `f` run while the Desk's saves wait for it (saves.rs: each writes what the store holds then, in order).
    pub(super) fn desk_saving<T>(&self, f: impl FnOnce() -> T) -> T {
        let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
        f()
    }

    /// What a change left to do, with the store let go: its saves, then its messages and questions, and new items
    /// for their lead (told a moment later, with any others that came meanwhile).
    pub(super) fn desk_done(&self, fx: Effects) {
        if fx.items.is_empty() && fx.dropped.is_empty() && fx.decisions.is_empty() && fx.sends.is_empty() && fx.asks.is_empty() && fx.news.is_empty() && fx.starts.is_empty() && fx.joined.is_empty() && fx.uses.is_empty() {
            return;
        }
        {
            // Each save writes what the store holds now (saves.rs), in the order the saves run.
            let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
            let now = now_ms();
            let (items, decisions) = self.write(|st| {
                // An item at work with a holder it wasn't stamped for: its work (and files) start now.
                for id in &fx.items {
                    if let Some(it) = st.desk.item_mut(id).filter(|i| i.state == "working") {
                        let sid = it.holder.as_ref().map(|h| h.sid.clone()).unwrap_or_default();
                        if !sid.is_empty() && it.held_from.as_ref().is_none_or(|(by, _)| *by != sid) {
                            it.held_from = Some((sid, now));
                        }
                    }
                }
                let items: Vec<DeskItem> = fx.items.iter().filter_map(|id| st.desk.item(id).cloned()).collect();
                let decisions: Vec<DeskDecision> = st.desk.decisions.iter().filter(|d| fx.decisions.contains(&d.id)).cloned().collect();
                (items, decisions)
            });
            for it in &items {
                crate::db::desk_put_item(it);
            }
            for id in &fx.dropped {
                crate::db::desk_drop_item(id);
            }
            for d in &decisions {
                crate::db::desk_put_decision(d);
            }
            fx.joined.iter().for_each(|(project, sid)| crate::db::desk_put_member(project, sid, now));
        }
        self.changed();
        for (sid, text) in fx.sends {
            // Off this thread: typing into a terminal session waits for it to take the message.
            self.later(0, move |h| {
                if let Err(e) = h.send_to_session(&sid, &text, &[], false) {
                    eprintln!("cue: desk: couldn't send {sid} its item: {e}");
                }
            });
        }
        for (lsid, project, q, choices) in fx.asks {
            self.desk_ask(&lsid, &project, &q, &choices, None);
        }
        self.desk_ask_first(fx.uses);
        for (project, id) in fx.news {
            let first = self.write(|st| {
                let fresh = st.desk.fresh.entry(project.clone()).or_default();
                fresh.push(id);
                fresh.len() == 1
            });
            if first {
                self.later(NEWS_WAIT_MS, move |h| h.desk_tell_fresh(&project));
            }
        }
        self.desk_later(fx.starts, fx.checked);
    }

    /// `project`'s done items older than `before` (updated ms, id), newest first, `take` of them, as the Desk shows
    /// them: the ones Cue holds as it holds them, the rest as saved. The list loads them as you scroll.
    pub(crate) fn desk_done_page(&self, project: &str, before: Option<(u64, &str)>, take: u32) -> Vec<DeskItem> {
        let saved = crate::db::desk_done_page(project, before, take.min(100));
        self.read(|st| saved.iter().map(|i| shown(st, st.desk.item(&i.id).filter(|x| x.state == "done").unwrap_or(i))).collect())
    }

    /// `state.desk`: each project with items or a lead, its lead ({ sid, name, state: live | parked | ended }), its
    /// items (not done, or done today) and today's decisions, newest first.
    pub(super) fn desk_view(st: &Store, now: u64) -> Value {
        let today = secs(super::today::local_start(now, false));
        let mut projects: Vec<String> = st.desk.items.iter().map(|i| i.project.clone()).chain(st.desk.lead.keys().cloned()).chain(st.desk.members.iter().filter(|(_, m)| !m.is_empty()).map(|(p, _)| p.clone())).collect();
        projects.sort();
        projects.dedup();
        let list: Vec<Value> = projects
            .iter()
            .map(|p| {
                let items: Vec<DeskItem> = st.desk.items.iter().filter(|i| &i.project == p && (i.state != "done" || i.done_at.unwrap_or(i.updated) >= today)).map(|i| shown(st, i)).collect();
                let decisions: Vec<&DeskDecision> = st.desk.decisions.iter().rev().filter(|d| &d.project == p && d.at >= today).collect();
                let members = st.desk.members.get(p).cloned().unwrap_or_default();
                json!({ "project": p, "lead": Self::desk_lead_view(st, p), "items": items, "decisions": decisions, "members": members, "checks": super::desk_checks::checks_view(st, p) })
            })
            .collect();
        json!({ "projects": list, "installed_ms": super::desk_checks::installed_ms() })
    }
}
