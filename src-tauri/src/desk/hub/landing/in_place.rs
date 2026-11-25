//! An item on a branch in the project's own folder ("A branch", desk/lands.rs). As the item starts, Cue switches the
//! repository's main checkout from its main line onto a branch of the item's own (`desk/<item>`), and records the
//! main line in the repository's config (`cue.mainline`, with the item's branch as `cue.mainlinebranch`). While the
//! checkout is on that branch, `main_checkout` (desk/hub/landing/main_line.rs) still names the main line, so other
//! items land on it, not on the item's branch: their landing moves the main line's ref only (a compare-and-swap),
//! and never touches the folder's files, which are the item's. One item at a time: a folder holds one branch.
//!
//! At the item's DONE it lands through the landing line as a copy's work does (desk/hub/landing/trees.rs); then the
//! folder is switched back onto the main line, its files as the main line has them, and the branch deleted. A dropped
//! item's folder is switched back too, its work (committed or not) kept on its branch.
//!
//! Only the item's own work moves: the folder is put on the branch only when it has nothing uncommitted (new files
//! too), no other item works in it meanwhile, and a landing or drop that finds another session's changes there
//! (desk/hub/theirs/theirs.rs) waits, the folder left as it is, rather than take them in or delete them. Each
//! switch holds the landing line's claim. A folder left on an item's branch with no item at work on it (Cue stopped
//! midway, a drop that had to wait) is put back by the tick (`desk_in_place_tick`), or its lead told once why not.

use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::landing::finish::Claim;
use crate::desk::hub::landing::git::{git, git_said};
use crate::desk::hub::landing::main_line::{in_progress, same_dir};
use crate::desk::hub::landing::tree_files::{rev, snapshot};
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree, KeptTree};

/// The item branch and main line `top`'s config records (`top` a main checkout), whatever it's on now.
pub(crate) fn recorded(top: &str) -> Option<(String, String)> {
    let get = |k: &str| git("", top, &[], &["config", "--get", k]).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    Some((get("cue.mainlinebranch")?, get("cue.mainline")?))
}

/// When main checkout `top` is on an item's branch (`on`, the branch it's on): the main line it came from.
pub(crate) fn main_line_of(top: &str, on: &str) -> Option<String> {
    if on.is_empty() || !on.starts_with("desk/") {
        return None;
    }
    recorded(top).filter(|(b, _)| b == on).map(|(_, m)| m)
}

/// (the item's branch, the main line) when `repo`'s main checkout is on an item's branch now.
pub(crate) fn on_item_branch(repo: &str) -> Option<(String, String)> {
    let on = git("", repo, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string())?;
    main_line_of(repo, &on).map(|m| (on, m))
}

/// The claim key a project's steps after landing hold while they run in main checkout `repo` (desk/hub/landing/finish.rs).
pub(crate) fn steps_key(repo: &str) -> String {
    format!("steps:{repo}")
}

/// Whether main checkout `repo` can't be put on an item's branch now: it's on one, or steps after landing run there.
pub(crate) fn folder_busy(hub: &Hub, repo: &str) -> bool {
    if on_item_branch(repo).is_some() {
        return true;
    }
    // Compared once out of the store's lock: same_dir looks at the disk.
    let steps: Vec<String> = hub.read(|st| st.desk.run.finish_claims.iter().filter_map(|k| k.strip_prefix("steps:").map(str::to_string)).collect());
    steps.iter().any(|d| same_dir(d, repo))
}

fn forget(repo: &str) {
    for k in ["cue.mainlinebranch", "cue.mainline", "cue.mainlinetold", "cue.mainlineowners", "cue.mainlineproject"] {
        let _ = git("", repo, &[], &["config", "--unset", k]);
    }
}

/// The sessions whose changes in the folder are item `it`'s own: its holder (and whom it answers to) and those that
/// held it before.
fn sids_of(it: &DeskItem) -> Vec<String> {
    let mut ids = it.former_holders.clone();
    if let Some(h) = it.holder.as_ref() {
        ids.push(h.sid.clone());
        ids.push(h.to().to_string());
    }
    ids.retain(|i| !i.is_empty());
    ids
}

/// The item at work in main checkout `repo` and its own sessions, kept with the record of its branch (git config
/// `cue.mainlineproject`, `cue.mainlineowners`): a let-go tried again once the item is gone from the Desk (dropped,
/// or Cue restarted) still tells its sessions' changes from others' there. Added to, never cut, while it works.
pub(crate) fn save_owners(repo: &str, it: &DeskItem) {
    let (project, mut ids) = owners(repo);
    let before = ids.len();
    for i in sids_of(it) {
        if !ids.contains(&i) {
            ids.push(i);
        }
    }
    if ids.len() != before {
        let _ = git("", repo, &[], &["config", "cue.mainlineowners", &ids.join(" ")]);
    }
    if project.is_empty() && !it.project.is_empty() {
        let _ = git("", repo, &[], &["config", "cue.mainlineproject", &it.project]);
    }
}

/// (the project, the sessions) `save_owners` kept for `repo`.
fn owners(repo: &str) -> (String, Vec<String>) {
    let get = |k: &str| git("", repo, &[], &["config", "--get", k]).map(|v| v.trim().to_string()).unwrap_or_default();
    (get("cue.mainlineproject"), get("cue.mainlineowners").split_whitespace().map(str::to_string).collect())
}

/// `repo`'s main line's tip: its HEAD, or, while an item works on its own branch there, the main line it came from.
pub(crate) fn main_head(repo: &str) -> Option<String> {
    match on_item_branch(repo) {
        Some((_, line)) => rev(repo, &format!("refs/heads/{line}")),
        None => rev(repo, "HEAD"),
    }
}

/// The repository whose own folder holds `dir` (its nearest `.git` folder above, a worktree's `.git` file passed
/// over) when its config may record an item's branch: read from the file, no git run, as the tick looks at many.
fn maybe_recorded(dir: &str) -> Option<String> {
    let top = std::path::Path::new(dir).ancestors().find(|a| a.join(".git").is_dir())?;
    let config = std::fs::read_to_string(top.join(".git/config")).ok()?;
    // Its real path (/var is /private/var): the one item copies record, and others' edits there are matched by.
    let top = std::fs::canonicalize(top).unwrap_or_else(|_| top.to_path_buf());
    config.contains("mainlinebranch").then(|| top.to_string_lossy().to_string())
}

/// The main line `branch` of `repo` moved from `old` to `new` without touching any file: only if it's still at `old`.
pub(crate) fn move_main(repo: &str, branch: &str, old: &str, new: &str) -> Result<(), String> {
    git_said("", repo, &[], &["update-ref", "-m", "cue: desk landing", &format!("refs/heads/{branch}"), new, old]).map(|_| ())
}

/// In-place tree `t`'s folder (the main checkout) put on its item's own branch: the commit its work starts
/// from, and the folder. Refused (the item then works in the folder as it is) when the folder isn't on its main line,
/// has anything not committed (a new file too: it would be taken in with the item's work), or is in the middle of a
/// merge or rebase. The main line is recorded before the switch, so a stop between leaves a record of a branch the
/// folder isn't on, which counts for nothing (and the tick clears it).
pub(crate) fn prepare_in_place(t: &ItemTree) -> Result<(String, String), String> {
    let repo = &t.repo;
    if let Some((b, _)) = on_item_branch(repo) {
        return Err(format!("the folder is on {b}, another item's branch"));
    }
    let onto = git("", repo, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).ok_or("the folder isn't on a branch")?;
    if onto.starts_with("desk/") {
        return Err(format!("the folder is on {onto}, not its main line"));
    }
    if let Some(what) = in_progress(repo) {
        return Err(format!("the folder is in the middle of {what}"));
    }
    crate::desk::hub::landing::tree_files::exclude_trees(repo);
    let changed = git("", repo, &[], &["status", "--porcelain", "--untracked-files=all"]).ok_or("git can't read the folder's changes")?;
    if !changed.trim().is_empty() {
        let files: Vec<String> = changed
            .lines()
            .take(5)
            .map(|l| {
                let f = l.get(3..).unwrap_or(l).trim();
                if l.starts_with("??") { format!("{f}, new") } else { f.to_string() }
            })
            .collect();
        return Err(format!("the folder has changes not committed ({}): they'd be taken in with this item's work", files.join("; ")));
    }
    let base = rev(repo, &onto).ok_or("no main line")?;
    // Another item's sessions, recorded earlier, aren't this one's.
    for k in ["cue.mainlineowners", "cue.mainlineproject", "cue.mainlinetold"] {
        let _ = git("", repo, &[], &["config", "--unset", k]);
    }
    let set = git_said("", repo, &[], &["config", "cue.mainline", &onto]).and_then(|_| git_said("", repo, &[], &["config", "cue.mainlinebranch", &t.branch]));
    if let Err(e) = set {
        forget(repo);
        return Err(format!("Cue couldn't record the main line: {e}"));
    }
    // Its branch still has work main doesn't (a round that never landed, or kept when it was dropped): it starts from
    // it, not over it, so that work lands with this round's.
    let switched = if crate::desk::hub::landing::unlanded::left_on(repo, &t.branch) > 0 {
        git_said("", repo, &[], &["switch", "-q", &t.branch])
            .map(|_| git("", repo, &[], &["merge-base", &t.branch, &base]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or(base.clone()))
            .map_err(|e| crate::desk::hub::landing::git::because(format!("git couldn't switch to {}", t.branch), &e))
    } else {
        git_said("", repo, &[], &["switch", "-q", "-C", &t.branch, &base]).map(|_| base.clone()).map_err(|e| crate::desk::hub::landing::git::because(format!("git couldn't make {}", t.branch), &e))
    };
    match switched {
        Ok(base) => Ok((base, repo.clone())),
        Err(e) => {
            forget(repo);
            Err(e)
        }
    }
}

/// In-place tree `t` of item `id` put in its folder (`prepare_in_place`), on a branch no other copy has checked out:
/// `desk/<id>`, or, while a copy of the repository is on that (one kept from before, #347), `desk/<id>-again`.
/// Picked up again with `kept` (its copy kept when it was dropped, #608): its branch starts from that copy's work,
/// committed or not, and the kept copy goes once the folder is on it (its work all on the branch); never is a
/// worktree moved onto the project's folder. `t.branch` is the branch it's on.
pub(crate) fn prepare_in_place_for(t: &mut ItemTree, id: &str, kept: Option<&KeptTree>) -> Result<(String, String), String> {
    let repo = t.repo.clone();
    let list = git("", &repo, &[], &["worktree", "list", "--porcelain"]).unwrap_or_default();
    let out: Vec<&str> = list.lines().filter_map(|l| l.strip_prefix("branch refs/heads/")).collect();
    let exists = |b: &str| rev(&repo, &format!("refs/heads/{b}")).is_some();
    let names = std::iter::once(format!("desk/{id}")).chain(std::iter::once(format!("desk/{id}-again"))).chain((2..50).map(|n| format!("desk/{id}-again-{n}")));
    // With a kept copy, a name not in use at all: its work goes on it, over nothing.
    let name = names.into_iter().find(|b| !out.contains(&b.as_str()) && (kept.is_none() || !exists(b))).ok_or("no free branch name")?;
    let Some(k) = kept else {
        t.branch = name;
        return prepare_in_place(t);
    };
    let at = std::path::Path::new(&k.path).join(".git").exists();
    let tip = if at { rev(&k.path, "HEAD") } else { None }.or_else(|| rev(&repo, &format!("refs/heads/{}", k.branch))).ok_or_else(|| format!("its kept branch {} is gone", k.branch))?;
    let work = if at { snapshot(&k.path) } else { None };
    let start = match work {
        Some(w) if rev(&repo, &format!("{tip}^{{tree}}")).as_deref() != Some(w.as_str()) => git("", &repo, &[], &["commit-tree", &w, "-p", &tip, "-m", "Desk item's work, kept as it was dropped"]).map(|c| c.trim().to_string()).ok_or("git couldn't keep its kept copy's work")?,
        _ => tip,
    };
    git_said("", &repo, &[], &["branch", "-q", &name, &start]).map_err(|e| crate::desk::hub::landing::git::because(format!("git couldn't make {name}"), &e))?;
    t.branch = name.clone();
    match prepare_in_place(t) {
        Ok(r) => {
            // Its work is all on the branch now: the kept copy goes, and its old branch with it when it holds nothing more.
            if at {
                crate::desk::hub::landing::tree_files::remove(&ItemTree { repo: repo.clone(), path: k.path.clone(), branch: String::new(), ..Default::default() });
            }
            if k.branch != name && matches!(crate::safe_git::run("", &repo, &[], &["merge-base", "--is-ancestor", &format!("refs/heads/{}", k.branch), &start]), Ok((_, 0))) {
                let _ = git("", &repo, &[], &["branch", "-q", "-D", &k.branch]);
            }
            Ok(r)
        }
        Err(e) => {
            // The kept copy stays as it was, holding the work; the branch made for it goes.
            let _ = git("", &repo, &[], &["branch", "-q", "-D", &name]);
            Err(e)
        }
    }
}

/// Main checkout `repo` switched from the files of tree `work` onto branch `main_line`: HEAD on it, its files and
/// index as its tip has them. Only the files that differ are written; ignored files stay.
fn switch_back(repo: &str, main_line: &str, work: &str) -> Result<(), String> {
    let to = rev(repo, &format!("refs/heads/{main_line}^{{tree}}")).ok_or_else(|| format!("Cue can't read {main_line}"))?;
    let from = git("", repo, &[], &["symbolic-ref", "-q", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_default();
    git_said("", repo, &[], &["symbolic-ref", "HEAD", &format!("refs/heads/{main_line}")])?;
    let moved = git_said("", repo, &[], &["read-tree", work]).and_then(|_| {
        let _ = git("", repo, &[], &["update-index", "-q", "--refresh"]);
        git_said("", repo, &[], &["read-tree", "-m", "-u", work, &to])
    });
    if let Err(e) = moved {
        // Nothing written (read-tree -u checks every file first): back on the item's branch, its index as it was.
        let _ = git("", repo, &[], &["symbolic-ref", "HEAD", &from]);
        let _ = git("", repo, &[], &["read-tree", "HEAD"]);
        let _ = git("", repo, &[], &["update-index", "-q", "--refresh"]);
        return Err(e);
    }
    let left = git("", repo, &[], &["status", "--porcelain", "--untracked-files=no"]).unwrap_or_default();
    if !left.trim().is_empty() {
        return Err(format!("files still differ from {main_line}: {}", left.lines().take(5).collect::<Vec<_>>().join(", ")));
    }
    Ok(())
}

/// In-place tree `t` let go of: its folder switched back onto the main line, never removed. `landed`: its work is on
/// the main line, so the branch goes; `expect`, the folder's files as landed: when they've changed since (its holder
/// still writing), nothing moves, so nothing written after is lost. Else work on it (committed or not) is kept on its
/// branch, which stays; one with none goes. Nothing happens to a folder no longer on `t.branch`, but a record left
/// pointing at it is cleared. Err: why the folder is still on the branch.
pub(crate) fn leave(t: &ItemTree, landed: bool, expect: Option<&str>) -> Result<(), String> {
    let repo = &t.repo;
    let Some((on, main_line)) = on_item_branch(repo).filter(|(b, _)| *b == t.branch) else {
        if recorded(repo).is_some_and(|(b, _)| b == t.branch) {
            forget(repo);
        }
        if landed || crate::desk::hub::landing::unlanded::left_on(repo, &t.branch) == 0 {
            let _ = git("", repo, &[], &["branch", "-q", "-D", &t.branch]);
        }
        return Ok(());
    };
    let work = snapshot(repo).ok_or_else(|| format!("Cue can't read {repo}"))?;
    if expect.is_some_and(|e| e != work) {
        return Err("its files changed after it landed (still being written?): Cue tries again".to_string());
    }
    let base = if t.base.is_empty() { None } else { rev(repo, &format!("{}^{{tree}}", t.base)) };
    let mut keep = false;
    if !landed && (base.as_deref() != Some(work.as_str()) || crate::desk::hub::landing::unlanded::left_on(repo, &on) > 0) {
        keep = true;
        // Not committed yet: committed on its branch, so it's kept there.
        let head = rev(repo, "HEAD").unwrap_or_default();
        if rev(repo, "HEAD^{tree}").as_deref() != Some(work.as_str()) {
            let made = git("", repo, &[], &["commit-tree", &work, "-p", &head, "-m", "Desk item's work, kept as it was dropped"]).map(|c| c.trim().to_string());
            match made.map(|c| move_main(repo, &on, &head, &c)) {
                Some(Ok(())) => {}
                Some(Err(e)) => return Err(format!("its work couldn't be kept on {on}: {e}")),
                None => return Err(format!("its work couldn't be kept on {on}")),
            }
        }
    }
    switch_back(repo, &main_line, &work).map_err(|e| format!("git couldn't put it back: {e}"))?;
    forget(repo);
    if !keep {
        let _ = git("", repo, &[], &["branch", "-q", "-D", &on]);
    }
    Ok(())
}

impl Hub {
    /// Of `files` (changed in in-place folder `repo`), those another session changed, not item `it`'s own: "<who>
    /// changed <files>", each who once. None when they're all its own.
    pub(crate) fn desk_folder_theirs(&self, it: &DeskItem, repo: &str, files: &[String]) -> Option<String> {
        let found = self.others_in(it, repo, files);
        let mut by: Vec<(String, Vec<String>)> = Vec::new();
        for (f, who) in found {
            match by.iter_mut().find(|(w, _)| *w == who) {
                Some((_, fs)) => fs.push(f),
                None => by.push((who, vec![f])),
            }
        }
        (!by.is_empty()).then(|| by.iter().map(|(w, fs)| format!("{w} changed {}", fs.join(", "))).collect::<Vec<_>>().join("; "))
    }

    /// Folder `repo`, on item branch `branch` with no item at work on it (`it`: the item it was, if Cue knows it):
    /// switched back onto its main line, the work on it kept on its branch, unless another session changed files there
    /// (they'd be taken onto the branch and gone from the folder): then it stays as it is and the lead is told, once.
    /// True when it's back.
    pub(crate) fn desk_in_place_let_go(&self, repo: &str, branch: &str, it: Option<&DeskItem>) -> bool {
        let Some((on, line)) = on_item_branch(repo).filter(|(b, _)| b == branch) else {
            if recorded(repo).is_some_and(|(b, _)| b == branch) {
                forget(repo);
            }
            return true;
        };
        // Its sessions kept with the record (`save_owners`): a let-go tried again after the item left the Desk (dropped,
        // or Cue restarted) still knows its own changes there from others'.
        if let Some(i) = it {
            save_owners(repo, i);
        }
        let mut it = it.cloned().unwrap_or_default();
        let (project, ids) = owners(repo);
        if it.project.is_empty() {
            it.project = project;
        }
        for i in ids {
            if !sids_of(&it).contains(&i) {
                it.former_holders.push(i);
            }
        }
        let base = it.tree.as_ref().map(|t| t.base.clone()).filter(|b| !b.is_empty() && rev(repo, b).is_some());
        let base = base.or_else(|| git("", repo, &[], &["merge-base", &on, &format!("refs/heads/{line}")]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        let Some(work) = snapshot(repo) else { return false };
        let files = base.as_deref().map(|b| crate::desk::hold_check::changed(repo, b, &work)).unwrap_or_default();
        let what = if it.id.is_empty() { format!("{on}, a Desk item's branch") } else { format!("{on}, the branch of Desk item #{} ({}), no longer at work", it.id, it.title) };
        if let Some(theirs) = self.desk_folder_theirs(&it, repo, &files) {
            self.desk_in_place_tell(repo, &it, &format!("{on}: {theirs}"), &format!(
                "Cue left {repo} on {what}: {theirs} there, which isn't that item's work. Putting the folder back on {line} would take those onto its branch and out of the folder, so it stays as it is. Once they're committed or moved, Cue puts it back by itself (that item's work kept on {on})."
            ));
            return false;
        }
        // Not beside a landing: tried again on the tick.
        let Ok(_claim) = Claim::landing(self) else { return false };
        let t = ItemTree { repo: repo.to_string(), path: repo.to_string(), branch: on.clone(), base: base.unwrap_or_default(), in_place: true, ..Default::default() };
        match leave(&t, false, None) {
            Ok(()) => true,
            Err(e) => {
                self.desk_in_place_tell(repo, &it, &format!("{on}: {e}"), &format!("Cue couldn't put {repo} back on {line} from {what}: {e}. It tries again by itself."));
                false
            }
        }
    }

    /// In-place item `id` (`t`), its work on the main line as `t.landed`: its folder put back on the main line. Files
    /// changed there since its work was landed (`t.landed_work`) are its sessions' late edits, or another session's:
    /// its own are committed on its branch ("left after landing"), which stays, and the folder goes back all the
    /// same, Ok(words for its holder and the lead naming the branch); another's hold it, the folder left as it is,
    /// the lead told once. Err: why the folder is still on the branch.
    pub(crate) fn desk_in_place_back(&self, id: &str, t: &ItemTree) -> Result<Option<String>, String> {
        let repo = &t.repo;
        if on_item_branch(repo).is_none_or(|(b, _)| b != t.branch) {
            return leave(t, true, None).map(|_| None);
        }
        let work = snapshot(repo).ok_or_else(|| format!("Cue can't read {repo}"))?;
        if t.landed_work.is_empty() || work == t.landed_work {
            return leave(t, true, Some(&t.landed_work)).map(|_| None);
        }
        let line = recorded(repo).map(|(_, m)| m).unwrap_or_else(|| "the main line".into());
        let files = crate::desk::hold_check::changed(repo, &t.landed_work, &work);
        let it = self.read(|st| st.desk.item(id).cloned()).unwrap_or_default();
        if let Some(theirs) = self.desk_folder_theirs(&it, repo, &files) {
            self.desk_in_place_tell(repo, &it, &format!("{} after landing: {theirs}", t.branch), &format!(
                "Desk item #{id} ({}) landed on {line}, but since then {theirs} in {repo}, which isn't that item's work. Putting the folder back on {line} would take those onto its branch {} and out of the folder, so it stays as it is. Once they're committed or moved, Cue puts it back by itself.",
                it.title, t.branch
            ));
            return Err(format!("{theirs} since it landed, not this item's work: it goes back once those are committed or moved"));
        }
        // Its own: kept on a branch of their own (`desk/<id>-late`, not the item's, which counts as unlanded work), not
        // on the main line, and the folder goes back as landed.
        let head = rev(repo, "HEAD").ok_or_else(|| format!("Cue can't read {}", t.branch))?;
        let made = git("", repo, &[], &["commit-tree", &work, "-p", &head, "-m", &format!("Desk item #{id}: changes left after landing")]).map(|c| c.trim().to_string());
        let made = made.ok_or("its changes since landing couldn't be kept")?;
        let late = std::iter::once(format!("{}-late", t.branch)).chain((2..50).map(|n| format!("{}-late-{n}", t.branch))).find(|b| rev(repo, &format!("refs/heads/{b}")).is_none()).ok_or("no free branch name for its changes since landing")?;
        git_said("", repo, &[], &["branch", "-q", &late, &made]).map_err(|e| format!("its changes since landing couldn't be kept on {late}: {e}"))?;
        if let Err(e) = leave(t, true, Some(&work)) {
            // Tried again from the start: they're all in the folder still.
            let _ = git("", repo, &[], &["branch", "-q", "-D", &late]);
            return Err(e);
        }
        let short = git("", repo, &[], &["rev-parse", "--short", &made]).map(|s| s.trim().to_string()).unwrap_or_default();
        let shown: Vec<&str> = files.iter().take(5).map(String::as_str).collect();
        Ok(Some(format!(
            "Desk item #{id} ({}): it's on {line}, but changes made in {repo} after it landed ({}) aren't: they're kept on branch {late}, commit {short}, and the folder is back on {line}. Land them as a new item if they're wanted.",
            it.title,
            shown.join(", ")
        )))
    }

    /// `words` to the lead of `it`'s project, once per folder while they stay the same (kept in its config).
    /// `key` says what it's about, whatever words it's put in (the item known or not, after a restart).
    fn desk_in_place_tell(&self, repo: &str, it: &DeskItem, key: &str, words: &str) {
        eprintln!("cue: desk: {words}");
        let key = key.replace('\n', " ");
        let said = git("", repo, &[], &["config", "--get", "cue.mainlinetold"]).map(|s| s.trim().to_string()).unwrap_or_default();
        if said == key {
            return;
        }
        let _ = git("", repo, &[], &["config", "cue.mainlinetold", &key]);
        let lead = self.read(|st| st.desk.lead_sid(&it.project).map(str::to_string));
        if let Some(lead) = lead {
            self.desk_done(Effects { sends: vec![crate::desk::hub::core::desk::kinded((lead, words.to_string()), crate::sessions::Kind::Desk)], ..Default::default() });
        }
    }

    /// Cue's tick (and its first after a start): folders Cue left on an item's branch with no item at work on it put
    /// back on their main line (`desk_in_place_let_go`), then the next item waiting for one may start there.
    pub(crate) fn desk_in_place_tick(&self) {
        let (dirs, items): (Vec<String>, Vec<DeskItem>) = self.read(|st| {
            let mut dirs: Vec<String> = st.sessions.iter().filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone()).collect();
            dirs.extend(st.desk.items.iter().filter_map(|i| i.tree.as_ref().map(|t| t.repo.clone())));
            dirs.extend(st.desk.items.iter().map(|i| i.tree_wait_repo.clone()));
            (dirs, st.desk.items.clone())
        });
        let mut repos: Vec<String> = dirs.iter().filter(|d| !d.is_empty()).filter_map(|d| maybe_recorded(d)).collect();
        repos.sort();
        repos.dedup();
        for repo in repos {
            let Some((branch, _)) = recorded(&repo) else { continue };
            let held = items.iter().find(|i| {
                matches!(i.state.as_str(), "working" | "finishing" | "queued") && i.tree.as_ref().is_some_and(|t| t.in_place && t.branch == branch && same_dir(&t.repo, &repo))
            });
            if let Some(i) = held {
                // Holders change while it works: each kept with the record.
                save_owners(&repo, i);
                continue;
            }
            let id = branch.strip_prefix("desk/").map(|b| b.split("-again").next().unwrap_or(b));
            let it = items.iter().find(|i| i.tree.as_ref().is_some_and(|t| t.in_place && t.branch == branch)).or_else(|| id.and_then(|id| items.iter().find(|i| i.id == id)));
            if self.desk_in_place_let_go(&repo, &branch, it) {
                let _ = self.desk_trees_free(&repo);
            }
        }
    }
}
