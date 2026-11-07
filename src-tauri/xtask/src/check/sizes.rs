//! Size limits: a function outside test code over `sizes::LONG_FN` lines, or a source file over
//! `sizes::LONG_FILE`, fails the check. Rust (`src-tauri/src`) and the window's JavaScript (`ui/*.js`) are
//! held to the same limits. A file listed in [[size_exempt]] is exempt as a whole until it's split, and an
//! entry for a file within the limits is stale. Stylesheets and HTML aren't checked.

use crate::allow::SizeExempt;
use crate::sizes::{Sizes, LONG_FILE, LONG_FN};
use std::collections::BTreeSet;

pub fn sizes(sizes: &Sizes, allow: &[SizeExempt]) -> Vec<String> {
    let exempt: BTreeSet<&str> = allow.iter().map(|j| j.file.as_str()).collect();
    let checked = |file: &str| (file.ends_with(".rs") || file.ends_with(".js")) && !exempt.contains(file);
    let mut out: Vec<String> = sizes
        .functions
        .iter()
        .filter(|f| !f.test && checked(&f.file))
        .map(|f| format!("{}:{} {} is {} lines, over {LONG_FN}: split it into a short function that reads as steps and helpers of their own", f.file, f.line, f.name, f.lines))
        .collect();
    out.extend(sizes.files.iter().filter(|(file, _)| checked(file)).map(|(file, lines)| format!("{file} is {lines} lines, over {LONG_FILE}: split it into modules along its seams")));
    let over: BTreeSet<&str> = sizes.functions.iter().filter(|f| !f.test).map(|f| f.file.as_str()).chain(sizes.files.iter().map(|(file, _)| file.as_str())).collect();
    out.extend(exempt.difference(&over).map(|file| format!("[[size_exempt]] {file} is within the size limits now: remove it from docs/dev/lint-allow.toml")));
    out
}
