//! The Desk's queue (#576): an item routed to a session busy with another waits with no holder, marked for that
//! session (`DeskItem::next_for`). It starts there once the session is free (its item got Looks right, or was
//! dropped or parked), or in a fresh session with a short hand-off once it has waited `WAIT_MS`.

use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, Holder};
use crate::desk::hub::core::desk::{Effects, NewStart, decide, rule_of, secs};

/// How long an item waits for the session it's next for before it goes to a fresh one.
pub(crate) const WAIT_MS: u64 = 20 * 60_000;

/// Whether `it` is held by `h` (the same session, or the same teammate).
fn holds(it: &DeskItem, h: &Holder) -> bool {
    it.holder.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid)
}

/// An item other than `id` that keeps `h` busy: one it holds that isn't done or parked (at work, queued behind
/// another, stopped, paused, finishing or waiting for your check). Under #576 a session keeps its item until Looks
/// right. The one at work first.
pub(crate) fn busy_with(st: &Store, h: &Holder, id: &str) -> Option<String> {
    let open: Vec<&DeskItem> = st.desk.items.iter()
        .filter(|i| i.id != id && holds(i, h) && !matches!(i.state.as_str(), "done" | "held" | "new"))
        .collect();
    open.iter().find(|i| i.state == "working").or(open.first()).map(|i| i.id.clone())
}

/// Whether an item other than `id` already waits in the queue for `h` with nothing ahead of it: `h` takes that one
/// first. Only for a moment on a running Desk (the sweep starts it); a paused Desk's for as long as it's paused.
pub(crate) fn ahead_for(st: &Store, h: &Holder, id: &str) -> bool {
    st.desk.items.iter().any(|i| {
        i.id != id && i.state == "queued" && i.holder.is_none() && i.after.is_none()
            && i.next_for.as_ref().is_some_and(|n| n.sid == h.sid && n.lead_sid == h.lead_sid)
    })
}

/// When item `it` goes to a fresh session if the one it's next for stays busy: only while it waits for that session
/// (no item ahead of it), and its Desk isn't paused.
pub(crate) fn fresh_at(it: &DeskItem) -> Option<u64> {
    let waiting = it.state == "queued" && it.holder.is_none() && it.after.is_none() && it.next_for.is_some();
    (waiting && !crate::desk::hub::away::pause::is_paused(&it.project)).then_some(it.queued_at + WAIT_MS)
}

/// "14:20": local time at `ms`.
pub(crate) fn clock(ms: u64) -> String {
    let tm = crate::logs::usage::local_tm(ms / 1000);
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// Item `id` routed to `h`, which is busy (or after item `waits`): it waits in the queue with no holder, marked for
/// `h`. A holder at work on it stops. What the lead is told.
#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_for(st: &mut Store, project: &str, by: &crate::desk::hub::turn::route::By, id: &str, h: &Holder, waits: Option<String>, refused: String, why: &str, now: u64, fx: &mut Effects) -> Option<String> {
    let busy = busy_with(st, h, id);
    let it = st.desk.item_mut(id)?;
    let same = it.state == "queued" && it.holder.is_none() && it.after == waits && it.next_for.as_ref().is_some_and(|n| n.sid == h.sid && n.lead_sid == h.lead_sid);
    if !same {
        if let Some(b) = it.holder.clone() {
            if it.state == "working" {
                let crew = if b.kind == "crew" { format!("For your teammate {}: ", b.name) } else { String::new() };
                fx.sends.push((b.to().to_string(), format!("{crew}Desk item #{} ({}) has moved to {}: stop work on it.", it.id, it.title, h.name)));
            }
            it.remember_holder();
            it.moved_from = Some(b);
        }
        (it.holder, it.state, it.paused, it.done_at) = (None, "queued".into(), None, None);
        it.crew.clear();
        (it.next_for, it.queued_at, it.after) = (Some(h.clone()), now, waits.clone());
        (it.tree_wait, it.no_copy_wait) = (false, false);
        it.tree_wait_repo.clear();
        it.after_why = why.to_string();
        it.route_failed.clear();
        it.start_failed.clear();
        it.reopen_refused.clear();
        it.after_refused = refused;
        it.updated = secs(now);
        fx.items.push(id.to_string());
        let text = match &waits {
            Some(a) => format!("Queued #{id} after #{a}, next for {}", h.name),
            None => format!("Queued #{id}, next for {}", h.name),
        };
        decide(st, project, Some(id.to_string()), by.name, by.sid, &text, why, now, fx);
    }
    // The same route said again (a later turn repeating itself): nothing new to tell.
    if same {
        return Some(format!("Taken: #{id}, queued for {}.", h.name));
    }
    let at = st.desk.item(id).map_or(now, |i| i.queued_at) + WAIT_MS;
    let mins = WAIT_MS / 60_000;
    Some(match (&waits, busy) {
        (Some(a), _) => format!("Queued: #{id} is next for {0}, after #{a} gets Looks right; then it goes to {0} if it's free, else to a fresh session once {0} has stayed busy {mins} minutes.", h.name),
        (None, Some(x)) => format!("Queued: #{id} is next for {0} (busy with #{x}); it goes to a fresh session at {1} if {0} is still busy.", h.name, clock(at)),
        (None, None) => format!("Queued: #{id} is next for {0}; it goes to a fresh session at {1} if {0} is busy then.", h.name, clock(at)),
    })
}

/// Whether session `h` closed while Cue ran: its items go to a fresh session at once, not after `WAIT_MS`. One not
/// yet seen since Cue started (sessions still loading) isn't taken for closed.
fn closed(st: &Store, h: &Holder) -> bool {
    st.sessions.get(h.to()).is_none() && st.ended.iter().any(|(_, s)| s.origin.session_id == h.to())
}

/// Items in the queue for a session: each starts in that session once it's free (one item per freed session, in
/// queue order), or in a fresh session once it has waited `WAIT_MS`. A paused Desk's wait.
pub(crate) fn sweep(st: &mut Store, now: u64, fx: &mut Effects) {
    let mut waiting: Vec<(u32, u64, String)> = st.desk.items.iter()
        .filter(|i| i.state == "queued" && i.holder.is_none() && i.after.is_none() && i.next_for.is_some())
        .map(|i| (i.queue_order.unwrap_or(u32::MAX), i.queued_at, i.id.clone())).collect();
    if waiting.is_empty() {
        return;
    }
    waiting.sort();
    let mut taken: Vec<String> = Vec::new();
    for (_, _, id) in waiting {
        let Some(it) = st.desk.item(&id) else { continue };
        let (project, h) = (it.project.clone(), it.next_for.clone().unwrap_or_default());
        if crate::desk::hub::away::pause::is_paused(&project) {
            continue;
        }
        let alive = st.sessions.get(h.to()).is_some();
        if alive && !taken.contains(&h.to().to_string()) && busy_with(st, &h, &id).is_none() {
            taken.push(h.to().to_string());
            start_in(st, &project, &id, &h, now, fx);
        } else if now >= it.queued_at + WAIT_MS || closed(st, &h) {
            let mins = WAIT_MS / 60_000;
            let handoff = handoff(st, it, &h, &if alive { format!("it was busy for {mins} minutes") } else { "it closed".into() });
            let why = [it.after_why.trim(), handoff.as_str()].iter().filter(|w| !w.is_empty()).copied().collect::<Vec<_>>().join("\n\n");
            let lead = st.desk.lead.get(&project).cloned();
            let Some(it) = st.desk.item_mut(&id) else { continue };
            (it.next_for, it.state, it.updated) = (None, "new".into(), secs(now));
            fx.items.push(id.clone());
            let (by, by_sid) = lead.as_ref().map_or(("Desk", ""), |l| (l.name.as_str(), l.sid.as_str()));
            fx.starts.push(NewStart { project, id, why, by: by.into(), by_sid: by_sid.into() });
        }
    }
}

/// Item `id`, next for a session, started now (Start now, or the lead's START), not waiting for anything: in that
/// session if it's free, else in a fresh session with the hand-off. False when it isn't next for a session.
pub(crate) fn start_next_now(st: &mut Store, project: &str, id: &str, why: &str, by: (&str, &str), now: u64, fx: &mut Effects) -> bool {
    let Some(it) = st.desk.item(id).filter(|i| i.holder.is_none()) else { return false };
    let Some(h) = it.next_for.clone() else { return false };
    let alive = st.sessions.get(h.to()).is_some();
    if alive && busy_with(st, &h, id).is_none() {
        if let Some(it) = st.desk.item_mut(id) {
            it.after = None;
            if !why.trim().is_empty() {
                it.after_why = why.to_string();
            }
        }
        start_in(st, project, id, &h, now, fx);
        return true;
    }
    let handoff = handoff(st, it, &h, if alive { "it was busy when the item was started now" } else { "it closed" });
    let why = [why.trim(), handoff.as_str()].iter().filter(|w| !w.is_empty()).copied().collect::<Vec<_>>().join("\n\n");
    let Some(it) = st.desk.item_mut(id) else { return false };
    (it.next_for, it.after, it.state, it.updated) = (None, None, "new".into(), secs(now));
    // Started now: with every copy in use it works where its session is, in line for the next free one (#548).
    (it.tree_wait, it.no_copy_wait) = (false, true);
    it.tree_wait_repo.clear();
    fx.items.push(id.to_string());
    fx.starts.push(NewStart { project: project.into(), id: id.into(), why, by: by.0.into(), by_sid: by.1.into() });
    true
}

/// Item `id` starts in `h`, the session it was next for, now free: held by it, at work, and sent it.
fn start_in(st: &mut Store, project: &str, id: &str, h: &Holder, now: u64, fx: &mut Effects) {
    let rule = rule_of(st, project);
    let lead = st.desk.lead.get(project).cloned();
    let h = crate::desk::hub::core::desk::holder_of(st, &h.sid).filter(|_| h.lead_sid.is_empty()).unwrap_or_else(|| h.clone());
    let Some(it) = st.desk.item_mut(id) else { return };
    let (why, refused) = (it.after_why.clone(), it.after_refused.clone());
    // Reopened while it was busy: back to it now with your words, as a new piece of its chat (#577).
    let back = std::mem::take(&mut it.back_message);
    let back = (!back.is_empty() && it.moved_from.as_ref().is_some_and(|m| m.sid == h.sid && m.lead_sid == h.lead_sid)).then_some(back);
    crate::desk::hub::turn::route::held_by(it, &h, refused, None, &why, now);
    match back {
        Some(message) => {
            it.held_by(&h.sid, now);
            fx.backs.push((id.to_string(), message.clone()));
            fx.sends.push((h.to().to_string(), message));
        }
        None => fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "", &rule))),
    }
    fx.items.push(id.to_string());
    let (by, by_sid) = lead.as_ref().map_or(("Desk", ""), |l| (l.name.as_str(), l.sid.as_str()));
    decide(st, project, Some(id.to_string()), by, by_sid, &format!("Started #{id} in {}, free now", h.name), "", now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
}

/// What a fresh session is told of item `it`, which was next for `h` and came to it instead (`because`: why): why it
/// came to it, and the files `h` touched for its items, to read first.
fn handoff(st: &Store, it: &DeskItem, h: &Holder, because: &str) -> String {
    let mut out = format!("Hand-off: #{} was next for {}, which knows this area, but {because}, so the item came to you.", it.id, h.name);
    let mut files: Vec<String> = Vec::new();
    let theirs = |i: &DeskItem| holds(i, h) || i.held_before.iter().any(|(by, _, _)| *by == h.sid);
    for i in st.desk.items.iter().filter(|i| i.project == it.project && theirs(i)) {
        let shown = crate::desk::hub::core::view::shown(st, i);
        for f in i.files.iter().chain(shown.files.iter()) {
            if !files.contains(f) {
                files.push(f.clone());
            }
        }
    }
    if !files.is_empty() {
        files.truncate(30);
        out.push_str(&format!(" Files {} touched for its items, to read first: {}.", h.name, files.join(", ")));
    }
    out
}

impl Hub {
    /// The queue's sweep on its own (a wait run out with nothing else happening), its follow-up done as any change's.
    pub(crate) fn desk_queue_tick(&self) {
        let now = crate::clock::now_ms();
        let fx = self.write(|st| {
            if st.desk.run.queue_timer <= now {
                st.desk.run.queue_timer = 0;
            }
            let mut fx = Effects::default();
            sweep(st, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// The sweep before a change's follow-up, and the next wait to run out kept in time: a sweep is due then.
    pub(crate) fn desk_queue_sweep(&self, fx: &mut Effects) {
        let now = crate::clock::now_ms();
        let due = self.write(|st| {
            sweep(st, now, fx);
            let next = st.desk.items.iter().filter_map(fresh_at).filter(|at| *at > now).min()?;
            let timer = st.desk.run.queue_timer;
            if timer > now && timer <= next {
                return None;
            }
            st.desk.run.queue_timer = next;
            Some(next)
        });
        if let Some(at) = due {
            self.later(at - now, |h| h.desk_queue_tick());
        }
    }
}

#[cfg(test)]
impl Hub {
    /// Item `id`'s files as its holder's work left them, nothing else changed: for a hand-off to name.
    pub(crate) fn desk_put_files(&self, id: &str, files: &[&str]) {
        self.write(|st| {
            if let Some(it) = st.desk.item_mut(id) {
                it.files = files.iter().map(|f| f.to_string()).collect();
            }
        });
    }
}
