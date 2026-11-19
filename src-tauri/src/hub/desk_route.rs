//! Routing a Desk item (hub/desk.rs): route, note, give, start fresh, reopen, release what waits on it, and the
//! decisions made on the way.

use super::*;
use crate::desk::{DeskDecision, DeskItem, Holder};
use super::desk_members::{ask_first, join, may_route, UseAsk};
use super::desk::{Effects, NewStart, ask_of, at_work, holder_of, resolve, rule_of, secs};

/// Items queued after `id` start now, with no reason (why it was queued isn't why it starts).
/// While its Desk is paused (#366), nothing starts: they stay queued, and Resume starts them.
pub(super) fn release(st: &mut Store, id: &str, now: u64, fx: &mut Effects) {
    let project = st.desk.item(id).map(|i| &i.project)
        .or_else(|| st.desk.items.iter().find(|i| i.after.as_deref() == Some(id)).map(|i| &i.project));
    if project.is_some_and(|p| super::desk_pause::is_paused(p)) {
        return;
    }
    let rule = project.map(|p| rule_of(st, p)).unwrap_or_default();
    let lead = project.and_then(|p| st.desk.lead.get(p)).cloned();
    for it in st.desk.items.iter_mut().filter(|i| i.after.as_deref() == Some(id) && i.state == "queued") {
        it.after = None;
        it.updated = secs(now);
        start_released(it, &rule, lead.as_ref(), fx);
        fx.items.push(it.id.clone());
    }
}

pub(super) fn start_released(it: &mut DeskItem, rule: &str, lead: Option<&crate::desk::Lead>, fx: &mut Effects) {
    if let Some(h) = &it.holder {
        it.state = "working".into();
        fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "", rule)));
    } else {
        it.state = "new".into();
        let (by, by_sid) = lead.map_or(("Desk", ""), |l| (l.name.as_str(), l.sid.as_str()));
        fx.starts.push(NewStart { project: it.project.clone(), id: it.id.clone(), why: String::new(), by: by.into(), by_sid: by_sid.into() });
    }
}

/// Whether item `from` is `id`, or waits on it: directly, or through the items it waits on.
pub(super) fn waits_on(st: &Store, from: &str, id: &str) -> bool {
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
    st.desk.decisions.push(DeskDecision { id: id.clone(), project: project.into(), item_id: item, by: by.into(), sid: sid.into(), text: text.into(), why: why.into(), at: secs(now), undo: "can".into(), call: false, minor: false, delivery: None, act: String::new() });
    fx.decisions.push(id);
}

/// A judgment call (a DECIDED line) noted on `project`'s Desk: `decide`, shown in its Decisions list.
#[allow(clippy::too_many_arguments)]
pub(super) fn decide_call(st: &mut Store, project: &str, item: Option<String>, by: &str, sid: &str, (text, why, minor): (&str, &str, bool), now: u64, fx: &mut Effects) {
    decide(st, project, item, by, sid, text, why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.call = true;
        d.minor = minor;
    }
}

/// Why item `id` can't wait on item `a`, as the lead is told ("#a, which …"); None when it can. Not after an item
/// that's unknown or over, nor one that (through those it waits on) waits on this one: both would wait forever.
/// The item then starts now.
fn after_refused(st: &Store, a: &str, id: &str) -> Option<String> {
    if st.desk.item(a).zip(st.desk.item(id)).is_some_and(|(a, i)| a.project != i.project) {
        return Some(format!("#{a} belongs to another Desk"));
    }
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
/// What the lead is told of it (#248): "Taken: …" and "Asked: …" (neither sent after its turn: the user's card asks), or
/// "Refused: …" with the reason and what it can do instead. None
/// when Cue tells it another way: a name no session goes by, or an `after` or a reopen refused (with its next message,
/// from the Desk block); a new session that can't start (at once, from desk_lead.rs).
#[allow(clippy::too_many_arguments)]
pub(super) fn route(st: &mut Store, project: &str, by: &By, id: &str, to: &str, after: Option<String>, why: &str, now: u64, fx: &mut Effects) -> Option<String> {
    let rule = rule_of(st, project);
    if let Some(no) = unroutable(st, project, id) {
        return Some(no);
    }
    let fresh = to.eq_ignore_ascii_case("new") || to.eq_ignore_ascii_case("me");
    let holder = if fresh { None } else { resolve(st, project, to) };
    let to_new = fresh || holder.as_ref().is_some_and(|h| h.sid == by.sid && h.lead_sid.is_empty());
    // Finishing (its commit and install): the lead may take it off its finish only once it stopped on a question;
    // while its commit or install is under way, it's refused before anything else (the user isn't asked either).
    // A route back to its holder with a why is never dropped (#338): the why goes to it as a note, the item as it is.
    if let Some(busy) = super::desk_finish::under_way(st, id) {
        let held = st.desk.item(id).and_then(|i| i.holder.clone());
        let back = held.filter(|b| holder.as_ref().is_some_and(|h| h.sid == b.sid && h.lead_sid == b.lead_sid));
        if let Some(b) = back.filter(|_| !why.trim().is_empty()) {
            return Some(match note(st, project, by, id, why, now, fx) {
                Ok(()) => format!("Noted: #{id} {busy}, so it stays as it is; your why went to {} as a note.", b.name),
                Err(no) => no,
            });
        }
        return Some(format!("Refused: #{id} {busy}. You can wait for it, tell its holder something (NOTE: #{id} | …), or ask the user (ASK: #{id} | …)."));
    }
    // A session not on the Desk: you're asked first, and the item stays.
    if let Some(h) = holder.as_ref().filter(|h| !to_new && !may_route(st, project, h, id)) {
        ask_first(st, &UseAsk { lead: by.sid.into(), project: project.into(), sid: h.to().into(), name: h.name.clone(), item: id.into(), to: to.into(), after, why: why.into() }, fx);
        return Some(format!("Asked: #{id} goes to {0} once the user says yes ({0} isn't on the Desk).", h.name));
    }
    let finishing = st.desk.item(id).is_some_and(|i| i.state == "finishing");
    let unfinished = finishing && (to_new || holder.is_some());
    if unfinished {
        // To the lead itself ("me", its own name): a new session, so nobody holds it till that starts.
        if let Err(no) = unfinish(st, id, holder.as_ref().filter(|_| !to_new), why, now) {
            return Some(no);
        }
    }
    if to_new {
        if to.eq_ignore_ascii_case("new") {
            move_off(st, by, id, why, now, fx);
        }
        return start_fresh(st, project, by, id, after, why, now, fx);
    }
    let refused = after.as_deref().and_then(|a| after_refused(st, a, id)).unwrap_or_default();
    let waits = after.filter(|_| refused.is_empty());
    // Its holder on it now (mid-turn, or asking you): a route to it again is a turn repeating itself. Not on it (its
    // turn cut off by a crash or a quit, the item still "working"): the route sends it again.
    let busy = holder.as_ref().is_some_and(|h| at_work(st, h) || ask_of(st, h).is_some());
    // With you to check: routing it reopens it (back to work, sent again).
    let it = st.desk.item_mut(id)?;
    let reopen = it.state == "to_check";
    let Some(h) = holder else {
        no_such_session(it, to, now, fx);
        return None;
    };
    let before = it.holder.clone();
    let same = before.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid);
    // The same route said again (a later turn repeating itself): nothing to do. Sent back to its holder at work with
    // a new why (what's left, a correction): it's sent that, and its Steps show it.
    let unchanged = !unfinished && match &waits {
        Some(a) => it.state == "queued" && it.after.as_deref() == Some(a.as_str()),
        None => it.state == "working" && busy && why.trim().eq_ignore_ascii_case(it.after_why.trim()),
    };
    if same && unchanged {
        // Though an `after` refused is still news to the lead (it meant this one to wait).
        if !refused.is_empty() && it.after_refused != refused {
            it.after_refused = refused;
            fx.items.push(it.id.clone());
        }
        return Some(taken(id, &h.name, waits.as_deref()));
    }
    if let Some(note) = reopen.then(|| reopen_refusal(it, before.as_ref(), same && waits.is_none(), why)).flatten() {
        if it.reopen_refused != note {
            it.reopen_refused = note;
            it.updated = secs(now);
            fx.items.push(it.id.clone());
        }
        return None;
    }
    let was_working = it.state == "working" && !unfinished;
    // Moved from a holder at work on it: that one stops.
    if let Some(b) = before.filter(|_| !same && it.state == "working") {
        fx.sends.push(moved_words(&b, it, &h));
    }
    fx.items.push(it.id.clone());
    held_by(it, &h, refused, waits.clone(), why, now);
    if waits.is_none() {
        fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, why, &rule)));
    }
    let text = routed_text(id, &h.name, waits.as_deref(), (reopen || unfinished) && same, same && was_working);
    decide(st, project, Some(id.to_string()), by.name, by.sid, &text, why, now, fx);
    // Back to the session that had it, with what's left (#374): one of the lead's own moves.
    if same && waits.is_none() && (reopen || unfinished || was_working) {
        if let Some(d) = st.desk.decisions.last_mut() {
            d.act = "send_back".into();
        }
    }
    Some(taken(id, &h.name, waits.as_deref()))
}

/// The lead's note on item `id` of `project` (NOTE:, or a route back to its holder while its finish is under way),
/// sent to whoever holds it, at any stage (#338): at work, queued, finishing, in the landing line or with you to check.
/// The item stays as it is; the note is one of its Steps, shown on its Thread, with how it was delivered.
/// Err: what the lead is told when there's no one to send it to.
pub(super) fn note(st: &mut Store, project: &str, by: &By, id: &str, words: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let words = words.trim();
    let it = st.desk.item(id).filter(|i| i.project == project).ok_or_else(|| format!("#{id} note not sent: this Desk has no item #{id}"))?;
    let h = it.holder.clone().ok_or_else(|| format!("#{id} note not sent: nobody holds it yet; route it first"))?;
    let stage = match it.state.as_str() {
        "finishing" => match it.finish.as_ref().map(|f| f.step.as_str()) {
            Some("landing") => ", in the landing line",
            _ => ", finishing",
        },
        "to_check" => ", with the user to check",
        "queued" => ", queued",
        "done" => ", done",
        _ => "",
    };
    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    let message = format!("{crew}Note from the Desk lead on Desk item #{id} ({}{stage}): {words}", it.title);
    decide(st, project, Some(id.to_string()), by.name, by.sid, words, &format!("note to {}", h.name), now, fx);
    if let Some(step) = st.desk.decisions.last_mut() {
        step.undo = "none".into();
        step.delivery = Some(crate::desk::model::DeskDelivery { session_id: h.to().to_string(), text: message.clone(), at_ms: now });
    }
    fx.sends.push((h.to().to_string(), message));
    Ok(())
}

/// "Taken: #12 to web." (after `waits`, when it's queued): what the lead is told of a route made (#248).
fn taken(id: &str, name: &str, waits: Option<&str>) -> String {
    match waits {
        Some(a) => format!("Taken: #{id} to {name}, after #{a}."),
        None => format!("Taken: #{id} to {name}."),
    }
}

/// Why the lead can't route item `id` at all, as it's told (#248): Cue has no such open item, it's another Desk's, or
/// it's done. None: it can (the rest is `route`'s).
fn unroutable(st: &Store, project: &str, id: &str) -> Option<String> {
    match st.desk.item(id) {
        None => Some(format!("Refused: #{id} isn't an open item on this Desk. You can route an item the Desk block lists, or add it first with ITEM:.")),
        Some(i) if i.project != project => Some(format!("Refused: #{id} belongs to another Desk. You can route only this Desk's items.")),
        Some(i) if i.state == "done" => Some(format!("Refused: #{id} is done. You can ask the user to reopen it (ASK: #{id} | …), or add a new ITEM for what's left.")),
        _ => None,
    }
}

/// The lead's route of finishing item `id` (#248), to holder `h` (None: a new session): once it stopped on a question
/// (a word, a clash, a long wait, a failed install), its finish is let go and it's back at work: sent back to its
/// holder (with a why: what to fix), or handed on. Nothing of the finish is left to refer to it: no commit is being
/// made, and it's out of the install line (`under_way`); commits it made stay, as Send back leaves them.
/// Err: what the lead is told when it can't: it's sent back to its holder with no why (`route` refuses one whose
/// finish is under way before this).
fn unfinish(st: &mut Store, id: &str, h: Option<&Holder>, why: &str, now: u64) -> Result<(), String> {
    let it = st.desk.item_mut(id).ok_or_else(|| format!("Refused: #{id} isn't an open item on this Desk."))?;
    let question = it.finish.as_ref().map(|f| f.question.clone()).unwrap_or_default();
    let back = h.filter(|h| it.holder.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid));
    if let Some(b) = back.filter(|_| why.trim().is_empty()) {
        let name = &b.name;
        return Err(format!("Refused: #{id} stopped while finishing ({question}), and sending it back to {name} needs a why. You can send it back with one (ROUTE: #{id} -> {name} | why: what to fix), hand it to another session, or ask the user (ASK: #{id} | …)."));
    }
    // Kept on the DONE it answers, as Send back keeps yours: its card says why it went back.
    if let Some(c) = it.checks.last_mut() {
        c.back = if why.trim().is_empty() { question } else { why.trim().to_string() };
    }
    (it.state, it.done_at, it.finish) = ("working".into(), None, None);
    // To a new session: nobody holds it till it starts (start_fresh takes only an item nobody holds).
    if h.is_none() {
        it.remember_holder();
        it.moved_from = it.holder.clone();
        (it.holder, it.state) = (None, "new".into());
        it.crew.clear();
    }
    it.updated = secs(now);
    Ok(())
}

/// The lead routed item `id` to a new session while another session (or a teammate) holds it, at work or queued
/// (#364): it moves off that holder, which is told to stop when at work on it, and nobody holds it till the new session
/// starts (start_fresh). The holder is kept (`moved_from`) so the new session is sent its hand-off note. An item the
/// lead itself holds, with the user to check, finishing or done is left to start_fresh and unfinish; so is the route it
/// was given said again (the same why: a later turn repeating itself, which starts nothing).
fn move_off(st: &mut Store, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) {
    let Some(it) = st.desk.item_mut(id) else { return };
    let Some(b) = it.holder.clone().filter(|b| !(b.sid == by.sid && b.lead_sid.is_empty())) else { return };
    if !matches!(it.state.as_str(), "new" | "queued" | "working") || why.trim().eq_ignore_ascii_case(it.after_why.trim()) {
        return;
    }
    if it.state == "working" {
        let to = Holder { kind: "session".into(), sid: String::new(), name: "a new session".into(), lead_sid: String::new() };
        fx.sends.push(moved_words(&b, it, &to));
    }
    it.remember_holder();
    it.moved_from = Some(b);
    (it.holder, it.state, it.after) = (None, "new".into(), None);
    it.crew.clear();
    it.updated = secs(now);
    fx.items.push(id.to_string());
}

/// `it` held by `h` from now, at work (or queued after `waits`), as the lead routed it.
fn held_by(it: &mut DeskItem, h: &Holder, refused: String, waits: Option<String>, why: &str, now: u64) {
    it.updated = secs(now);
    it.route_failed.clear();
    it.start_failed.clear();
    it.reopen_refused.clear();
    it.after_refused = refused;
    if it.holder.as_ref().is_some_and(|old| old.to() != h.to()) {
        it.remember_holder();
    }
    it.holder = Some(h.clone());
    it.moved_from = None;
    it.crew.clear();
    // Routed or given (again): no longer paused (#365).
    it.paused = None;
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
/// What the lead is told (`route`).
#[allow(clippy::too_many_arguments)]
fn start_fresh(st: &mut Store, project: &str, by: &By, id: &str, after: Option<String>, why: &str, now: u64, fx: &mut Effects) -> Option<String> {
    let free = |i: &DeskItem| i.project == project && matches!(i.state.as_str(), "new" | "queued" | "working") && i.holder.as_ref().is_none_or(|h| h.sid == by.sid);
    let refused = after.as_deref().and_then(|a| after_refused(st, a, id)).unwrap_or_default();
    let it = st.desk.item_mut(id)?;
    if !free(it) {
        return Some(match (&it.holder, it.state.as_str()) {
            (Some(h), "to_check") => format!("Refused: #{id} is with the user to check. You can send it back to {0} with what's left (ROUTE: #{id} -> {0} | why: …), or ask the user (ASK: #{id} | …).", h.name),
            (Some(h), _) => format!("Refused: #{id} is held by {0}, and -> new only starts a session for an item nobody holds. You can route it to another session by name, or send it back to {0} with a why.", h.name),
            (None, state) => format!("Refused: #{id} is {}. You can ask the user (ASK: #{id} | …).", state.replace('_', " ")),
        });
    }
    if let Some(after) = after.filter(|_| refused.is_empty()) {
        if it.state == "queued" && it.after.as_ref() == Some(&after) && it.holder.is_none() {
            return Some(format!("Taken: #{id} to a new session, after #{after}."));
        }
        it.holder = None;
        it.crew.clear();
        it.state = "queued".into();
        it.after = Some(after.clone());
        it.after_why = why.to_string();
        it.route_failed.clear();
        it.start_failed.clear();
        it.after_refused.clear();
        it.updated = secs(now);
        fx.items.push(id.into());
        decide(st, project, Some(id.into()), by.name, by.sid, &format!("Queued #{id} after #{after} for a new session"), why, now, fx);
        Some(format!("Taken: #{id} to a new session, after #{after}."))
    } else {
        it.after = None;
        it.after_refused = refused;
        fx.items.push(id.into());
        fx.starts.push(NewStart { project: project.into(), id: id.into(), why: why.into(), by: by.name.into(), by_sid: by.sid.into() });
        Some(format!("Taken: #{id} to a new session (Cue is starting it)."))
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
pub(super) fn give(st: &mut Store, id: &str, sid: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    give_by(st, id, sid, ("You", "", ""), now, fx)
}

/// `give`, as `by` (name, session, why) decided: the lead, for a session started for it.
pub(super) fn give_by(st: &mut Store, id: &str, sid: &str, by: (&str, &str, &str), now: u64, fx: &mut Effects) -> Result<(), String> {
    give_to(st, id, sid, by, false, now, fx)
}

/// `give_by` to `sid`, a session Cue just started for the item in the Desk's own folder (desk_start_new): it's the
/// Desk's whatever project its folder's repository is named for (a folder you put in the project, say).
/// `handoff`: the hand-off of the holder it moved off (#364, `handoff_words`), sent with it ("" for none).
pub(super) fn give_started(st: &mut Store, id: &str, sid: &str, by: (&str, &str, &str), handoff: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    give_to(st, id, sid, by, true, now, fx)?;
    if let Some(sent) = fx.sends.iter_mut().rev().find(|(to, _)| to == sid).filter(|_| !handoff.trim().is_empty()) {
        sent.1.push_str("\n\n");
        sent.1.push_str(handoff.trim());
    }
    Ok(())
}

/// What a new session is told of the holder item `it` moved off when the lead routed it to new (#364): its hand-off
/// note (`note`: its path as its log names it, "" when Cue saw none written), else its last reply (`last`).
pub(super) fn handoff_words(it: &DeskItem, note: &str, last: &str) -> String {
    let Some(b) = it.moved_from.as_ref() else { return String::new() };
    let who = if b.kind == "crew" { format!("teammate {}", b.name) } else { b.name.clone() };
    let mut out = format!("Hand-off: #{} was with {who} before you, and the Desk lead moved it to you.", it.id);
    if !note.trim().is_empty() {
        out.push_str(&format!(" Read its hand-off note at {} first. Check it against git status and the files it names, then carry on from its next step.", note.trim()));
    } else {
        let last: String = last.trim().chars().take(1500).collect();
        if !last.is_empty() {
            out.push_str(&format!(" It left no hand-off note Cue saw; its last reply:\n\n{}", last.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")));
        }
        out.push_str("\nCheck git status and the files it names before you carry on.");
    }
    out
}

fn give_to(st: &mut Store, id: &str, sid: &str, by: (&str, &str, &str), started: bool, now: u64, fx: &mut Effects) -> Result<(), String> {
    let h = holder_of(st, sid).ok_or("that session isn't open")?;
    let project = st.sessions.get(sid).map(|s| s.project.clone()).unwrap_or_default();
    let scope = st.desk.item(id).ok_or("no such item")?.project.clone();
    let workspace = st.desk.project_of(&scope).to_string();
    let rule = rule_of(st, &scope);
    if st.desk.lead_of(sid).is_some_and(|id| id != scope) {
        return Err("that session runs another Desk".into());
    }
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    if workspace != project && !started {
        return Err(format!("{} isn't a session of {}", h.name, workspace));
    }
    if it.state == "done" {
        return Err("it's done".into());
    }
    if let Some(b) = it.holder.clone().filter(|b| it.state == "working" && !(b.sid == h.sid && b.lead_sid.is_empty())) {
        fx.sends.push(moved_words(&b, it, &h));
    }
    if it.holder.as_ref().is_some_and(|old| old.to() != h.to()) {
        it.remember_holder();
    }
    it.holder = Some(h.clone());
    it.moved_from = None;
    it.crew.clear();
    // Routed or given (again): no longer paused (#365).
    it.paused = None;
    it.state = "working".into();
    it.after = None;
    it.after_why.clear();
    it.route_failed.clear();
    it.start_failed.clear();
    // A new session started for the lead's route keeps why Cue couldn't queue it: the lead hasn't heard yet.
    if !started {
        it.after_refused.clear();
    }
    it.done_at = None;
    it.updated = secs(now);
    it.after_why = by.2.to_string();
    fx.sends.push((sid.to_string(), crate::desk::handed_words(it, by.2, &rule)));
    fx.items.push(id.to_string());
    let text = if by.1.is_empty() { format!("Gave #{id} to {}", h.name) } else { format!("Started {} for #{id}", h.name) };
    // Given an item, by you or as a new session for it: on the Desk from now.
    join(st, &scope, sid, fx);
    decide(st, &scope, Some(id.to_string()), by.0, by.1, &text, by.2, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
    Ok(())
}

/// The holder item `it` moved off (#364, `moved_from`), as its hand-off is looked up: its session (None for a teammate,
/// which writes no note of its own), when its stint on the item began (ms, 0: not known), and its last reply.
pub(super) fn moved_from_of(st: &Store, it: &DeskItem) -> Option<(Option<String>, u64, String)> {
    let b = it.moved_from.as_ref()?;
    let from = it.held_from.as_ref().filter(|(by, _)| *by == b.sid).map(|(_, at)| *at)
        .or_else(|| it.held_before.iter().rev().find(|(by, _, _)| *by == b.sid).map(|(_, at, _)| *at))
        .unwrap_or(0);
    let last = st.sessions.get(b.to()).filter(|_| b.kind != "crew").and_then(|s| s.thread.iter().rev().find(|e| e.role == "agent")).map(|e| e.text.clone()).unwrap_or_default();
    Some(((b.kind != "crew").then(|| b.sid.clone()), from, last))
}
