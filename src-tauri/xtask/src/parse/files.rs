//! Every file of a kind under a folder, test files included (the size counts and the tmux rule read
//! them all, not only what the module tree reaches).

use std::path::Path;

/// Every file under `dir` whose extension is one of `exts`, as (path under `dir` with `/`, text),
/// sorted by path.
pub fn under(dir: &Path, exts: &[&str]) -> Result<Vec<(String, String)>, String> {
    let mut paths = vec![];
    collect(dir, exts, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/"), text))
        })
        .collect()
}

fn collect(dir: &Path, exts: &[&str], out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, exts, out)?;
        } else if path.extension().is_some_and(|e| exts.iter().any(|x| e == *x)) {
            out.push(path);
        }
    }
    Ok(())
}

/// A Rust file, parsed.
pub struct RustFile {
    /// Its path under `src-tauri/src`.
    pub rel: String,
    pub lines: usize,
    pub ast: syn::File,
}

/// Every `.rs` file under `src`, parsed once (the crate's module tree, the sizes and the tmux rule all
/// read from these).
pub fn parse_rust(src: &Path) -> Result<Vec<RustFile>, String> {
    under(src, &["rs"])?
        .into_iter()
        .map(|(rel, text)| {
            let ast = syn::parse_file(&text).map_err(|e| format!("src-tauri/src/{rel}: {e}"))?;
            Ok(RustFile { lines: text.lines().count(), rel, ast })
        })
        .collect()
}
