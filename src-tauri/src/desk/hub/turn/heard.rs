//! What the Desk reads from other sessions at a turn's end (desk/hub/core/desk.rs): any session's ITEM lines, design
//! lines, replies on an item, and a holder's lines (DONE, DECIDED); and a new item.

use crate::desk::hub::prelude::*;
use crate::desk::parse::{self, Line};
use crate::desk::DeskItem;
use crate::desk::hub::core::desk::{Effects, secs, shown_name};
use crate::desk::hub::turn::route::decide_call;
use crate::desk::hub::turn::said::item_said;

/// Session `sid` (not its project's lead) said `text`: its ITEM: lines are new items of its project, from it, which
/// the lead is told of to route. A session on an item (at work on it, or it's to check) keeps its own fixes and next
/// steps in that item: an ITEM: line from it without a `why:` (how it's separate) isn't added, and it's told so.
pub(crate) fn session_items(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
    let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))).filter(|(p, _)| !p.is_empty()) else { return };
    let scopes = st.desk.scopes(sid, &project);
    if scopes.len() != 1 {
        if parse::lines(text).iter().any(|line| matches!(line, Line::Item { .. })) {
            fx.sends.push((sid.to_string(), "Several Desks could receive this item. Add it through the intended Desk so it reaches the right lead.".into()));
        }
        return;
    }
    let project = scopes[0].clone();
    let told = st.desk.lead_sid(&project).is_some();
    let on = |i: &&DeskItem| matches!(i.state.as_str(), "working" | "finishing" | "to_check") && i.holder.as_ref().is_some_and(|h| h.is(sid));
    let mine = st.desk.items.iter().filter(on).max_by_key(|i| i.updated).map(|i| i.id.clone());
    let mut kept = Vec::new();
    for line in parse::lines(text) {
        let Line::Item { kind, title, see, done_when, why, .. } = line else { continue };
        if let Some(mine) = mine.as_ref().filter(|_| why.trim().is_empty()) {
            kept.push(format!("Not added as a new item: {}. Its fix or next step stays in #{mine}, the item you're on: do it there, or say it in your reply.", title.trim()));
            continue;
        }
        let (id, fresh) = item_said(st, &project, &kind, &title, &see, &done_when, &name, now, fx);
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

/// Session `sid` said what follows each option of the items it holds (IF PICKED: lines, #543): an item's lines in
/// this turn replace any it had. Lines about an item it doesn't hold (nor leads the Desk of) are left alone.
pub(crate) fn next_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
    let mut said: Vec<(String, crate::desk::model::FollowUp)> = Vec::new();
    for line in parse::lines(text) {
        if let Line::IfPicked { id, next } = line {
            said.push((st.desk.canon(&id), next));
        }
    }
    let lead = st.desk.lead_of(sid);
    let mut ids: Vec<String> = said.iter().map(|(i, _)| i.clone()).collect();
    ids.dedup();
    for id in ids {
        let Some(it) = st.desk.item_mut(&id) else { continue };
        let mine = it.holder.as_ref().is_some_and(|h| h.is(sid)) || lead.as_ref() == Some(&it.project);
        if !mine || !it.picked.is_empty() || matches!(it.state.as_str(), "done" | "dropped") {
            continue;
        }
        it.next = said.iter().filter(|(i, _)| *i == id).map(|(_, n)| n.clone()).collect();
        fx.items.push(id);
    }
}

/// Session `sid` said where items' designs are (DESIGNS: #id <link>): each item of its project gets that link, a newer
/// one replacing the last (designs moved).
pub(crate) fn designs_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
    let Some(project) = st.sessions.get(sid).map(|s| s.project.clone()).filter(|p| !p.is_empty()).or_else(|| st.desk.lead_of(sid)) else { return };
    let scopes = st.desk.scopes(sid, &project);
    for line in parse::lines(text) {
        let Line::Designs { id, url } = line else { continue };
        let id = st.desk.canon(&id);
        let Some(it) = st.desk.item_mut(&id).filter(|i| scopes.contains(&i.project) && i.designs != url) else { continue };
        it.designs = url;
        fx.items.push(id);
    }
}

/// Session `sid` (not a lead) wrote TO LEAD: lines (#474): each goes to the lead of its item's Desk, as from it and
/// that item, on whichever Desk that is (#714). Without #<id>, the item it's on (the one it holds that changed last);
/// holding none, its one Desk. An item no Desk has, or words with no Desk to go to, come back to it saying why.
pub(crate) fn to_lead_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
    let wants: Vec<(Option<String>, String)> = parse::lines(text).into_iter().filter_map(|l| if let Line::ToLead { id, text } = l { Some((id, text)) } else { None }).collect();
    if wants.is_empty() || st.desk.lead_of(sid).is_some() {
        return;
    }
    let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))) else { return };
    let scopes = st.desk.scopes(sid, &project);
    let held = |i: &&DeskItem| i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid));
    let on = st.desk.items.iter().filter(held).max_by_key(|i| (i.state == "working", i.updated)).map(|i| i.id.clone());
    let mut back = Vec::new();
    for (id, words) in wants {
        let id = id.map(|i| st.desk.canon(&i)).or_else(|| on.clone());
        let it = id.as_deref().and_then(|i| st.desk.item(i));
        let message = match (&id, it) {
            // An item on any Desk (#714): to that Desk's lead.
            (Some(_), Some(it)) => (
                it.project.clone(),
                format!("From {name} on #{} ({}):\n{words}\nAnswer it with NOTE: #{} | <your words>.", it.id, it.title, it.id),
            ),
            (Some(i), _) => {
                back.push(format!("TO LEAD: not sent: #{i} isn't an item on any Desk. Name one you're on, or leave the # out."));
                continue;
            }
            (None, _) if scopes.len() == 1 => (scopes[0].clone(), format!("From {name}:\n{words}\nIt holds no item here, so a NOTE can't reach it.")),
            (None, _) => {
                back.push("TO LEAD: not sent: you hold no item and aren't on one Desk. Add #<id> for the item it's about.".into());
                continue;
            }
        };
        fx.to_lead.push((message.0, sid.to_string(), message.1, words));
    }
    if !back.is_empty() {
        fx.sends.push((sid.to_string(), back.join("\n")));
    }
}

/// A turn's words as a reply keeps them: the Desk's own lines left out, cut to `SAID_CHARS`.
fn said_words(text: &str) -> String {
    let words: Vec<&str> = text.lines().filter(|l| !desk_line_start(l)).collect();
    let words = words.join("\n").trim().to_string();
    match words.char_indices().nth(crate::desk::model::SAID_CHARS) {
        Some((cut, _)) => format!("{}…", words[..cut].trim_end()),
        None => words,
    }
}

/// A holder ended a turn at work on items `on`, saying `text` (in parts, `turn`): its words kept on each as a reply
/// on its Thread, marked done for one its DONE line names. What a Stop hook made it write after its answer is kept
/// apart, folded under the reply (#310); a turn no hook sent back is its words whole.
pub(crate) fn replied(st: &mut Store, sid: &str, text: &str, turn: &[crate::model::TurnPart], on: &[String], now: u64, fx: &mut Effects) {
    use crate::desk::model::{Said, SAID_KEPT};
    let part = |hooked: bool| turn.iter().filter(|p| p.hooked == hooked).map(|p| p.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    let (words, after) = if turn.iter().any(|p| p.hooked) { (said_words(&part(false)), said_words(&part(true))) } else { (said_words(text), String::new()) };
    // Nothing before the hook (a turn that was all follow-up): what it wrote is the reply.
    let (words, after) = if words.is_empty() { (after, String::new()) } else { (words, after) };
    if words.is_empty() || on.is_empty() {
        return;
    }
    let done: Vec<String> = parse::lines(text).into_iter().filter_map(|l| if let Line::Done { ids } = l { Some(ids) } else { None }).flatten().map(|i| st.desk.canon(&i)).collect();
    for id in on {
        let Some(it) = st.desk.item_mut(id) else { continue };
        let by = it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default();
        // The same turn's words again, with more after them (a turn that ended on a usage limit is told twice): one
        // reply, the longer words.
        let again = it.said.last_mut().filter(|s| s.by == by && words.starts_with(s.text.trim_end_matches('…')));
        if let Some(s) = again {
            s.text = words.clone();
            s.after = after.clone();
            s.at = now;
            s.done |= done.contains(id);
            s.sid = sid.to_string();
            fx.said.push(id.clone());
            continue;
        }
        it.said.push(Said { at: now, by, text: words.clone(), after: after.clone(), done: done.contains(id), sid: sid.to_string() });
        let over = it.said.len().saturating_sub(SAID_KEPT);
        it.said.drain(..over);
        fx.said.push(id.clone());
    }
}

/// How a session on an item adds one that's truly separate (said after an ITEM: line Cue didn't add).
const SEPARATE_ITEM_WORDS: &str = "Only something truly separate from it is a new item, with why in one line: ITEM: <bug|feature|task> | <title> | you'll see: <what the user sees when it's done, plain words, an item never a card> | done when: <the check> | why: <how it's separate>";

/// Holder `sid` said `text` at its turn's end: items it says are done go to you to check (and what was queued after
/// them starts), and (unless it's the lead, whose DECIDED lines are the lead's) its DECIDED lines are decisions on
/// the item it holds.
pub(crate) fn holder_said(st: &mut Store, sid: &str, text: &str, decided: bool, now: u64, fx: &mut Effects) {
    // Its crew or background agents still at work: not done yet, whatever it says. The item stays at work; a DONE
    // said meanwhile is held, and acted on once they've finished (`held_done`), so it needn't be said again (#393).
    let finished = if crate::desk::hub::stall::stall::still_at_it(st, sid) {
        let held = |i: &DeskItem| i.holder.as_ref().is_some_and(|h| h.is(sid)) && matches!(i.state.as_str(), "working" | "queued");
        let names_held = parse::lines(text).into_iter().any(|l| matches!(l, Line::Done { ref ids } if ids.iter().any(|id| st.desk.item(&st.desk.canon(id)).is_some_and(held))));
        if names_held {
            st.desk.run.done_held.insert(sid.to_string(), text.to_string());
        }
        // A CHECK for an item already finishing or to check still counts (#526); one with a held DONE waits with it.
        checks(st, sid, text, now, fx);
        None
    } else {
        dones(st, sid, text, now, fx)
    };
    after_dones(st, sid, text, decided, finished, now, fx);
}

/// Holder `sid`'s turn that said DONE while its crew or background agents were at work (`done_held`): once they've
/// finished, that DONE goes ahead (what it named still at work with it goes to you to check). Before then, nothing.
pub(crate) fn held_done(st: &mut Store, sid: &str, now: u64, fx: &mut Effects) {
    if !st.desk.run.done_held.contains_key(sid) || crate::desk::hub::stall::stall::still_at_it(st, sid) {
        return;
    }
    let Some(text) = st.desk.run.done_held.remove(sid) else { return };
    dones(st, sid, &text, now, fx);
}

/// The items holder `sid`'s DONE lines in `text` name go to you to check: the last one, if any.
fn dones(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) -> Option<String> {
    let mut finished: Option<String> = None;
    let mut said_done: Vec<String> = Vec::new();
    let here = st.sessions.get(sid).is_some_and(|s| s.origin.machine.is_empty());
    for line in parse::lines(text) {
        let Line::Done { ids } = line else { continue };
        for said in ids {
            let id = st.desk.canon(&said);
            // At work, or finishing and stopped on a question (a word in it, a conflict, a failed build): finishing
            // (again), its commit made a moment later (desk/hub/landing/finish.rs), and what waits on it started once it is.
            // Or waiting for its branch on another machine: a DONE naming its new commit is tried at once (#596).
            let waits = |i: &DeskItem| i.remote.as_ref().is_some_and(|r| !r.wait.is_empty());
            let again = |i: &DeskItem| i.state == "finishing" && (i.finish.as_ref().is_some_and(|f| !f.question.is_empty()) || waits(i));
            let Some(it) = st.desk.item_mut(&id).filter(|i| i.holder.as_ref().is_some_and(|h| h.is(sid)) && (matches!(i.state.as_str(), "working" | "queued") || again(i))) else { continue };
            it.made = parse::made_in(text, here);
            // Work on another machine: the branch and commit its DONE names are the ones Cue lands (#596).
            if let Some(r) = it.remote.as_mut() {
                (r.said_branch, r.said_commit) = parse::done_names(text, &said).unwrap_or_default();
            }
            let by = if it.crew.is_empty() { it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default() } else { it.crew.clone() };
            it.checks.push(crate::desk::model::DeskCheck { at: now, by, ..Default::default() });
            fx.checked.push(id.clone());
            it.state = "finishing".into();
            // A new DONE: last round's CHECK line is gone; this turn's (`checks`, after) is the one shown (#526).
            (it.paused, it.asks, it.check, it.check_back) = (None, None, None, false);
            it.finish = Some(crate::desk::hub::landing::finish::begun(now, it.finish.as_ref()));
            it.after = None;
            it.updated = secs(now);
            fx.items.push(id.clone());
            crate::desk::hub::lead::feed::finished(st, &id, "said", now);
            said_done.push(id.clone());
            finished = Some(id);
        }
    }
    checks(st, sid, text, now, fx);
    for id in said_done {
        check_back(st, sid, &id, fx);
    }
    finished
}

/// Item `id`'s holder `sid` just said DONE (#668): its check line comes back to it once when it gave no steps, or its
/// words aren't plain (Desk words, code names); else nothing. A CHECK said after that is kept as it is.
fn check_back(st: &mut Store, sid: &str, id: &str, fx: &mut Effects) {
    let Some(it) = st.desk.item_mut(id).filter(|i| !i.check_back) else { return };
    let Some(words) = crate::desk::check_back_words(id, it.check.as_ref()) else { return };
    it.check_back = true;
    fx.items.push(id.to_string());
    fx.sends.push((sid.to_string(), words));
}

/// Holder `sid`'s CHECK lines in `text` (#526): what you check on an item it said DONE for, in this turn or before,
/// kept on it (the last one said wins). One for an item it doesn't hold, or that isn't done by it yet, does nothing.
fn checks(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
    for line in parse::lines(text) {
        let Line::Check { id, kind, text, steps } = line else { continue };
        let id = st.desk.canon(&id);
        let Some(it) = st.desk.item_mut(&id).filter(|i| i.holder.as_ref().is_some_and(|h| h.is(sid)) && matches!(i.state.as_str(), "finishing" | "to_check")) else { continue };
        let [at, act, expect] = <[String; 3]>::try_from(steps).unwrap_or_default();
        let check = Some(crate::desk::model::Check { kind, text, at, act, expect });
        if it.check != check {
            it.check = check;
            it.updated = secs(now);
            fx.items.push(id);
        }
    }
}

/// Its DECIDED lines, as decisions on the item it's at (`finished`: the one this turn said DONE for).
fn after_dones(st: &mut Store, sid: &str, text: &str, decided: bool, finished: Option<String>, now: u64, fx: &mut Effects) {
    let calls = crate::sessions::crew_log::calls_marked(text);
    if calls.is_empty() || !decided {
        return;
    }
    // The item it's at: the one this turn said DONE (not the next, which that just started), else the one it holds
    // that changed last, at work before done.
    let held = |i: &&DeskItem| i.holder.as_ref().is_some_and(|h| h.is(sid)) && i.state != "done";
    let at = |i: &&DeskItem| finished.as_ref().map_or(i.state == "working", |f| &i.id == f);
    let Some((id, project, by)) = st.desk.items.iter().filter(held).max_by_key(|i| (at(i), i.updated)).map(|i| (i.id.clone(), i.project.clone(), i.holder.as_ref().map(|h| if h.sid == sid { h.name.clone() } else { String::new() }).unwrap_or_default())) else { return };
    let by = if by.is_empty() { st.sessions.get(sid).map(shown_name).unwrap_or_default() } else { by };
    for (what, why, minor) in calls {
        decide_call(st, &project, Some(id.clone()), &by, sid, (&what, &why, minor), false, now, fx);
    }
}

/// An item's kind as kept: "bug" | "feature" | "task", or "" (one you added: the lead says what it is as it routes
/// it); anything else is a task.
pub(crate) fn kind_of(kind: &str) -> &str {
    if matches!(kind, "bug" | "feature" | "task" | "") { kind } else { "task" }
}

pub(crate) fn new_item(st: &Store, project: &str, kind: &str, title: &str, done_when: &str, from: &str, now: u64) -> DeskItem {
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

/// Whether `line` starts as one of the Desk's lines would (ITEM:, ROUTE:, ASK:, DONE:, CHECK:, CONTEXT:, DECIDED:), after list marks
/// and bold: a cheap look before parsing.
pub(crate) fn desk_line_start(line: &str) -> bool {
    let l = parse::unmarked(line.trim().trim_start_matches('`')).trim_start_matches('`');
    let Some((word, _)) = l.split_once(':') else { return false };
    let word = word.trim_end_matches(['*', '`']).trim();
    matches!(word, "ITEM" | "ROUTE" | "ASK" | "DONE" | "NOT DONE" | "CHECK" | "CONTEXT" | "DESIGNS" | "IF PICKED" | "TITLE" | "SEE" | "SPLIT" | "CUT" | "MERGE" | "NOTE" | "FEATURE" | "CLOSE" | "TAKE COPY" | "LAND FIRST" | "COPY FIRST" | "START" | "PARK" | "HOLD" | "UNHOLD" | "TELL DESK" | "TELL DESKS" | "DISAGREE" | "UNPARK" | "DROP" | "PAUSE" | "RESUME" | "ANSWER" | "PASS" | "LANDS" | "TO LEAD" | "COPY" | "LOOP" | "MOVE" | "INSTALL") || word.eq_ignore_ascii_case("DECIDED")
}
