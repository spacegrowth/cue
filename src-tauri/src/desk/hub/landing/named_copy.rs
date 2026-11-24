//! An item at work in another repository, on a branch the lead names (#557). Work for Cue's add-on lives in the
//! add-on's own repository, on a branch of its own (Loops on `loops-d`, say), never on Cue's main line. So the lead
//! says `COPY: #<id> | in: add-on | branch: loops-d | why: …` (or a repository's folder in place of "add-on"), and as
//! the item starts its holder gets a copy of that repository on that branch: a worktree in
//! `<its main checkout>/.claude/worktrees/desk-<n>`, its edits there let through as any copy's are
//! (desk/hub/landing/copy_guard.rs).
//!
//! At its DONE what's in its copy is committed on its branch, as one commit named after the item, the copy goes and
//! the branch stays (it's where the work is; Cue never removes it). The item goes to check with that commit as its
//! own, so its Files changed show it as any other item's.
//!
//! In a repository named by its folder nothing goes to a main line, and neither its project's check nor its words are
//! run on it. In the add-on's (#565), however the lead named it, it lands as Cue's own items do: its branch merged into the add-on's main line
//! (its words checked, a clash back to its holder with its copy kept), then the Cue it's built into installed with
//! it in the next install, and to check once the running Cue has that add-on commit. Work that changed nothing
//! the app builds from the add-on (mocks beside it, say) goes to check with no install.
//!
//! How #582's work was left off the add-on's main line: its copy was made by a Cue from before #565, which didn't
//! note whose add-on it was, so its DONE only committed on its branch and removed the copy. Sent back from there, it
//! got a copy of Cue (none of the add-on's any more), and each DONE after landed that empty copy while its add-on
//! commit stayed on desk/582: "1 change not landed yet". Now whose add-on a copy is is looked up again as it lands
//! when it wasn't noted, and an item's work left on the add-on's branches (its own, or holding a commit of its) is
//! merged into the add-on's main line at its DONE, wherever it works now (`land_addon_left`).

use crate::desk::hub::core::desk::{secs, Effects};
use crate::desk::hub::landing::git::{git, git_said};
use crate::desk::hub::landing::main_line::{main_checkout, merge_ref, same_dir};
use crate::desk::hub::landing::tree_files::{exclude_trees, remove, rev, snapshot};
use crate::desk::hub::prelude::*;
use crate::desk::hub::turn::route::{decide, By};
use crate::desk::{CopyOn, DeskItem, ItemTree};

/// The main checkout `said` names: "add-on" is the repository Cue's add-on link (`src-tauri/ext`, in the folder of
/// `from` or one above it) points into; anything else is a repository's main checkout as its folder. With it, for
/// the add-on (said either way), the main checkout of the Cue it's built into ("" for any other).
pub(crate) fn repo_named(said: &str, from: &str) -> Result<(String, String), String> {
    let top_of = |dir: &std::path::Path| dir.ancestors().find(|a| a.join(".git").exists()).map(|a| a.to_path_buf());
    let cue;
    let dir = if said.eq_ignore_ascii_case("add-on") || said.eq_ignore_ascii_case("addon") {
        let link = std::path::Path::new(from).ancestors().map(|a| a.join("src-tauri/ext")).find(|l| l.exists()).ok_or("Cue has no add-on here (no src-tauri/ext in the Desk's folder)")?;
        let real = std::fs::canonicalize(&link).map_err(|e| format!("Cue can't read the add-on's link: {e}"))?;
        let here = link.parent().and_then(|p| p.parent()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        cue = main_checkout(&here).map(|(m, _)| m).unwrap_or(here);
        top_of(&real).ok_or("the add-on isn't in a git repository")?
    } else {
        let p = std::path::Path::new(said);
        if !p.is_absolute() {
            return Err(format!("\"{said}\" isn't a folder: say in: add-on, or the repository's full path"));
        }
        let top = top_of(p).ok_or_else(|| format!("{said} isn't in a git repository"))?;
        // The add-on's repository named by its folder: the add-on all the same.
        cue = cue_of_addon(&top.to_string_lossy(), &[from.to_string()]);
        top
    };
    // A worktree's `.git` is a file: its main checkout is the one copies are made from.
    if !dir.join(".git").is_dir() {
        return Err(format!("{} is a worktree, not its repository's main checkout", dir.display()));
    }
    Ok((dir.to_string_lossy().to_string(), cue))
}

/// The repositories a reply's COPY lines name, each as said and what it is (`repo_named`), looked up from the folder
/// `from` the lead works in. Read before the store is taken: it looks at the disk.
pub(crate) fn repos_said(text: &str, from: &str) -> Vec<(String, Result<(String, String), String>)> {
    let mut out: Vec<(String, Result<(String, String), String>)> = Vec::new();
    for line in crate::desk::parse::lines(text) {
        if let crate::desk::parse::Line::Copy { repo, .. } = line {
            if !out.iter().any(|(r, _)| *r == repo) {
                let named = repo_named(&repo, from);
                out.push((repo, named));
            }
        }
    }
    out
}

/// The lead's COPY line: item `id` of `project` gets a copy of `repo` (its main checkout, as `repos_said` found it) on
/// `branch` as it starts, or now when it's at work. Err: what the lead is told.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_said(st: &mut Store, project: &str, by: &By, id: &str, repo: Result<(String, String), String>, branch: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let it = st.desk.item(id).filter(|i| i.project == project).ok_or_else(|| format!("#{id} got no copy: this Desk has no item #{id}"))?;
    if matches!(it.state.as_str(), "done" | "finishing" | "to_check") {
        return Err(format!("#{id} got no copy: it's {} already", if it.state == "done" { "done" } else { "finishing" }));
    }
    let (repo, cue) = repo.map_err(|e| format!("#{id} got no copy: {e}"))?;
    if let Some(t) = &it.tree {
        if t.keep && t.repo == repo && t.branch == branch {
            return Ok(());
        }
        return Err(format!("#{id} got no copy: it has one already ({}, branch {}). Take it back first (TAKE COPY: #{id} | why: …), then say COPY again", t.path, t.branch));
    }
    let working = it.state == "working";
    let holder = it.holder.as_ref().map(|h| h.to().to_string());
    let it = st.desk.item_mut(id).ok_or("gone")?;
    it.copy_on = Some(CopyOn { repo: repo.clone(), branch: branch.to_string(), cue });
    it.updated = secs(now);
    fx.items.push(id.to_string());
    // At work already: its copy is made now (desk/hub/landing/trees.rs), and its holder told where by this message.
    if let (true, Some(to)) = (working, holder) {
        fx.sends.push((to, format!("Desk item #{id}: the Desk lead gave it its own copy of {repo}, on branch {branch}. Move your work there.")));
    }
    let text = format!("Gave #{id} a copy of {repo} on branch {branch}");
    decide(st, project, Some(id.to_string()), by.name, by.sid, &text, why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
        d.act = "copy".into();
    }
    Ok(())
}

/// Copy `t` (a branch the lead named) made for item `id`: on its branch when there is one (where the work so far is),
/// else on a new one from the main line's tip. The commit it starts from, and its folder.
pub(crate) fn prepare_named(t: &ItemTree, id: &str) -> Result<(String, String), String> {
    exclude_trees(&t.repo);
    let _ = git("", &t.repo, &[], &["worktree", "prune"]);
    let path = t.path.clone();
    if std::path::Path::new(&path).exists() {
        // Left by a Cue that stopped: used again when it's on this branch, else left as it is.
        let on = git("", &path, &[], &["branch", "--show-current"]).map(|b| b.trim().to_string()).unwrap_or_default();
        if on != t.branch {
            return Err(format!("{path} is there already (for #{id}'s copy)"));
        }
    } else {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let because = |e: String| crate::desk::hub::landing::git::because(format!("git couldn't make a copy on {}", t.branch), &e);
        if rev(&t.repo, &format!("refs/heads/{}", t.branch)).is_some() {
            git_said("", &t.repo, &[], &["worktree", "add", "-q", &path, &t.branch]).map_err(because)?;
        } else {
            let (_, onto) = main_checkout(&t.repo).ok_or("no main checkout")?;
            let tip = rev(&t.repo, &onto).ok_or("no main line")?;
            git_said("", &t.repo, &[], &["worktree", "add", "-q", "-b", &t.branch, &path, &tip]).map_err(because)?;
        }
    }
    let base = rev(&path, "HEAD").ok_or("its copy has no commit")?;
    Ok((base, path))
}

/// Of `made`, the commits item `id` doesn't have yet (a round stopped on a clash listed them already).
fn new_of(hub: &Hub, id: &str, made: &[(String, String)]) -> Vec<(String, String)> {
    let had: Vec<(String, String)> = hub.read(|st| st.desk.item(id).map(|i| i.commit_in.clone())).unwrap_or_default();
    made.iter().filter(|m| !had.contains(m)).cloned().collect()
}

/// Whether the add-on's main line (`main`, its main checkout) changed, from `before` to `head`, anything Cue `cue`
/// builds from it: a file in the folder its add-on link points to. True when git can't say.
fn in_app(cue: &str, main: &str, before: &str, head: &str) -> bool {
    let (Ok(link), Ok(top)) = (std::fs::canonicalize(std::path::Path::new(cue).join("src-tauri/ext")), std::fs::canonicalize(main)) else { return true };
    let Ok(sub) = link.strip_prefix(&top) else { return true };
    if before.is_empty() {
        return true;
    }
    let sub = sub.to_string_lossy().to_string();
    let sub = if sub.is_empty() { ".".to_string() } else { sub };
    git("", main, &[], &["diff", "--name-only", before, head, "--", &sub]).is_none_or(|out| !out.trim().is_empty())
}

/// The Cue (its main checkout) among `folders` whose add-on link (`src-tauri/ext`) points into repository `repo`; ""
/// when none does.
pub(crate) fn cue_of_addon(repo: &str, folders: &[String]) -> String {
    let Ok(top) = std::fs::canonicalize(repo) else { return String::new() };
    for f in folders.iter().filter(|f| !f.is_empty()) {
        let Some((cue, _)) = main_checkout(f) else { continue };
        let Ok(link) = std::fs::canonicalize(std::path::Path::new(&cue).join("src-tauri/ext")) else { continue };
        if link.starts_with(&top) && !same_dir(&cue, repo) {
            return cue;
        }
    }
    String::new()
}

/// Item `it`'s work left off repository `repo`'s main line: its own branches' tips (but `keep`, a dropped copy's work
/// you kept) and its commits there, each (a branch holding it, the commit).
fn left_off(repo: &str, it: &DeskItem, keep: &[String]) -> Vec<(String, String)> {
    let full = |r: &str| git("", repo, &[], &["rev-parse", "-q", "--verify", &format!("{r}^{{commit}}")]).map(|s| s.trim().to_string());
    let mut out: Vec<(String, String)> = Vec::new();
    for b in crate::desk::hub::landing::unlanded::own_branches(&it.id).into_iter().filter(|b| !keep.contains(b)) {
        if let Some(tip) = full(&format!("refs/heads/{b}")) {
            out.push((b, tip));
        }
    }
    for (_, c) in it.commit_in.iter().filter(|(r, _)| main_checkout(r).is_some_and(|(m, _)| same_dir(&m, repo))) {
        let Some(sha) = full(c) else { continue };
        let on = git("", repo, &[], &["branch", "--contains", &sha, "--format=%(refname:short)"]).unwrap_or_default();
        let branch = on.lines().map(str::trim).find(|b| !b.is_empty()).map_or_else(|| format!("desk/{}", it.id), str::to_string);
        out.push((branch, sha));
    }
    out.retain(|(_, sha)| crate::desk::hub::landing::unlanded::rev_left(repo, sha) > 0);
    out
}

/// What a holder is told of a copy on a branch the lead named.
pub(crate) fn named_line(t: &ItemTree) -> String {
    if !t.addon_of.is_empty() {
        return format!(
            "Work in its own copy of {}: {} (branch {}). Change files only there, make no worktree or branch of your own, \
             and don't merge or push: at your DONE, Cue commits everything in it on {} as one commit, merges {} into \
             the add-on's main line and installs Cue with it.",
            t.repo, t.path, t.branch, t.branch, t.branch
        );
    }
    format!(
        "Work in its own copy of {}: {} (branch {}). Change files only there, make no worktree or branch of your own, \
         and don't merge or push: at your DONE, Cue commits everything in it on {} as one commit and leaves it there \
         (nothing goes to a main line).",
        t.repo, t.path, t.branch, t.branch
    )
}

/// Everything in copy `t` committed on its branch: the commits made on it since it started (`t.base`), and one more
/// for what wasn't committed yet (named `message`). Each (repository, short hash), oldest first; none when it holds
/// nothing new.
pub(crate) fn commit_on_branch(t: &ItemTree, message: &str) -> Result<Vec<(String, String)>, String> {
    let work = snapshot(&t.path).ok_or_else(|| format!("Cue can't read its copy at {}", t.path))?;
    let head = rev(&t.path, "HEAD").ok_or("its copy has no commit")?;
    if rev(&t.path, "HEAD^{tree}").as_deref() != Some(work.as_str()) {
        let c = git("", &t.path, &[], &["commit-tree", &work, "-p", &head, "-m", message]).map(|c| c.trim().to_string()).ok_or("git couldn't make its commit")?;
        git_said("", &t.path, &[], &["update-ref", "-m", "desk: item done", &format!("refs/heads/{}", t.branch), &c, &head]).map_err(|e| crate::desk::hub::landing::git::because("git couldn't move its branch".into(), &e))?;
    }
    let range = if t.base.is_empty() { t.branch.clone() } else { format!("{}..{}", t.base, t.branch) };
    let list = git("", &t.repo, &[], &["rev-list", "--reverse", "--abbrev-commit", &range]).unwrap_or_default();
    Ok(list.lines().map(|c| (t.repo.clone(), c.trim().to_string())).filter(|(_, c)| !c.is_empty()).collect())
}

impl Hub {
    /// Item `id`, at work in a copy on a branch the lead named, said DONE: its work committed on that branch, the
    /// copy gone and the branch kept, and the item to check with its commits. Nothing in it: as any item whose holder
    /// worked where it was.
    pub(crate) fn land_on_branch(&self, id: &str, t: &ItemTree) {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return };
        // Made by a Cue that didn't note whose add-on it is (before #565: #582's): looked up now.
        let looked_up;
        let t = if t.addon_of.is_empty() {
            looked_up = ItemTree { addon_of: self.addon_cue(&it, &t.repo), ..t.clone() };
            &looked_up
        } else {
            t
        };
        let title = it.title;
        let made = match commit_on_branch(t, &format!("{title} (#{id})")) {
            Ok(m) => m,
            Err(e) => {
                self.finish_ask(id, "error", &e, "");
                return;
            }
        };
        // The add-on's (#565): merged into its main line first; stopped, its copy stays for what follows.
        let merged = if t.addon_of.is_empty() {
            None
        } else {
            match self.merge_addon(id, t, &title) {
                Some(m) => Some(m),
                None => return,
            }
        };
        remove(t);
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.tree = None;
                i.copy_on = None;
            }
        });
        let _ = self.desk_trees_free(&t.repo);
        if let Some((before, merge)) = merged {
            self.save_made(id, &new_of(self, id, &made));
            if let Some(m) = merge {
                self.finish_fin(id, |f| f.merges.push(m));
            }
            self.install_addon(id, &t.addon_of, &t.repo, &before);
            return;
        }
        if made.is_empty() {
            self.finish_fin(id, |f| (f.step, f.wait) = ("commit".into(), String::new()));
            self.desk_finish_shared(id);
            return;
        }
        self.save_made(id, &made);
        // No main line to wait for, nothing to install: to check.
        self.finish_after(id, &[]);
    }

    /// Item `id`'s branch (its copy `t`, of the add-on) merged into the add-on's main line, as a crew's branch is
    /// merged into Cue's. Some: the main line's tip before, and the merge (main checkout, merge commit) when one was
    /// made. None: stopped, said where (a clash goes back to its holder, its copy kept to settle it in).
    fn merge_addon(&self, id: &str, t: &ItemTree, title: &str) -> Option<(String, Option<(String, String)>)> {
        let project = self.read(|st| st.desk.item(id).map(|i| st.desk.project_of(&i.project).to_string())).unwrap_or_default();
        let Some((main, _)) = main_checkout(&t.repo).filter(|(_, b)| !b.is_empty()) else {
            self.finish_ask(id, "conflict", "Not merged: the add-on's main checkout isn't on a branch", "");
            return None;
        };
        let before = rev(&main, "HEAD").unwrap_or_default();
        let tip = rev(&t.repo, &format!("refs/heads/{}", t.branch)).unwrap_or_default();
        match merge_ref(&project, &main, &t.branch, &tip, &format!("{title} (#{id})")) {
            Ok(Some(m)) => Some((before, rev(&m, "HEAD").map(|sha| (m, sha)))),
            Ok(None) => Some((before, None)),
            Err((step, question)) => {
                // Its branch moved on under its copy (its work committed there): the copy's index brought to it, its
                // files as they are, so it can be settled there.
                let _ = git("", &t.path, &[], &["reset", "-q"]);
                let it = self.read(|st| st.desk.item(id).cloned());
                match (step, it) {
                    ("clash", Some(it)) => self.finish_clash(&it, &question),
                    _ => self.finish_ask(id, step, &question, ""),
                }
                None
            }
        }
    }

    /// Item `id`'s work on the main line of the add-on (`repo`) since `before`: the Cue it's built into (`cue`) queued
    /// for the next install, as Cue's own items are, the add-on's main line's tip what it has to have. Nothing the app
    /// builds from the add-on changed: to check with no install.
    fn install_addon(&self, id: &str, cue: &str, repo: &str, before: &str) {
        let cue = cue.to_string();
        let Some((main, _)) = main_checkout(repo) else { return self.finish_checked(id) };
        let head = rev(&main, "HEAD").unwrap_or_default();
        let project = self.read(|st| st.desk.item(id).map(|i| i.project.clone())).unwrap_or_default();
        let installs = crate::desk::hub::lead::checks::builds_this_cue(&cue) && crate::desk::lands::app_source_lands(&project).updates_app();
        if !installs || head.is_empty() || !in_app(&cue, &main, before, &head) {
            self.finish_set(id, |i| {
                if let Some(f) = i.finish.as_mut() {
                    (f.addon_repo, f.addon) = (main.clone(), head.clone());
                }
            });
            return self.finish_checked(id);
        }
        let cue_head = rev(&cue, "HEAD").unwrap_or_default();
        let now = now_ms();
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                (f.repo, f.commit, f.addon_repo, f.addon) = (cue, cue_head, main, head);
                (f.step, f.at, f.wait, f.wait_from, f.question) = ("queued".into(), now, String::new(), 0, String::new());
            }
        });
        self.later(crate::desk::hub::landing::finish::INSTALL_EVERY_MS, |h| h.desk_finish_tick());
    }

    /// The Cue whose add-on repository `repo` is, for item `it`: its Desk's folder's, or one its work is in ("" for none).
    pub(crate) fn addon_cue(&self, it: &DeskItem, repo: &str) -> String {
        let mut folders: Vec<String> = self.desk_folder(&it.project).into_iter().collect();
        folders.extend(self.item_repos(it));
        cue_of_addon(repo, &folders)
    }

    /// Item `id`, landed on Cue's main line or with nothing there: its work left on the add-on's branches (#582's)
    /// merged into the add-on's main line now, and Cue queued to install with it. True when that's what happens to it
    /// (or it stopped on a clash or a question); false when it has no such work.
    pub(crate) fn land_addon_left(&self, id: &str) -> bool {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return false };
        let keep: Vec<String> = self.read(|st| st.desk.kept.iter().filter(|k| k.id == id).map(|k| k.branch.clone()).collect());
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        for repo in self.item_repos(&it) {
            let cue = self.addon_cue(&it, &repo);
            if cue.is_empty() || crate::desk::lands::lands_of(&it.project, &repo).how == "pr" {
                continue;
            }
            let left = left_off(&repo, &it, &keep);
            if left.is_empty() {
                continue;
            }
            if main_checkout(&repo).is_none_or(|(_, b)| b.is_empty()) {
                self.finish_ask(id, "conflict", "Not merged: the add-on's main checkout isn't on a branch", "");
                return true;
            }
            let before = rev(&repo, "HEAD").unwrap_or_default();
            for (branch, sha) in left {
                match merge_ref(&project, &repo, &branch, &sha, &format!("{} (#{id})", it.title)) {
                    Ok(Some(m)) => {
                        if let Some(head) = rev(&m, "HEAD") {
                            self.finish_fin(id, |f| f.merges.push((m, head)));
                        }
                    }
                    Ok(None) => {}
                    Err(("clash", question)) => {
                        self.finish_clash(&it, &question);
                        return true;
                    }
                    Err((step, question)) => {
                        self.finish_ask(id, step, &question, "");
                        return true;
                    }
                }
            }
            self.install_addon(id, &cue, &repo, &before);
            return true;
        }
        false
    }

    /// A dropped item's copy on a branch the lead named: what's in it committed on that branch (it's kept there), the
    /// copy removed.
    pub(crate) fn desk_named_dropped(&self, id: &str, title: &str, t: &ItemTree) {
        if let Err(e) = commit_on_branch(t, &format!("{title} (#{id}, not finished)")) {
            eprintln!("cue: desk: #{id}'s copy stays at {}: {e}", t.path);
            return;
        }
        remove(t);
    }
}

#[cfg(test)]
impl Hub {
    /// Item `id`'s copy as a Cue from before #565 made it: whose add-on it is not noted.
    pub(crate) fn desk_copy_unnoted(&self, id: &str) {
        self.write(|st| {
            if let Some(t) = st.desk.item_mut(id).and_then(|i| i.tree.as_mut()) {
                t.addon_of.clear();
            }
        });
    }

    /// Item `id` with commit `short` of `repo` as its own, as a DONE that committed it there saved it.
    pub(crate) fn desk_add_commit_in(&self, id: &str, repo: &str, short: &str) {
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.commit_in.push((repo.to_string(), short.to_string()));
            }
        });
    }
}
