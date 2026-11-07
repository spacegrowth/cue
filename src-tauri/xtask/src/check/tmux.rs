//! tmux started by name. In tests every tmux call must go through the crate's seam (`terminal/focus.rs`:
//! `bin("tmux")`/`tmux_bin()`), which first points tmux at a private test server; a `Command::new("tmux")`
//! anywhere else, test code included, would reach the server of whoever runs the tests (an agent's
//! `$TMUX`), where a `kill-session` or `kill-server` is the user's real terminals gone. Read syntactically
//! over every file, tests included: a literal program "tmux" (or ".../tmux") outside the seam's functions.

use crate::allow::TmuxSeam;
use crate::parse::files::RustFile;
use syn::visit::Visit;

/// A literal tmux program outside the seam: `file:line in function`.
pub fn literal_tmux(files: &[RustFile], seam: &[TmuxSeam]) -> Vec<String> {
    let mut out = vec![];
    for f in files {
        let mut v = Finder { fns: vec![], found: vec![] };
        v.visit_file(&f.ast);
        for (line, func) in v.found {
            if !seam.iter().any(|s| s.file == f.rel && s.function == func) {
                out.push(format!("{}:{line} in {func}", f.rel));
            }
        }
    }
    out
}

struct Finder {
    fns: Vec<String>,
    found: Vec<(usize, String)>,
}

impl<'ast> Visit<'ast> for Finder {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.fns.push(f.sig.ident.to_string());
        syn::visit::visit_item_fn(self, f);
        self.fns.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.fns.push(f.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, f);
        self.fns.pop();
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let (syn::Expr::Path(p), Some(syn::Expr::Lit(lit))) = (&*c.func, c.args.first()) {
            let segs: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
            if segs.ends_with(&["Command".to_string(), "new".to_string()]) {
                if let syn::Lit::Str(s) = &lit.lit {
                    let program = s.value();
                    if program == "tmux" || program.ends_with("/tmux") {
                        let func = self.fns.last().cloned().unwrap_or_else(|| "(no function)".into());
                        self.found.push((s.span().start().line, func));
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_tmux_is_found_outside_the_seam_tests_included() {
        let text = r#"
fn bin(name: &str) -> String { name.into() }
fn ok() { Command::new(bin("tmux")); }
fn seam() { Command::new("tmux"); }
#[cfg(test)]
mod tests {
    #[test]
    fn t() { std::process::Command::new("tmux").arg("kill-session"); Command::new("/opt/homebrew/bin/tmux"); }
}
"#;
        let files = vec![RustFile { rel: "x.rs".into(), lines: 0, ast: syn::parse_file(text).unwrap() }];
        let seam = [TmuxSeam { file: "x.rs".into(), function: "seam".into(), reason: "the seam".into() }];
        assert_eq!(literal_tmux(&files, &seam), ["x.rs:8 in t", "x.rs:8 in t"]);
    }
}
