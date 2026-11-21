//! Holds on a repository's code (#540, docs/desk-coordination-plan.md, option B step 1). A lead takes one for an
//! item that will move or rewrite part of the code (`HOLD: #<id> | area: <globs> | till: <words> | why: …`); Cue keeps
//! it against the repository, not the Desk, so every Desk on that repository sees it: in its lead's Desk block and
//! under its header. It ends by itself when its item lands, is done, dropped or parked; `UNHOLD: #<id>` lifts it early.
//! Saved in desk_holds (db/desk.rs), one row an item.

use serde::{Deserialize, Serialize};

use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::core::view::desk_folder;
use crate::hub::Store;

/// One hold: item `item` of Desk `desk` holds `area` (globs, as the lead wrote them) of repository `repo` (its main
/// checkout's folder; see `repo_key`) till `till` (the lead's words: "it lands", a time), for `why`. `by`: the lead's
/// name then; `at`: when it was taken (ms).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Hold {
    pub item: String,
    pub desk: String,
    pub repo: String,
    pub area: Vec<String>,
    #[serde(default)]
    pub till: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub at: u64,
    /// When the user was asked about it standing an hour past its till (desk/escalate.rs; 0: never).
    #[serde(default)]
    pub asked: u64,
}

/// A change to the saved holds: one put (taken, or taken again), or item `.0`'s gone.
#[derive(Clone, Debug, PartialEq)]
pub enum HoldSave {
    Put(Hold),
    Gone(String),
}

/// The repository Desk `desk` works on, as holds are kept against it: its folder's main checkout when Cue knows it
/// (from memory: logs/repos.rs), else the folder itself; a Desk with no folder here (its lead on another machine)
/// goes by its project.
pub(crate) fn repo_key(st: &Store, desk: &str) -> String {
    let folder = desk_folder(st, desk);
    if folder.is_empty() {
        return format!("project:{}", st.desk.project_of(desk));
    }
    let key = crate::logs::repos::repo_root(&folder).map(|r| r.to_string_lossy().into_owned()).unwrap_or(folder);
    bare(&key)
}

/// A folder as holds compare it: macOS's /private/var and /tmp as plain /var and /tmp (one folder spelled two ways:
/// a Desk made from its real path, a session's working folder as its shell had it), no "/" at its end.
fn bare(p: &str) -> String {
    let p = p.strip_prefix("/private").filter(|r| r.starts_with('/')).unwrap_or(p);
    let t = p.trim_end_matches('/');
    if t.is_empty() { p.to_string() } else { t.to_string() }
}

/// Whether hold `h` is on the repository Desk `desk` works on. A hold taken before Cue knew the folder's repository
/// went by the folder: that still matches.
fn on_repo(st: &Store, desk: &str, h: &Hold) -> bool {
    h.desk == desk || h.repo == repo_key(st, desk) || h.repo == bare(&desk_folder(st, desk))
}

/// Whether hold `h` still stands: its item is there, not done, not parked, and its work not landed yet (on the main
/// line: to check, or waiting for an install).
pub(crate) fn standing(st: &Store, h: &Hold) -> bool {
    st.desk.item(&h.item).is_some_and(|i| {
        let landed = i.state == "to_check" || i.finish.as_ref().is_some_and(|f| matches!(f.step.as_str(), "queued" | "building" | "installing" | "starting"));
        i.state != "done" && i.state != "held" && !landed
    })
}

/// The holds standing on the repository Desk `desk` works on, its own among them, oldest first.
pub(crate) fn holds_on(st: &Store, desk: &str) -> Vec<Hold> {
    let mut out: Vec<Hold> = st.desk.holds.iter().filter(|h| standing(st, h) && on_repo(st, desk, h)).cloned().collect();
    out.sort_by_key(|h| h.at);
    out
}

/// Hold `h` in words, the holding Desk's name first: "cue #509 holds a, b till it lands (why)".
pub(crate) fn words(st: &Store, h: &Hold) -> String {
    let why = if h.why.is_empty() { String::new() } else { format!(" ({})", h.why) };
    format!("{} #{} holds {} till {}{why}", st.desk.name_of(&h.desk), h.item, h.area.join(", "), till(h))
}

fn till(h: &Hold) -> &str {
    if h.till.is_empty() { "it lands" } else { &h.till }
}

/// The line on the holds in Desk `desk`'s summary for its lead, when there are any: every Desk's on its repository.
pub(crate) fn summary_line(st: &Store, desk: &str) -> Option<String> {
    let holds = holds_on(st, desk);
    (!holds.is_empty()).then(|| {
        let each: Vec<String> = holds.iter().map(|h| words(st, h) + if h.desk == desk { " [this Desk's]" } else { "" }).collect();
        format!("Holds on this code: {}. Before routing into a held part, wait after its item or ask its lead.\n", each.join("; "))
    })
}

/// The holds on Desk `desk`'s repository, as its view shows them under its header.
pub(crate) fn holds_view(st: &Store, desk: &str) -> serde_json::Value {
    let list: Vec<serde_json::Value> = holds_on(st, desk).iter().map(|h| serde_json::json!({
        "item": h.item, "desk": h.desk, "desk_name": st.desk.name_of(&h.desk), "area": h.area, "till": till(h),
        "why": h.why, "by": h.by, "at": h.at, "mine": h.desk == desk,
    })).collect();
    serde_json::Value::Array(list)
}

/// The lead of Desk `desk` (`by`) said `HOLD: #<id> | area: … | till: … | why: …`: the hold taken (or taken again,
/// replacing its last), saved with `fx`, and every other Desk lead on the repository told of it in a band
/// (desk/tell.rs). Both are told the open items on other Desks whose copies already changed a held file (`changes`,
/// desk/hold_check.rs): each other lead only its own. What the lead is told of it, held or refused.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hold_said(st: &mut Store, desk: &str, by: &crate::desk::hub::turn::route::By, id: &str, area: Vec<String>, till_words: String, why: String, changes: &crate::desk::hold_check::Changes, now: u64, fx: &mut Effects) -> String {
    if let Err(no) = holdable(st, desk, id) {
        return format!("Refused: HOLD #{id}: {no}");
    }
    if area.is_empty() {
        return format!("Refused: HOLD #{id}: it names no code. You can write the files it holds after area:, as HOLD: #{id} | area: src/hub/*.rs, ui/desk_*.js | till: it lands | why: <reason>.");
    }
    let h = Hold { item: id.to_string(), desk: desk.to_string(), repo: repo_key(st, desk), area, till: till_words.trim().to_string(), why: why.trim().to_string(), by: by.name.to_string(), at: now, asked: 0 };
    let mut said = format!("Held: #{id} holds {} till {}. Every Desk on this repository sees it until #{id} lands, is dropped or parks, or you write UNHOLD: #{id}.", h.area.join(", "), till(&h));
    st.desk.holds.retain(|x| x.item != id);
    st.desk.holds.push(h.clone());
    let others = crate::desk::tell::others_on_repo(st, desk);
    if !others.is_empty() {
        let why = if h.why.is_empty() { String::new() } else { format!(" ({})", h.why) };
        let words = format!("{} holds {} for #{id} till {}{why}", st.desk.name_of(desk), h.area.join(", "), till(&h));
        let clashes = crate::desk::hold_check::clashes(st, desk, &h.area, changes);
        let mut told = Vec::new();
        for d in &others {
            let own = crate::desk::hold_check::own_clash_words(d, &clashes);
            let words = if own.is_empty() { words.clone() } else { format!("{words}.{own}") };
            told.extend(crate::desk::tell::send(st, desk, by.name, by.sid, std::slice::from_ref(d), Some(id.to_string()), &words, false, now, fx));
        }
        said.push_str(&format!(" Told {}. {}", told.join(", "), crate::desk::hold_check::clash_words(st, &clashes)));
    }
    fx.holds.push(HoldSave::Put(h));
    said
}

/// Whether Desk `desk`'s lead may hold item `id`: one of its own, open and not parked.
fn holdable(st: &Store, desk: &str, id: &str) -> Result<(), String> {
    match st.desk.item(id) {
        None => Err(format!("this Desk has no item #{id}. You can hold one of this Desk's own items.")),
        Some(i) if i.project != desk => Err(format!("#{id} is on {}'s Desk, not this one. You can ask its lead to hold it.", st.desk.name_of(&i.project))),
        Some(i) if i.state == "done" => Err(format!("#{id} is done already. You can hold an open item.")),
        Some(i) if i.state == "held" => Err(format!("#{id} is parked. You can hold it once it's routed again.")),
        Some(_) => Ok(()),
    }
}

/// The lead of Desk `desk` said `UNHOLD: #<id>`: its hold lifted. What the lead is told of it.
pub(crate) fn unhold_said(st: &mut Store, desk: &str, id: &str, fx: &mut Effects) -> String {
    let Some(h) = st.desk.holds.iter().find(|h| h.item == id && standing(st, h)).cloned() else {
        return format!("Refused: UNHOLD #{id}: #{id} holds no code. You can see the holds that stand in your Desk block.");
    };
    if h.desk != desk {
        return format!("Refused: UNHOLD #{id}: it's {}'s hold. You can ask its lead to lift it.", st.desk.name_of(&h.desk));
    }
    st.desk.holds.retain(|x| x.item != id);
    fx.holds.push(HoldSave::Gone(id.to_string()));
    // Items that waited at landing for it land now (desk/hold_check.rs).
    fx.land = true;
    format!("Unheld: #{id} no longer holds {}.", h.area.join(", "))
}

/// The holds whose item is done, dropped or parked, taken off: what to save of it.
pub(crate) fn ended(st: &mut Store) -> Vec<HoldSave> {
    let gone: Vec<String> = st.desk.holds.iter().filter(|h| !standing(st, h)).map(|h| h.item.clone()).collect();
    st.desk.holds.retain(|h| !gone.contains(&h.item));
    gone.into_iter().map(HoldSave::Gone).collect()
}
