//! How the arguments of a call run: code passed to a crate function is a frame of its own (the function
//! decides when and under what it runs), code passed to a spawn is a thread's, anything else runs in line.

use super::calls::Args;
use super::events::{Ev, Frame};
use super::{path_segs, single_ident, Walker};
use crate::parse::index::FnId;
use crate::parse::resolve::Resolved;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::Expr;

/// How the arguments of a call run.
#[derive(Clone, Copy)]
pub(super) enum ArgMode {
    /// In line with the call (an outside function: `iter().map(|x| …)` runs its closure now).
    Inline,
    /// Passed to crate function `target`, whose parameters start at argument `offset` (1 for a method
    /// called by path, `Type::m(recv, …)`).
    Crate { target: FnId, offset: usize },
}

impl Walker<'_, '_, '_> {
    /// A thread or task started here: what it's handed runs in a frame of its own.
    pub(super) fn spawn_call(&mut self, label: String, args: &Args, line: usize) {
        self.in_frame(Frame::Spawn(label), line, |w| {
            for a in args {
                w.code_arg(a);
            }
        });
    }

    pub(super) fn args(&mut self, args: &Args, mode: ArgMode) {
        for (i, a) in args.iter().enumerate() {
            match mode {
                ArgMode::Crate { target, offset } if i >= offset => self.crate_arg(a, target, i - offset),
                _ => self.code_arg(a),
            }
        }
    }

    /// An argument run in line with where it is: a closure's body, a closure or parameter named here
    /// (called), a crate function named as a value (called), or any other expression.
    fn code_arg(&mut self, a: &Expr) {
        if let Some(ident) = single_ident(a) {
            if self.is_local_closure(&ident) {
                return self.inline_closure(&ident);
            }
            if let Some(index) = self.param_index(&ident).filter(|&i| self.f.params[i].fn_like) {
                let held = self.held();
                return self.push(Ev::ParamCall { index, line: a.span().start().line, held });
            }
        }
        match a {
            Expr::Async(b) => self.visit_block(&b.block),
            Expr::Path(p) => self.fn_value(&p.path, a.span().start().line),
            _ => self.visit_expr(a),
        }
    }

    /// An argument passed to crate function `target` as its parameter `index`: code in it is a frame.
    fn crate_arg(&mut self, a: &Expr, target: FnId, index: usize) {
        let line = a.span().start().line;
        let frame = Frame::ClosureArg { target, index };
        match a {
            Expr::Closure(c) => self.in_frame(frame, line, |w| w.closure_body(c)),
            _ => match single_ident(a) {
                Some(ident) if self.is_local_closure(&ident) => self.in_frame(frame, line, |w| w.inline_closure(&ident)),
                Some(ident) if self.param_index(&ident).is_some_and(|i| self.f.params[i].fn_like) => {
                    let held = self.held();
                    let own = self.param_index(&ident).expect("checked");
                    self.push(Ev::ParamPass { index: own, target, target_index: index, line, held });
                }
                _ if self.names_crate_fn(a) => self.in_frame(frame, line, |w| w.code_arg(a)),
                _ => self.visit_expr(a),
            },
        }
    }

    fn names_crate_fn(&self, a: &Expr) -> bool {
        let Expr::Path(p) = a else { return false };
        matches!(self.cx.ix.resolve_path(&self.scope_of(), &path_segs(&p.path)), Resolved::Crate(_))
    }

    /// A path used as a value: a crate function named so (`.map(leads::name_of)`) is called by whoever takes it.
    fn fn_value(&mut self, path: &syn::Path, line: usize) {
        if let Resolved::Crate(ids) = self.cx.ix.resolve_path(&self.scope_of(), &path_segs(path)) {
            let held = self.held();
            for &target in &ids {
                self.push(Ev::Call { target, line, held: held.clone() });
            }
        }
    }
}
