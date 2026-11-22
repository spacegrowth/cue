//! A lead moving one of its items to another Desk (#639): `MOVE: #<id> -> <Desk> | why: …`. The item keeps what it
//! has (its replies and notes, its pictures, its copy and branch) and leaves this Desk's list for the other's. A
//! session at work on it is told to stop and kept as the one it moved off, so the session the other Desk's lead
//! starts for it is sent its hand-off note (desk/hub/turn/route.rs `handoff_words`). That lead is told it came, as
//! a band from this Desk (desk/tell.rs), and it shows on that Desk as new, to be routed.

use crate::desk::holds::{repo_key, HoldSave};
use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::turn::route::decide;
use crate::desk::parse::Line;
use crate::hub::Store;

/// `MOVE: #<id> -> <Desk> | why: <reason>` (body after "MOVE:"): None unless it names an item and a Desk.
pub(crate) fn parse(body: &str) -> Option<Line> {
    let (head, rest) = body.split_once('|').map_or((body, ""), |(h, r)| (h, r));
    let (id, to) = ["->", "→", "=>"].iter().find_map(|a| head.split_once(a))?;
    let id = id.trim();
    let to = to.trim().trim_matches(['`', '"', '*']).trim();
    if !id.starts_with('#') || to.is_empty() || to.starts_with('<') {
        return None;
    }
    let id: String = id.trim_start_matches('#').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    let why = rest.trim();
    let why = why.get(..4).filter(|w| w.eq_ignore_ascii_case("why:")).map_or(why, |_| why[4..].trim());
    (!id.is_empty()).then(|| Line::Move { id: id.to_ascii_lowercase(), to: to.to_string(), why: why.to_string() })
}

/// The Desk `t` names (its id or name, any case), other than `desk`, or why there's none, in words after "Refused: ".
fn desk_named(st: &Store, desk: &str, t: &str) -> Result<String, String> {
    let mut all: Vec<&String> = st.desk.spaces.keys().chain(st.desk.lead.keys()).collect();
    all.sort();
    all.dedup();
    let named = |d: &str| d == t || st.desk.name_of(d).eq_ignore_ascii_case(t);
    let others: Vec<&String> = all.iter().copied().filter(|d| *d != desk).collect();
    let mut names: Vec<String> = others.iter().map(|d| st.desk.name_of(d).to_string()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    if named(desk) {
        return Err("it's on your Desk already.".into());
    }
    if let Some(d) = others.iter().find(|d| named(d)) {
        return Ok((*d).clone());
    }
    Err(if names.is_empty() { "there's no other Desk. The user can add one first.".into() } else { format!("no Desk by that name. You can move it to one of these: {}.", names.join(", ")) })
}

/// The Desk item `id` can move to from Desk `desk` (`to` names it) and its copy's branch, if it has one; or why it
/// can't, in words after "Refused: MOVE #id: ".
fn movable(st: &Store, desk: &str, id: &str, to: &str) -> Result<(String, Option<String>), String> {
    let Some(it) = st.desk.item(id) else {
        return Err(format!("there's no item #{id}. You can move an item your Desk block lists."));
    };
    if it.project != desk {
        return Err(format!("#{id} is another Desk's. You can move only your own Desk's items."));
    }
    if it.state == "done" {
        return Err(format!("#{id} is done. Nothing to move."));
    }
    if it.state == "finishing" || st.desk.run.finish_claims.contains(id) {
        return Err(format!("#{id} is landing now. You can move it once it's to check or back at work."));
    }
    if st.desk.items.iter().any(|i| i.part_of == id && i.state != "done") {
        return Err(format!("#{id} is a feature with pieces on this Desk. You can move each piece instead."));
    }
    let to = desk_named(st, desk, to)?;
    // Its copy is a branch of this Desk's repository: it can go only to a Desk on the same one.
    let branch = it.tree.as_ref().map(|t| t.branch.clone());
    if branch.is_some() && repo_key(st, desk) != repo_key(st, &to) {
        let to_name = st.desk.name_of(&to);
        return Err(format!("#{id} has a copy of this Desk's repository, and {to_name} works on another one. You can move it to a Desk on this repository, or drop its copy first (TAKE COPY: #{id})."));
    }
    Ok((to, branch))
}

/// The lead of Desk `desk` (`by`, session `sid`) said `MOVE: #<id> -> <to> | why: <why>`: item `id` moved to that
/// Desk, and what the lead is told ("Moved: …" or "Refused: …").
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_said(st: &mut Store, desk: &str, by: &str, sid: &str, id: &str, to: &str, why: &str, now: u64, fx: &mut Effects) -> String {
    let (to, branch) = match movable(st, desk, id, to) {
        Ok(t) => t,
        Err(no) => return format!("Refused: MOVE #{id}: {no}"),
    };
    let to_name = st.desk.name_of(&to).to_string();
    let from_name = st.desk.name_of(desk).to_string();
    let Some(it) = st.desk.item_mut(id) else { return String::new() };
    // A session at work on it, or waiting to start it: told to stop, and kept for its hand-off.
    let mut handoff = String::new();
    if let Some(b) = it.holder.clone().filter(|_| matches!(it.state.as_str(), "new" | "queued" | "working")) {
        if it.state == "working" {
            let crew = if b.kind == "crew" { format!("For your teammate {}: ", b.name) } else { String::new() };
            fx.sends.push((b.to().to_string(), format!("{crew}Desk item #{} ({}) has moved to the {to_name} Desk: stop work on it. Write your hand-off note for it now (what's done, what's next, the files), so the session that takes it there can carry on.", it.id, it.title)));
        }
        handoff = format!(" {} worked on it: route it to new and the session Cue starts is sent {}'s hand-off.", b.name, b.name);
        it.remember_holder();
        it.moved_from = Some(b);
        (it.holder, it.state) = (None, "new".into());
    }
    // This Desk's queue, waits and feature are left behind; so is anything kept about its holder here.
    it.project = to.clone();
    (it.after, it.next_for, it.queued_at, it.queue_order) = (None, None, 0, None);
    it.after_why.clear();
    it.part_of.clear();
    it.crew.clear();
    (it.stop, it.told, it.waits, it.paused) = (None, None, None, None);
    it.updated = crate::desk::hub::core::desk::secs(now);
    let title = it.title.clone();
    st.desk.run.nudged.remove(id);
    st.desk.run.left_said.remove(id);
    st.desk.run.stuck_told.remove(id);
    st.desk.run.done_told.remove(id);
    // A hold it has on the code goes with it.
    for h in st.desk.holds.iter_mut().filter(|h| h.item == id) {
        h.desk = to.clone();
        fx.holds.push(HoldSave::Put(h.clone()));
    }
    fx.items.push(id.to_string());
    fx.news.push((to.clone(), id.to_string()));
    let with = match &branch {
        Some(b) => format!("its notes and its branch {b}"),
        None => "its notes".to_string(),
    };
    let why_said = if why.trim().is_empty() { String::new() } else { format!(" Why: {}", why.trim()) };
    decide(st, desk, Some(id.to_string()), by, sid, &format!("Moved #{id} to {to_name}"), why, now, fx);
    decide(st, &to, Some(id.to_string()), by, sid, &format!("#{id} came from {from_name}"), why, now, fx);
    if st.desk.lead_sid(&to).is_some() {
        let words = format!("#{id} ({title}) moved to your Desk from {from_name}, with {with}.{handoff}{why_said} It's on your list to route.");
        fx.tells.push((to.clone(), crate::desk::tell::band_text(&from_name, by, Some(id), &words), sid.to_string()));
    }
    let stopped = if handoff.is_empty() { "" } else { " Its session was told to stop and write its hand-off." };
    format!("Moved: #{id} to {to_name}, with {with}.{stopped} It's off your Desk.")
}
