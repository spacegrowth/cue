//! What the Desk reads from other sessions at a turn's end (hub/desk.rs): any session's ITEM lines, design
//! lines, replies on an item, and a holder's lines (DONE, DECIDED); and a new item.

use super::*;
use crate::desk::parse::{self, Line};
use crate::desk::DeskItem;
use super::desk::{Effects, secs, shown_name};
use super::desk_route::decide_call;
use super::desk_said::item_said;

/// Session `sid` (not its project's lead) said `text`: its ITEM: lines are new items of its project, from it, which
/// the lead is told of to route. A session on an item (at work on it, or it's to check) keeps its own fixes and next
/// steps in that item: an ITEM: line from it without a `why:` (how it's separate) isn't added, and it's told so.
pub(super) fn session_items(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
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
        let Line::Item { kind, title, see, done_when, why } = line else { continue };
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

/// Session `sid` said where items' designs are (DESIGNS: #id <link>): each item of its project gets that link, a newer
/// one replacing the last (designs moved).
pub(super) fn designs_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
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
pub(super) fn replied(st: &mut Store, sid: &str, text: &str, turn: &[crate::model::TurnPart], on: &[String], now: u64, fx: &mut Effects) {
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
const SEPARATE_ITEM_WORDS: &str = "Only something truly separate from it is a new item, with why in one line: ITEM: <bug|feature|task> | <title> | you'll see: <what the user sees when it's done, plain words> | done when: <the check> | why: <how it's separate>";

/// Holder `sid` said `text` at its turn's end: items it says are done go to you to check (and what was queued after
/// them starts), and (unless it's the lead, whose DECIDED lines are the lead's) its DECIDED lines are decisions on
/// the item it holds.
pub(super) fn holder_said(st: &mut Store, sid: &str, text: &str, decided: bool, now: u64, fx: &mut Effects) {
    let mut finished: Option<String> = None;
    // What this turn made, for the Desk to link to: its web links, and its files when the session is on this Mac.
    let here = st.sessions.get(sid).is_some_and(|s| s.origin.machine.is_empty());
    // Its crew or background agents still at work: not done yet, whatever it says. The item stays at work, and once
    // they've finished, a turn of its with no DONE is asked if it's done (desk_stall).
    let busy = super::desk_stall::still_at_it(st, sid);
    for line in parse::lines(text) {
        let Line::Done { ids } = line else { continue };
        if busy {
            continue;
        }
        for id in ids {
            let id = st.desk.canon(&id);
            // At work, or finishing and stopped on a question (a word in it, a conflict, a failed build): finishing
            // (again), its commit made a moment later (desk_finish.rs), and what waits on it started once it is.
            let again = |i: &DeskItem| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| !f.question.is_empty());
            let Some(it) = st.desk.item_mut(&id).filter(|i| i.holder.as_ref().is_some_and(|h| h.is(sid)) && (matches!(i.state.as_str(), "working" | "queued") || again(i))) else { continue };
            it.made = parse::made_in(text, here);
            let by = if it.crew.is_empty() { it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default() } else { it.crew.clone() };
            it.checks.push(crate::desk::model::DeskCheck { at: now, by, ..Default::default() });
            fx.checked.push(id.clone());
            it.state = "finishing".into();
            it.paused = None;
            it.finish = Some(super::desk_finish::begun(now, it.finish.as_ref()));
            it.after = None;
            it.updated = secs(now);
            fx.items.push(id.clone());
            finished = Some(id);
        }
    }
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
        decide_call(st, &project, Some(id.clone()), &by, sid, (&what, &why, minor), now, fx);
    }
}

/// An item's kind as kept: "bug" | "feature" | "task", or "" (one you added: the lead says what it is as it routes
/// it); anything else is a task.
pub(super) fn kind_of(kind: &str) -> &str {
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
pub(super) fn desk_line_start(line: &str) -> bool {
    let l = parse::unmarked(line.trim().trim_start_matches('`')).trim_start_matches('`');
    let Some((word, _)) = l.split_once(':') else { return false };
    let word = word.trim_end_matches(['*', '`']).trim();
    matches!(word, "ITEM" | "ROUTE" | "ASK" | "DONE" | "CONTEXT" | "DESIGNS" | "TITLE" | "SEE" | "SPLIT" | "CUT" | "MERGE" | "NOTE" | "CLOSE" | "TAKE COPY" | "LAND FIRST" | "START" | "HOLD" | "PAUSE" | "RESUME" | "ANSWER" | "PASS") || word.eq_ignore_ascii_case("DECIDED")
}
