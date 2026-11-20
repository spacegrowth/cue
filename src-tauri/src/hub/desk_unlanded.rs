//! An item is done only once every commit on its own branch is on main (#448). An item's own branch is
//! `desk/<item>` (its copy's, hub/desk_trees.rs) or `desk/<item>-again` (its copy made again after the first was taken
//! back). Cue removes a copy's branch as it lands, so one still there with work main doesn't have is work that never
//! landed.
//!
//! How #417's rounds 2 and 3 were left on its branch: its first round landed from its copy. Reopened, every copy was
//! in use, so the lead started it "without waiting for a copy" (it works where its holder is, the main checkout). Its
//! holder made a worktree of its own on `desk/417` and committed there. At its DONE Cue only looked at the main
//! checkout: nothing new there, its first round's commit on main, so it went to check and was closed. Now a worktree
//! on the item's own branch is merged as its crew's is (hub/desk_finish.rs `item_folders`), and anything still left
//! on its branch keeps it to check: "2 changes not landed yet", and Looks right says so instead of closing it.
//!
//! And a reopened item (sent back from your check) is done again only once something new of it landed on main since
//! (#448): #418, reopened, was said DONE as "already on main" with no new commit. With nothing new, its DONE sends it
//! back to its session, never to you.

use super::desk_git::git;
use super::desk_main_line::{main_checkout, main_ref, same_dir};
use super::*;
use crate::desk::DeskItem;

/// Item `id`'s own branches.
fn own_branches(id: &str) -> [String; 2] {
    [format!("desk/{id}"), format!("desk/{id}-again")]
}

/// The worktrees of `top`'s repository on item `id`'s own branches, but `skip` (its own copy, landed as one): a
/// worktree its holder made itself.
pub(super) fn branch_trees(top: &str, id: &str, skip: Option<&str>) -> Vec<String> {
    let Some(list) = git("", top, &[], &["worktree", "list", "--porcelain"]) else { return Vec::new() };
    let own = own_branches(id);
    let mut out = Vec::new();
    let mut path = "";
    for line in list.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = p;
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            if own.iter().any(|o| o == b) && !skip.is_some_and(|s| same_dir(s, path)) && std::path::Path::new(path).is_dir() {
                out.push(path.to_string());
            }
        }
    }
    out
}

/// How many of item `id`'s changes on its own branches in `top`'s repository main doesn't have (branches in `keep`,
/// a dropped copy's work you kept, left out).
pub(super) fn unlanded_in(top: &str, id: &str, keep: &[String]) -> usize {
    own_branches(id).iter().filter(|b| !keep.contains(b)).map(|b| left_on(top, b)).sum()
}

/// How many changes on branch `b` of `top`'s repository main doesn't have: none when there's no such branch, or main
/// has all of it (merged, or landed as one squash); one with only merges main lacks counts one.
pub(super) fn left_on(top: &str, b: &str) -> usize {
    let Some(main) = main_ref(top) else { return 0 };
    let rev = |what: String| git("", top, &[], &["rev-parse", "-q", "--verify", &what]).map(|s| s.trim().to_string());
    let (Some(main_tree), Some(tip)) = (rev(format!("refs/heads/{main}^{{tree}}")), rev(format!("refs/heads/{b}"))) else { return 0 };
    if matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", &tip, &main]), Ok((_, 0))) {
        return 0;
    }
    // Main with it merged in is main as it is: all its changes are there already.
    let filters = crate::safe_git::no_repo_filters("", top);
    if let Ok((out, 0)) = crate::safe_git::run("", top, &filters, &["merge-tree", "--write-tree", &main, &tip]) {
        if out.lines().next().map(str::trim) == Some(main_tree.as_str()) {
            return 0;
        }
    }
    let cherry = git("", top, &[], &["cherry", &main, &tip]).unwrap_or_default();
    cherry.lines().filter(|l| l.starts_with('+')).count().max(1)
}

/// Whether a commit on `top`'s main since `at` (ms) changed one of `paths`.
fn committed_since(top: &str, at: u64, paths: &[String]) -> bool {
    let Some(main) = main_ref(top) else { return false };
    if paths.is_empty() {
        return false;
    }
    let since = format!("--since=@{}", at / 1000);
    let mut args = vec!["log", "-1", "--format=%H", since.as_str(), main.as_str(), "--"];
    args.extend(paths.iter().map(String::as_str));
    git("", top, &[], &args).is_some_and(|o| !o.trim().is_empty())
}

/// Whether a commit on one of item `id`'s own branches in `top`'s repository made since `at` (ms) is on main now.
fn branch_landed_since(top: &str, id: &str, at: u64) -> bool {
    let Some(main) = main_ref(top) else { return false };
    let since = format!("--since=@{}", at / 1000);
    own_branches(id).iter().any(|b| {
        let made = git("", top, &[], &["rev-list", "--max-count=50", &since, &format!("refs/heads/{b}"), "--"]).unwrap_or_default();
        made.lines().any(|c| matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", c.trim(), &main]), Ok((_, 0))))
    })
}

/// What a reopened item's DONE with nothing new landed says, back to its session.
pub(crate) const NOTHING_NEW: &str = "Nothing of it landed on main since it was reopened, so it isn't done: what it was sent back for still needs a change. Make it, then say DONE";

/// `NOTHING_NEW`, saying so when `n` of its changes sit on its own branch, not main.
pub(super) fn nothing_new_words(id: &str, n: usize) -> String {
    if n == 0 {
        return NOTHING_NEW.to_string();
    }
    format!("Nothing of it landed on main since it was reopened: {} on its branch (desk/{id}). Land them on main, then say DONE", unlanded_words(n))
}

/// "1 change not landed yet", "3 changes not landed yet".
pub(crate) fn unlanded_words(n: usize) -> String {
    format!("{n} change{} not landed yet", if n == 1 { "" } else { "s" })
}

impl Hub {
    /// The main checkouts item `it` worked in on this Mac: where it landed, its commits', its copy's and its holder's.
    fn item_repos(&self, it: &DeskItem) -> Vec<String> {
        let holder = self.read(|st| it.holder.as_ref().and_then(|h| st.sessions.get(h.to())).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone()));
        let folders = it.finish.iter().map(|f| f.repo.clone()).chain(it.commit_in.iter().map(|(f, _)| f.clone())).chain(it.tree.iter().map(|t| t.repo.clone())).chain(holder);
        let mut out: Vec<String> = Vec::new();
        for f in folders.filter(|f| !f.is_empty()) {
            if let Some((m, _)) = main_checkout(&f) {
                if !out.iter().any(|o| same_dir(o, &m)) {
                    out.push(m);
                }
            }
        }
        out
    }

    /// Whether reopened item `id` landed nothing new on main (in its repositories) since it was reopened: no new commit Cue made
    /// or merged for it, none on main since then changing a file its holder edited, and none made on its own branch
    /// since then on main. Something new: no longer reopened.
    pub(super) fn desk_nothing_new(&self, id: &str) -> bool {
        let Ok((it, _, sources)) = self.desk_change_sources(id) else { return false };
        let Some((at, had)) = it.reopened else { return false };
        let merged = it.finish.as_ref().is_some_and(|f| !f.merges.is_empty());
        let own = |top: &String| -> Vec<String> {
            let here = |folder: &str| main_checkout(folder).is_some_and(|(m, _)| same_dir(&m, top));
            sources.iter().filter(|(f, _)| here(f)).flat_map(|(_, paths)| paths.iter().cloned()).collect()
        };
        if it.commit_in.len() > had || merged || self.item_repos(&it).iter().any(|top| committed_since(top, at, &own(top)) || branch_landed_since(top, id, at)) {
            self.finish_set(id, |i| i.reopened = None);
            return false;
        }
        true
    }

    /// How many of item `id`'s changes on its own branch aren't on main (none for a project that lands by pull
    /// request: its branch is GitHub's to merge), kept on its `finish` so its line says so.
    pub(super) fn desk_unlanded(&self, id: &str) -> usize {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return 0 };
        let keep: Vec<String> = self.read(|st| st.desk.kept.iter().filter(|k| k.id == id).map(|k| k.branch.clone()).collect());
        let n: usize = self.item_repos(&it).iter().filter(|r| crate::lands::lands_of(&it.project, r).how != "pr").map(|r| unlanded_in(r, id, &keep)).sum();
        let was = it.finish.as_ref().map(|f| f.unlanded);
        if was.is_some_and(|w| w != n) {
            let saved = self.desk_saving(|| {
                let it = self.write(|st| {
                    let i = st.desk.item_mut(id)?;
                    i.finish.as_mut()?.unlanded = n;
                    Some(i.clone())
                });
                it.inspect(crate::db::desk_put_item)
            });
            if saved.is_some() {
                self.changed();
            }
        }
        n
    }
}
