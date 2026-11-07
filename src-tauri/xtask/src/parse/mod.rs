//! Reading the app crate: every file the module tree reaches from `lib.rs` (test modules left out),
//! parsed with syn, then indexed (`index`) and made resolvable (`resolve`).

pub mod attrs;
pub mod files;
pub mod index;
pub mod modinfo;
pub mod resolve;
pub mod sig;

use std::path::{Path, PathBuf};

/// One parsed source file of the crate.
pub struct SourceFile {
    /// Its path under `src-tauri/src`, with `/` separators (`hub/mod.rs`).
    pub rel: String,
    /// The module it is (`["hub"]` for `hub/mod.rs`, `[]` for `lib.rs`).
    pub module: Vec<String>,
    pub ast: syn::File,
}

/// The crate as the compiler sees it outside tests.
pub struct Crate {
    pub files: Vec<SourceFile>,
    /// Modules declared but not analysed (not on disk, or outside src/: the optional `ext` add-on), as `file: mod name`.
    pub missing: Vec<String>,
}

impl Crate {
    /// Every file reached from `src/lib.rs`, sorted by path; `parsed`: every `.rs` file under `src`.
    pub fn load(src: &Path, parsed: &[files::RustFile]) -> Result<Crate, String> {
        let mut krate = Crate { files: vec![], missing: vec![] };
        krate.load_file(src, parsed, &src.join("lib.rs"), vec![], true)?;
        krate.files.sort_by(|a, b| a.rel.cmp(&b.rel));
        Ok(krate)
    }

    /// `path` as module `module`; `owns_dir`: its child modules live in its own folder (`lib.rs`,
    /// `mod.rs`, or a `#[path]` file) rather than in a folder named after it.
    fn load_file(&mut self, src: &Path, parsed: &[files::RustFile], path: &Path, module: Vec<String>, owns_dir: bool) -> Result<(), String> {
        let rel = rel_under(src, path).ok_or_else(|| format!("{} is not under {}", path.display(), src.display()))?;
        let ast = parsed.iter().find(|f| f.rel == rel).map(|f| f.ast.clone()).ok_or_else(|| format!("{rel}: not parsed"))?;
        let dir = path.parent().unwrap_or(src).to_path_buf();
        let child_dir = if owns_dir { dir.clone() } else { dir.join(path.file_stem().unwrap_or_default()) };
        let decls = out_of_line_mods(&ast.items, &[]);
        self.files.push(SourceFile { rel: rel.clone(), module: module.clone(), ast });
        for decl in decls {
            let target = match &decl.path_attr {
                Some(p) => Some((dir.join(p), true)),
                None => child_file(&child_dir.join(decl.inline.join("/")), &decl.name),
            };
            let mut child = module.clone();
            child.extend(decl.inline.iter().cloned());
            child.push(decl.name.clone());
            // Not on disk, or (a `#[path]` module like the optional add-on) outside src/: not analysed.
            match target.filter(|(file, _)| file.exists() && rel_under(src, file).is_some()) {
                Some((file, owns)) => self.load_file(src, parsed, &file, child, owns)?,
                None => self.missing.push(format!("{rel}: mod {}", decl.name)),
            }
        }
        Ok(())
    }
}

/// `path` under `src` with `/` separators, `..` worked out; None when it's outside.
fn rel_under(src: &Path, path: &Path) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    for c in path.strip_prefix(src).ok()?.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    Some(parts.join("/"))
}

/// A `mod name;` declared in a file, inside the inline modules `inline` of that file.
struct ModDecl {
    name: String,
    inline: Vec<String>,
    path_attr: Option<String>,
}

fn out_of_line_mods(items: &[syn::Item], inline: &[String]) -> Vec<ModDecl> {
    let mut out = vec![];
    for item in items {
        let syn::Item::Mod(m) = item else { continue };
        if attrs::is_test(&m.attrs) {
            continue;
        }
        let name = m.ident.to_string();
        match &m.content {
            Some((_, inner)) => {
                let mut deeper = inline.to_vec();
                deeper.push(name);
                out.extend(out_of_line_mods(inner, &deeper));
            }
            None => out.push(ModDecl { name, inline: inline.to_vec(), path_attr: attrs::path_attr(&m.attrs) }),
        }
    }
    out
}

/// `dir/name.rs` (a file that names its own child folder) or `dir/name/mod.rs` (one that owns it).
fn child_file(dir: &Path, name: &str) -> Option<(PathBuf, bool)> {
    let flat = dir.join(format!("{name}.rs"));
    if flat.exists() {
        return Some((flat, false));
    }
    let nested = dir.join(name).join("mod.rs");
    nested.exists().then_some((nested, true))
}
