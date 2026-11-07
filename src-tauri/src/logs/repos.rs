//! What the disk says about each folder a session runs in: its repository's main checkout, the top of its
//! checkout, the branch checked out there, and the project its repository says it goes with. Read from
//! memory: the disk is only ever read on a thread of its own, so a folder on a disk that stalls (an
//! automounted /home, a network share) holds nothing else up.
//!
//! Asking never waits: a folder not known yet is looked up and is unknown (no repository) until the lookup
//! comes back; the tick's `refresh` says so, and the hub then derives the projects it keeps again. Where a
//! new folder first shows up (a hook from a session in it), `first_look` waits for it a short while
//! (`FIRST_WAIT`) with no lock held: a local folder answers in well under that, so its first card already
//! says its project. Every folder asked about lately is looked up again on each `refresh` (the 3-second
//! tick), so a branch checked out or a folder made a repository shows within a tick; one not asked about
//! for a long while is forgotten. A folder has one lookup out at a time, so one that hangs is never joined
//! by another.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// A folder, as its lookup found it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Its repository's main checkout (for a worktree, the checkout it was made from; for a bare
    /// repository, the repository); None outside one.
    pub root: Option<PathBuf>,
    /// The top of its checkout: the folder with its `.git` (a worktree's own folder).
    pub top: Option<PathBuf>,
    /// The branch checked out ("main"), a short commit when detached, "" outside a repository.
    pub branch: String,
    /// The project the checkout's `.cue/actions.json` says it belongs to (`"project"`), at its top.
    pub declared_top: Option<String>,
    /// ...and at the main checkout, when that's another folder.
    pub declared_root: Option<String>,
}

/// How long the first asker of a new folder waits for its lookup.
const FIRST_WAIT: Duration = Duration::from_millis(200);
/// How long `refresh` waits for its lookups before it reports what changed.
const REFRESH_WAIT: Duration = Duration::from_secs(1);
/// A folder not asked about for this long isn't looked up again until it's asked about.
const RECENT: Duration = Duration::from_secs(600);
/// A folder not asked about for this long is forgotten (unless its lookup is still out).
const FORGET: Duration = Duration::from_secs(6 * 3600);

struct Entry {
    facts: Option<Facts>,
    /// A lookup is out (one per folder at a time: one that hangs isn't joined by another every tick).
    looking: bool,
    /// Someone waited `first_wait` for it already, so nobody waits again.
    waited: bool,
    asked: Instant,
}

#[derive(Default)]
struct Known {
    folders: HashMap<String, Entry>,
    /// A lookup found something different since the last `refresh` reported.
    changed: bool,
}

/// The folders seen, and how one is looked up (the disk, or a test's stand-in).
pub struct Folders {
    known: Mutex<Known>,
    found: Condvar,
    look: fn(&str) -> Facts,
    first_wait: Duration,
}

impl Folders {
    pub fn new(look: fn(&str) -> Facts, first_wait: Duration) -> Self {
        Folders { known: Mutex::new(Known::default()), found: Condvar::new(), look, first_wait }
    }

    /// `cwd`'s facts from memory; None while they're unknown (looked up now, if no lookup is out).
    pub fn facts(&'static self, cwd: &str) -> Option<Facts> {
        let mut known = crate::locking::lock(&self.known, "folder facts");
        self.asked(&mut known, cwd).facts.clone()
    }

    /// `cwd` asked about: its entry, its lookup started if it's unknown and none is out.
    fn asked<'k>(&'static self, known: &'k mut Known, cwd: &str) -> &'k mut Entry {
        let e = known.folders.entry(cwd.to_string()).or_insert_with(|| Entry { facts: None, looking: false, waited: false, asked: Instant::now() });
        e.asked = Instant::now();
        if e.facts.is_none() && !e.looking {
            e.looking = true;
            self.look_up(cwd.to_string());
        }
        e
    }

    /// `cwd`'s facts, waiting `first_wait` for them the first time it's asked about (nobody waits again).
    /// Call with no lock held.
    pub fn first_look(&'static self, cwd: &str) -> Option<Facts> {
        let mut known = crate::locking::lock(&self.known, "folder facts");
        let e = self.asked(&mut known, cwd);
        if e.facts.is_some() || std::mem::replace(&mut e.waited, true) {
            return e.facts.clone();
        }
        let deadline = Instant::now() + self.first_wait;
        loop {
            if let Some(f) = known.folders.get(cwd).and_then(|e| e.facts.clone()) {
                return Some(f);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            known = crate::locking::Guard::wait_timeout(known, &self.found, left);
        }
    }

    /// Look every folder asked about lately up again, and wait a little for them. True when one changed (or a
    /// folder unknown till now came back) since the last refresh.
    pub fn refresh(&'static self) -> bool {
        let mut known = crate::locking::lock(&self.known, "folder facts");
        known.folders.retain(|_, e| e.looking || e.asked.elapsed() < FORGET);
        let due: Vec<String> = known.folders.iter_mut()
            .filter(|(_, e)| !e.looking && e.asked.elapsed() < RECENT)
            .map(|(cwd, e)| {
                e.looking = true;
                cwd.clone()
            })
            .collect();
        for cwd in &due {
            self.look_up(cwd.clone());
        }
        let deadline = Instant::now() + REFRESH_WAIT;
        while due.iter().any(|c| known.folders.get(c).is_some_and(|e| e.looking)) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            known = crate::locking::Guard::wait_timeout(known, &self.found, left);
        }
        std::mem::take(&mut known.changed)
    }

    /// Look `cwd` up on a thread of its own and keep what it finds.
    fn look_up(&'static self, cwd: String) {
        std::thread::spawn(move || {
            let seen = (self.look)(&cwd);
            let mut known = crate::locking::lock(&self.known, "folder facts");
            let Known { folders, changed } = &mut *known;
            if let Some(e) = folders.get_mut(&cwd) {
                *changed |= e.facts.as_ref() != Some(&seen);
                e.facts = Some(seen);
                e.looking = false;
            }
            self.found.notify_all();
        });
    }
}

static FOLDERS: LazyLock<Folders> = LazyLock::new(|| Folders::new(look_on_disk, FIRST_WAIT));

/// `cwd`'s facts, from memory; None outside a known folder (or one still being looked up).
pub fn facts(cwd: &str) -> Option<Facts> {
    if cwd.is_empty() {
        return None;
    }
    FOLDERS.facts(cwd)
}

/// `cwd` seen, by whoever is about to need its facts: waited for a short while the first time (see the
/// module's note). Call with no lock held.
pub fn first_look(cwd: &str) {
    if !cwd.is_empty() {
        FOLDERS.first_look(cwd);
    }
}

/// Look the folders asked about lately up again (the 3-second tick); true when one changed.
pub fn refresh() -> bool {
    FOLDERS.refresh()
}

/// The main checkout of the repository `cwd` is in (`Facts::root`).
pub fn repo_root(cwd: &str) -> Option<PathBuf> {
    facts(cwd)?.root
}

/// The top of the checkout `cwd` is in (`Facts::top`).
pub fn checkout_top(cwd: &str) -> Option<PathBuf> {
    facts(cwd)?.top
}

/// The branch checked out in `cwd` (`Facts::branch`).
pub fn branch(cwd: &str) -> String {
    facts(cwd).map(|f| f.branch).unwrap_or_default()
}

/// A repository's name: its main checkout's folder ("cue"; a bare `cue.git` is "cue").
pub fn repo_name(root: &Path) -> String {
    let n = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    n.strip_suffix(".git").filter(|s| !s.is_empty()).map(str::to_string).unwrap_or(n)
}

/// What the disk says about `cwd`: the lookup the refreshes run, off every caller's thread.
fn look_on_disk(cwd: &str) -> Facts {
    let dir = Path::new(cwd);
    let top = dir.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf);
    let root = find_root(dir);
    let declared_root = root.as_deref().filter(|r| Some(*r) != top.as_deref()).and_then(declared);
    Facts { branch: head_file(dir).map(|h| branch_in(&h)).unwrap_or_default(), declared_top: top.as_deref().and_then(declared), declared_root, root, top }
}

/// The git folder a checkout's `.git` file points at (a worktree's, a submodule's).
fn linked_gitdir(here: &Path, git: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git).ok()?;
    let gitdir = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    Some(if Path::new(gitdir).is_absolute() { PathBuf::from(gitdir) } else { here.join(gitdir) })
}

/// The HEAD file of the repository `dir` is in (a worktree's `.git` is a file pointing at its own).
fn head_file(dir: &Path) -> Option<PathBuf> {
    for here in dir.ancestors() {
        let git = here.join(".git");
        if git.is_dir() {
            return Some(git.join("HEAD"));
        }
        if git.is_file() {
            return Some(linked_gitdir(here, &git)?.join("HEAD"));
        }
    }
    None
}

/// The branch a HEAD file names, or its commit, short, when detached.
fn branch_in(head: &Path) -> String {
    let text = std::fs::read_to_string(head).unwrap_or_default();
    let text = text.trim();
    match text.strip_prefix("ref: refs/heads/") {
        Some(name) => name.to_string(),
        None => text.chars().take(7).collect(),
    }
}

fn find_root(dir: &Path) -> Option<PathBuf> {
    for here in dir.ancestors() {
        let git = here.join(".git");
        if git.is_dir() {
            return Some(here.to_path_buf());
        }
        if git.is_file() {
            let gitdir = linked_gitdir(here, &git)?;
            // A worktree's git folder names the repository's own in `commondir`; a submodule's has none
            // (it's its own repository, checked out here).
            let Ok(common) = std::fs::read_to_string(gitdir.join("commondir")) else { return Some(here.to_path_buf()) };
            let common = gitdir.join(common.trim());
            let common = common.canonicalize().unwrap_or(common);
            return Some(match common.file_name().and_then(|n| n.to_str()) {
                Some(".git") => common.parent().map(Path::to_path_buf).unwrap_or(common),
                _ => common,
            });
        }
    }
    None
}

/// The project a checkout's .cue/actions.json says it belongs to (`"project"`).
fn declared(top: &Path) -> Option<String> {
    let text = std::fs::read_to_string(top.join(".cue").join("actions.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?
        .get("project").and_then(serde_json::Value::as_str).map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= 80)
}

#[cfg(test)]
#[path = "repos_tests.rs"]
mod tests;
