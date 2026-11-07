//! What an item's attributes say: test-only, a `#[path]`, a Tauri command.

use quote::ToTokens;
use syn::Attribute;

/// Test-only: `#[test]`, or a `#[cfg(..)]` that needs `test` (`cfg(test)`, `cfg(all(test, ..))`), but
/// not `cfg(not(test))`.
pub fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if a.path().is_ident("test") {
            return true;
        }
        if !a.path().is_ident("cfg") {
            return false;
        }
        let tokens = a.meta.to_token_stream().to_string().replace(' ', "");
        tokens.contains("test") && !tokens.contains("not(test)")
    })
}

/// The file a `#[path = "…"]` points to.
pub fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find(|a| a.path().is_ident("path")).and_then(|a| match &a.meta {
        syn::Meta::NameValue(nv) => match &nv.value {
            syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    })
}

/// `#[tauri::command]` (or `#[command]` after a `use tauri::command`).
pub fn is_command(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        let segs: Vec<String> = a.path().segments.iter().map(|s| s.ident.to_string()).collect();
        segs.last().is_some_and(|l| l == "command") && (segs.len() == 1 || segs[0] == "tauri")
    })
}
