//! `json!({..})`'s body isn't a list of expressions: it's JSON whose values (and parenthesized keys) are
//! Rust expressions. It's read as JSON's token trees, each value parsed as the expression it is, so a call
//! inside one (`json!({"crew": crate::leads::view(ids)})`) counts like any other.

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

/// Whether `m` is serde_json's `json!` (by its last path segment, as the crate writes it).
pub fn is_json(m: &syn::Macro) -> bool {
    m.path.segments.last().is_some_and(|s| s.ident == "json")
}

/// The expressions in a `json!` body, in the order written: each value, and each key that's an expression
/// (`(name): …`). A literal key or `null`/`true`/`false` is an expression too, harmless to visit.
pub fn expressions(body: TokenStream) -> Vec<syn::Expr> {
    let mut out = vec![];
    value(body.into_iter().collect(), &mut out);
    out
}

/// One JSON value's tokens: an object or array is walked into; anything else is an expression.
fn value(tokens: Vec<TokenTree>, out: &mut Vec<syn::Expr>) {
    if let [TokenTree::Group(g)] = tokens.as_slice() {
        match g.delimiter() {
            Delimiter::Brace => return members(g.stream(), true, out),
            Delimiter::Bracket => return members(g.stream(), false, out),
            _ => {}
        }
    }
    let stream: TokenStream = tokens.into_iter().collect();
    if let Ok(e) = syn::parse2::<syn::Expr>(stream) {
        out.push(e);
    }
}

/// An object's `key: value` members or an array's values, split at their top-level commas.
fn members(body: TokenStream, object: bool, out: &mut Vec<syn::Expr>) {
    for member in split(body.into_iter().collect(), is_comma) {
        if member.is_empty() {
            continue;
        }
        if !object {
            value(member, out);
            continue;
        }
        let mut parts = split_once(member, is_key_colon);
        let val = parts.pop().unwrap_or_default();
        if let Some(key) = parts.pop() {
            value(key, out);
        }
        value(val, out);
    }
}

fn is_comma(tokens: &[TokenTree], i: usize) -> bool {
    matches!(&tokens[i], TokenTree::Punct(p) if p.as_char() == ',')
}

/// A lone `:` (not half of a path's `::`).
fn is_key_colon(tokens: &[TokenTree], i: usize) -> bool {
    let colon = |t: &TokenTree, spacing: Spacing| matches!(t, TokenTree::Punct(p) if p.as_char() == ':' && p.spacing() == spacing);
    colon(&tokens[i], Spacing::Alone) && !(i > 0 && colon(&tokens[i - 1], Spacing::Joint))
}

/// `tokens` cut at every token `at` says is a separator (groups are single tokens, so only the top level
/// is cut).
fn split(tokens: Vec<TokenTree>, at: fn(&[TokenTree], usize) -> bool) -> Vec<Vec<TokenTree>> {
    let mut parts = vec![vec![]];
    for i in 0..tokens.len() {
        if at(&tokens, i) {
            parts.push(vec![]);
        } else if let Some(last) = parts.last_mut() {
            last.push(tokens[i].clone());
        }
    }
    parts
}

/// `tokens` cut at the first separator only: [before, after], or [all] when there's none.
fn split_once(tokens: Vec<TokenTree>, at: fn(&[TokenTree], usize) -> bool) -> Vec<Vec<TokenTree>> {
    match (0..tokens.len()).find(|&i| at(&tokens, i)) {
        Some(i) => vec![tokens[..i].to_vec(), tokens[i + 1..].to_vec()],
        None => vec![tokens],
    }
}
