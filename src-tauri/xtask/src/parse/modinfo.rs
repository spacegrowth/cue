//! What each module declares (functions, types, child modules) and imports (`use`, `use …::*`), with
//! `crate`/`self`/`super` paths made absolute.

use super::index::FnId;
use std::collections::{BTreeMap, BTreeSet};

/// What a module declares and imports.
#[derive(Default)]
pub struct ModInfo {
    /// Free functions by name.
    pub fns: BTreeMap<String, Vec<FnId>>,
    pub types: BTreeSet<String>,
    pub children: BTreeSet<String>,
    /// `use` aliases: local name → full path (`crate::…` made absolute, starting with "crate").
    pub uses: BTreeMap<String, Vec<String>>,
    /// `use path::*` targets.
    pub globs: Vec<Vec<String>>,
}

impl ModInfo {
    pub fn read_items(&mut self, module: &[String], items: &[syn::Item]) {
        for item in items {
            match item {
                syn::Item::Use(u) => self.read_use(module, &[], &u.tree),
                syn::Item::Mod(m) => {
                    self.children.insert(m.ident.to_string());
                }
                syn::Item::Struct(s) => {
                    self.types.insert(s.ident.to_string());
                }
                syn::Item::Enum(e) => {
                    self.types.insert(e.ident.to_string());
                }
                syn::Item::Trait(t) => {
                    self.types.insert(t.ident.to_string());
                }
                _ => {}
            }
        }
    }

    fn read_use(&mut self, module: &[String], prefix: &[String], tree: &syn::UseTree) {
        let joined = |name: &str| {
            let mut p = prefix.to_vec();
            p.push(name.to_string());
            p
        };
        match tree {
            syn::UseTree::Path(p) => self.read_use(module, &joined(&p.ident.to_string()), &p.tree),
            syn::UseTree::Name(n) if n.ident == "self" => {
                if let Some(last) = prefix.last() {
                    self.uses.insert(last.clone(), absolute(module, prefix));
                }
            }
            syn::UseTree::Name(n) => {
                    self.uses.insert(n.ident.to_string(), absolute(module, &joined(&n.ident.to_string())));
                }
            syn::UseTree::Rename(r) => {
                    self.uses.insert(r.rename.to_string(), absolute(module, &joined(&r.ident.to_string())));
                }
            syn::UseTree::Glob(_) => self.globs.push(absolute(module, prefix)),
            syn::UseTree::Group(g) => g.items.iter().for_each(|t| self.read_use(module, prefix, t)),
        }
    }
}

/// `crate::a::b` → `["crate", "a", "b"]`; `self::x`/`super::x` made so too; anything else (an outside
/// crate, or a child module named without `self::`) as written. The second is told apart at lookup.
pub(super) fn absolute(module: &[String], path: &[String]) -> Vec<String> {
    let mut base: Vec<String> = module.to_vec();
    let mut rest = path;
    match path.first().map(String::as_str) {
        Some("crate") => return path.to_vec(),
        Some("self") => rest = &path[1..],
        Some("super") => {
            while rest.first().is_some_and(|s| s == "super") {
                base.pop();
                rest = &rest[1..];
            }
        }
        _ => return path.to_vec(),
    }
    let mut out = vec!["crate".to_string()];
    out.extend(base);
    out.extend(rest.iter().cloned());
    out
}
