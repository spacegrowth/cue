//! An item is done only once every commit on its own branch is on main (#448). An item's own branch is
//! `desk/<item>` (its copy's, desk/hub/landing/trees.rs) or `desk/<item>-again` (its copy made again after the first was taken
//! back). Cue removes a copy's branch as it lands, so one still there with work main doesn't have is work that never
//! landed.
//!
//! How #417's rounds 2 and 3 were left on its branch: its first round landed from its copy. Reopened, every copy was
//! in use, so the lead started it "without waiting for a copy" (it works where its holder is, the main checkout). Its
//! holder made a worktree of its own on `desk/417` and committed there. At its DONE Cue only looked at the main
//! checkout: nothing new there, its first round's commit on main, so it went to check and was closed. Now a worktree
//! on the item's own branch is merged as its crew's is (desk/hub/landing/finish.rs `item_folders`), and anything still left
//! on its branch keeps it to check: "2 changes not landed yet", and Looks right says so instead of closing it.
//!
//! And a reopened item (sent back from your check) is done again only once something new of it landed on main since
//! (#448): #418, reopened, was said DONE as "already on main" with no new commit. With nothing new, its DONE sends it
//! back to its session, never to you.

use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::main_line::{main_checkout, main_ref, same_dir};
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// Item `id`'s own branches.
pub(crate) fn own_branches(id: &str) -> [String; 2] {
    [format!("desk/{id}"), format!("desk/{id}-again")]
}

/// The worktrees of `top`'s repository on item `id`'s own branches, but `skip` (its own copy, landed as one): a
/// worktree its holder made itself.
pub(crate) fn branch_trees(top: &str, id: &str, skip: Option<&str>) -> Vec<String> {
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
pub(crate) fn unlanded_in(top: &str, id: &str, keep: &[String]) -> usize {
    own_branches(id).iter().filter(|b| !keep.contains(b)).map(|b| left_on(top, b)).sum()
}

/// How many changes on branch `b` of `top`'s repository main doesn't have: none when there's no such branch, or main
/// has all of it (merged, or landed as one squash); one with only merges main lacks counts one.
pub(crate) fn left_on(top: &str, b: &str) -> usize {
    rev_left(top, &format!("refs/heads/{b}"))
}

/// How many changes up to `rev` (a branch, a commit) of `top`'s repository main doesn't have, as `left_on`.
pub(crate) fn rev_left(top: &str, rev_of: &str) -> usize {
    let Some(main) = main_ref(top) else { return 0 };
    let rev = |what: String| git("", top, &[], &["rev-parse", "-q", "--verify", &what]).map(|s| s.trim().to_string());
    let (Some(main_tree), Some(tip)) = (rev(format!("refs/heads/{main}^{{tree}}")), rev(format!("{rev_of}^{{commit}}"))) else { return 0 };
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

/// Whether a commit on `top`'s main since `at` (ms) changed one of `paths`, other than the item's own commits Cue
/// already knew of when it was reopened (`known`, full or short). git dates a commit to the second, so the commit
/// that landed it before it was sent back can fall in the second it was reopened, and `--since` takes it in.
fn committed_since(top: &str, at: u64, paths: &[String], known: &[String]) -> bool {
    let Some(main) = main_ref(top) else { return false };
    if paths.is_empty() {
        return false;
    }
    let since = format!("--since=@{}", at / 1000);
    let mut args = vec!["log", "--max-count=50", "--format=%H", since.as_str(), main.as_str(), "--"];
    args.extend(paths.iter().map(String::as_str));
    let old = |c: &str| known.iter().any(|k| !k.is_empty() && (c.starts_with(k.as_str()) || k.starts_with(c)));
    git("", top, &[], &args).is_some_and(|o| o.lines().map(str::trim).any(|c| !c.is_empty() && !old(c)))
}

/// Whether a commit on main since `at` (ms) changed one of `files` (absolute, outside the item's folder) in its own
/// repository: work landed in a repository beside the project's (#465: a private one linked into it, whose commits
/// the project's main never shows). A file whose folder is gone is left out.
fn landed_elsewhere(files: &[String], at: u64, known: &[String]) -> bool {
    let mut tops: Vec<(String, Option<String>)> = Vec::new();
    let mut by_main: Vec<(String, Vec<String>)> = Vec::new();
    for f in files {
        let path = std::path::Path::new(f);
        let (Some(dir), Some(name)) = (path.parent().and_then(|d| std::fs::canonicalize(d).ok()), path.file_name()) else { continue };
        let dir = dir.to_string_lossy().to_string();
        let top = match tops.iter().find(|(d, _)| *d == dir) {
            Some((_, t)) => t.clone(),
            None => {
                let t = git("", &dir, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
                tops.push((dir.clone(), t.clone()));
                t
            }
        };
        let Some(top) = top else { continue };
        let Some(rel) = std::path::Path::new(&dir).join(name).strip_prefix(&top).ok().map(|r| r.to_string_lossy().to_string()) else { continue };
        let Some((main, _)) = main_checkout(&top) else { continue };
        match by_main.iter_mut().find(|(m, _)| same_dir(m, &main)) {
            Some((_, paths)) => paths.push(rel),
            None => by_main.push((main, vec![rel])),
        }
    }
    by_main.iter().any(|(main, paths)| committed_since(main, at, paths, known))
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

/// Whether an item's "done when" says it won't change the repository: "no code changes", "no commit", "nothing in
/// the repo changes". The lead says so for mocks, docs and plans, and for work done elsewhere (another machine, a
/// service): none of it takes or waits for a copy, and its DONE with no commit goes to you to check.
pub(crate) fn rules_out_code(done_when: &str) -> bool {
    // Clause by clause, so "no code" means no code there, not "no code path panics".
    done_when.split([';', ',', '.', ':', '\n']).any(|clause| {
        let w: String = clause.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
        let w = format!(" {} ", w.split_whitespace().map(|x| if x == "repository" { "repo" } else { x }).collect::<Vec<_>>().join(" "));
        w.ends_with(" no code ") || w.ends_with(" no commit ") || w.ends_with(" no commits ") || KEEPS_THE_REPO.iter().any(|p| w.contains(p))
    })
}

/// Words in a "done when" that say the repository won't change, wherever they sit in a clause.
const KEEPS_THE_REPO: [&str; 15] = [
    " no code at all ",
    " no code needed ",
    " no code change ",
    " no code changes ",
    " nothing in the repo changes ",
    " nothing in the repo is changed ",
    " no repo changes ",
    " no changes to the repo ",
    " no change to the repo ",
    " repo is unchanged ",
    " repo stays unchanged ",
    " repo unchanged ",
    " doesn t change the repo ",
    " does not change the repo ",
    " nothing is committed ",
];

/// What a reopened item's DONE with nothing new landed says, back to its session.
pub(crate) const NOTHING_NEW: &str = "Nothing of it landed on main since it was reopened, so it isn't done: what it was sent back for still needs a change. Make it, then say DONE";

/// `NOTHING_NEW`, saying so when `n` of its changes sit on its own branch, not main.
pub(crate) fn nothing_new_words(id: &str, n: usize) -> String {
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
    pub(crate) fn desk_nothing_new(&self, id: &str) -> bool {
        let Ok((it, _, sources)) = self.desk_change_sources(id) else { return false };
        let Some((at, had)) = it.reopened else { return false };
        // Mocks, a doc, a plan: nothing of it was ever for main, so nothing new there is asked of it (#549).
        if self.desk_no_code(&it) {
            self.finish_set(id, |i| i.reopened = None);
            return false;
        }
        let merged = it.finish.as_ref().is_some_and(|f| !f.merges.is_empty());
        let own = |top: &String| -> Vec<String> {
            let here = |folder: &str| main_checkout(folder).is_some_and(|(m, _)| same_dir(&m, top));
            sources.iter().filter(|(f, _)| here(f)).flat_map(|(_, paths)| paths.iter().cloned()).collect()
        };
        // Its commits Cue knew of when it was reopened: none of them is something new.
        let known: Vec<String> =
            it.commit_in.iter().take(had).map(|(_, c)| c.clone()).chain(std::iter::once(it.commit.clone())).chain(it.finish.iter().map(|f| f.commit.clone())).collect();
        if it.commit_in.len() > had
            || merged
            || self.item_repos(&it).iter().any(|top| committed_since(top, at, &own(top), &known) || branch_landed_since(top, id, at))
            || landed_elsewhere(&self.desk_edits_elsewhere(id), at, &known)
        {
            self.finish_set(id, |i| i.reopened = None);
            return false;
        }
        true
    }

    /// Whether item `it` has no code to land (#549): nothing ever committed or merged for it, nothing on its own
    /// branches, and its work is mocks, notes, plans or a decision: it went to check as such (`mocks_only`), its "done
    /// when" says no code, or its result is somewhere else (a canvas its DESIGNS: line names, options with what
    /// follows each: #543). Such an item is done once its holder says so and you (or the lead's CLOSE) agree; main never
    /// shows it.
    pub(crate) fn desk_no_code(&self, it: &DeskItem) -> bool {
        let merged = it.finish.as_ref().is_some_and(|f| !f.merges.is_empty());
        let had = it.reopened.map_or(0, |(_, n)| n);
        if !it.commit.is_empty() || !it.commit_in.is_empty() || had > 0 || merged {
            return false;
        }
        let result_elsewhere = !it.designs.trim().is_empty() || !it.next.is_empty();
        (it.mocks_only || rules_out_code(&it.done_when) || result_elsewhere) && self.desk_unlanded(&it.id) == 0
    }

    /// How many of item `id`'s changes on its own branch aren't on main (none for a project that lands by pull
    /// request: its branch is GitHub's to merge), kept on its `finish` so its line says so.
    pub(crate) fn desk_unlanded(&self, id: &str) -> usize {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return 0 };
        let keep: Vec<String> = self.read(|st| st.desk.kept.iter().filter(|k| k.id == id).map(|k| k.branch.clone()).collect());
        let n: usize = self.item_repos(&it).iter().filter(|r| crate::desk::lands::lands_of(&it.project, r).how != "pr").map(|r| unlanded_in(r, id, &keep)).sum();
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
