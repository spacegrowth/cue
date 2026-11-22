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
    return crate::install::self_install::running_commit();
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
pub fn outside_app(path: &str) -> bool {
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

/// The paths commit `sha` of `repo` changed (a merge: what it brought onto its first parent); None when git can't say.
pub fn changed_in(repo: &str, sha: &str) -> Option<Vec<String>> {
    let parent = crate::safe_git::run("", repo, &[], &["rev-parse", &format!("{sha}^1")]).ok().filter(|(_, c)| *c == 0)?.0.trim().to_string();
    let full = crate::safe_git::run("", repo, &[], &["rev-parse", &format!("{sha}^{{commit}}")]).ok().filter(|(_, c)| *c == 0)?.0.trim().to_string();
    changed(repo, &parent, &full, &[])
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

/// The window at `head` of `repo` checked before it's served: scripts/check-window.mjs run on a clean copy of that
/// commit (its ui/, tests/ui/ and the check, with `repo`'s node_modules and add-on linked in). Err says what's wrong
/// (a name its scripts call that none declares, one declared twice, a page that doesn't load). A commit from before
/// the check has none to run: Ok.
pub fn check(repo: &str, head: &str) -> Result<(), String> {
    check_in(&live_dir(), repo, head)
}

/// `check`, its copy made in `dir` and removed after.
pub(crate) fn check_in(dir: &Path, repo: &str, head: &str) -> Result<(), String> {
    if !looks_like_commit(head) {
        return Err(format!("not a commit: {head}"));
    }
    let has = crate::safe_git::run("", repo, &[], &["cat-file", "-e", &format!("{head}:scripts/check-window.mjs")]);
    if !matches!(has, Ok((_, 0))) {
        return Ok(());
    }
    // A folder of its own: two items done at the same commit are checked at once, and one removing a shared
    // "{head}.check" as it ended took the other's files from under it (#318).
    static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let work = dir.join(format!("{head}.{}-{run}.check", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = check_copy(&work, repo, head);
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// How many times the check's copy is made, in a fresh folder each time, before a check fails on it: git or tar
/// failing once (a moment's contention on the repository) is no word on the item's window (#323).
const COPY_TRIES: u64 = 3;

#[cfg(test)]
thread_local! {
    /// Tests: how many copies on this thread fail before one is made.
    static COPIES_FAIL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn check_copy(work: &Path, repo: &str, head: &str) -> Result<(), String> {
    let mut tried = 0;
    loop {
        tried += 1;
        match copy_out(work, repo, head) {
            Ok(()) => break,
            Err(_) if tried < COPY_TRIES => {
                let _ = std::fs::remove_dir_all(work);
                std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
                // A short pause, longer each time, for whatever held the repository to let go.
                std::thread::sleep(std::time::Duration::from_millis(300 * tried));
            }
            Err(why) => return Err(format!("{why} (tried {COPY_TRIES} times; the window itself wasn't checked)")),
        }
    }
    run_check(work)
}

/// `head`'s ui/, tests/ui/ and the check, unpacked in `work`, with `repo`'s node_modules and add-on linked in.
fn copy_out(work: &Path, repo: &str, head: &str) -> Result<(), String> {
    #[cfg(test)]
    if COPIES_FAIL.with(|n| n.replace(n.get().saturating_sub(1)) > 0) {
        return Err(format!("git couldn't copy {head} to check its window"));
    }
    let tar = work.join("copy.tar");
    let tar_s = tar.to_string_lossy().to_string();
    let filters = crate::safe_git::no_repo_filters("", repo);
    let mut args = vec!["archive", "--format=tar", "-o", &tar_s, head, "ui", "scripts/check-window.mjs"];
    // The UI tests' harness, for the load: when the commit has it.
    let harness = crate::safe_git::run("", repo, &[], &["cat-file", "-e", &format!("{head}:tests/ui")]);
    if matches!(harness, Ok((_, 0))) {
        args.push("tests/ui");
    }
    match crate::safe_git::run("", repo, &filters, &args) {
        Ok((_, 0)) => {}
        Ok((said, _)) | Err(said) => return Err(format!("git couldn't copy {head} to check its window: {}", said.trim())),
    }
    let out = std::process::Command::new("/usr/bin/tar").arg("-xf").arg(&tar).arg("-C").arg(work).output();
    let _ = std::fs::remove_file(&tar);
    if !out.is_ok_and(|o| o.status.success()) {
        return Err(format!("couldn't unpack {head} to check its window"));
    }
    let repo = Path::new(repo);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if repo.join("node_modules").exists() {
            symlink(repo.join("node_modules"), work.join("node_modules")).map_err(|e| e.to_string())?;
        }
        // The window as the running Cue has it: with the optional add-on's part when it's built in.
        if let Ok(ext) = std::fs::canonicalize(repo.join("src-tauri/ext")) {
            std::fs::create_dir_all(work.join("src-tauri")).map_err(|e| e.to_string())?;
            symlink(ext, work.join("src-tauri/ext")).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// scripts/check-window.mjs run in the copy `work`.
fn run_check(work: &Path) -> Result<(), String> {
    // Started from the Finder, Cue has a short PATH: node is found on the login shell's.
    let out = std::process::Command::new(crate::desk::lands::login_shell())
        .arg("-lc")
        .arg("exec node scripts/check-window.mjs .")
        .current_dir(work)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("couldn't run the window's check: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if said.is_empty() { err } else { said })
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

/// Every copy but `keep` removed; a window check at work (its "….check" folder, removed by its own end) left alone.
fn tidy(dir: &Path, keep: &str) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() && name != keep && !name.ends_with(".check") {
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
