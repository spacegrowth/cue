//! What the Desk reads from its lead at a turn's end (hub/desk.rs): the lead's lines (ITEM, ROUTE, DONE, TITLE,
//! SPLIT, CUT, MERGE, KEEP…), and `desk_turn_ended`, which reads every turn's lines.

use super::*;
use crate::desk::parse::{self, Line};
use crate::desk::DeskItem;
use super::desk::{Effects, resolve, secs, shown_name};
use super::desk_route::{By, decide, decide_call, note, route, waits_on};
use super::desk_heard::{designs_said, desk_line_start, holder_said, kind_of, new_item, replied, session_items};

/// The first "#id" in `text` that's an item of `project`.
fn item_named(st: &Store, project: &str, text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter_map(|w| w.strip_prefix('#')).map(str::to_ascii_lowercase).find(|id| st.desk.item(id).is_some_and(|i| i.project == project))
}

/// An ITEM: line said by session `by` (named `from`): a new item of `project`, or the one of that kind and title
/// not done yet, with you to check too (the line said again). The item's id, and whether it's new.
#[allow(clippy::too_many_arguments)]
pub(super) fn item_said(st: &mut Store, project: &str, kind: &str, title: &str, see: &str, done_when: &str, from: &str, now: u64, fx: &mut Effects) -> (String, bool) {
    let same_kind = kind_of(kind);
    let open = |i: &&DeskItem| i.project == project && i.kind == same_kind && i.state != "done" && i.title.trim().eq_ignore_ascii_case(title.trim());
    if let Some(open) = st.desk.items.iter().find(open) {
        return (open.id.clone(), false);
    }
    let mut it = new_item(st, project, kind, title, done_when, from, now);
    it.see = see.trim().into();
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
    // What the lead is told of its routes, in one message once its lines are read (#248).
    let mut answers: Vec<String> = Vec::new();
    for line in parse::lines(text) {
        match line {
            Line::Item { kind, title, see, done_when, .. } => last_new = Some(item_said(st, project, &kind, &title, &see, &done_when, &name, now, fx).0),
            Line::Route { id, to, after, why, kind, .. } => {
                // An old tag ("#d7k2") names the item by its number from now.
                let id = if id == "new" { last_new.clone() } else { Some(st.desk.canon(&id)) };
                let after = if after.as_deref() == Some("new") { last_new.clone() } else { after.map(|a| st.desk.canon(&a)) };
                let Some(id) = id else {
                    answers.push("Refused: ROUTE #new has no ITEM line before it in this reply. You can add the ITEM: line first, or route the item by its number.".into());
                    continue;
                };
                // The lead says what it is as it routes it (before, so whoever takes it is told).
                if let (Some(k), Some(it)) = (kind, st.desk.item_mut(&id).filter(|i| i.project == project)) {
                    it.kind = k;
                    fx.items.push(id.clone());
                }
                // A route made isn't answered: the lead knows what it said, and each answer costs it a turn.
                if let Some(said) = route(st, project, &by, &id, &to, after, &why, now, fx).filter(|s| !s.starts_with("Taken:")) {
                    answers.push(said);
                }
            }
            Line::Ask { question, choices, item } => {
                // About an item of this Desk's: it shows on that item as waiting on you (`shown`).
                let item = item.map(|i| st.desk.canon(&i)).filter(|i| st.desk.item(i).is_some_and(|x| x.project == project));
                fx.asks.push((lsid.to_string(), project.to_string(), question, choices, item))
            }
            Line::Context { who, what, why } if what == "keep" => keep(st, project, &by, &who, &why, now, fx),
            Line::Done { ids } => lead_done(st, project, lsid, &ids, fx),
            Line::Title { id, title } => retitle(st, project, &by, &id, &title, now, fx),
            Line::See { id, see, done_when } => {
                let id = st.desk.canon(&id);
                if let Err(no) = see_said(st, project, &id, &see, &done_when, now, fx) {
                    answers.push(no);
                }
            }
            Line::Split { id, titles } => split(st, project, &by, &id, &titles, now, fx),
            Line::Cut { id, on, why } => cut(st, project, &by, &id, &on, &why, now, fx),
            Line::Merge { id, into, why } => merge(st, project, &by, &id, &into, &why, now, fx),
            Line::Note { id, text } => {
                let id = st.desk.canon(&id);
                if let Err(no) = note(st, project, &by, &id, &text, now, fx) {
                    answers.push(no);
                }
            }
            Line::Unstick { what, id, commit, why } => {
                // HOLD: #new holds the reply's last ITEM (#385).
                let id = match last_new.clone().filter(|_| id == "new") { Some(n) => n, None => st.desk.canon(&id) };
                if let Err(no) = super::desk_unstick::unstick(st, project, &by, &what, &id, &commit, &why, now, fx) {
                    answers.push(no);
                }
            }
            Line::Feature { id, name, pieces } => {
                if let Err(no) = super::desk_features::feature_said(st, project, &by, id, &name, &pieces, last_new.as_deref(), now, fx) {
                    answers.push(no);
                }
            }
            Line::Answer { id, text } => cleanup_said(st, project, &by, &id, Some(text), now, fx, &mut answers),
            Line::Pause { on, why } => fx.pause.push((project.to_string(), on, name.clone(), lsid.to_string(), why)),
            Line::Pass { id, .. } => cleanup_said(st, project, &by, &id, None, now, fx, &mut answers),
            Line::Context { .. } | Line::Designs { .. } => {}
        }
    }
    for line in parse::unread_routes(text) {
        answers.push(format!("Couldn't read this line as a route: {line}\nWrite it as ROUTE: #<id> -> <session> | why: <reason> ({}).", parse::route_fix(&line)));
    }
    route_answers(st, project, lsid, answers, fx);
    for (what, why, minor) in crate::sessions::crew_log::calls_marked(text) {
        let item = item_named(st, project, &what).or_else(|| item_named(st, project, &why));
        decide_call(st, project, item, &name, lsid, (&what, &why, minor), now, fx);
    }
}

/// The lead's ANSWER (Some) or PASS (None) on item `id`'s landing clean-up question (desk_cleanup.rs); told when
/// none waits on it.
#[allow(clippy::too_many_arguments)]
fn cleanup_said(st: &mut Store, project: &str, by: &By, id: &str, answer: Option<String>, now: u64, fx: &mut Effects, told: &mut Vec<String>) {
    let id = st.desk.canon(id);
    match super::desk_cleanup::card_for(st, project, &id) {
        Some(card) => {
            // Answered, it's one of the things the lead did on its own (#360): a step on the item.
            if let Some(a) = &answer {
                decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Answered #{id}'s session: {a}"), "", now, fx);
                if let Some(d) = st.desk.decisions.last_mut() {
                    d.undo = "none".into();
                    d.act = "answer".into();
                }
            }
            fx.cleanup.push((card, answer));
        }
        None => told.push(format!("No question from #{id} waits on you: nothing to answer or pass.")),
    }
}

/// The lead cut a link in a circle (CUT:): item `id` no longer waits on item `on` (desk_waits.rs). Its commit, when
/// that's what waited, lands at the next finishing tick. Noted as its decision; the lead told when there's no such
/// link.
#[allow(clippy::too_many_arguments)]
fn cut(st: &mut Store, project: &str, by: &By, id: &str, on: &str, why: &str, now: u64, fx: &mut Effects) {
    let id = st.desk.canon(id);
    if !st.desk.item(&id).is_some_and(|i| i.project == project) {
        fx.sends.push((by.sid.to_string(), format!("#{id} not cut: this Desk has no item #{id}")));
        return;
    }
    match super::desk_waits::cut_in(st, &id, on, now) {
        Ok(done) => {
            fx.items.extend(done.items);
            fx.sends.extend(done.sends);
            fx.news.extend(done.news);
            decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Cut its wait on #{}", st.desk.canon(on)), why, now, fx);
        }
        Err(e) => fx.sends.push((by.sid.to_string(), format!("Not cut: {e}"))),
    }
}

/// The lead folded item `id`, a repeat of item `into`, into it (MERGE:): its words and pictures are added to `into`'s
/// (its holder, at work on it, told them), anything queued after it waits on `into` instead, and it leaves the list.
/// Only one not begun: no session is started for it (one at work on it, or to check, stays; the lead is told why).
#[allow(clippy::too_many_arguments)]
fn merge(st: &mut Store, project: &str, by: &By, id: &str, into: &str, why: &str, now: u64, fx: &mut Effects) {
    let (id, into) = (st.desk.canon(id), st.desk.canon(into));
    let note = |fx: &mut Effects, why: String| fx.sends.push((by.sid.to_string(), format!("#{id} not merged: {why}")));
    if id == into {
        return;
    }
    let Some(copy) = st.desk.item(&id).filter(|i| i.project == project).cloned() else { return note(fx, format!("this Desk has no item #{id}")) };
    let Some(target) = st.desk.item(&into).filter(|i| i.project == project).cloned() else { return note(fx, format!("this Desk has no item #{into}")) };
    if !matches!(copy.state.as_str(), "new" | "queued") {
        let who = copy.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        return note(fx, if copy.state == "done" { "it's done already".into() } else { format!("{who} has begun it already") });
    }
    if target.state == "done" {
        return note(fx, format!("#{into} is done; route #{id} instead"));
    }
    if waits_on(st, &into, &id) {
        return note(fx, format!("#{into} waits on it; CUT that wait first"));
    }
    let said = if copy.words.is_empty() { copy.title.clone() } else { copy.words.clone() };
    let added: Vec<String> = copy.images.iter().filter(|p| !target.images.contains(p)).cloned().collect();
    if let Some(t) = st.desk.item_mut(&into) {
        if t.words.is_empty() {
            t.words = t.title.clone();
        }
        t.words = format!("{}\n\nFrom #{id}: {said}", t.words.trim_end());
        t.images.extend(added.iter().cloned());
        t.updated = secs(now);
    }
    // Its holder at work on it hears what came with the repeat.
    if let Some(h) = target.holder.as_ref().filter(|_| target.state == "working") {
        let pics = added.iter().map(|p| format!(" `{p}`")).collect::<String>();
        fx.sends.push((h.to().to_string(), format!("Desk item #{id} was folded into #{into}, which you have. It said: {said}{}", if pics.is_empty() { String::new() } else { format!("\nPictures:{pics}") })));
    }
    for it in st.desk.items.iter_mut().filter(|i| i.after.as_deref() == Some(id.as_str())) {
        it.after = Some(into.clone());
        it.updated = secs(now);
        fx.items.push(it.id.clone());
    }
    st.desk.items.retain(|i| i.id != id);
    fx.dropped.push(id.clone());
    fx.items.push(into.clone());
    decide(st, project, Some(into.clone()), by.name, by.sid, &format!("Folded #{id} into it"), why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
}

/// What lead `lsid` of `project` is told of the routes of the turn that just ended (#248), in one message: each
/// refused route, and each line Cue couldn't read (once, when said twice). A repeat said again later is answered
/// again: the lead is never left thinking a refused route was made. Not "Asked:" (the user's card is in the lead's
/// name, and it hears the answer). With the lead asking the user something (a card from this turn, or before), a
/// message can't reach it: it's told in its next Desk block instead (`routes_held`).
fn route_answers(st: &mut Store, project: &str, lsid: &str, answers: Vec<String>, fx: &mut Effects) {
    let mut out: Vec<String> = Vec::new();
    for said in answers.into_iter().filter(|s| !s.starts_with("Asked:")) {
        if !out.contains(&said) {
            out.push(said);
        }
    }
    if out.is_empty() || st.sessions.get(lsid).is_none() {
        return;
    }
    let asking = fx.uses.iter().any(|u| u.lead == lsid)
        || fx.asks.iter().any(|a| a.0 == lsid)
        || st.sessions.state(lsid).as_deref() == Some("deciding");
    if asking {
        let held = st.desk.routes_held.entry(project.to_string()).or_default();
        for line in out {
            held.push_str(&line);
            held.push('\n');
        }
    } else {
        fx.sends.push((lsid.to_string(), out.join("\n")));
    }
}

/// The lead gave item `id` a short plain title (TITLE:): the words it was added with are kept (`words`), shown on its
/// page. Not for a done item.
fn retitle(st: &mut Store, project: &str, by: &By, id: &str, title: &str, now: u64, fx: &mut Effects) {
    let id = st.desk.canon(id);
    let Some(it) = st.desk.item_mut(&id).filter(|i| i.project == project && i.state != "done") else { return };
    if it.title == title.trim() {
        return;
    }
    if it.words.is_empty() {
        it.words = it.title.clone();
    }
    it.title = title.trim().to_string();
    it.updated = secs(now);
    fx.items.push(id.clone());
    decide(st, project, Some(id), by.name, by.sid, &format!("Named it \"{}\"", title.trim()), "", now, fx);
}

/// The lead gave item `id` its two lines (SEE:, #382): what you'll see when it's done, and the check its session runs.
/// Either may be left out to keep what it has. Told when there's no such item.
fn see_said(st: &mut Store, project: &str, id: &str, see: &str, done_when: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let it = st.desk.item_mut(id).filter(|i| i.project == project).ok_or_else(|| format!("Refused: SEE #{id}: this Desk has no item #{id}."))?;
    let (see, done_when) = (see.trim(), done_when.trim());
    let mut changed = false;
    if !see.is_empty() && it.see != see {
        it.see = see.into();
        changed = true;
    }
    if !done_when.is_empty() && it.done_when != done_when {
        it.done_when = done_when.into();
        changed = true;
    }
    if changed {
        it.updated = secs(now);
        fx.items.push(id.to_string());
    }
    Ok(())
}

/// The lead split item `id` (SPLIT:): it takes the first title, and each of the rest becomes a new item of the same
/// kind, with the same words and pictures, from whoever added it; the lead is told their numbers to route them. Only
/// one not given out yet: work begun on it stays one item (the lead is told why).
#[allow(clippy::too_many_arguments)]
fn split(st: &mut Store, project: &str, by: &By, id: &str, titles: &[String], now: u64, fx: &mut Effects) {
    let id = st.desk.canon(id);
    let Some(it) = st.desk.item(&id).filter(|i| i.project == project).cloned() else {
        fx.sends.push((by.sid.to_string(), format!("#{id} not split: this Desk has no item #{id}")));
        return;
    };
    if it.holder.is_some() || it.state != "new" {
        let who = it.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        let why = if it.state == "done" { "it's done already".to_string() } else { format!("{who} has it already; add the other part as an ITEM") };
        fx.sends.push((by.sid.to_string(), format!("#{id} not split: {why}")));
        return;
    }
    let words = if it.words.is_empty() { it.title.clone() } else { it.words.clone() };
    let mut ids = vec![];
    for t in &titles[1..] {
        let mut n = new_item(st, project, &it.kind, t, "", &it.from, now);
        n.words = words.clone();
        n.images = it.images.clone();
        n.split_from = id.clone();
        ids.push(n.id.clone());
        fx.items.push(n.id.clone());
        fx.news.push((project.to_string(), n.id.clone()));
        st.desk.items.push(n);
    }
    retitle(st, project, by, &id, &titles[0], now, fx);
    if let Some(o) = st.desk.item_mut(&id) {
        o.words = words;
        // Told again with its new title (with the new ones), to route it with them.
        fx.news.push((project.to_string(), id.clone()));
    }
    let list = ids.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ");
    decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Split off {list}"), "", now, fx);
    for n in &ids {
        decide(st, project, Some(n.clone()), by.name, by.sid, &format!("Split from #{id}"), "", now, fx);
    }
}

/// The lead said DONE on `ids`. Its DONE sends to check what it holds itself at work (holder_said), and closes for
/// you, as Looks right would, an item finishing or to check (#337: you told it it's done): at once when it's to check,
/// else once its finishing brings it there, its commit and install still made. For any other item Cue tells it, in
/// one short message shown in its chat, why the item stays as it is ("#69 not closed: Ann has it"), so nothing it
/// says is dropped without a word. Once while the item stays as it is.
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
        if matches!(it.state.as_str(), "to_check" | "finishing") {
            if let Some(it) = st.desk.item_mut(&id) {
                it.close_on_check = true;
            }
            fx.items.push(id.clone());
            fx.closes.push(id);
            continue;
        }
        // Told once while it stays as it is: a change to it (its state, a check, a route) lets the next DONE be told.
        let seen = format!("{}@{}", it.state, it.updated);
        if st.desk.done_told.get(&id) == Some(&seen) {
            continue;
        }
        let holder = it.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        let why = match it.state.as_str() {
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
const DONE_NOTE_FOR_LEAD: &str = "Say DONE for an item only once it's finishing or waiting on the user's check. Don't say DONE for these again.";

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

impl Hub {
    /// Session `sid`'s turn ended, saying `text`: a lead's lines (ITEM, ROUTE, ASK, DECIDED, and DONE for what it
    /// keeps), any session's ITEM lines, and a holder's (DONE, DECIDED) acted on; an item it ended on with no DONE
    /// is asked about once (desk_stall.rs).
    /// `turn`: its parts, so a reply keeps what a Stop hook made it write apart from its answer.
    pub(super) fn desk_turn_ended(&self, sid: &str, text: &str, turn: &[crate::model::TurnPart]) {
        self.keep_from_now(sid);
        // Away mode ends by itself (its end time, its budget): noticed as a Desk session's turn ends.
        self.desk_away_check(sid);
        // Most turns say none of the Desk's lines and end on no item at work: then the store is only read.
        // A route in the wrong form counts too: the lead is told it couldn't be read (#248).
        let said = text.lines().any(|l| desk_line_start(l) || parse::route_like(l));
        let held = self.read(|st| super::desk_stall::at_work_with(st, sid));
        let done_held = self.read(|st| st.desk.done_held.contains_key(sid));
        if !said && held.is_empty() && !done_held {
            return;
        }
        // Each item at work with it: its work git says isn't committed yet, asked with the store let go (#213).
        let loose: Vec<(String, Vec<String>)> = held.iter().map(|id| (id.clone(), self.desk_uncommitted(id))).collect();
        let from_disk = self.feature_pieces_on_disk(text);
        let now = now_ms();
        let fx = self.write(|st| {
            super::desk_features::bring_back(st, from_disk);
            let mut fx = Effects::default();
            // A DONE it said while its crew or agents worked: they're done now, so it goes ahead (#393).
            super::desk_heard::held_done(st, sid, now, &mut fx);
            for (id, files) in &loose {
                if let Some(it) = st.desk.item_mut(id) {
                    it.loose = files.clone();
                }
            }
            // What it was at work on as its turn ended: what this turn's DONE lines don't close stalled there.
            // Its reply goes only to the items it's tagged with (#176): the one the message it answers names, and
            // those its DONE lines name; untagged, to none. A turn that ends untagged with one item at work still
            // stalls there: Cue's ask names the item, so the answer to it is tagged.
            let held = super::desk_stall::at_work_with(st, sid);
            let mut on = super::desk_stall::about(st, text, &held, super::desk_stall::now_on(st, sid).as_deref());
            replied(st, sid, text, turn, &on, now, &mut fx);
            if on.is_empty() && held.len() == 1 {
                on = held;
            }
            if !said {
                super::desk_stall::stalls_asked(st, sid, text, &on, now, &mut fx);
                return fx;
            }
            designs_said(st, sid, text, &mut fx);
            let lead = st.desk.lead_of(sid);
            match &lead {
                Some(project) => lead_said(st, project, sid, text, now, &mut fx),
                None => session_items(st, sid, text, now, &mut fx),
            }
            if st.desk.items.iter().any(|i| i.holder.as_ref().is_some_and(|h| h.is(sid))) {
                holder_said(st, sid, text, lead.is_none(), now, &mut fx);
            }
            super::desk_stall::stalls_asked(st, sid, text, &on, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Session `sid`'s background agents have finished and it's at its prompt: a DONE it said while they worked goes
    /// ahead now (#393). Only read unless there's one.
    pub(super) fn desk_helpers_done(&self, sid: &str) {
        if !self.read(|st| st.desk.done_held.contains_key(sid)) {
            return;
        }
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            super::desk_heard::held_done(st, sid, now, &mut fx);
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
}
