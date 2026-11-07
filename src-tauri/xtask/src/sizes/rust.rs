//! Rust function lengths: from the line of `fn` to the line of the body's closing brace, for every
//! function with a body (free, method, trait default, nested), test code marked.

use super::Found;
use crate::parse::attrs::is_test;
use syn::visit::Visit;

pub(super) fn functions(ast: &syn::File) -> Vec<Found> {
    let mut v = Lengths { test_depth: 0, out: vec![] };
    v.visit_file(ast);
    v.out
}

struct Lengths {
    /// How many test items (`#[cfg(test)]`, `#[test]`) enclose the current one.
    test_depth: usize,
    out: Vec<Found>,
}

impl Lengths {
    fn record(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) {
        let first = sig.fn_token.span.start().line;
        let last = block.brace_token.span.close().start().line;
        self.out.push(Found { line: first, name: sig.ident.to_string(), lines: last + 1 - first, test: self.test_depth > 0 || is_test(attrs) });
    }

    /// `inner` with the item's test-ness counted.
    fn within(&mut self, attrs: &[syn::Attribute], inner: impl FnOnce(&mut Self)) {
        let test = usize::from(is_test(attrs));
        self.test_depth += test;
        inner(self);
        self.test_depth -= test;
    }
}

impl<'ast> Visit<'ast> for Lengths {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.record(&f.attrs, &f.sig, &f.block);
        self.within(&f.attrs, |v| syn::visit::visit_item_fn(v, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.record(&f.attrs, &f.sig, &f.block);
        self.within(&f.attrs, |v| syn::visit::visit_impl_item_fn(v, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(block) = &f.default {
            self.record(&f.attrs, &f.sig, block);
        }
        self.within(&f.attrs, |v| syn::visit::visit_trait_item_fn(v, f));
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        self.within(&m.attrs, |v| syn::visit::visit_item_mod(v, m));
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        self.within(&i.attrs, |v| syn::visit::visit_item_impl(v, i));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lengths_run_from_fn_to_the_closing_brace_and_tests_are_marked() {
        let text = "fn a() {\n}\n\n#[cfg(test)]\nmod t {\n    #[test]\n    fn b() {\n        x();\n    }\n}\nimpl S {\n    pub fn c(&self)\n    {\n    }\n}\n";
        let found: Vec<(usize, String, usize, bool)> = super::functions(&syn::parse_file(text).unwrap()).into_iter().map(|f| (f.line, f.name, f.lines, f.test)).collect();
        assert_eq!(found, [(1, "a".into(), 2, false), (7, "b".into(), 3, true), (12, "c".into(), 3, false)]);
    }
}
