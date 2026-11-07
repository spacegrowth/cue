//! Calls: who's called (a crate function, a closure by name, the function's own parameter, a spawn, an
//! outside effect), what's taken (a lock, a guard from a function that hands one out), and how each
//! argument that is code (a closure, a function named as a value) runs.

use super::classify::{self, BARE_LOCK_METHODS, DISK_METHODS};
use super::args::ArgMode;
use super::events::{Effect, Ev, LockSite};
use super::{path_segs, single_ident, Walker};
use crate::parse::index::FnId;
use crate::parse::resolve::Resolved;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::Expr;

pub(super) type Args = syn::punctuated::Punctuated<Expr, syn::Token![,]>;

/// The functions that take a lock: `crate::locking::{lock, read, write}`.
const LOCKING: &str = "locking";
const LOCK_FNS: &[&str] = &["lock", "read", "write"];

impl Walker<'_, '_, '_> {
    pub(super) fn call(&mut self, c: &syn::ExprCall) {
        let line = c.func.span().start().line;
        if let Some(ident) = single_ident(&c.func) {
            if ident == "drop" {
                return self.dropped(&c.args);
            }
            if self.is_local_closure(&ident) {
                self.args(&c.args, ArgMode::Inline);
                return self.inline_closure(&ident);
            }
            if let Some(index) = self.param_index(&ident) {
                self.args(&c.args, ArgMode::Inline);
                let held = self.held();
                return self.push(Ev::ParamCall { index, line, held });
            }
        }
        let Expr::Path(p) = &*c.func else {
            self.visit_expr(&c.func);
            self.args(&c.args, ArgMode::Inline);
            self.facts.unresolved += 1;
            return;
        };
        let segs = path_segs(&p.path);
        if segs.ends_with(&["mem".to_string(), "drop".to_string()]) {
            return self.dropped(&c.args);
        }
        match self.cx.ix.resolve_path(&self.scope_of(), &segs) {
            Resolved::Crate(ids) => {
                // A method called by path takes its receiver as the first argument.
                let offset = usize::from(self.cx.ix.fns[ids[0]].has_receiver);
                self.crate_call(&ids, &c.args, line, offset);
            }
            Resolved::Outside(path) => self.outside_call(&path, &c.args, line),
            Resolved::Unknown => {
                self.args(&c.args, ArgMode::Inline);
                self.facts.unresolved += 1;
            }
        }
    }

    pub(super) fn method(&mut self, m: &syn::ExprMethodCall) {
        self.visit_expr(&m.receiver);
        let line = m.method.span().start().line;
        let name = m.method.to_string();
        let target = self.method_target(m);
        let unknown = target == Resolved::Unknown;
        if m.args.is_empty() && BARE_LOCK_METHODS.contains(&name.as_str()) && target == Resolved::Unknown {
            return self.bare_lock(&m.receiver, line);
        }
        if name == "spawn" && !m.args.is_empty() {
            return self.spawn_call("Builder::spawn".into(), &m.args, line);
        }
        match target {
            Resolved::Crate(ids) => self.crate_call(&ids, &m.args, line, 0),
            _ => {
                self.args(&m.args, ArgMode::Inline);
                if m.args.is_empty() && DISK_METHODS.contains(&name.as_str()) {
                    let held = self.held();
                    self.push(Ev::Effect { effect: Effect::Fs, detail: format!("Path::{name}"), line, held });
                } else if unknown && self.cx.ix.method_owners.contains_key(&name) {
                    // A crate type has a method of this name: this call may be one, and isn't followed.
                    self.facts.unresolved += 1;
                }
            }
        }
    }

    /// The lock a call takes and hands out a guard of: a `locking` function's, or a guard function's.
    pub(super) fn call_acquires(&self, c: &syn::ExprCall) -> Option<String> {
        let Expr::Path(p) = &*c.func else { return None };
        match self.cx.ix.resolve_path(&self.scope_of(), &path_segs(&p.path)) {
            Resolved::Crate(ids) if self.is_lock_fn(ids[0]) => Some(lock_name(&c.args)),
            Resolved::Crate(ids) => self.cx.guard_fns.get(&ids[0]).cloned(),
            _ => None,
        }
    }

    pub(super) fn method_acquires(&self, m: &syn::ExprMethodCall) -> Option<String> {
        match self.method_target(m) {
            Resolved::Crate(ids) => self.cx.guard_fns.get(&ids[0]).cloned(),
            _ if m.args.is_empty() && BARE_LOCK_METHODS.contains(&m.method.to_string().as_str()) => self.allowed_bare(&m.receiver),
            _ => None,
        }
    }

    fn is_lock_fn(&self, id: FnId) -> bool {
        let f = &self.cx.ix.fns[id];
        f.module == [LOCKING] && f.self_ty.is_none() && LOCK_FNS.contains(&f.name.as_str())
    }

    fn crate_call(&mut self, ids: &[FnId], args: &Args, line: usize, offset: usize) {
        self.facts.followed += 1;
        if self.is_lock_fn(ids[0]) {
            self.args(args, ArgMode::Inline);
            let idents = args.first().map(idents_in).unwrap_or_default();
            return self.lock_taken(lock_name(args), line, idents);
        }
        self.args(args, ArgMode::Crate { target: ids[0], offset });
        let held = self.held();
        for &target in ids {
            self.push(Ev::Call { target, line, held: held.clone() });
        }
        let f = &self.cx.ix.fns[ids[0]];
        if f.module == ["db"] {
            self.push(Ev::Effect { effect: Effect::Db, detail: format!("db::{}", f.name), line, held });
        }
        if let Some(lock) = self.cx.guard_fns.get(&ids[0]).cloned() {
            self.facts.lock_sites.push(LockSite { line, name: lock.clone(), idents: vec![] });
            self.temps.push((self.stmt, lock));
        }
    }

    fn outside_call(&mut self, path: &[String], args: &Args, line: usize) {
        self.facts.outside += 1;
        if let Some(label) = classify::spawn(path) {
            return self.spawn_call(label, args, line);
        }
        self.args(args, ArgMode::Inline);
        let literal = args.first().and_then(str_literal);
        if let Some((effect, detail)) = classify::effect(path, literal.as_deref()) {
            let held = self.held();
            self.push(Ev::Effect { effect, detail, line, held });
        }
    }

    /// A lock taken here as a temporary (a `let` makes it a guard afterwards, see `local`).
    fn lock_taken(&mut self, name: String, line: usize, idents: Vec<String>) {
        let held = self.held();
        self.push(Ev::Lock { name: name.clone(), line, held });
        self.facts.lock_sites.push(LockSite { line, name: name.clone(), idents });
        self.temps.push((self.stmt, name));
    }

    /// `m.lock()` and the like: a lock taken another way than `locking::lock`. Allowed ones are locks
    /// like any other; the rest are listed for the lint.
    fn bare_lock(&mut self, receiver: &Expr, line: usize) {
        match self.allowed_bare(receiver) {
            Some(name) => self.lock_taken(name, line, idents_in(receiver)),
            None => self.facts.bare_locks.push((line, crate::parse::sig::tokens(receiver))),
        }
    }

    fn allowed_bare(&self, receiver: &Expr) -> Option<String> {
        let written = crate::parse::sig::tokens(receiver);
        self.cx.bare_allowed.iter().find(|b| b.file == self.rel() && b.receiver == written).map(|b| b.name.clone())
    }

    fn dropped(&mut self, args: &Args) {
        match args.first().and_then(single_ident) {
            Some(ident) => self.release(&ident),
            None => self.args(args, ArgMode::Inline),
        }
    }

    /// The method a call names: on `self`'s type, a typed variable's or field's, else the crate's only
    /// method of that name (see `Index::method_by_name`).
    fn method_target(&self, m: &syn::ExprMethodCall) -> Resolved {
        let name = m.method.to_string();
        match self.receiver_type(&m.receiver) {
            Some(ty) => self.cx.ix.method_on(&ty, &name),
            // `OpenOptions::new().create(true).open(p)`: a value an outside function made, never a crate
            // method however its name matches one.
            None if self.made_outside(&m.receiver) => Resolved::Outside(vec![name]),
            None => self.cx.ix.method_by_name(&name),
        }
    }

    /// Whether `e` is a chain of calls that starts at an outside function's result (`File::open(p)?`,
    /// `std::fs::OpenOptions::new().append(true)`).
    fn made_outside(&self, e: &Expr) -> bool {
        match e {
            Expr::Paren(p) => self.made_outside(&p.expr),
            Expr::Reference(r) => self.made_outside(&r.expr),
            Expr::Try(t) => self.made_outside(&t.expr),
            Expr::MethodCall(m) => self.receiver_type(&m.receiver).is_none() && self.made_outside(&m.receiver),
            Expr::Call(c) => match &*c.func {
                Expr::Path(p) => matches!(self.cx.ix.resolve_path(&self.scope_of(), &path_segs(&p.path)), Resolved::Outside(_)),
                _ => false,
            },
            _ => false,
        }
    }

    fn receiver_type(&self, e: &Expr) -> Option<String> {
        match e {
            Expr::Paren(p) => self.receiver_type(&p.expr),
            Expr::Reference(r) => self.receiver_type(&r.expr),
            Expr::Unary(u) => self.receiver_type(&u.expr),
            Expr::Field(f) => {
                let owner = self.receiver_type(&f.base)?;
                let field = match &f.member {
                    syn::Member::Named(i) => i.to_string(),
                    syn::Member::Unnamed(i) => i.index.to_string(),
                };
                self.cx.ix.fields.get(&(owner, field)).and_then(|ty| self.cx.ix.type_hint(ty))
            }
            _ => {
                let ident = single_ident(e)?;
                if ident == "self" {
                    return self.f.self_ty.clone();
                }
                if let Some((_, _, ty)) = self.types.iter().rev().find(|t| t.1 == ident) {
                    return Some(ty.clone());
                }
                let p = &self.f.params[self.param_index(&ident)?];
                self.cx.ix.type_hint(&p.ty)
            }
        }
    }
}

/// A `locking` call's name for its lock: its second argument, a string literal.
fn lock_name(args: &Args) -> String {
    args.iter().nth(1).and_then(str_literal).unwrap_or_else(|| "(a name computed at run time)".to_string())
}

fn str_literal(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => Some(s.value()),
            syn::Lit::CStr(s) => Some(s.value().to_string_lossy().into_owned()),
            _ => None,
        },
        Expr::Reference(r) => str_literal(&r.expr),
        _ => None,
    }
}

/// Every identifier and field name in an expression (`&self.store` → `self`, `store`).
fn idents_in(e: &Expr) -> Vec<String> {
    struct Collect(Vec<String>);
    impl<'ast> Visit<'ast> for Collect {
        fn visit_ident(&mut self, i: &'ast proc_macro2::Ident) {
            self.0.push(i.to_string());
        }
        fn visit_index(&mut self, i: &'ast syn::Index) {
            self.0.push(i.index.to_string());
        }
    }
    let mut c = Collect(vec![]);
    c.visit_expr(e);
    c.0
}
