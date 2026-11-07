//! `xtask sizes`: functions over 60 lines and files over 1,000, measured from the parsed source (Rust)
//! or a brace-aware scan (the window's JavaScript), not by counting braces in raw text.

mod js;
mod js_lex;
mod rust;

use crate::parse::files::RustFile;
use std::path::Path;

pub const LONG_FN: usize = 60;
pub const LONG_FILE: usize = 1000;

pub struct FnSize {
    /// `src-tauri/src/hub/activity.rs:670` or `ui/app.js:233`.
    pub at: String,
    pub name: String,
    pub lines: usize,
    pub test: bool,
}

pub struct Sizes {
    /// Longest first.
    pub functions: Vec<FnSize>,
    /// (path from the repo root, lines), longest first.
    pub files: Vec<(String, usize)>,
}

/// Measured over `rs`, every file under `src-tauri/src` (tests included, marked), and `ui/*.{js,css,html}`.
pub fn measure(repo: &Path, rs: &[RustFile]) -> Result<Sizes, String> {
    let ui: Vec<(String, String)> = crate::parse::files::under(&repo.join("ui"), &["js", "css", "html"])?.into_iter().filter(|(rel, _)| !rel.contains('/')).collect();
    let mut functions = vec![];
    for f in rs {
        functions.extend(rust::functions(&f.ast).into_iter().map(|found| found.at(&format!("src-tauri/src/{}", f.rel))));
    }
    for (rel, text) in ui.iter().filter(|(rel, _)| rel.ends_with(".js")) {
        functions.extend(js::functions(text).into_iter().map(|f| f.at(&format!("ui/{rel}"))));
    }
    functions.retain(|f| f.lines > LONG_FN);
    functions.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.at.cmp(&b.at)));
    let all = rs.iter().map(|f| (format!("src-tauri/src/{}", f.rel), f.lines)).chain(ui.iter().map(|(rel, t)| (format!("ui/{rel}"), t.lines().count())));
    let mut files: Vec<(String, usize)> = all.filter(|(_, n)| *n > LONG_FILE).collect();
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(Sizes { functions, files })
}

/// A function found in one file, before it's placed.
struct Found {
    line: usize,
    name: String,
    lines: usize,
    test: bool,
}

impl Found {
    fn at(self, file: &str) -> FnSize {
        FnSize { at: format!("{file}:{}", self.line), name: self.name, lines: self.lines, test: self.test }
    }
}
