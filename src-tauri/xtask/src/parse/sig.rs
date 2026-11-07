//! What a declaration's types say, read from their tokens: a lock type, the name a type goes by, which
//! parameters are callable.

use super::index::Param;
use quote::ToTokens;
use std::collections::BTreeSet;

pub fn tokens(t: &impl ToTokens) -> String {
    t.to_token_stream().to_string()
}

/// A lock, or a cell holding one, or a once-initialized cell (listed: it's shared state too).
pub(super) fn is_lock_type(ty: &str) -> bool {
    ["Mutex", "RwLock", "OnceLock", "LazyLock"].iter().any(|k| ty.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == *k))
}

/// The name a type is known by: the last segment of its path, through references.
pub fn type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => type_name(&r.elem),
        syn::Type::Paren(p) => type_name(&p.elem),
        _ => None,
    }
}

/// Generic parameters bound by a `Fn` trait (`F: Fn(&str) -> bool`, or in a `where`).
pub(super) fn callable_generics(g: &syn::Generics) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in &g.params {
        if let syn::GenericParam::Type(t) = p {
            if t.bounds.iter().any(|b| names_fn(&tokens(b))) {
                out.insert(t.ident.to_string());
            }
        }
    }
    for pred in g.where_clause.iter().flat_map(|w| &w.predicates) {
        if let syn::WherePredicate::Type(t) = pred {
            if t.bounds.iter().any(|b| names_fn(&tokens(b))) {
                out.insert(tokens(&t.bounded_ty));
            }
        }
    }
    out
}

fn names_fn(ty: &str) -> bool {
    ["Fn", "FnMut", "FnOnce"].iter().any(|f| ty.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == *f))
}

pub(super) fn param(t: &syn::PatType, callable: &BTreeSet<String>) -> Param {
    let ident = match &*t.pat {
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        _ => None,
    };
    let ty = tokens(&t.ty);
    let generic = type_name(&t.ty).is_some_and(|n| callable.contains(&n));
    Param { ident, fn_like: names_fn(&ty) || generic, ty }
}
