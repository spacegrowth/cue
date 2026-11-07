//! Per function, what calling it may do, through everything it calls in line: the locks it takes, the
//! external calls it makes, and the locks held when it calls each closure parameter. Worked out to a
//! fixpoint over the whole crate (recursion included), deterministically: functions in source order,
//! and the first way found to reach a lock or effect is the one kept as its witness.

use super::interpret::{interpret, Ctx, Held, Observer};
use crate::parse::index::FnId;
use crate::walk::events::{Effect, Ev};
use std::collections::BTreeMap;

/// How a function reaches a lock or an effect: at `line` of its own body, or by calling `callee` there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub line: usize,
    pub callee: Option<FnId>,
}

/// The locks held when a closure parameter is called: in line, and on a thread (or later).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamUse {
    pub attached: Option<Held>,
    pub detached: Option<Held>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub locks: BTreeMap<String, Step>,
    /// (kind, the function that makes the call) → what it calls, and how this function reaches it.
    pub effects: BTreeMap<(Effect, FnId), (String, Step)>,
    pub params: BTreeMap<usize, ParamUse>,
}

/// The summaries of every function, `events[id]` being function `id`'s; `is_db`: a function of
/// `crate::db`, whose own file and SQLite calls are its "db" effect, not counted again.
pub fn summarize(events: &[Vec<Ev>], is_db: &dyn Fn(FnId) -> bool) -> Vec<Summary> {
    let mut sums = vec![Summary::default(); events.len()];
    for _ in 0..MAX_ROUNDS {
        let next: Vec<Summary> = (0..events.len())
            .map(|id| {
                let mut s = Summarize { id, sums: &sums, is_db, out: Summary::default() };
                interpret(&events[id], &Ctx::top(), &sums, &mut s);
                s.out
            })
            .collect();
        if next == sums {
            return sums;
        }
        sums = next;
    }
    panic!("the call-graph summaries didn't settle in {MAX_ROUNDS} rounds");
}

/// Far more rounds than the crate's deepest call chain needs; past it something doesn't settle.
const MAX_ROUNDS: usize = 200;

struct Summarize<'s> {
    id: FnId,
    sums: &'s [Summary],
    is_db: &'s dyn Fn(FnId) -> bool,
    out: Summary,
}

impl Observer for Summarize<'_> {
    fn lock(&mut self, name: &str, line: usize, _: &Held, ctx: &Ctx) {
        if ctx.attached {
            self.out.locks.entry(name.to_string()).or_insert(Step { line, callee: None });
        }
    }

    fn call(&mut self, target: FnId, line: usize, _: &Held, ctx: &Ctx) {
        if !ctx.attached {
            return;
        }
        let step = Step { line, callee: Some(target) };
        for lock in self.sums[target].locks.keys() {
            self.out.locks.entry(lock.clone()).or_insert(step);
        }
        if (self.is_db)(target) {
            return;
        }
        for (key, (detail, _)) in &self.sums[target].effects {
            self.out.effects.entry(*key).or_insert_with(|| (detail.clone(), step));
        }
    }

    fn param(&mut self, index: usize, held: &Held, attached: bool) {
        let used = self.out.params.entry(index).or_default();
        let set = if attached { &mut used.attached } else { &mut used.detached };
        set.get_or_insert_with(Held::new).extend(held.iter().cloned());
    }

    fn effect(&mut self, effect: Effect, detail: &str, line: usize, _: &Held, ctx: &Ctx) {
        if ctx.attached {
            self.out.effects.entry((effect, self.id)).or_insert_with(|| (detail.to_string(), Step { line, callee: None }));
        }
    }
}
