//! The Desk in the hub (desk/ has its records and words): each project's items and decisions in the store, what
//! Dispatch and the holders say at their turns' ends, what the window does to an item, and `state.desk`.
//! A change is made under the store's lock and leaves `Effects`: its saves and messages, done once it's let go.

use super::*;
use crate::desk::parse::{self, Line};
use crate::desk::{DeskDecision, DeskItem, Holder, Waiting};

/// How a message reaches a project's Dispatch (`desk_claim`).
pub(crate) enum DeskClaim {
    /// Live: sent to it.
    Send(String),
    /// Parked: resumed with it.
    Resume(String),
    /// None: a new one started with it.
    Start,
    /// Being started or resumed by another message: held, and sent once it's up. Its outcome: Dispatch's id, or
    /// why it wasn't sent (disconnected: the start gave up without a word).
    Wait(std::sync::mpsc::Receiver<Result<String, String>>),
}
use crate::sessions::Session;

/// What a change to the Desk leaves to do with the store let go.
#[derive(Default)]
struct Effects {
    /// Items to save as they are now, and items dropped.
    items: Vec<String>,
    dropped: Vec<String>,
    decisions: Vec<String>,
    /// Messages to sessions: (session, text).
    sends: Vec<(String, String)>,
    /// Dispatch's questions for you: (its session, question, choices).
    asks: Vec<(String, String, Vec<String>)>,
}

fn secs(ms: u64) -> u64 {
    ms / 1000
}

/// What a session goes by on the Desk: the name you gave it, Claude's, else its folder's.
fn shown_name(s: &Session) -> String {
    if !s.name.is_empty() {
        return s.name.clone();
    }
    match s.claude_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => crate::model::folder_of(&s.origin.cwd),
    }
}

/// A crew holder's (lead session, agent id).
fn crew_parts(h: &Holder) -> Option<(&str, &str)> {
    (h.kind == "crew" && !h.lead_sid.is_empty()).then_some((h.lead_sid.as_str(), h.sid.as_str()))
}

/// The sessions Dispatch may route `project`'s items to: the project's own, as its summary lists them (never a
/// Dispatch), oldest first.
fn routable<'a>(st: &'a Store, project: &str) -> Vec<&'a Session> {
    let mut ours: Vec<&Session> = st.sessions.iter().filter(|s| s.dispatch.is_empty() && s.project == project).collect();
    ours.sort_by_key(|s| s.since_ms);
    ours
}

/// The session of `project` a ROUTE names (`to`): by name (yours or Claude's), by "sid:<start of its id>" or the
/// start of its id (six characters or more), by folder, else a crew member by the name its lead gave it.
fn resolve(st: &Store, project: &str, to: &str) -> Option<Holder> {
    let want = to.trim().trim_start_matches('@').to_lowercase();
    let id = want.strip_prefix("sid:").unwrap_or(&want).trim().to_string();
    let all = routable(st, project);
    let session = |s: &Session| Holder { kind: if s.team.is_empty() { "session".into() } else { "lead".into() }, sid: s.origin.session_id.clone(), name: shown_name(s), lead_sid: String::new() };
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
fn shown(st: &Store, it: &DeskItem) -> DeskItem {
    let mut out = it.clone();
    let Some(h) = it.holder.as_ref() else { return out };
    let mut h = h.clone();
    if let Some((lead, agent)) = crew_parts(&h) {
        if let Some(t) = st.sessions.get(lead).and_then(|s| s.team.iter().find(|t| t.agent_id == agent || t.call == agent)) {
            h.name = t.name.clone();
            out.latest = t.step.clone();
            if !t.files.is_empty() {
                out.files = t.files.clone();
            }
        }
    } else if let Some(s) = st.sessions.get(&h.sid) {
        h.name = shown_name(s);
        h.kind = if s.team.is_empty() { "session".into() } else { "lead".into() };
        out.latest = if s.doing.is_empty() { last_words(s) } else { s.doing.clone() };
        let mut files: Vec<String> = s.team.iter().flat_map(|t| t.files.iter().cloned()).collect();
        files.extend(s.changes.iter().flat_map(|c| c.files.iter().map(|f| f.path.clone())));
        files.dedup();
        if !files.is_empty() {
            out.files = files;
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

/// A decision noted on `project`'s Desk.
#[allow(clippy::too_many_arguments)]
fn decide(st: &mut Store, project: &str, item: Option<String>, by: &str, sid: &str, text: &str, why: &str, now: u64, fx: &mut Effects) {
    let id = st.desk.decision_id(now);
    st.desk.decisions.push(DeskDecision { id: id.clone(), project: project.into(), item_id: item, by: by.into(), sid: sid.into(), text: text.into(), why: why.into(), at: secs(now), undo: "can".into() });
    fx.decisions.push(id);
}

/// Why item `id` can't wait on item `a`, as Dispatch is told ("#a, which …"); None when it can. Not after an item
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

/// Item `id` to the session named `to` (now, or after another item), as Dispatch `dsid` asked.
#[allow(clippy::too_many_arguments)]
fn route(st: &mut Store, project: &str, dsid: &str, id: &str, to: &str, after: Option<String>, why: &str, now: u64, fx: &mut Effects) {
    let holder = resolve(st, project, to);
    let refused = after.as_deref().and_then(|a| after_refused(st, a, id)).unwrap_or_default();
    let waits = after.filter(|_| refused.is_empty());
    let Some(it) = st.desk.item_mut(id).filter(|i| i.project == project && i.state != "done") else { return };
    let Some(h) = holder else {
        it.updated = secs(now);
        it.route_failed = to.to_string();
        fx.items.push(it.id.clone());
        return;
    };
    let before = it.holder.clone();
    let same = before.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid);
    // The same route said again (a later turn repeating itself): nothing to do.
    let unchanged = match &waits {
        Some(a) => it.state == "queued" && it.after.as_deref() == Some(a.as_str()),
        // At work, or done and with you to check (sending it back is yours to do: send_back).
        None => matches!(it.state.as_str(), "working" | "to_check"),
    };
    if same && unchanged {
        // Though an `after` refused is still news to Dispatch (it meant this one to wait).
        if !refused.is_empty() && it.after_refused != refused {
            it.after_refused = refused;
            fx.items.push(it.id.clone());
        }
        return;
    }
    // Moved from a holder at work on it: that one stops.
    if let Some(b) = before.filter(|_| !same && it.state == "working") {
        let crew = if b.kind == "crew" { format!("For your teammate {}: ", b.name) } else { String::new() };
        fx.sends.push((b.to().to_string(), format!("{crew}Desk item #{id} ({}) has moved to {}: stop work on it.", it.title, h.name)));
    }
    it.updated = secs(now);
    fx.items.push(it.id.clone());
    it.route_failed.clear();
    it.after_refused = refused;
    it.holder = Some(h.clone());
    it.after_why = why.to_string();
    let text = match &waits {
        Some(a) => {
            it.state = "queued".into();
            it.after = Some(a.clone());
            format!("Queued #{id} after #{a} for {}", h.name)
        }
        None => {
            it.state = "working".into();
            it.after = None;
            fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, why)));
            format!("Routed #{id} to {}", h.name)
        }
    };
    decide(st, project, Some(id.to_string()), "Dispatch", dsid, &text, why, now, fx);
}

/// The first "#id" in `text` that's an item of `project`.
fn item_named(st: &Store, project: &str, text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter_map(|w| w.strip_prefix('#')).map(str::to_ascii_lowercase).find(|id| st.desk.item(id).is_some_and(|i| i.project == project))
}

/// Dispatch `dsid` of `project` said `text` at its turn's end.
fn dispatch_said(st: &mut Store, project: &str, dsid: &str, text: &str, now: u64, fx: &mut Effects) {
    let mut last_new: Option<String> = None;
    for line in parse::lines(text) {
        match line {
            Line::Item { kind, title, done_when } => {
                // An item of that kind and title not yet done (the line said again): that one, not a second.
                let same_kind = kind_of(&kind);
                let open = |i: &&DeskItem| i.project == project && i.kind == same_kind && matches!(i.state.as_str(), "new" | "queued" | "working") && i.title.trim().eq_ignore_ascii_case(title.trim());
                if let Some(open) = st.desk.items.iter().find(open) {
                    last_new = Some(open.id.clone());
                    continue;
                }
                let it = new_item(st, project, &kind, &title, &done_when, "dispatch", now);
                last_new = Some(it.id.clone());
                fx.items.push(it.id.clone());
                st.desk.items.push(it);
            }
            Line::Route { id, to, after, why } => {
                let id = if id == "new" { last_new.clone() } else { Some(id) };
                let after = if after.as_deref() == Some("new") { last_new.clone() } else { after };
                if let Some(id) = id {
                    route(st, project, dsid, &id, &to, after, &why, now, fx);
                }
            }
            Line::Ask { question, choices } => fx.asks.push((dsid.to_string(), question, choices)),
            Line::Done { .. } => {}
        }
    }
    for (what, why) in crate::sessions::crew_log::calls_in(text) {
        let item = item_named(st, project, &what).or_else(|| item_named(st, project, &why));
        decide(st, project, item, "Dispatch", dsid, &what, &why, now, fx);
    }
}

/// Holder `sid` said `text` at its turn's end: items it says are done go to you to check (and what was queued after
/// them starts), and its DECIDED lines are decisions on the item it holds.
fn holder_said(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
    for line in parse::lines(text) {
        let Line::Done { ids } = line else { continue };
        for id in ids {
            let Some(it) = st.desk.item_mut(&id).filter(|i| i.holder.as_ref().is_some_and(|h| h.is(sid)) && matches!(i.state.as_str(), "working" | "queued")) else { continue };
            it.state = "to_check".into();
            it.after = None;
            it.updated = secs(now);
            fx.items.push(id.clone());
            release(st, &id, now, fx);
        }
    }
    let calls = crate::sessions::crew_log::calls_in(text);
    if calls.is_empty() {
        return;
    }
    // The item it's at: the one it holds that changed last, at work before done.
    let held = |i: &&DeskItem| i.holder.as_ref().is_some_and(|h| h.is(sid)) && i.state != "done";
    let Some((id, project, by)) = st.desk.items.iter().filter(held).max_by_key(|i| (i.state == "working", i.updated)).map(|i| (i.id.clone(), i.project.clone(), i.holder.as_ref().map(|h| if h.sid == sid { h.name.clone() } else { String::new() }).unwrap_or_default())) else { return };
    let by = if by.is_empty() { st.sessions.get(sid).map(shown_name).unwrap_or_default() } else { by };
    for (what, why) in calls {
        decide(st, &project, Some(id.clone()), &by, sid, &what, &why, now, fx);
    }
}

/// An item's kind as kept: "bug" | "feature" | "task" (anything else is a task).
fn kind_of(kind: &str) -> &str {
    if matches!(kind, "bug" | "feature" | "task") { kind } else { "task" }
}

fn new_item(st: &Store, project: &str, kind: &str, title: &str, done_when: &str, from: &str, now: u64) -> DeskItem {
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

/// Whether `line` starts as one of the Desk's lines would (ITEM:, ROUTE:, ASK:, DONE:, DECIDED:), after list marks
/// and bold: a cheap look before parsing.
fn desk_line_start(line: &str) -> bool {
    let l = line.trim().trim_start_matches(['-', '*', '+', '`', ' ']);
    let Some((word, _)) = l.split_once(':') else { return false };
    let word = word.trim_end_matches(['*', '`']).trim();
    matches!(word, "ITEM" | "ROUTE" | "ASK" | "DONE") || word.eq_ignore_ascii_case("DECIDED")
}

/// What you did to item `id` ("looks_right" | "send_back" | "start_now" | "put_before" | "drop"), in the store.
fn act_on(st: &mut Store, id: &str, act: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
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
            it.state = "working".into();
            it.done_at = None;
            let words = if text.trim().is_empty() { "Not done yet: have another look.".to_string() } else { text.trim().to_string() };
            let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
            fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({}) isn't done: {words}\nSay DONE: #{id} when it is.", it.title)));
        }
        "start_now" => {
            let h = holder.ok_or("nobody holds it yet")?;
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
        let (items, decisions, dispatch) = crate::db::desk_load(now_ms());
        st.desk.items = items;
        st.desk.decisions = decisions;
        st.desk.dispatch = dispatch.into_iter().collect();
    }

    /// What a session starting is told beyond your words: a Dispatch's instructions, if it's one.
    pub(super) fn desk_skill(&self, sid: &str) -> Option<&'static str> {
        self.read(|st| st.desk.dispatch_of(sid)).map(|_| crate::desk::DISPATCH_SKILL)
    }

    /// How a message (`waiting`) reaches `project`'s Dispatch, decided in one step so two at once never start two:
    /// sent to it while it's live (paused is live: a message starts it again); resumed with it when it's parked;
    /// started with it when there's none (ended, or never). Resume and Start claim the project until
    /// `desk_claim_done`; a message that comes meanwhile is held, and sent then (Wait).
    pub(crate) fn desk_claim(&self, project: &str, message: &str, failed: &[(String, String)]) -> DeskClaim {
        self.write(|st| {
            if let Some(held) = st.desk.starting.get_mut(project) {
                let (reply, outcome) = std::sync::mpsc::channel();
                held.push(Waiting { message: message.to_string(), failed: failed.to_vec(), reply });
                return DeskClaim::Wait(outcome);
            }
            let sid = st.desk.dispatch.get(project).cloned();
            if let Some(sid) = sid.as_ref().filter(|sid| st.sessions.get(sid).is_some()) {
                return DeskClaim::Send(sid.clone());
            }
            st.desk.starting.insert(project.to_string(), vec![]);
            match sid.filter(|sid| st.parked.iter().any(|p| &p.session.origin.session_id == sid)) {
                Some(sid) => DeskClaim::Resume(sid),
                None => DeskClaim::Start,
            }
        })
    }

    /// The start or resume `desk_claim` allowed is over: the project is let go, and the messages held meanwhile
    /// handed back to be sent.
    pub(crate) fn desk_claim_done(&self, project: &str) -> Vec<Waiting> {
        self.write(|st| st.desk.starting.remove(project)).unwrap_or_default()
    }

    /// How many messages wait on `project`'s Dispatch starting.
    #[cfg(test)]
    pub(crate) fn desk_held(&self, project: &str) -> usize {
        self.read(|st| st.desk.starting.get(project).map_or(0, Vec::len))
    }

    /// Session `sid` is starting as `project`'s Dispatch: kept before it starts, so it starts with its instructions.
    pub(crate) fn desk_dispatch_starting(&self, project: &str, sid: &str) {
        let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
        self.write(|st| st.desk.dispatch.insert(project.to_string(), sid.to_string()));
        crate::db::desk_put_dispatch(project, sid, now_ms());
    }

    /// It started: marked as the project's Dispatch on its record (the window tells it apart).
    pub(crate) fn desk_dispatch_started(&self, project: &str, sid: &str) {
        if self.write(|st| st.sessions.set_dispatch(sid, project)) {
            self.changed();
        }
    }

    /// The folder `project`'s Dispatch starts in: the repository of a session of the project on this Mac (live,
    /// then parked).
    pub(crate) fn desk_folder(&self, project: &str) -> Option<String> {
        let cwds: Vec<String> = self.read(|st| {
            let live = st.sessions.iter().map(|s| &s.origin);
            let parked = st.parked.iter().map(|p| &p.session.origin);
            live.chain(parked).filter(|o| o.machine.is_empty() && !o.cwd.is_empty() && st.desk.dispatch_of(&o.session_id).is_none()).cloned().collect::<Vec<_>>()
        })
        .into_iter()
        .filter(|o| super::crew_log::project_shown_for(o) == project)
        .map(|o| o.cwd)
        .collect();
        let cwd = cwds.into_iter().next()?;
        Some(crate::logs::repos::repo_root(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or(cwd))
    }

    /// What Dispatch is told before each message of yours: the project's items, who holds them, and the sessions it
    /// can route to, in one `<desk …>…</desk>` block (no `</desk>` inside it: the window strips the block by it).
    /// With the routes it couldn't make that it's told of: `desk_told` once the message has reached it.
    pub(crate) fn desk_summary(&self, project: &str) -> (String, Vec<(String, String)>) {
        let (text, failed) = self.read(|st| {
            let mut out = format!("project=\"{}\">\nItems:\n", project.replace('"', "'"));
            let (mut failed, mut told) = (vec![], vec![]);
            let items: Vec<DeskItem> = st.desk.items.iter().filter(|i| i.project == project && i.state != "done").map(|i| shown(st, i)).collect();
            if items.is_empty() {
                out.push_str("(none yet)\n");
            }
            for it in &items {
                out.push_str(&format!("#{} {}: {} | {}", it.id, it.kind, it.title, it.state.replace('_', " ")));
                if let Some(h) = &it.holder {
                    out.push_str(&format!(" | held by {} ({})", h.name, h.kind));
                }
                if let Some(a) = &it.after {
                    out.push_str(&format!(" | after #{a}"));
                }
                if !it.done_when.is_empty() {
                    out.push_str(&format!(" | done when: {}", it.done_when));
                }
                if !it.files.is_empty() {
                    out.push_str(&format!(" | files: {}", it.files.iter().take(8).cloned().collect::<Vec<_>>().join(", ")));
                }
                out.push('\n');
                if !it.route_failed.is_empty() {
                    failed.push((it.id.clone(), it.route_failed.clone()));
                    // Still new only when nobody holds it; else it stays where it is.
                    let stays = match &it.holder {
                        Some(h) => format!("It stays with {}.", h.name),
                        None => "It's still new.".to_string(),
                    };
                    told.push(format!("Cue couldn't route #{} to \"{}\": no session of this project goes by that name. {stays}\n", it.id, it.route_failed));
                }
                if !it.after_refused.is_empty() {
                    failed.push((it.id.clone(), it.after_refused.clone()));
                    told.push(format!("Cue couldn't queue #{} after {}: it started now. If the two would clash (the same files), say how to order them.\n", it.id, it.after_refused));
                }
            }
            for line in told {
                out.push_str(&line);
            }
            let names: Vec<String> = routable(st, project)
                .iter()
                .map(|s| {
                    let crew: Vec<&str> = s.team.iter().map(|t| t.name.as_str()).collect();
                    let what = if crew.is_empty() { "session".to_string() } else { format!("lead, crew: {}", crew.join(", ")) };
                    let id: String = s.origin.session_id.chars().take(8).collect();
                    format!("{} (sid:{id}, {what}, {})", shown_name(s), if s.state.is_empty() { "idle" } else { &s.state })
                })
                .collect();
            out.push_str(&format!("Sessions you can route to: {}\n", if names.is_empty() { "none open in this project".to_string() } else { names.join("; ") }));
            (out, failed)
        });
        // Titles and names are anyone's words: none of them may close the block early.
        (format!("<desk {}</desk>", text.replace("</desk", "</ desk")), failed)
    }

    /// Dispatch has been told of these failed routes (item, name) with a message that reached it: each is cleared,
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
                }
            }
            items
        });
        self.desk_done(Effects { items, ..Default::default() });
    }

    /// The "New item" button: an item of yours on `project`'s Desk.
    pub(crate) fn desk_add(&self, project: &str, kind: &str, title: &str, done_when: &str) -> Result<DeskItem, String> {
        if project.trim().is_empty() || title.trim().is_empty() {
            return Err("an item needs a project and a title".into());
        }
        let it = self.write(|st| {
            let it = new_item(st, project, kind, title, done_when, "you", now_ms());
            st.desk.items.push(it.clone());
            it
        });
        self.desk_done(Effects { items: vec![it.id.clone()], ..Default::default() });
        Ok(it)
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
        match self.send_to_session(&d.sid, &crate::desk::undo_words(&d), &[], false) {
            Ok(said) if super::team::kept(&said) => Ok(format!("Cue tells {} to undo this when its turn ends", d.by)),
            Ok(_) => Ok(format!("Asked {} to undo this", d.by)),
            Err(e) => {
                self.write(|st| st.desk.decisions.iter_mut().filter(|x| x.id == id).for_each(|x| x.undo = "can".into()));
                self.desk_done(Effects { decisions: vec![id.to_string()], ..Default::default() });
                Err(e)
            }
        }
    }

    /// Session `sid`'s turn ended, saying `text`: Dispatch's lines (ITEM, ROUTE, ASK, DECIDED) or a holder's
    /// (DONE, DECIDED) acted on.
    pub(super) fn desk_turn_ended(&self, sid: &str, text: &str) {
        // Most turns say none of the Desk's lines: then the store isn't taken at all.
        if !text.lines().any(desk_line_start) {
            return;
        }
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            match st.desk.dispatch_of(sid) {
                Some(project) => dispatch_said(st, &project, sid, text, now, &mut fx),
                None if st.desk.items.iter().any(|i| i.holder.as_ref().is_some_and(|h| h.is(sid))) => holder_said(st, sid, text, now, &mut fx),
                None => {}
            }
            fx
        });
        self.desk_done(fx);
    }

    /// Dispatch asks you something: a question card in the Inbox, from Dispatch. Your answer goes back to it as
    /// a message (`desk_answered`).
    fn desk_ask(&self, dsid: &str, question: &str, choices: &[String]) {
        let Some(origin) = self.read(|st| st.sessions.origin(dsid)) else { return };
        let options: Vec<Value> = choices.iter().map(|c| json!({ "label": c, "description": "" })).collect();
        let input = json!({ "questions": [{ "question": question, "header": "Dispatch", "options": options, "multiSelect": false }] });
        let ask = NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() };
        // Nobody waits on the card: its answer is sent to Dispatch as a message (`desk_answered`).
        let (id, _) = self.add_ask(origin, ask);
        self.write(|st| {
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.from_dispatch = true;
                // Answered, Dispatch is at its prompt (not at work) until the answer reaches it.
                it.at_prompt = true;
            }
        });
        self.changed();
    }

    /// Card `id` answered with `d`: one from Dispatch is filed and its answer sent to Dispatch. False for any other.
    pub(super) fn desk_answered(&self, id: &str, d: &Decision) -> bool {
        let Some((sid, question)) = self.read(|st| st.items.iter().find(|i| i.id == id && i.from_dispatch).map(|i| (i.origin.session_id.clone(), i.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string()))) else { return false };
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        let answer = answer.or_else(|| d.message.clone()).unwrap_or_else(|| if d.behavior == "deny" { "no".into() } else { "yes".into() });
        self.write(|st| st.waiters.remove(id));
        self.finish(id, "answered", &format!("answered: {answer}"));
        let words = format!("Your ASK: {question}\nThe answer: {answer}");
        self.later(0, move |h| {
            if let Err(e) = h.send_to_session(&sid, &words, &[], false) {
                eprintln!("cue: desk: couldn't send Dispatch its answer: {e}");
            }
        });
        true
    }

    /// What a change left to do, with the store let go: its saves, then its messages and questions.
    fn desk_done(&self, fx: Effects) {
        if fx.items.is_empty() && fx.dropped.is_empty() && fx.decisions.is_empty() && fx.sends.is_empty() && fx.asks.is_empty() {
            return;
        }
        {
            // Each save writes what the store holds now (saves.rs), in the order the saves run.
            let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
            let (items, decisions) = self.read(|st| {
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
        for (dsid, q, choices) in fx.asks {
            self.desk_ask(&dsid, &q, &choices);
        }
    }

    /// `state.desk`: each project with items or a Dispatch, its Dispatch, its items (not done, or done today) and
    /// today's decisions, newest first.
    pub(super) fn desk_view(st: &Store, now: u64) -> Value {
        let today = secs(super::today::local_start(now, false));
        let mut projects: Vec<String> = st.desk.items.iter().map(|i| i.project.clone()).chain(st.desk.dispatch.keys().cloned()).collect();
        projects.sort();
        projects.dedup();
        let shown_live = |sid: &String| st.sessions.get(sid).is_some() || st.parked.iter().any(|p| &p.session.origin.session_id == sid);
        let list: Vec<Value> = projects
            .iter()
            .map(|p| {
                let items: Vec<DeskItem> = st.desk.items.iter().filter(|i| &i.project == p && (i.state != "done" || i.done_at.unwrap_or(i.updated) >= today)).map(|i| shown(st, i)).collect();
                let decisions: Vec<&DeskDecision> = st.desk.decisions.iter().rev().filter(|d| &d.project == p && d.at >= today).collect();
                json!({ "project": p, "dispatch": st.desk.dispatch.get(p).filter(|s| shown_live(s)), "items": items, "decisions": decisions })
            })
            .collect();
        json!({ "projects": list })
    }
}
