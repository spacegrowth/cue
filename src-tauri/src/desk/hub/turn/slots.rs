//! How many of a Desk's items work on a machine at once (#798): the one "at once" number per machine in Desk settings
//! (desk/hub/landing/remote_item.rs `machine_cap`: another machine's under Machines, this Mac's Items at once), on
//! this Mac too however its items work (in copies of their own, on a branch, or in the shared folder). An item routed
//! or started past it waits in Up next ("waiting for a free slot", `DeskItem::slot_wait`) and starts by itself, in
//! order, as one there finishes. Counted: items at work there (not paused), those routed to a session there and in line
//! for a copy, and new sessions being started there. Not counted: paused, finishing, to check, parked or done. Your own
//! Start now asks first (#712, remote_item.rs `start_full`).

use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, Holder};
use crate::desk::hub::core::desk::{Effects, decide, rule_of, secs};
use crate::desk::hub::landing::remote_item::{machine_cap, start_place};

/// Whether item `it` takes one of its Desk's slots now.
pub(crate) fn takes(st: &Store, it: &DeskItem) -> bool {
    match &it.holder {
        Some(_) => (it.state == "working" && it.paused.is_none()) || (it.state == "queued" && it.tree_wait && !it.slot_wait),
        // A new session being started for it, its place claimed (desk/hub/lead/run.rs `claim_place`).
        None => st.desk.run.start_machine.contains_key(&it.id),
    }
}

/// The machine session `h` runs on ("" for this Mac).
pub(crate) fn holder_machine(st: &Store, h: &Holder) -> String {
    st.sessions.get(h.to()).map(|s| s.origin.machine.clone()).unwrap_or_default()
}

/// The machine item `it` works on, or would ("" for this Mac): its holder's, the session it's next for, where its new
/// session is starting, else where one would start now.
pub(crate) fn machine_of(st: &Store, it: &DeskItem) -> String {
    if let Some(h) = it.holder.as_ref().or(it.next_for.as_ref()) {
        return holder_machine(st, h);
    }
    if let Some(m) = st.desk.run.start_machine.get(&it.id) {
        return m.clone();
    }
    start_place(st, it).ok().flatten().map(|o| o.machine).unwrap_or_default()
}

/// How many of Desk `project`'s slots on `machine` are taken, item `but` left out (the one asking).
pub(crate) fn load(st: &Store, project: &str, machine: &str, but: &str) -> usize {
    st.desk.items.iter().filter(|i| i.project == project && i.id != but && takes(st, i) && machine_of(st, i) == machine).count()
}

/// Whether item `id` of Desk `project` may begin work on `machine` now: it takes a slot already, or one is free there.
pub(crate) fn free_on(st: &Store, project: &str, machine: &str, id: &str) -> bool {
    if st.desk.item(id).is_some_and(|i| takes(st, i)) {
        return true;
    }
    machine_cap(st, project, machine).is_none_or(|n| load(st, project, machine, id) < n)
}

/// `free_on` where item `id` works, or would.
pub(crate) fn free(st: &Store, project: &str, id: &str) -> bool {
    let m = st.desk.item(id).map(|i| machine_of(st, i)).unwrap_or_default();
    free_on(st, project, &m, id)
}

/// The machine's name as said ("this Mac", "box").
fn said(machine: &str) -> String {
    if machine.is_empty() { "this Mac".into() } else { crate::machines::display_name(machine) }
}

/// What the lead is told of a route that waits for a slot on `machine`.
pub(crate) fn words(st: &Store, project: &str, machine: &str, id: &str, to: &str) -> String {
    let n = machine_cap(st, project, machine).unwrap_or(0);
    format!("Queued: #{id} goes to {to} once {} has a free slot ({n} of this Desk's items at once there, the user's setting); it starts by itself, in queue order.", said(machine))
}

/// Item `id`, routed by the lead to session `h` while `h`'s machine has no free slot: held by `h` but not sent yet, it
/// waits queued for a slot. What the lead is told.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hold_for(st: &mut Store, project: &str, by: (&str, &str), id: &str, h: &Holder, why: &str, now: u64, fx: &mut Effects) -> Option<String> {
    let machine = holder_machine(st, h);
    let it = st.desk.item_mut(id)?;
    let same = it.slot_wait && it.holder.as_ref().is_some_and(|b| b.sid == h.sid && b.lead_sid == h.lead_sid);
    if !same {
        crate::desk::hub::turn::route::held_by(it, h, String::new(), None, why, now);
        (it.state, it.slot_wait, it.queued_at) = ("queued".into(), true, now);
        fx.items.push(id.to_string());
        decide(st, project, Some(id.to_string()), by.0, by.1, &format!("Queued #{id} for {}: waiting for a free slot", h.name), why, now, fx);
    }
    Some(words(st, project, &machine, id, &h.name))
}

/// Item `it`, released from the item it waited on to begin with its holder, while there's no free slot where it works
/// (`room` false): it waits for one, queued, its holder kept and not sent yet. True when it waits.
pub(crate) fn hold_released(it: &mut DeskItem, room: bool, now: u64) -> bool {
    if room || it.holder.is_none() || it.next_for.is_some() {
        return false;
    }
    (it.state, it.slot_wait, it.queued_at) = ("queued".into(), true, now);
    true
}

/// Items waiting for a slot with their holder: each begins, in your order then the one waiting longest, once its
/// machine has a free slot. One whose holder has gone goes back to the lead to route. A paused Desk's wait.
pub(crate) fn sweep(st: &mut Store, now: u64, fx: &mut Effects) {
    for it in st.desk.items.iter_mut() {
        // Taken out of the wait by other means (moved, dropped, parked, started by you).
        if it.slot_wait && (it.state != "queued" || it.after.is_some() || (it.holder.is_none() && !it.room_wait)) {
            it.slot_wait = false;
        }
    }
    let mut waiting: Vec<(u32, u64, String)> = st.desk.items.iter()
        .filter(|i| i.slot_wait && i.holder.is_some())
        .map(|i| (i.queue_order.unwrap_or(u32::MAX), i.queued_at, i.id.clone())).collect();
    waiting.sort();
    for (_, _, id) in waiting {
        let Some(it) = st.desk.item(&id) else { continue };
        let project = it.project.clone();
        if crate::desk::hub::away::pause::is_paused(&project) || !free(st, &project, &id) {
            continue;
        }
        let alive = it.holder.as_ref().is_some_and(|h| st.sessions.get(h.to()).is_some());
        let rule = rule_of(st, &project);
        let Some(it) = st.desk.item_mut(&id) else { continue };
        it.slot_wait = false;
        it.updated = secs(now);
        fx.items.push(id.clone());
        if !alive {
            // Its session closed while it waited: back to new, for the lead to route again.
            it.remember_holder();
            (it.holder, it.state) = (None, "new".into());
            continue;
        }
        it.state = "working".into();
        let to = it.holder.as_ref().map(|h| h.to().to_string()).unwrap_or_default();
        let why = it.after_why.clone();
        fx.sends.push(crate::desk::hub::core::desk::kinded((to, crate::desk::handed_words(it, &why, &rule)), crate::sessions::Kind::Brief));
    }
}

/// Whether item `it` waits for a free slot now (as the window and the lead say it): its own wait for one, or one next
/// for a session, or waiting for room, that can't begin for want of one where it would work.
pub(crate) fn waits(st: &Store, it: &DeskItem) -> bool {
    if it.state != "queued" || it.after.is_some() {
        return false;
    }
    it.slot_wait || ((it.next_for.is_some() || it.room_wait) && it.holder.is_none() && !free(st, &it.project, &it.id))
}
