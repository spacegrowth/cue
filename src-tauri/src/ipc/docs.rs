// A markdown file a reply names, read for Cue's reader (ui/chat/reader.js). Replies name files every which way:
// relative to the session's folder, to the repo's top, or by name alone. This finds the one meant: the path as
// given, then the same relative path from each folder above the session's up to its repo's top, then a search
// of the repo for a file ending in that path (or with that name). Local sessions only: the UI never asks for a
// session on another machine (its paths aren't on this Mac, and looking them up here can stall).
use super::blocking;
use std::path::{Path, PathBuf};

/// Files a reader opens; anything else goes on opening as before (open_link).
const DOC_EXTS: &[&str] = &["md", "markdown"];
/// Bigger than any design doc; a log someone named .md is not read whole into the window.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// How far a search by name goes: folders down from the repo's top, and entries looked at in all.
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 40_000;
/// Folders a search never goes into: built, fetched or hidden things, not anyone's docs.
const SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "vendor", "Pods", "DerivedData"];

#[tauri::command]
pub(crate) async fn read_doc(path: String, cwd: Option<String>) -> Result<serde_json::Value, String> {
    blocking(move || {
        let found = resolve_doc(&path, cwd.as_deref()).ok_or_else(|| format!("Couldn't find {}", shown(&path, cwd.as_deref())))?;
        let meta = std::fs::metadata(&found).map_err(|e| e.to_string())?;
        if meta.len() > MAX_BYTES {
            return Err("It's too big to read here".into());
        }
        let text = std::fs::read_to_string(&found).map_err(|e| e.to_string())?;
        let modified_ms = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
        Ok(serde_json::json!({ "path": found.to_string_lossy(), "text": text, "modified_ms": modified_ms }))
    }).await?
}

/// The path as the reply wrote it, for "Couldn't find …": relative to the session's folder when it's in it.
fn shown(path: &str, cwd: Option<&str>) -> String {
    cwd.and_then(|c| path.strip_prefix(&format!("{}/", c.trim_end_matches('/')))).unwrap_or(path).to_string()
}

pub(crate) fn is_doc(path: &str) -> bool {
    Path::new(path).extension().map(|e| DOC_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false)
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix('~'), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => PathBuf::from(format!("{home}{rest}")),
        _ => PathBuf::from(p),
    }
}

/// The file `path` means, said in a session whose folder is `cwd`; None when there's no such doc.
pub(crate) fn resolve_doc(path: &str, cwd: Option<&str>) -> Option<PathBuf> {
    if !is_doc(path) {
        return None;
    }
    let p = expand_home(path);
    if p.is_absolute() && p.is_file() {
        return Some(p);
    }
    let cwd = cwd.map(|c| PathBuf::from(c.trim_end_matches('/'))).filter(|c| c.is_absolute() && c.is_dir())?;
    // What the reply wrote, relative: the UI joins a relative path to the session's folder, so take that off again.
    let rel: PathBuf = if p.is_absolute() { p.strip_prefix(&cwd).map(Path::to_path_buf).ok()? } else { p };
    let rel = PathBuf::from(rel.to_string_lossy().trim_start_matches("./"));
    let top = cwd.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf).unwrap_or_else(|| cwd.clone());
    // The same relative path from the session's folder and each one above it, up to the repo's top.
    for dir in cwd.ancestors() {
        let here = dir.join(&rel);
        if here.is_file() {
            return Some(here);
        }
        if dir == top {
            break;
        }
    }
    search(&top, &rel)
}

/// The markdown files made most recently in the repos of `folders` (each session's folder on this Mac; its repo's
/// top when it's in one), newest first, at most `limit`: Read's "New docs". Each with its repo's top, and when it
/// was made and last changed. Searched as `search` does (hidden, built and fetched folders skipped, so far deep and no further).
#[tauri::command]
pub(crate) async fn recent_docs(folders: Vec<String>, limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
    blocking(move || {
        let mut tops: Vec<PathBuf> = Vec::new();
        for f in folders {
            let dir = PathBuf::from(f.trim_end_matches('/'));
            if !dir.is_absolute() || !dir.is_dir() {
                continue;
            }
            let top = dir.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf).unwrap_or(dir);
            // One repo once, and not again inside another listed one.
            if !tops.iter().any(|t| top.starts_with(t)) {
                tops.retain(|t| !t.starts_with(&top));
                tops.push(top);
            }
        }
        let mut found: Vec<(u64, u64, PathBuf, &PathBuf)> = Vec::new();
        for top in &tops {
            walk_docs(top, &mut |p, meta| {
                let ms = |t: std::io::Result<std::time::SystemTime>| t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                // Made: its birth time; a disk that keeps none, when it last changed.
                let changed = ms(meta.modified());
                let made = match ms(meta.created()) { 0 => changed, c => c };
                found.push((made, changed, p, top));
            });
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(found.into_iter().take(limit.unwrap_or(10)).map(|(made, changed, p, top)| serde_json::json!({ "path": p.to_string_lossy(), "top": top.to_string_lossy(), "created_ms": made, "modified_ms": changed })).collect())
    }).await?
}

/// Every markdown file under `top`, as far as a search goes (MAX_DEPTH, MAX_ENTRIES, SKIP_DIRS and hidden folders).
fn walk_docs(top: &Path, each: &mut dyn FnMut(PathBuf, std::fs::Metadata)) {
    let mut seen = 0usize;
    let mut level = vec![top.to_path_buf()];
    for _ in 0..=MAX_DEPTH {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                seen += 1;
                if seen > MAX_ENTRIES {
                    return;
                }
                let name = e.file_name();
                let s = name.to_string_lossy();
                let Ok(kind) = e.file_type() else { continue };
                if kind.is_dir() {
                    if !s.starts_with('.') && !SKIP_DIRS.contains(&s.as_ref()) {
                        next.push(e.path());
                    }
                } else if kind.is_file() && is_doc(&s) {
                    if let Ok(meta) = e.metadata() {
                        each(e.path(), meta);
                    }
                }
            }
        }
        if next.is_empty() {
            return;
        }
        level = next;
    }
}

/// A file under `top` whose path ends in `rel` (all of it), else one with its name; the shallowest wins.
fn search(top: &Path, rel: &Path) -> Option<PathBuf> {
    let name = rel.file_name()?.to_os_string();
    let (mut whole, mut named): (Option<PathBuf>, Option<PathBuf>) = (None, None);
    let mut seen = 0usize;
    let mut level = vec![top.to_path_buf()];
    for _ in 0..=MAX_DEPTH {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                seen += 1;
                if seen > MAX_ENTRIES {
                    return whole.or(named);
                }
                let fname = e.file_name();
                let Ok(kind) = e.file_type() else { continue };
                if kind.is_dir() {
                    let s = fname.to_string_lossy();
                    if !s.starts_with('.') && !SKIP_DIRS.contains(&s.as_ref()) {
                        next.push(e.path());
                    }
                } else if kind.is_file() && fname == name {
                    let full = e.path();
                    if whole.is_none() && full.ends_with(rel) {
                        whole = Some(full);
                    } else if named.is_none() {
                        named = Some(full);
                    }
                }
            }
        }
        // Breadth first: the first level with a match has the shallowest one.
        if whole.is_some() {
            return whole;
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    whole.or(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let top = std::env::temp_dir().join(format!("cue-docs-{}-{}", std::process::id(), rand_id()));
        for d in [".git", "app/src", "docs/design", "node_modules/x"] {
            std::fs::create_dir_all(top.join(d)).unwrap();
        }
        std::fs::write(top.join("docs/design/router.md"), "# Router").unwrap();
        std::fs::write(top.join("HANDOFF.md"), "# Hand-off").unwrap();
        std::fs::write(top.join("node_modules/x/README.md"), "no").unwrap();
        top
    }
    fn rand_id() -> u128 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() }

    #[test]
    fn finds_what_a_reply_meant() {
        let top = tree();
        let sub = top.join("app/src");
        let c = sub.to_str().unwrap();
        let want = top.join("docs/design/router.md");
        // Written relative to the repo's top while the session sits lower down (joined to its folder by the UI).
        assert_eq!(resolve_doc(&format!("{c}/docs/design/router.md"), Some(c)), Some(want.clone()));
        // Relative, as written.
        assert_eq!(resolve_doc("docs/design/router.md", Some(c)), Some(want.clone()));
        // A tail of the path, or the name alone: found by a search of the repo.
        assert_eq!(resolve_doc("design/router.md", Some(c)), Some(want.clone()));
        assert_eq!(resolve_doc("router.md", Some(c)), Some(want.clone()));
        assert_eq!(resolve_doc("HANDOFF.md", Some(c)), Some(top.join("HANDOFF.md")));
        // Absolute and there.
        assert_eq!(resolve_doc(want.to_str().unwrap(), None), Some(want));
        // Not a doc, not there, or only in a skipped folder.
        assert_eq!(resolve_doc("app/src/main.rs", Some(c)), None);
        assert_eq!(resolve_doc("nope.md", Some(c)), None);
        assert_eq!(resolve_doc("x/README.md", Some(c)), None);
        std::fs::remove_dir_all(&top).ok();
    }

    #[test]
    fn new_docs_are_the_repos_newest() {
        let top = tree();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(top.join("docs/newest.md"), "# New").unwrap();
        let sub = top.join("app/src").to_string_lossy().to_string();
        let got = tauri::async_runtime::block_on(recent_docs(vec![sub, top.to_string_lossy().to_string()], Some(2))).unwrap();
        let names: Vec<String> = got.iter().map(|v| v["path"].as_str().unwrap().rsplit('/').next().unwrap().to_string()).collect();
        // Newest first, the repo searched once (both folders are in it), node_modules never.
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "newest.md");
        assert!(!names.contains(&"README.md".to_string()));
        std::fs::remove_dir_all(&top).ok();
    }
}
