//! How a Desk item is shown (desk/hub/core/desk.rs): its holder as it is now, its files and sources, when its
//! holder held it, a page of done items, and `state.desk`.

use crate::desk::hub::prelude::*;
use crate::desk::{DeskDecision, DeskItem, Holder};
use crate::sessions::Session;
use crate::desk::hub::core::desk::{ask_of, ask_view, at_work, crew_parts, last_words, secs, shown_name};

/// `it` as `shown`, with its pieces when it's a feature (#282).
pub(crate) fn with_pieces(st: &Store, it: &DeskItem) -> DeskItem {
    let mut out = shown(st, it);
    out.pieces = crate::desk::hub::turn::features::pieces_shown(st, it, |p| shown(st, p));
    out
}

/// `it` as the window shows it: its holder as it is now (name, lead or not), waiting on you when its holder is,
/// with its holder's latest and files.
pub(crate) fn shown(st: &Store, it: &DeskItem) -> DeskItem {
    let mut out = it.clone();
    out.stints = stints(st, it);
    // Waiting for a free copy (#401): which items are using them.
    if it.tree_wait && it.state == "queued" {
        out.copy_users = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == it.tree_wait_repo)).map(|i| i.id.clone()).collect();
        out.copy_users.sort_by_key(|x| (x.parse::<u64>().unwrap_or(u64::MAX), x.clone()));
    }
    let Some(h) = it.holder.as_ref() else { return out };
    let mut h = h.clone();
    holder_view(st, it, &mut h, &mut out);
    at_work_view(st, it, &h, &mut out);
    asks_view(st, it, &h, &mut out);
    stops_view(st, it, &h, &mut out);
    out.holder = Some(h);
    out
}

/// Its holder as shown: a teammate's or a session's name, and what it's at on this item (its latest, its files).
fn holder_view(st: &Store, it: &DeskItem, h: &mut Holder, out: &mut DeskItem) {
    if let Some((lead, agent)) = crew_parts(h) {
        out.now_on = it.state == "working";
        if let Some(t) = st.sessions.get(lead).and_then(|s| s.team.iter().find(|t| t.agent_id == agent || t.call == agent)) {
            h.name = t.name.clone();
            out.latest = t.step.clone();
            out.files = inside(t.files.iter());
        }
    } else if let Some(s) = st.sessions.get(&h.sid) {
        h.name = shown_name(s);
        h.kind = if s.team.is_empty() { "session".into() } else { "lead".into() };
        // Its words and its work are the item's only while it's on this one (#176): it can hold several.
        out.now_on = it.state == "working" && crate::desk::hub::stall::stall::now_on(st, &h.sid).as_deref() == Some(it.id.as_str());
        out.latest = if !out.now_on { String::new() } else if s.doing.is_empty() { last_words(s) } else { s.doing.clone() };
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
}

/// The item at work, by what its holder is doing on it: an ask, a stop, or its crew at work.
fn at_work_view(st: &Store, it: &DeskItem, h: &Holder, out: &mut DeskItem) {
    // Only the item at work: a queued one's holder asks about the item before it. Waiting on you while its holder
    // asks, or while its holder's turn ended on it (with no DONE, or it would be to check): its last line then.
    // Waiting on you only with one plain thing to do (#182): its ask (Answer, Pick, Allow), or the question its turn
    // ended on (Answer). A stop with nothing to ask you goes to the Desk lead to settle: shown as stopped, not red,
    // until the lead asks you about it (ASK: #id …) or its time is up (desk/hub/stall/stall.rs).
    if it.state == "working" && out.now_on {
        if let Some(a) = ask_of(st, h) {
            out.state = "needs_you".into();
            out.ask = Some(ask_view(a));
        } else if let Some(s) = crate::desk::hub::stall::stall::stalled(st, it, h) {
            match crate::desk::hub::stall::stall::stop_shown(st, it, s, now_ms()) {
                crate::desk::hub::stall::stall::Stop::Asks(q) => {
                    out.state = "needs_you".into();
                    out.latest = q;
                }
                crate::desk::hub::stall::stall::Stop::Waits(w) => {
                    out.waiting = w.clone();
                    out.latest = w;
                }
                crate::desk::hub::stall::stall::Stop::Lead => {
                    out.stopped = true;
                    out.latest = match it.loose.is_empty() {
                        true => crate::desk::hub::stall::stall::last_line(s),
                        false => crate::desk::hub::stall::stall::not_committed(&it.loose),
                    };
                }
            }
        } else if let Some(crew) = crew_parts(h).is_none().then(|| crate::desk::hub::stall::stall::runs_on(st, &h.sid)).flatten() {
            // Idle only because its own teammates, or its tests or build, are at work (#511, #556): at work on it, and
            // it says so.
            out.latest = crew;
        }
    }
}

/// Holder `h` (a session of its own) is at work on a turn it began after `at` (ms): past what it asked then (#559).
pub(crate) fn carried_on(st: &Store, h: &Holder, at: u64) -> bool {
    crew_parts(h).is_none() && st.sessions.get(&h.sid).is_some_and(|s| matches!(s.state.as_str(), "working" | "agent") && s.since_ms > at)
}

/// Asks that make it wait on you: its holder's question (#475), a queued item's ask, the lead's ASK.
fn asks_view(st: &Store, it: &DeskItem, h: &Holder, out: &mut DeskItem) {
    // Its holder asked you something (#475): waiting on you with that while it's live (#559). Not once its holder
    // is at work on a newer turn: it carried on past it, and that turn's end asks again if it still needs you.
    let live = it.asks.as_ref().filter(|(_, at)| it.state == "working" && !carried_on(st, h, *at));
    if let Some((q, _)) = live.filter(|_| it.paused.is_none() && out.ask.is_none()) {
        out.state = "needs_you".into();
        (out.latest, out.stopped, out.act) = (q.clone(), false, None);
        out.waiting.clear();
    }
    if live.is_none() {
        out.asks = None;
    }
    // Queued, but its holder is on it anyway (#342): its ask, or the question its turn ended on, waits on you here
    // too, not only in its own chat. A stop with no question stays the lead's (it's queued, not stalled). Not once you
    // sent it back and it waits for a free working folder (#419): your Send back is the answer, it goes with it.
    let back_waits = it.tree_wait && !it.back_waiting.is_empty();
    if it.state == "queued" && !back_waits && crew_parts(h).is_none() && crate::desk::hub::stall::stall::on_queued(st, &h.sid, &it.id) {
        if let Some(a) = ask_of(st, h) {
            out.state = "needs_you".into();
            out.ask = Some(ask_view(a));
        } else if let Some(q) = crate::desk::hub::stall::stall::stalled(st, it, h).and_then(|s| s.thread.iter().rev().find(|e| e.role == "agent")).and_then(|e| crate::desk::hub::stall::stall::question_in(&e.text)) {
            out.state = "needs_you".into();
            out.latest = q;
        }
    }
    // The lead's ASK about this item: it waits on you with it.
    if matches!(it.state.as_str(), "new" | "queued" | "working" | "finishing") && out.ask.is_none() {
        if let Some(a) = st.items.iter().find(|i| i.desk && i.status == "pending" && i.tool_input["desk_item"].as_str() == Some(it.id.as_str())) {
            out.state = "needs_you".into();
            out.stopped = false;
            out.ask = Some(ask_view(a));
        }
    }
}

/// Finishing and stopped, paused by the lead, or to resume; and what its holder did last while at work.
fn stops_view(st: &Store, it: &DeskItem, h: &Holder, out: &mut DeskItem) {
    // Finishing and stopped (a word, a conflict, a long wait, a failed install): stopped while its Desk lead has it
    // (#483), else it needs you; the lead's ASK about it waits on you whatever.
    if let Some(f) = it.finish.as_ref().filter(|f| it.state == "finishing" && !f.question.is_empty()) {
        out.latest = f.question.clone();
        if f.lead && crate::desk::hub::stall::stall::lead_live(st, &it.project) && out.ask.is_none() {
            out.stopped = true;
        } else if out.ask.is_none() {
            out.state = "needs_you".into();
        }
    }
    // Paused by the lead (#365): it keeps its place, its holder stopped (or stopping) at a clean point; not stopped,
    // waiting or to resume, and nothing for you unless something asks you (its holder's ask, the lead's ASK).
    if it.state == "working" && it.paused.is_some() && out.ask.is_none() {
        out.state = "paused".into();
        out.stopped = false;
        out.waiting.clear();
    }
    if out.state == "working" && !out.stopped && out.waiting.is_empty() {
        // Work handed to a live session is under way while it takes the message, or while its crew finishes.
        let active = if crew_parts(h).is_some() {
            at_work(st, h)
        } else {
            // Or a note of yours waits for it, kept in Cue or queued in its box (#419): it goes in next.
            crate::desk::hub::stall::stall::still_at_it(st, &h.sid) || (out.now_on && st.sessions.get(&h.sid).is_some_and(|s| {
                s.state != "stopped" && (s.held.is_some() || !s.running.is_empty() || s.queued.is_some() || s.thread.last().is_some_and(|e| e.role == "you" && e.pending))
            }))
        };
        if !active && !wrote_just_now(st, h) {
            out.state = "needs_resume".into();
            if !it.restart.is_empty() {
                out.latest = it.restart.clone();
            }
        }
    }
    // At work on it: what its holder did last, and when, so its row shows whether it's moving or stalled (#285).
    if out.state == "working" && out.now_on && !out.stopped && out.waiting.is_empty() && out.ask.is_none() {
        out.act = crate::desk::hub::stall::stall::act_of(st, h);
    }
}

/// Holder `h`, a session on this Mac, wrote to its conversation within the last minute: at work, whatever its state
/// says (#291). As of the last second's look (`holder_logs`): the file isn't read here, under the store's lock.
fn wrote_just_now(st: &Store, h: &Holder) -> bool {
    st.holder_logs.get(&h.sid).is_some_and(|&at| crate::hub::activity::wrote_lately(at, now_ms()))
}

/// The conversations to look at for `wrote_just_now`: each open item's holder on this Mac (session, its log).
pub(crate) fn holder_logs(st: &Store) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for h in st.desk.items.iter().filter(|i| i.state != "done").filter_map(|i| i.holder.as_ref()) {
        let Some(s) = st.sessions.get(&h.sid).filter(|s| s.origin.machine.is_empty() && !s.origin.transcript_path.is_empty()) else { continue };
        if !out.iter().any(|(sid, _)| sid == &h.sid) {
            out.push((h.sid.clone(), s.origin.transcript_path.clone()));
        }
    }
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
            let is_t = |a: &str| !a.is_empty() && (a == t.agent_id || a == t.call);
            // Given to it as a crew member once, though the item has since moved on (#302): still that item's.
            o.id != it.id && o.held_before.iter().any(|(by, _, _)| is_t(by))
                || o.holder.as_ref().is_some_and(|h| crew_parts(h).is_some_and(|(_, a)| is_t(a)) || (h.sid == *sid && named))
        })
    };
    s.team.iter().filter(|t| t.started_ms >= from && t.started_ms < until && !holds(t)).collect()
}

/// When `it` was session `sid`'s to work on (ms): from when it went to it until its next item did (u64::MAX while
/// none has), counting an item that went to it and has since moved on (#302). None when another of its items was at
/// work then as well (whose work is whose can't be told), or the item has no start for `sid`.
pub(crate) fn held_window(st: &Store, sid: &str, it: &DeskItem) -> Option<(u64, u64)> {
    let (_, from) = it.held_from.as_ref().filter(|(by, _)| by == sid)?;
    let mut until = u64::MAX;
    for o in st.desk.items.iter().filter(|o| o.id != it.id) {
        if let Some(at) = o.held_before.iter().filter(|(by, at, _)| by == sid && at > from).map(|(_, at, _)| *at).min() {
            until = until.min(at);
        }
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

/// Item `it`'s stints, oldest first (#575): each session it went to at work, from then until that session's next item
/// went to it at work, the item went to another holder, or you said it looks right (whichever came first; open while
/// none has), and the item it moved on to. Its page draws that session's chat for those times only.
pub(crate) fn stints(st: &Store, it: &DeskItem) -> Vec<crate::desk::Stint> {
    let own = it.held_before.iter().map(|(by, from, until)| (by, *from, Some(*until))).chain(it.held_from.iter().map(|(by, from)| (by, *from, None)));
    let checked = (it.state == "done").then(|| it.done_at.map(|s| s * 1000)).flatten();
    own.map(|(sid, from, until)| {
        // The first item the same session went on to after this one, and when.
        let next = st.desk.items.iter().filter(|o| o.id != it.id).flat_map(|o| {
            o.held_before.iter().map(|(by, at, _)| (by, *at)).chain(o.held_from.iter().map(|(by, at)| (by, *at))).filter(|(by, at)| *by == sid && *at > from).map(move |(_, at)| (at, o.id.as_str()))
        }).min();
        let ends = [until, next.map(|(at, _)| at), checked.filter(|&c| c > from)];
        let end = ends.iter().flatten().min().copied();
        let next = next.filter(|(at, _)| Some(*at) == end).map_or(String::new(), |(_, id)| id.to_string());
        crate::desk::Stint { sid: sid.clone(), from, until: end, next }
    }).collect()
}

/// Where `it`'s files (as `shown` gives them) were changed: the machine, and each folder with its files in it (paths
/// relative to that folder), for Looks right's commit (desk/hub/landing/commit.rs).
pub(crate) fn item_sources(st: &Store, it: &DeskItem) -> (String, Vec<(String, Vec<String>)>) {
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

impl Hub {
    /// Item `id` as the window shows it, with when its holder last wrote looked at first (off the store's lock).
    pub(crate) fn item_shown(&self, id: &str) -> Option<DeskItem> {
        self.look_at_holder_logs(now_ms());
        self.read(|st| st.desk.item(id).map(|i| shown(st, i)))
    }

    /// How work lands on Desk `desk`, saved from the window's setting (desk/lands.rs), for the folder the Desk works in.
    /// Kept per Desk, the key its items land by (`it.project`), so a Desk on another repository never shares Cue's
    /// own (#460). Returns it as the Desk shows it.
    pub(crate) fn desk_lands_set(&self, desk: &str, lands: &Value) -> Result<Value, String> {
        let folder = self.read(|st| desk_folder(st, desk));
        crate::desk::lands::save(desk, crate::desk::lands::place_of(&folder), lands)?;
        self.changed();
        Ok(crate::desk::lands::desk_view(desk, &folder))
    }

    /// `project`'s done items older than `before` (updated ms, id), newest first, `take` of them, as the Desk shows
    /// them: the ones Cue holds as it holds them, the rest as saved. The list loads them as you scroll.
    pub(crate) fn desk_done_page(&self, project: &str, before: Option<(u64, &str)>, take: u32) -> Vec<DeskItem> {
        let saved = crate::db::desk_done_page(project, before, take.min(100));
        // A feature done long ago: its pieces from the disk (#282), read before the store is.
        let pieces: std::collections::HashMap<String, Vec<DeskItem>> = saved.iter().filter(|i| i.kind == "feature").map(|i| (i.id.clone(), crate::db::desk_pieces(&i.id))).collect();
        self.read(|st| saved.iter().map(|i| {
            let mut out = with_pieces(st, st.desk.item(&i.id).filter(|x| x.state == "done").unwrap_or(i));
            if out.pieces.is_empty() {
                out.pieces = pieces.get(&i.id).into_iter().flatten().map(|p| shown(st, st.desk.item(&p.id).unwrap_or(p))).collect();
                out.pieces.sort_by_key(|p| (p.id.parse::<u64>().unwrap_or(u64::MAX), p.created));
            }
            out
        }).collect())
    }

    /// `project`'s lead's own moves (#374) older than `before` (ms, id), newest first, `take` of them: the "What the
    /// lead moved" list loads them as you scroll (today's come with `decisions`).
    pub(crate) fn desk_moves_page(&self, project: &str, before: Option<(u64, &str)>, take: u32) -> Vec<DeskDecision> {
        crate::db::desk_moves_page(project, before, take.min(100))
    }

    /// `project`'s "What the lead moved" list cleared: it shows only the moves after now.
    pub(crate) fn desk_moves_clear(&self, project: &str) -> Result<(), String> {
        let now = crate::clock::now_ms();
        crate::db::desk_moves_clear(project, now)?;
        self.write(|st| st.desk.moves_cleared.insert(project.to_string(), now));
        self.changed();
        Ok(())
    }

    /// `state.desk`: each project with items or a lead, its lead ({ sid, name, state: live | parked | ended }), its
    /// items (not done, or done today) and today's decisions, newest first.
    pub(crate) fn desk_view(st: &Store, now: u64) -> Value {
        let today = secs(crate::hub::today::local_start(now, false));
        let mut projects: Vec<String> = st.desk.spaces.keys().cloned().chain(st.desk.items.iter().map(|i| i.project.clone())).chain(st.desk.lead.keys().cloned()).chain(st.desk.members.iter().filter(|(_, m)| !m.is_empty()).map(|(p, _)| p.clone())).collect();
        projects.sort();
        projects.dedup();
        let list: Vec<Value> = projects
            .iter()
            .map(|p| {
                let items: Vec<DeskItem> = st.desk.items.iter().filter(|i| &i.project == p && (i.state != "done" || i.done_at.unwrap_or(i.updated) >= today)).map(|i| with_pieces(st, i)).collect();
                // While Away is on, the decisions since it started too (an overnight Away keeps them).
                let from = crate::desk::hub::away::away::away_since(p).map_or(today, |since| today.min(secs(since)));
                let decisions: Vec<&DeskDecision> = st.desk.decisions.iter().rev().filter(|d| &d.project == p && d.at >= from).collect();
                // Older steps and calls on the items shown, for their pages' Thread (today's are in `decisions`).
                let older: Vec<&DeskDecision> = st.desk.decisions.iter().filter(|d| &d.project == p && d.at < from && d.item_id.as_ref().is_some_and(|i| items.iter().any(|it| &it.id == i))).collect();
                let members = st.desk.members.get(p).cloned().unwrap_or_default();
                let work_history = crate::desk::hub::core::members::work_history(st, p);
                // The sessions its lists show (#363); the rest fold into one "older" line.
                let with_work = crate::desk::hub::core::members::with_work(st, p, now);
                json!({ "with_work": with_work, "work_history": work_history, "id": p, "project": p, "name": st.desk.spaces.get(p).map(|d| &d.name), "independent": st.desk.spaces.contains_key(p), "workspace_project": st.desk.project_of(p), "folder": st.desk.spaces.get(p).map(|d| &d.folder), "lead": Self::desk_lead_view(st, p), "items": items, "decisions": decisions, "older": older, "moves_cleared": st.desk.moves_cleared.get(p).copied().unwrap_or(0), "members": members, "checks": crate::desk::hub::lead::checks::checks_view(st, p), "kept": st.desk.kept.iter().filter(|k| &k.project == p).collect::<Vec<_>>(), "waits": crate::desk::hub::core::waits::waits_view(st, p), "circles": crate::desk::hub::core::waits::circles_view(st, p), "ask_before": crate::desk::one_way::desk_view(st.desk.project_of(p)), "away": crate::desk::hub::away::away::away_view(st, p, now), "held": crate::desk::hub::away::away_holds::held_view(st, p), "away_log": crate::desk::hub::away::away_log::log_view(st, p), "paused": crate::desk::hub::away::pause::pause_view(p), "lands": crate::desk::lands::desk_view(p, &desk_folder(st, p)), "holds": crate::desk::holds::holds_view(st, p) })
            })
            .collect();
        json!({ "projects": list, "installed_ms": crate::desk::hub::lead::checks::installed_ms(), "install_at": crate::desk::hub::landing::install::install_at(st) })
    }
}

/// The folder Desk `desk` works in: its own (an independent Desk), else its lead's. "" for a lead on another
/// machine: its folder is never looked at here.
pub(crate) fn desk_folder(st: &Store, desk: &str) -> String {
    if let Some(d) = st.desk.spaces.get(desk) {
        return d.folder.clone();
    }
    st.desk.lead.get(desk).and_then(|l| st.sessions.get(&l.sid)).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone()).unwrap_or_default()
}

/// The folder item `it`'s holder works in on this Mac ("" for none, or one on another machine), for how its work
/// lands (desk/lands.rs).
pub(crate) fn holder_folder(st: &Store, it: &DeskItem) -> String {
    it.holder.as_ref().and_then(|h| st.sessions.get(h.to())).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone()).unwrap_or_default()
}

impl Hub {
    /// How work lands for item `it`, its holder's folder looked at first if Cue hasn't yet (a short wait, once).
    /// Call with no lock held.
    pub(crate) fn item_lands(&self, it: &DeskItem) -> crate::desk::lands::Lands {
        let folder = self.read(|st| holder_folder(st, it));
        crate::logs::repos::first_look(&folder);
        crate::desk::lands::lands_of(&it.project, &folder)
    }
}

#[cfg(test)]
mod stint_tests {
    use super::*;
    use crate::desk::Stint;

    fn item(id: &str, state: &str, from: Option<(&str, u64)>, before: Vec<(&str, u64, u64)>) -> DeskItem {
        DeskItem { id: id.into(), state: state.into(), held_from: from.map(|(s, a)| (s.into(), a)), held_before: before.into_iter().map(|(s, a, b)| (s.into(), a, b)).collect(), ..Default::default() }
    }
    fn stint(sid: &str, from: u64, until: Option<u64>, next: &str) -> Stint {
        Stint { sid: sid.into(), from, until, next: next.into() }
    }

    #[test]
    fn an_item_is_its_sessions_from_pick_up_till_its_next_item_another_holder_or_looks_right() {
        let mut st = Store::default();
        // a went to s1, then to s2; s1 moved on to b before a left it; s2 still on a.
        st.desk.items.push(item("a", "working", Some(("s2", 5_000)), vec![("s1", 1_000, 5_000)]));
        st.desk.items.push(item("b", "working", Some(("s1", 3_000)), vec![]));
        assert_eq!(stints(&st, &st.desk.items[0]), vec![stint("s1", 1_000, Some(3_000), "b"), stint("s2", 5_000, None, "")]);
        // b is still on s1: open, nothing after it.
        assert_eq!(stints(&st, &st.desk.items[1]), vec![stint("s1", 3_000, None, "")]);
        // An item s1 had before a doesn't end a's stint.
        st.desk.items.push(item("old", "done", None, vec![("s1", 500, 900)]));
        assert_eq!(stints(&st, &st.desk.items[0])[0], stint("s1", 1_000, Some(3_000), "b"));
        // Looks right on b at 4s: its stint ends there, with no next.
        st.desk.items[1].state = "done".into();
        st.desk.items[1].done_at = Some(4);
        assert_eq!(stints(&st, &st.desk.items[1]), vec![stint("s1", 3_000, Some(4_000), "")]);
        // Shown with it; never on an item with no holder yet.
        assert_eq!(shown(&st, &st.desk.items[0]).stints.len(), 2);
        st.desk.items.push(item("c", "new", None, vec![]));
        assert!(shown(&st, &st.desk.items[3]).stints.is_empty());
    }
}
