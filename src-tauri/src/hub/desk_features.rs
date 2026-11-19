//! Big features (#282): items that are pieces of one feature, listed under its name on the Desk. The lead says
//! `FEATURE: <name> | #a #b …` (a new feature item) or `FEATURE: #<id> | #a #b …` (item `id` is the feature); each
//! piece keeps its own holder, state and page, and names its feature in `part_of`. A feature nobody holds is only
//! its pieces: Cue keeps it queued while any is open, and done once all are (`settle`).

use super::*;
use crate::desk::DeskItem;
use super::desk::{Effects, secs};
use super::desk_heard::new_item;
use super::desk_route::{By, decide};

impl Hub {
    /// The items a FEATURE: line in `text` names that are on the disk only (done days ago): read before the store is
    /// written, for `bring_back`.
    pub(super) fn feature_pieces_on_disk(&self, text: &str) -> Vec<DeskItem> {
        let named: Vec<String> = crate::desk::parse::lines(text).into_iter().filter_map(|l| if let crate::desk::parse::Line::Feature { pieces, .. } = l { Some(pieces) } else { None }).flatten().collect();
        if named.is_empty() {
            return Vec::new();
        }
        let missing: Vec<String> = self.read(|st| named.iter().map(|p| st.desk.canon(p)).filter(|p| st.desk.item(p).is_none()).collect());
        missing.iter().filter_map(|p| crate::db::desk_get_item(p)).collect()
    }
}

/// Items read from the disk back in the store, those it doesn't have now.
pub(super) fn bring_back(st: &mut Store, items: Vec<DeskItem>) {
    for it in items {
        if st.desk.item(&it.id).is_none() {
            st.desk.items.push(it);
        }
    }
}

/// The FEATURE: line lead `by` said on `project`: its pieces listed under the feature (made when it's named, not
/// numbered). `last_new`: the reply's last ITEM (a piece written "#new"). Err: what the lead is told, when nothing
/// could be grouped.
#[allow(clippy::too_many_arguments)]
pub(super) fn feature_said(st: &mut Store, project: &str, by: &By, id: Option<String>, name: &str, pieces: &[String], last_new: Option<&str>, now: u64, fx: &mut Effects) -> Result<(), String> {
    let (mut ok, no) = pieces_of(st, project, pieces, last_new);
    let fid = feature_of(st, project, by, id, name, &ok, &no, now, fx)?;
    ok.retain(|p| *p != fid);
    if ok.is_empty() {
        return Err(format!("Refused: FEATURE #{fid}: no pieces to put under it{}.", if no.is_empty() { String::new() } else { format!(": {}", no.join(", ")) }));
    }
    let mut moved_from = Vec::new();
    for p in &ok {
        if let Some(it) = st.desk.item_mut(p) {
            if it.part_of != fid {
                if !it.part_of.is_empty() {
                    moved_from.push(it.part_of.clone());
                }
                it.part_of = fid.clone();
                fx.items.push(p.clone());
            }
        }
    }
    let list = ok.iter().map(|p| format!("#{p}")).collect::<Vec<_>>().join(", ");
    decide(st, project, Some(fid.clone()), by.name, by.sid, &format!("Put {list} under it"), "", now, fx);
    settle(st, &fid, now, fx);
    for old in moved_from {
        settle(st, &old, now, fx);
    }
    if no.is_empty() { Ok(()) } else { Err(format!("FEATURE #{fid}: left out {}.", no.join(", "))) }
}

/// The pieces named, as ids: those that can go under a feature, and why the others can't. "#new" is `last_new`.
fn pieces_of(st: &Store, project: &str, pieces: &[String], last_new: Option<&str>) -> (Vec<String>, Vec<String>) {
    let (mut ok, mut no) = (Vec::new(), Vec::new());
    for p in pieces {
        let pid = if p == "new" { match last_new { Some(n) => n.to_string(), None => { no.push("#new (no ITEM before it in this reply)".to_string()); continue; } } } else { st.desk.canon(p) };
        match st.desk.item(&pid) {
            Some(it) if it.project != project => no.push(format!("#{pid} (not on this Desk)")),
            Some(_) if st.desk.items.iter().any(|i| i.part_of == pid) => no.push(format!("#{pid} (it's a feature itself)")),
            Some(_) => ok.push(pid),
            None => no.push(format!("#{pid} (no such item)")),
        }
    }
    (ok, no)
}

/// The feature they go under: item `id`, else the open feature named `name`, else a new one of that name.
#[allow(clippy::too_many_arguments)]
fn feature_of(st: &mut Store, project: &str, by: &By, id: Option<String>, name: &str, ok: &[String], no: &[String], now: u64, fx: &mut Effects) -> Result<String, String> {
    Ok(match id {
        Some(id) => {
            let id = st.desk.canon(&id);
            match st.desk.item(&id) {
                Some(f) if f.project == project && f.part_of.is_empty() => id,
                Some(f) if f.project == project => return Err(format!("Refused: FEATURE #{id}: it's a piece of #{} already; a feature can't be a piece.", f.part_of)),
                _ => return Err(format!("Refused: FEATURE #{id}: this Desk has no item #{id}.")),
            }
        }
        None => {
            // The same name again: the feature already made, not a second one.
            let same = st.desk.items.iter().find(|i| i.project == project && i.state != "done" && i.kind == "feature" && i.title.trim().eq_ignore_ascii_case(name.trim()) && st.desk.items.iter().any(|x| x.part_of == i.id));
            match same {
                Some(f) => f.id.clone(),
                None if ok.is_empty() => return Err(format!("Refused: FEATURE {name}: none of its pieces could go under it: {}.", no.join(", "))),
                None => {
                    let mut f = new_item(st, project, "feature", name, "", by.name, now);
                    f.state = "queued".into();
                    let fid = f.id.clone();
                    fx.items.push(fid.clone());
                    st.desk.items.push(f);
                    fid
                }
            }
        }
    })
}

/// Feature `fid` as its pieces have it: one nobody holds is done once all of its pieces are (done when the last one
/// was), and queued again while any is open. One with a holder is its own item: its state is its holder's to say.
pub(super) fn settle(st: &mut Store, fid: &str, now: u64, fx: &mut Effects) {
    let pieces: Vec<(bool, Option<u64>)> = st.desk.items.iter().filter(|i| i.part_of == fid).map(|i| (i.state == "done", i.done_at)).collect();
    let Some(f) = st.desk.item_mut(fid) else { return };
    if f.holder.is_some() || pieces.is_empty() {
        return;
    }
    let all_done = pieces.iter().all(|(d, _)| *d);
    let (state, done_at) = if all_done { ("done", Some(pieces.iter().filter_map(|(_, at)| *at).max().unwrap_or(secs(now)))) } else { ("queued", None) };
    if f.state != state || (all_done && f.done_at.is_none()) {
        f.state = state.into();
        f.done_at = done_at;
        f.updated = secs(now);
        fx.items.push(fid.to_string());
    }
}

/// The features whose pieces are among `ids`: to `settle` once those pieces changed.
pub(super) fn features_of(st: &Store, ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.iter().filter_map(|id| st.desk.item(id)).map(|i| i.part_of.clone()).filter(|p| !p.is_empty()).collect();
    out.sort();
    out.dedup();
    out
}

/// Item `it`'s pieces as they're shown, oldest first (by number, then when added): none for one that isn't a feature.
pub(super) fn pieces_shown(st: &Store, it: &DeskItem, shown: impl Fn(&DeskItem) -> DeskItem) -> Vec<DeskItem> {
    let mut out: Vec<DeskItem> = st.desk.items.iter().filter(|i| i.part_of == it.id && i.state != "dropped").map(shown).collect();
    out.sort_by_key(|i| (i.id.parse::<u64>().unwrap_or(u64::MAX), i.created));
    out
}

/// The lead's words on a feature or a piece, for its line in the Desk block: " | feature: 3 of 5 done (#a, #b…)" or
/// " | part of #F".
pub(super) fn feature_words(st: &Store, it: &DeskItem) -> String {
    let pieces: Vec<&DeskItem> = st.desk.items.iter().filter(|i| i.part_of == it.id).collect();
    let mut out = String::new();
    if !pieces.is_empty() {
        let done = pieces.iter().filter(|i| i.state == "done").count();
        let ids = pieces.iter().map(|i| format!("#{}", i.id)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!(" | feature: {done} of {} done ({ids}){}", pieces.len(), if it.holder.is_none() { "; only its pieces are routed" } else { "" }));
    }
    if !it.part_of.is_empty() {
        out.push_str(&format!(" | part of #{}", it.part_of));
    }
    out
}
