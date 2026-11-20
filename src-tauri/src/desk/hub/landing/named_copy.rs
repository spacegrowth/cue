//! An item at work in another repository, on a branch the lead names (#557). Work for Cue's add-on lives in the
//! add-on's own repository, on a branch of its own (Loops on `loops-d`, say), never on Cue's main line. So the lead
//! says `COPY: #<id> | in: add-on | branch: loops-d | why: …` (or a repository's folder in place of "add-on"), and as
//! the item starts its holder gets a copy of that repository on that branch: a worktree in
//! `<its main checkout>/.claude/worktrees/desk-<n>`, its edits there let through as any copy's are
//! (desk/hub/landing/copy_guard.rs).
//!
//! At its DONE nothing goes to a main line: what's in its copy is committed on its branch, as one commit named after
//! the item, the copy goes and the branch stays (it's where the work is; Cue never removes it). The item goes to
//! check with that commit as its own, so its Files changed show it as any other item's. Neither its project's check
//! nor its words are run on it: they're for the project's own main line, and it never reaches it.

use crate::desk::hub::core::desk::{secs, Effects};
use crate::desk::hub::landing::git::{git, git_said};
use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::landing::tree_files::{exclude_trees, remove, rev, snapshot};
use crate::desk::hub::prelude::*;
use crate::desk::hub::turn::route::{decide, By};
use crate::desk::{CopyOn, ItemTree};

/// The main checkout `said` names: "add-on" is the repository Cue's add-on link (`src-tauri/ext`, in the folder of
/// `from` or one above it) points into; anything else is a repository's main checkout as its folder.
pub(crate) fn repo_named(said: &str, from: &str) -> Result<String, String> {
    let top_of = |dir: &std::path::Path| dir.ancestors().find(|a| a.join(".git").exists()).map(|a| a.to_path_buf());
    let dir = if said.eq_ignore_ascii_case("add-on") || said.eq_ignore_ascii_case("addon") {
        let link = std::path::Path::new(from).ancestors().map(|a| a.join("src-tauri/ext")).find(|l| l.exists()).ok_or("Cue has no add-on here (no src-tauri/ext in the Desk's folder)")?;
        let real = std::fs::canonicalize(&link).map_err(|e| format!("Cue can't read the add-on's link: {e}"))?;
        top_of(&real).ok_or("the add-on isn't in a git repository")?
    } else {
        let p = std::path::Path::new(said);
        if !p.is_absolute() {
            return Err(format!("\"{said}\" isn't a folder: say in: add-on, or the repository's full path"));
        }
        top_of(p).ok_or_else(|| format!("{said} isn't in a git repository"))?
    };
    // A worktree's `.git` is a file: its main checkout is the one copies are made from.
    if !dir.join(".git").is_dir() {
        return Err(format!("{} is a worktree, not its repository's main checkout", dir.display()));
    }
    Ok(dir.to_string_lossy().to_string())
}

/// The repositories a reply's COPY lines name, each as said and what it is (`repo_named`), looked up from the folder
/// `from` the lead works in. Read before the store is taken: it looks at the disk.
pub(crate) fn repos_said(text: &str, from: &str) -> Vec<(String, Result<String, String>)> {
    let mut out: Vec<(String, Result<String, String>)> = Vec::new();
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
pub(crate) fn copy_said(st: &mut Store, project: &str, by: &By, id: &str, repo: Result<String, String>, branch: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let it = st.desk.item(id).filter(|i| i.project == project).ok_or_else(|| format!("#{id} got no copy: this Desk has no item #{id}"))?;
    if matches!(it.state.as_str(), "done" | "finishing" | "to_check") {
        return Err(format!("#{id} got no copy: it's {} already", if it.state == "done" { "done" } else { "finishing" }));
    }
    let repo = repo.map_err(|e| format!("#{id} got no copy: {e}"))?;
    if let Some(t) = &it.tree {
        if t.keep && t.repo == repo && t.branch == branch {
            return Ok(());
        }
        return Err(format!("#{id} got no copy: it has one already ({}, branch {}). Take it back first (TAKE COPY: #{id} | why: …), then say COPY again", t.path, t.branch));
    }
    let working = it.state == "working";
    let holder = it.holder.as_ref().map(|h| h.to().to_string());
    let it = st.desk.item_mut(id).ok_or("gone")?;
    it.copy_on = Some(CopyOn { repo: repo.clone(), branch: branch.to_string() });
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

/// What a holder is told of a copy on a branch the lead named.
pub(crate) fn named_line(t: &ItemTree) -> String {
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
        let title = self.read(|st| st.desk.item(id).map(|i| i.title.clone())).unwrap_or_default();
        let made = match commit_on_branch(t, &format!("{title} (#{id})")) {
            Ok(m) => m,
            Err(e) => {
                self.finish_ask(id, "error", &e, "");
                return;
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
        if made.is_empty() {
            self.finish_fin(id, |f| (f.step, f.wait) = ("commit".into(), String::new()));
            self.desk_finish_shared(id);
            return;
        }
        self.save_made(id, &made);
        self.finish_release(id);
        // No main line to wait for, nothing to install: to check.
        self.finish_after(id, &[]);
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
