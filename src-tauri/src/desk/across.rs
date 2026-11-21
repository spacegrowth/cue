//! An item of another Desk on the same code (#540, docs/desk-coordination-plan.md, option B step 3): `after:` and
//! LAND FIRST may name one, as `#509` (item numbers are the app's, not a Desk's) or `cue#509` / `cue #509` (its Desk's
//! name first, checked against the item's Desk). "The same code" is holds.rs's: the same repository (`repo_key`).
//! A Desk on other code is refused. The wait it makes is an item's `after` like any other, so the one waits list
//! (desk/hub/core/waits.rs), its circle check and `release` (desk/hub/turn/route.rs) cover it as they do within a Desk.

use crate::desk::holds::repo_key;
use crate::hub::Store;

/// An item as a line names it: ("cue", "509") for "cue#509" or "cue #509", ("", "509") for "#509" or "509". Lower case.
pub(crate) fn split(raw: &str) -> (String, String) {
    let raw = raw.trim().trim_matches(['`', '*', '"']).trim();
    match raw.rsplit_once('#') {
        Some((desk, id)) => (desk.trim().to_ascii_lowercase(), id.trim().to_ascii_lowercase()),
        None => (String::new(), raw.to_ascii_lowercase()),
    }
}

/// Whether Desks `a` and `b` work on the same code (the same Desk, or the same repository).
pub(crate) fn same_code(st: &Store, a: &str, b: &str) -> bool {
    a == b || repo_key(st, a) == repo_key(st, b)
}

/// Whether `name` (as a line wrote it, lower case) names Desk `d`: its id or its name.
fn names(st: &Store, d: &str, name: &str) -> bool {
    d.eq_ignore_ascii_case(name) || st.desk.name_of(d).eq_ignore_ascii_case(name)
}

/// The item `raw` names, for Desk `desk`'s lead: Ok(its id, canonical) when it's this Desk's or another Desk's on the
/// same code, or Cue has no such item (the caller says so in its own words). Err: why not, in words that follow
/// "#<id> " ("is on Elsewhere's Desk, which works on other code").
pub(crate) fn named(st: &Store, desk: &str, raw: &str) -> Result<String, String> {
    let (prefix, id) = split(raw);
    let Some(it) = st.desk.item(&id) else { return Ok(id) };
    let (id, of) = (it.id.clone(), it.project.clone());
    if !prefix.is_empty() && !names(st, &of, &prefix) {
        return Err(format!("is on {}'s Desk, not {prefix}'s", st.desk.name_of(&of)));
    }
    if !same_code(st, desk, &of) {
        return Err(format!("is on {}'s Desk, which works on other code", st.desk.name_of(&of)));
    }
    Ok(id)
}
