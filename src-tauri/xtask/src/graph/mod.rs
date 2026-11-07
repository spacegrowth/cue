//! The crate's lock graph and its external calls under the hub's store, from every function's events
//! and summaries. `Model` owns everything the report and the checks read.

mod cycles;
pub mod interpret;
pub mod summary;

use crate::parse::index::{FnId, Index};
use crate::walk::events::{Effect, Ev, Facts};
use crate::walk::{self, BareLock, Context};
use interpret::{interpret, Ctx, Held, Observer};
use std::collections::{BTreeMap, BTreeSet};
use summary::Summary;

pub struct FnInfo {
    pub path: String,
    pub name: String,
    pub rel: String,
    pub line: usize,
    pub is_async: bool,
    pub is_command: bool,
}

/// Where something was seen: in `holder` at `line`, directly or by calling `callee`; `lender`: the
/// function holding the lock this is under, when that's one running `holder`'s closure.
#[derive(Clone, Copy, Debug)]
pub struct Witness {
    pub holder: FnId,
    pub line: usize,
    pub callee: Option<FnId>,
    pub lender: Option<FnId>,
}

/// A lock or once-cell declaration, with the names `locking::lock` calls give it.
pub struct LockInfo {
    pub rel: String,
    pub line: usize,
    pub owner: String,
    pub ty: String,
    pub names: BTreeSet<String>,
}

/// An external call reachable while the store is held: by whom, of what kind, made where.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoreIoKey {
    pub holder: String,
    pub effect: Effect,
    pub site: String,
}

pub struct Model {
    pub fns: Vec<FnInfo>,
    pub facts: Vec<Facts>,
    pub sums: Vec<Summary>,
    /// (held, taken) → the first place it happens.
    pub edges: BTreeMap<(String, String), Witness>,
    /// Edges on a cycle (a self-edge is a lock taken again by the thread that holds it).
    pub cyclic: BTreeSet<(String, String)>,
    pub store_lock: String,
    pub store_io: BTreeMap<StoreIoKey, (String, Witness)>,
    pub locks: Vec<LockInfo>,
    /// Lock names taken somewhere but matched to no declaration by identifier.
    pub unmatched_names: BTreeSet<String>,
    pub missing_modules: Vec<String>,
}

pub fn analyze(ix: &Index, missing: &[String], bare_allowed: &[BareLock], store_lock: &str) -> Model {
    let guard_fns = guard_fns(ix, bare_allowed);
    let cx = Context { ix, guard_fns: &guard_fns, bare_allowed };
    let facts: Vec<Facts> = (0..ix.fns.len()).map(|id| walk::walk(&cx, id)).collect();
    let events: Vec<Vec<Ev>> = facts.iter().map(|f| f.events.clone()).collect();
    let is_db = |id: FnId| ix.fns[id].module == ["db"];
    let sums = summary::summarize(&events, &is_db);
    let mut obs = Graph { sums: &sums, store_lock, id: 0, is_db: &is_db, paths: ix.fns.iter().map(|f| f.path()).collect(), edges: BTreeMap::new(), store_io: BTreeMap::new() };
    for (id, ev) in events.iter().enumerate() {
        obs.id = id;
        interpret(ev, &Ctx::top(), &sums, &mut obs);
    }
    let (edges, store_io) = (obs.edges, obs.store_io);
    let cyclic = cycles::cyclic_edges(edges.keys());
    let (locks, unmatched_names) = lock_infos(ix, &facts);
    let fns = ix.fns.iter().enumerate().map(|(id, f)| FnInfo { path: f.path(), name: f.name.clone(), rel: ix.rel(id).to_string(), line: f.line, is_async: f.is_async, is_command: f.is_command }).collect();
    Model { fns, facts, sums, edges, cyclic, store_lock: store_lock.to_string(), store_io, locks, unmatched_names, missing_modules: missing.to_vec() }
}

/// Functions that hand out a guard: the ones whose return type names one and whose body takes exactly
/// one lock (`Hub::st` → "the hub's store").
fn guard_fns(ix: &Index, bare_allowed: &[BareLock]) -> BTreeMap<FnId, String> {
    let none = BTreeMap::new();
    let cx = Context { ix, guard_fns: &none, bare_allowed };
    let mut out = BTreeMap::new();
    for id in (0..ix.fns.len()).filter(|&id| ix.fns[id].returns_guard && ix.fns[id].module != ["locking"]) {
        let names: BTreeSet<String> = walk::walk(&cx, id).events.iter().filter_map(|e| if let Ev::Lock { name, .. } = e { Some(name.clone()) } else { None }).collect();
        if names.len() == 1 {
            out.insert(id, names.into_iter().next().expect("one"));
        }
    }
    out
}

/// Declarations with the names their lock calls give them, matched by the identifier in the call's
/// first argument (`&INDEX`, `&self.store`): one in the same file first, else the only one in the crate.
fn lock_infos(ix: &Index, facts: &[Facts]) -> (Vec<LockInfo>, BTreeSet<String>) {
    let mut locks: Vec<LockInfo> = ix.locks.iter().map(|d| LockInfo { rel: ix.files[d.file].rel.clone(), line: d.line, owner: d.owner.clone(), ty: d.ty.clone(), names: BTreeSet::new() }).collect();
    let mut unmatched = BTreeSet::new();
    for (id, f) in facts.iter().enumerate() {
        for site in f.lock_sites.iter().filter(|s| !s.idents.is_empty()) {
            let named = |i: usize| site.idents.iter().any(|x| x != "self" && *x == ix.locks[i].ident);
            let here: Vec<usize> = (0..locks.len()).filter(|&i| named(i) && ix.locks[i].file == ix.fns[id].file).collect();
            let anywhere: Vec<usize> = (0..locks.len()).filter(|&i| named(i)).collect();
            match (here.as_slice(), anywhere.as_slice()) {
                ([one], _) | ([], [one]) => {
                    locks[*one].names.insert(site.name.clone());
                }
                _ => {
                    unmatched.insert(site.name.clone());
                }
            }
        }
    }
    locks.sort_by(|a, b| (&a.rel, a.line).cmp(&(&b.rel, b.line)));
    (locks, unmatched)
}

impl Model {
    /// How `w` reaches `lock`: `a (file:line) → b (file:line) → … takes it (file:line)`.
    pub fn lock_chain(&self, w: &Witness, lock: &str) -> String {
        self.chain(w, |id| self.sums[id].locks.get(lock).copied(), "takes it")
    }

    /// How `w` reaches the effect `(effect, site)`.
    pub fn effect_chain(&self, w: &Witness, effect: Effect, site: FnId) -> String {
        self.chain(w, |id| self.sums[id].effects.get(&(effect, site)).map(|(_, s)| *s), "calls out")
    }

    fn chain(&self, w: &Witness, step_of: impl Fn(FnId) -> Option<summary::Step>, end: &str) -> String {
        let at = |id: FnId, line: usize| format!("{} ({}:{line})", self.fns[id].path, self.fns[id].rel);
        let mut out = vec![at(w.holder, w.line)];
        if let Some(l) = w.lender {
            out[0] = format!("{} in a closure {} runs holding the lock", out[0], self.fns[l].path);
        }
        let mut next = w.callee;
        while let (Some(id), true) = (next, out.len() < 40) {
            let Some(step) = step_of(id) else { break };
            out.push(at(id, step.line));
            next = step.callee;
        }
        format!("{}, {end}", out.join(" → "))
    }
}

/// The observer of the final pass: lock edges, and external calls under the store.
struct Graph<'g> {
    sums: &'g [Summary],
    store_lock: &'g str,
    id: FnId,
    is_db: &'g dyn Fn(FnId) -> bool,
    paths: Vec<String>,
    edges: BTreeMap<(String, String), Witness>,
    store_io: BTreeMap<StoreIoKey, (String, Witness)>,
}

impl Graph<'_> {
    fn edge(&mut self, held: &Held, taken: &str, w: Witness, ctx: &Ctx) {
        for h in held {
            let w = Witness { lender: ctx.lenders.get(h).copied(), ..w };
            self.edges.entry((h.clone(), taken.to_string())).or_insert(w);
        }
    }

    fn io(&mut self, effect: Effect, site: FnId, detail: &str, w: Witness) {
        let key = StoreIoKey { holder: self.paths[self.id].clone(), effect, site: self.paths[site].clone() };
        self.store_io.entry(key).or_insert_with(|| (detail.to_string(), w));
    }
}

impl Observer for Graph<'_> {
    fn lock(&mut self, name: &str, line: usize, held: &Held, ctx: &Ctx) {
        self.edge(held, name, Witness { holder: self.id, line, callee: None, lender: None }, ctx);
    }

    fn call(&mut self, target: FnId, line: usize, held: &Held, ctx: &Ctx) {
        let w = Witness { holder: self.id, line, callee: Some(target), lender: ctx.lenders.get(self.store_lock).copied() };
        let locks: Vec<String> = self.sums[target].locks.keys().cloned().collect();
        for lock in locks {
            self.edge(held, &lock, w, ctx);
        }
        if !held.contains(self.store_lock) || (self.is_db)(target) {
            return;
        }
        let effects: Vec<((Effect, FnId), String)> = self.sums[target].effects.iter().map(|(k, (d, _))| (*k, d.clone())).collect();
        for ((effect, site), detail) in effects {
            self.io(effect, site, &detail, w);
        }
    }

    fn param(&mut self, _: usize, _: &Held, _: bool) {}

    fn effect(&mut self, effect: Effect, detail: &str, line: usize, held: &Held, ctx: &Ctx) {
        if held.contains(self.store_lock) {
            self.io(effect, self.id, detail, Witness { holder: self.id, line, callee: None, lender: ctx.lenders.get(self.store_lock).copied() });
        }
    }
}
