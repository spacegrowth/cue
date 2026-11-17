//! The window's files reloaded in place: when everything between the commit this Cue was built from and the main
//! line's HEAD changes only the window's files (ui/) or files that never go into the app (tests, docs), a Desk item
//! done there needs no build. Its ui/ is copied out of that commit (git's, never the checkout's own unsaved edits)
//! into <Cue's folder>/live/<commit>/, served in place of the files built into Cue, and the window reloads; sessions
//! and the Desk run on in Cue itself, untouched. Anything else still goes through an install (self_install.rs).
//!
//! <Cue's folder>/live/current.json says which copy is served: {commit, base, at}. `base` is the commit of the Cue
//! that took it: a Cue built from any other commit (the next install) drops it as it starts, since that Cue's own
//! files are as new.

use crate::server::cue_dir;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

/// The copy served now: its folder. None: the files built into Cue.
static SERVED: RwLock<Option<PathBuf>> = RwLock::new(None);

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Current {
    /// The commit its files are from.
    pub commit: String,
    /// The commit of the Cue that took it.
    pub base: String,
    pub at: u64,
}

fn live_dir() -> PathBuf {
    cue_dir().join("live")
}

/// The commit this Cue was built from. A test's hub has none built in: its <Cue's folder>/install/running.json says.
pub fn running() -> Option<String> {
    #[cfg(not(test))]
    return crate::self_install::running_commit();
    #[cfg(test)]
    {
        let s = std::fs::read_to_string(cue_dir().join("install/running.json")).ok()?;
        let v: serde_json::Value = serde_json::from_str(&s).ok()?;
        v["commit"].as_str().map(str::to_string).filter(|c| !c.is_empty())
    }
}

/// What an item's commit on the main line needs to be in the running Cue.
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// Something that goes into the app changed: an install.
    Install,
    /// Only the window's files (and files outside the app): its ui/ served, the window reloaded.
    Reload,
    /// Nothing the app shows changed (tests, docs): nothing to do.
    Nothing,
}

/// A path that never goes into the app: tests, docs, the website, a note at the top.
fn outside_app(path: &str) -> bool {
    ["tests/", "docs/", "site/", "work/"].iter().any(|p| path.starts_with(p)) || (!path.contains('/') && path.ends_with(".md"))
}

/// What `head` of `repo` needs over the Cue built from `running`, with the window serving `served`'s ui/ (None: its
/// own). Install when git can't say.
pub fn plan(repo: &str, running: &str, served: Option<&str>, head: &str) -> Plan {
    let Some(paths) = changed(repo, running, head, &[]) else { return Plan::Install };
    if paths.iter().any(|p| !p.starts_with("ui/") && !outside_app(p)) {
        return Plan::Install;
    }
    let shown = served.unwrap_or(running);
    match changed(repo, shown, head, &["--", "ui"]) {
        Some(ui) if ui.is_empty() => Plan::Nothing,
        Some(_) => Plan::Reload,
        None => Plan::Install,
    }
}

/// The paths that differ between commits `a` and `b` (limited by `rest`); None when git can't say.
fn changed(repo: &str, a: &str, b: &str, rest: &[&str]) -> Option<Vec<String>> {
    if !looks_like_commit(a) || !looks_like_commit(b) {
        return None;
    }
    let mut args = vec!["diff", "--name-only", "-z", "--no-renames", a, b];
    args.extend_from_slice(rest);
    match crate::safe_git::run("", repo, &[], &args) {
        Ok((out, 0)) => Some(out.split('\0').filter(|p| !p.is_empty()).map(str::to_string).collect()),
        _ => None,
    }
}

fn looks_like_commit(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The copy served now, as current.json says, when its folder is there.
pub fn current() -> Option<Current> {
    current_in(&live_dir())
}

fn current_in(dir: &Path) -> Option<Current> {
    let c: Current = serde_json::from_str(&std::fs::read_to_string(dir.join("current.json")).ok()?).ok()?;
    dir.join(&c.commit).is_dir().then_some(c)
}

/// `head`'s ui/ from `repo`, served from now on in place of what's served, for the Cue built from `base`.
pub fn take(repo: &str, head: &str, base: &str) -> Result<(), String> {
    let to = take_in(&live_dir(), repo, head, base)?;
    let mut served = crate::locking::write(&SERVED, "live_ui.served");
    *served = Some(to);
    Ok(())
}

/// `take`, its copy and current.json in `dir`: the copy's folder.
fn take_in(dir: &Path, repo: &str, head: &str, base: &str) -> Result<PathBuf, String> {
    if !looks_like_commit(head) {
        return Err(format!("not a commit: {head}"));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let to = dir.join(head);
    if !to.join("index.html").is_file() {
        let tmp = dir.join(format!("{head}.part"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let tar = dir.join(format!("{head}.tar"));
        let tar_s = tar.to_string_lossy().to_string();
        let filters = crate::safe_git::no_repo_filters("", repo);
        let made = crate::safe_git::run("", repo, &filters, &["archive", "--format=tar", "-o", &tar_s, head, "ui"]);
        if !matches!(made, Ok((_, 0))) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("git couldn't copy ui/ out of {head}"));
        }
        let out = std::process::Command::new("/usr/bin/tar").arg("-xf").arg(&tar).arg("-C").arg(&tmp).arg("--strip-components=1").output();
        let _ = std::fs::remove_file(&tar);
        if !out.is_ok_and(|o| o.status.success()) || !tmp.join("index.html").is_file() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("couldn't unpack ui/ of {head}"));
        }
        let _ = std::fs::remove_dir_all(&to);
        std::fs::rename(&tmp, &to).map_err(|e| e.to_string())?;
    }
    let c = Current { commit: head.to_string(), base: base.to_string(), at: crate::clock::now_ms() };
    let json = serde_json::to_string(&c).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("current.json.tmp"), json).map_err(|e| e.to_string())?;
    std::fs::rename(dir.join("current.json.tmp"), dir.join("current.json")).map_err(|e| e.to_string())?;
    tidy(dir, head);
    Ok(to)
}

/// Every copy but `keep` removed.
fn tidy(dir: &Path, keep: &str) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() && name != keep {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// As Cue starts: the copy kept and served when the Cue that took it is this one; else (a new Cue, its own files as
/// new) dropped.
pub fn at_start() {
    let kept = kept_in(&live_dir(), running().as_deref());
    let mut served = crate::locking::write(&SERVED, "live_ui.served");
    *served = kept;
}

/// The copy in `dir` a Cue built from `running` serves as it starts; any other is removed.
fn kept_in(dir: &Path, running: Option<&str>) -> Option<PathBuf> {
    let keep = current_in(dir).filter(|c| running.is_some_and(|r| r == c.base));
    if keep.is_none() && dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
    keep.map(|c| dir.join(c.commit))
}

/// The served copy's file at `key` ("/index.html"), when one is served and has it.
fn served_file(key: &str) -> Option<Vec<u8>> {
    let served = crate::locking::read(&SERVED, "live_ui.served");
    let root = served.clone()?;
    drop(served);
    file_in(&root, key)
}

/// The file at `key` under `root`; none for a key that would leave it.
fn file_in(root: &Path, key: &str) -> Option<Vec<u8>> {
    let rel = Path::new(key.trim_start_matches('/'));
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    std::fs::read(root.join(rel)).ok()
}

/// Cue's files, the served copy's in place of the built-in ones it has.
struct Live(Box<dyn tauri::Assets<tauri::Wry>>);

impl tauri::Assets<tauri::Wry> for Live {
    fn get(&self, key: &tauri::utils::assets::AssetKey) -> Option<Cow<'_, [u8]>> {
        served_file(key.as_ref()).map(Cow::Owned).or_else(|| self.0.get(key))
    }

    fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
        self.0.iter()
    }

    fn csp_hashes(&self, html_path: &tauri::utils::assets::AssetKey) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
        self.0.csp_hashes(html_path)
    }
}

/// The app's files, served from the live copy when there is one (`at_start`, `take`).
pub fn wrap(mut context: tauri::Context<tauri::Wry>) -> tauri::Context<tauri::Wry> {
    at_start();
    struct Empty;
    impl tauri::Assets<tauri::Wry> for Empty {
        fn get(&self, _: &tauri::utils::assets::AssetKey) -> Option<Cow<'_, [u8]>> {
            None
        }
        fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(&self, _: &tauri::utils::assets::AssetKey) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }
    let built_in = context.set_assets(Box::new(Empty));
    context.set_assets(Box::new(Live(built_in)));
    context
}

#[cfg(test)]
#[path = "live_ui_tests.rs"]
mod tests;
