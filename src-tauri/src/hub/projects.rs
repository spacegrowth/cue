//! The projects the store keeps with each card, session and answer, derived from its folder's facts
//! (logs::repos). Those are read from memory: a folder not looked up yet says only its own name, so when the
//! lookups bring in something new, every project kept is derived again.

use super::*;

impl Hub {
    /// Folders' facts came in or changed (the tick's repos::refresh): the projects kept, derived again, and
    /// the parked records that changed saved.
    pub fn folders_found(&self) {
        let (changed, parked) = self.write(reproject);
        for sid in &parked {
            self.save_parked(sid);
        }
        if changed {
            self.changed();
        }
    }
}

/// Every project the store keeps, derived again: whether any changed, and the parked sessions whose did.
fn reproject(st: &mut Store) -> (bool, Vec<String>) {
    let mut changed = false;
    for it in st.items.iter_mut() {
        changed |= set(&mut it.project, crate::model::project_of_origin(&it.origin));
    }
    changed |= st.history.change_each(|it| set(&mut it.project, crate::model::project_of_origin(&it.origin)));
    changed |= st.sessions.reproject();
    for (_, s) in st.ended.iter_mut() {
        changed |= set(&mut s.project, crate::model::project_of_origin(&s.origin));
    }
    let mut parked = vec![];
    for p in st.parked.iter_mut() {
        if set(&mut p.session.project, crate::model::project_of_origin(&p.session.origin)) {
            parked.push(p.session.origin.session_id.clone());
        }
    }
    // An answer keeps only its session: it says that session's project.
    let of: HashMap<String, String> = st.history.iter().map(|i| (i.origin.session_id.clone(), i.project.clone())).collect();
    for a in st.answers.iter_mut() {
        if let Some(p) = of.get(&a.session_id) {
            changed |= set(&mut a.project, p.clone());
        }
    }
    (changed || !parked.is_empty(), parked)
}

/// `slot` set to `to`; true when that changed it.
fn set(slot: &mut String, to: String) -> bool {
    let differs = *slot != to;
    if differs {
        *slot = to;
    }
    differs
}
