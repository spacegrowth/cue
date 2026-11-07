//! Walking one function's body in evaluation order, keeping track of the locks it holds:
//! - a guard bound with `let` is held until `drop(guard)` or the end of its block;
//! - a guard not bound (a temporary: `lock(&X, "x").get(..)`) is held to the end of its statement, the
//!   way Rust 2021 drops temporaries (so through the body of a `match`, `if let`, `while let` or `for`
//!   whose head took it), except a plain `if`/`while` condition, which drops its own;
//! - a closure passed to a crate function or a spawn is its own frame (see `events::Frame`); one passed
//!   to anything else (`iter().map(..)`) or called by name runs in line.

mod args;
mod bindings;
mod calls;
pub mod classify;
pub mod events;
mod json;
mod visit;

use crate::parse::index::{FnDef, FnId, Index};
use crate::parse::resolve::Scope;
use events::{Ev, Facts, Frame};
use std::collections::BTreeMap;
use std::rc::Rc;
use syn::visit::Visit;

/// A lock taken the plain way on purpose, allowed by the lint's allow-list: where (file under src/ and
/// the receiver as written) and the name the lock graph gives it.
pub struct BareLock {
    pub file: String,
    pub receiver: String,
    pub name: String,
}

/// What every walk needs besides the function: the index, the functions that hand out a guard (with
/// the lock's name) and the allowed plain locks.
pub struct Context<'i, 'a> {
    pub ix: &'i Index<'a>,
    pub guard_fns: &'i BTreeMap<FnId, String>,
    pub bare_allowed: &'i [BareLock],
}

/// The facts of function `id`.
pub fn walk(cx: &Context, id: FnId) -> Facts {
    let f = &cx.ix.fns[id];
    let mut w = Walker::new(cx, f);
    w.visit_block(f.body);
    let state = w.frames.pop().expect("the function's own frame");
    Facts { events: state.events, ..w.facts }
}

struct Guard {
    scope: usize,
    ident: String,
    lock: String,
}

struct LocalClosure {
    scope: usize,
    ident: String,
    closure: Rc<syn::ExprClosure>,
    used: bool,
}

/// Where a frame's own locks start in `guards` and `temps`, and its events so far.
struct FrameState {
    guards: usize,
    temps: usize,
    events: Vec<Ev>,
}

struct Walker<'w, 'i, 'a> {
    cx: &'w Context<'i, 'a>,
    f: &'w FnDef<'a>,
    scope: usize,
    stmt: usize,
    guards: Vec<Guard>,
    /// Temporaries holding a lock: (the statement depth they end with, lock name).
    temps: Vec<(usize, String)>,
    closures: Vec<LocalClosure>,
    /// Local variables of a known crate type: (scope, name, type).
    types: Vec<(usize, String, String)>,
    frames: Vec<FrameState>,
    /// Closures being walked in line now (a closure that calls itself through a name is walked once).
    inlining: Vec<*const syn::ExprClosure>,
    facts: Facts,
}

impl<'w, 'i, 'a> Walker<'w, 'i, 'a> {
    fn new(cx: &'w Context<'i, 'a>, f: &'w FnDef<'a>) -> Self {
        Walker {
            cx,
            f,
            scope: 0,
            stmt: 0,
            guards: vec![],
            temps: vec![],
            closures: vec![],
            types: vec![],
            frames: vec![FrameState { guards: 0, temps: 0, events: vec![] }],
            inlining: vec![],
            facts: Facts::default(),
        }
    }

    fn scope_of(&self) -> Scope<'_> {
        Scope { module: &self.f.module, self_ty: self.f.self_ty.as_deref() }
    }

    fn rel(&self) -> &str {
        &self.cx.ix.files[self.f.file].rel
    }

    /// The locks this frame holds now, each once, oldest first.
    fn held(&self) -> Vec<String> {
        let frame = self.frames.last().expect("a frame");
        let mut out: Vec<String> = vec![];
        for lock in self.guards[frame.guards..].iter().map(|g| &g.lock).chain(self.temps[frame.temps..].iter().map(|(_, l)| l)) {
            if !out.contains(lock) {
                out.push(lock.clone());
            }
        }
        out
    }

    fn push(&mut self, ev: Ev) {
        self.frames.last_mut().expect("a frame").events.push(ev);
    }

    /// `body` walked as a frame of its own (its locks start from none), recorded as `frame`.
    fn in_frame(&mut self, frame: Frame, line: usize, body: impl FnOnce(&mut Self)) {
        let held = self.held();
        self.frames.push(FrameState { guards: self.guards.len(), temps: self.temps.len(), events: vec![] });
        body(self);
        let state = self.frames.pop().expect("the frame just pushed");
        self.push(Ev::Frame { frame, line, held, events: state.events });
    }

    /// `body` with temporaries of its own, dropped at its end (a statement, an `if` condition).
    fn temp_scope(&mut self, body: impl FnOnce(&mut Self)) {
        self.stmt += 1;
        body(self);
        let depth = self.stmt;
        self.temps.retain(|(d, _)| *d < depth);
        self.stmt -= 1;
    }

    fn enter_scope(&mut self) {
        self.scope += 1;
    }

    /// The end of a block: its guards are let go; closures it bound but never used are walked as
    /// frames of their own (whatever runs them, it isn't this body in line).
    fn exit_scope(&mut self) {
        let depth = self.scope;
        let unused: Vec<Rc<syn::ExprClosure>> = self.closures.iter().filter(|c| c.scope >= depth && !c.used).map(|c| c.closure.clone()).collect();
        for c in unused {
            let line = c.or1_token.span.start().line;
            self.in_frame(Frame::Unfollowed, line, |w| w.closure_body(&c));
        }
        self.guards.retain(|g| g.scope < depth);
        self.closures.retain(|c| c.scope < depth);
        self.types.retain(|t| t.0 < depth);
        self.scope -= 1;
    }

    fn closure_body(&mut self, c: &syn::ExprClosure) {
        self.enter_scope();
        self.visit_expr(&c.body);
        self.exit_scope();
    }

    /// A closure bound to a name, called (or passed on) here: walked in line, once at a time.
    fn inline_closure(&mut self, ident: &str) {
        let Some(c) = self.closures.iter_mut().rev().find(|c| c.ident == ident) else { return };
        c.used = true;
        let closure = c.closure.clone();
        let ptr = Rc::as_ptr(&closure);
        if self.inlining.contains(&ptr) {
            return;
        }
        self.inlining.push(ptr);
        self.closure_body(&closure);
        self.inlining.pop();
    }

    fn is_local_closure(&self, ident: &str) -> bool {
        self.closures.iter().any(|c| c.ident == ident)
    }

    /// The index of the function's parameter called `ident`.
    fn param_index(&self, ident: &str) -> Option<usize> {
        self.f.params.iter().position(|p| p.ident.as_deref() == Some(ident))
    }

}

fn pat_ident(p: &syn::Pat) -> Option<String> {
    match p {
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        syn::Pat::Type(t) => pat_ident(&t.pat),
        _ => None,
    }
}

fn strip_parens(e: &syn::Expr) -> &syn::Expr {
    match e {
        syn::Expr::Paren(p) => strip_parens(&p.expr),
        _ => e,
    }
}

/// The single identifier an expression is (`free`, `lead_free`), if it's one.
fn single_ident(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => Some(p.path.segments[0].ident.to_string()),
        _ => None,
    }
}

fn path_segs(p: &syn::Path) -> Vec<String> {
    p.segments.iter().map(|s| s.ident.to_string()).collect()
}
