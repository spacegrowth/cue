//! Reading a function's events with what the functions it calls do (their summaries): each lock taken,
//! call made, parameter called and effect, with every lock held at that moment, including the ones a
//! crate function holds while it runs a closure it was handed.

use super::summary::Summary;
use crate::parse::index::FnId;
use crate::walk::events::{Effect, Ev, Frame};
use std::collections::{BTreeMap, BTreeSet};

pub type Held = BTreeSet<String>;

/// Where the events run: with which locks already held by the code around them, and whether in line with
/// the function (`attached`; false for a thread it starts, or a closure handed off to run later).
/// `lenders`: of those locks, the ones held by a crate function that runs this code as a closure it was
/// handed (`leads::missed` holding INDEX while it calls `lead_free`), by lock.
#[derive(Clone)]
pub struct Ctx {
    pub outer: Held,
    pub attached: bool,
    pub lenders: BTreeMap<String, FnId>,
}

impl Ctx {
    pub fn top() -> Ctx {
        Ctx { outer: Held::new(), attached: true, lenders: BTreeMap::new() }
    }

    fn detached(held: Held) -> Ctx {
        Ctx { outer: held, attached: false, lenders: BTreeMap::new() }
    }

    /// Run by `target` under its locks `lent` (on top of these).
    fn lent_by(&self, held: &[String], target: FnId, lent: &Held) -> Ctx {
        let mine = self.with(held);
        let mut lenders = self.lenders.clone();
        for lock in lent.difference(&mine) {
            lenders.insert(lock.clone(), target);
        }
        Ctx { outer: mine.union(lent).cloned().collect(), attached: self.attached, lenders }
    }

    fn with(&self, held: &[String]) -> Held {
        self.outer.iter().cloned().chain(held.iter().cloned()).collect()
    }
}

pub trait Observer {
    fn lock(&mut self, name: &str, line: usize, held: &Held, ctx: &Ctx);
    fn call(&mut self, target: FnId, line: usize, held: &Held, ctx: &Ctx);
    fn param(&mut self, index: usize, held: &Held, attached: bool);
    fn effect(&mut self, effect: Effect, detail: &str, line: usize, held: &Held, ctx: &Ctx);
}

pub fn interpret(events: &[Ev], ctx: &Ctx, sums: &[Summary], obs: &mut impl Observer) {
    for ev in events {
        match ev {
            Ev::Lock { name, line, held } => obs.lock(name, *line, &ctx.with(held), ctx),
            Ev::Call { target, line, held } => obs.call(*target, *line, &ctx.with(held), ctx),
            Ev::ParamCall { index, held, .. } => obs.param(*index, &ctx.with(held), ctx.attached),
            Ev::ParamPass { index, target, target_index, held, .. } => {
                let used = sums[*target].params.get(target_index);
                if let Some(s) = used.and_then(|u| u.attached.as_ref()) {
                    obs.param(*index, &ctx.with(held).union(s).cloned().collect(), ctx.attached);
                }
                if let Some(s) = used.and_then(|u| u.detached.as_ref()) {
                    obs.param(*index, s, false);
                }
            }
            Ev::Effect { effect, detail, line, held } => obs.effect(*effect, detail, *line, &ctx.with(held), ctx),
            Ev::Frame { frame, held, events, .. } => frame_events(frame, held, events, ctx, sums, obs),
        }
    }
}

/// A frame's events, in the context its code runs in: a spawned thread's or an unfollowed closure's on
/// its own; a closure argument's as the function it's handed to runs it (in line under that function's
/// locks and the caller's, or on a thread of its own, or never seen run: kept for later).
fn frame_events(frame: &Frame, held: &[String], events: &[Ev], ctx: &Ctx, sums: &[Summary], obs: &mut impl Observer) {
    let Frame::ClosureArg { target, index } = frame else {
        return interpret(events, &Ctx::detached(Held::new()), sums, obs);
    };
    let used = sums[*target].params.get(index);
    if let Some(s) = used.and_then(|u| u.attached.as_ref()) {
        interpret(events, &ctx.lent_by(held, *target, s), sums, obs);
    }
    if let Some(s) = used.and_then(|u| u.detached.as_ref()) {
        interpret(events, &Ctx::detached(s.clone()), sums, obs);
    }
    if used.is_none_or(|u| u.attached.is_none() && u.detached.is_none()) {
        interpret(events, &Ctx::detached(Held::new()), sums, obs);
    }
}
