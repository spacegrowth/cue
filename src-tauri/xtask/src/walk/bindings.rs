//! `let` bindings: a closure bound to a name (walked where it's called), a variable of a known crate
//! type, and a guard (held until `drop` or the end of its block).

use super::{pat_ident, strip_parens, Guard, LocalClosure, Walker};
use std::rc::Rc;
use syn::visit::Visit;

impl Walker<'_, '_, '_> {
    pub(super) fn local(&mut self, l: &syn::Local) {
        let ident = pat_ident(&l.pat);
        let Some(init) = &l.init else { return };
        if let (Some(name), syn::Expr::Closure(c)) = (&ident, strip_parens(&init.expr)) {
            self.closures.push(LocalClosure { scope: self.scope, ident: name.clone(), closure: Rc::new(c.clone()), used: false });
            return;
        }
        self.visit_expr(&init.expr);
        if let Some((_, otherwise)) = &init.diverge {
            self.visit_expr(otherwise);
        }
        let Some(name) = ident else { return };
        if let syn::Pat::Type(t) = &l.pat {
            if let Some(ty) = self.cx.ix.type_hint(&crate::parse::sig::tokens(&t.ty)) {
                self.types.push((self.scope, name.clone(), ty));
            }
        }
        if let Some(lock) = self.guard_lock(&init.expr) {
            self.guards.push(Guard { scope: self.scope, ident: name, lock });
        }
    }

    /// The lock a `let` initializer hands out a guard of, if it does (`locking::lock(..)`, `self.st()`,
    /// `wait_turn(&gate)?`, through `&`, `*`, `?`, `.unwrap()`).
    pub(super) fn guard_lock(&self, e: &syn::Expr) -> Option<String> {
        match e {
            syn::Expr::Paren(p) => self.guard_lock(&p.expr),
            syn::Expr::Reference(r) => self.guard_lock(&r.expr),
            syn::Expr::Unary(u) => self.guard_lock(&u.expr),
            syn::Expr::Try(t) => self.guard_lock(&t.expr),
            syn::Expr::MethodCall(m) if m.method == "unwrap" || m.method == "expect" => self.guard_lock(&m.receiver),
            syn::Expr::Call(c) => self.call_acquires(c),
            syn::Expr::MethodCall(m) => self.method_acquires(m),
            _ => None,
        }
    }

    /// `drop(ident)`: the guard bound to `ident` (the latest so named) is let go.
    pub(super) fn release(&mut self, ident: &str) {
        if let Some(i) = self.guards.iter().rposition(|g| g.ident == ident) {
            self.guards.remove(i);
        }
    }
}
