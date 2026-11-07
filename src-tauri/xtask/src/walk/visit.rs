//! The walk over a body's syntax: blocks are scopes, statements end their temporaries, and the shapes
//! whose evaluation order or drop rules matter (assignments, `if`/`while` conditions, macros) are taken
//! in the order Rust runs them.

use super::Walker;
use syn::visit::Visit;

impl<'ast> Visit<'ast> for Walker<'_, '_, '_> {
    fn visit_block(&mut self, b: &'ast syn::Block) {
        self.enter_scope();
        for s in &b.stmts {
            self.visit_stmt(s);
        }
        self.exit_scope();
    }

    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        self.temp_scope(|w| match s {
            syn::Stmt::Local(l) => w.local(l),
            syn::Stmt::Item(_) => {}
            syn::Stmt::Expr(e, _) => w.visit_expr(e),
            syn::Stmt::Macro(m) => w.visit_macro(&m.mac),
        });
    }

    fn visit_item(&mut self, _: &'ast syn::Item) {}

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        self.call(c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        self.method(m);
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.closure_body(c);
    }

    /// The value is worked out before the place it goes to (`*lock(&X) = f()` calls `f` first).
    fn visit_expr_assign(&mut self, a: &'ast syn::ExprAssign) {
        self.visit_expr(&a.right);
        self.visit_expr(&a.left);
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        self.condition(&i.cond);
        self.visit_block(&i.then_branch);
        if let Some((_, e)) = &i.else_branch {
            self.visit_expr(e);
        }
    }

    fn visit_expr_while(&mut self, w: &'ast syn::ExprWhile) {
        self.condition(&w.cond);
        self.visit_block(&w.body);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let args = m.parse_body_with(syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated);
        for e in args.iter().flatten() {
            self.visit_expr(e);
        }
    }
}

impl Walker<'_, '_, '_> {
    /// An `if let`/`while let` scrutinee keeps its temporaries through the body; a plain condition doesn't.
    fn condition(&mut self, cond: &syn::Expr) {
        if matches!(cond, syn::Expr::Let(_)) {
            self.visit_expr(cond);
        } else {
            self.temp_scope(|w| w.visit_expr(cond));
        }
    }
}
