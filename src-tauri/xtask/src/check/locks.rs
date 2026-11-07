//! Locks taken the one way: through `locking::lock`/`read`/`write` with a name no other file uses (the
//! `locking` module itself, whose bookkeeping can't go through itself, aside).

use crate::allow::Allow;
use crate::graph::Model;
use std::collections::{BTreeMap, BTreeSet};

fn outside_locking(rel: &str) -> bool {
    !rel.starts_with("locking/")
}

/// Locks taken the plain way (`m.lock()`), and allowed ones nothing takes any more.
pub fn bare(model: &Model, allow: &Allow) -> Vec<String> {
    let mut out = vec![];
    for (id, facts) in model.facts.iter().enumerate() {
        let f = &model.fns[id];
        if !outside_locking(&f.rel) {
            continue;
        }
        for (line, receiver) in &facts.bare_locks {
            out.push(format!("{}:{line}: `{receiver}` locked another way than crate::locking::lock(&m, \"its name\") (or read, write)", f.rel));
        }
    }
    let taken: BTreeSet<&str> = model.facts.iter().flat_map(|f| f.lock_sites.iter().map(|s| s.name.as_str())).collect();
    out.extend(allow.bare_lock.iter().filter(|b| !taken.contains(b.name.as_str())).map(|b| format!("[[bare_lock]] {} in {} is never taken: remove it from docs/dev/lint-allow.toml", b.receiver, b.file)));
    out
}

/// A name given to locks in more than one file: two locks a log couldn't tell apart (a lock is used in
/// one file; others reach it through a function of that file).
pub fn shared_names(model: &Model) -> Vec<String> {
    let mut by_name: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (id, facts) in model.facts.iter().enumerate() {
        let rel = model.fns[id].rel.as_str();
        // Sites with an argument are `locking` calls; the others are calls of a function that hands out a guard.
        for site in facts.lock_sites.iter().filter(|s| !s.idents.is_empty() && outside_locking(rel)) {
            by_name.entry(site.name.as_str()).or_default().insert(rel);
        }
    }
    by_name.into_iter().filter(|(_, files)| files.len() > 1).map(|(n, files)| format!("\"{n}\" names a lock in {}", files.into_iter().collect::<Vec<_>>().join(" and "))).collect()
}
