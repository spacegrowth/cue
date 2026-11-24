//! Work its holder did outside the copy Cue gave it, landed as any other (#481). An item reached "to check" with its
//! work never on main: #455's holder, started without a copy, made a worktree of its own on desk/455, committed
//! there and said DONE; Cue looked only at the main checkout, found nothing new, and sent it to you. A branch named
//! for the item is merged since #448, but a session can name its branch anything, work in a folder it made, or
//! commit and remove the folder.
//!
//! So its holder's own log says where it worked while it held the item: each folder Claude Code noted it in, and each
//! commit its shell made (git's "[fix-grip b4d4e28] …" line). At its DONE each worktree it worked in, each branch
//! holding a commit it made, and its own desk/<item> branch with no worktree are merged into the main line as its
//! crew's are. And it never goes to check while one of its commits still isn't on main: it goes back to its session,
//! saying which and where. A commit nothing holds any more (amended, rebased, its branch deleted) was let go by it.

use crate::desk::hub::landing::commit::own_log;
use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::main_line::{main_checkout, same_dir};
use crate::desk::hub::landing::unlanded::{own_branches, rev_left};
use crate::desk::hub::prelude::*;
use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::landing::trees::Landed;
use crate::desk::{DeskItem, ItemTree};

/// Its holder's work outside its own copy, not on main.
#[derive(Default)]
pub(crate) struct OwnWork {
    /// Worktrees (none a main checkout) holding it: merged as its crew's folders are.
    pub(crate) folders: Vec<String>,
    /// Branches holding it that no worktree has: (main checkout, branch, its tip).
    pub(crate) refs: Vec<(String, String, String)>,
    /// Each commit of it, said for its holder ("b4d4e28 on fix-grip in /…/cue"), whatever holds it.
    pub(crate) off: Vec<String>,
}

/// One worktree of a repository: its folder, HEAD and branch ("" when detached).
struct Tree {
    path: String,
    head: String,
    branch: String,
}

fn trees(top: &str) -> Vec<Tree> {
    let Some(list) = git("", top, &[], &["worktree", "list", "--porcelain"]) else { return Vec::new() };
    let mut out: Vec<Tree> = Vec::new();
    for line in list.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push(Tree { path: p.to_string(), head: String::new(), branch: String::new() });
        } else if let (Some(h), Some(t)) = (line.strip_prefix("HEAD "), out.last_mut()) {
            t.head = h.to_string();
        } else if let (Some(b), Some(t)) = (line.strip_prefix("branch refs/heads/"), out.last_mut()) {
            t.branch = b.to_string();
        }
    }
    out
}

fn contains(top: &str, of: &str, sha: &str) -> bool {
    !of.is_empty() && matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", sha, of]), Ok((_, 0)))
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

/// What `holder_work` checks each place against: the item's own copy, other items' copies (#551), and repositories
/// landing by pull request.
struct Skip {
    copy: Option<(String, String)>,
    others: Vec<(String, String)>,
    project: String,
}

impl Skip {
    /// In its own copy, or in another item's: never merged as its work.
    fn in_copy(&self, path: &str, branch: &str) -> bool {
        let is = |(p, b): &(String, String)| (!path.is_empty() && same_dir(p, path)) || (!b.is_empty() && b == branch);
        self.copy.as_ref().is_some_and(is) || self.others.iter().any(is)
    }
    fn by_pr(&self, main: &str) -> bool {
        crate::desk::lands::lands_of(&self.project, main).how == "pr"
    }
}

/// Each other item's copy (folder, branch): its holder may have looked in one, as #406's did in #510's, but that work
/// is the other item's and lands with it (#551).
pub(crate) fn other_copies(st: &Store, it: &DeskItem) -> Vec<(String, String)> {
    // A done item's copy may be the one this item works in now; its own copy is never another's.
    let own = it.tree.as_ref().map(|t| t.path.as_str()).unwrap_or("");
    st.desk.items.iter().filter(|i| i.id != it.id && i.state != "done").filter_map(|i| i.tree.as_ref()).filter(|t| !t.path.is_empty() && t.path != own).map(|t| (t.path.clone(), t.branch.clone())).collect()
}

/// `f` is a worktree on another item's branch ("desk/<n>", "desk/<n>-again"…, `n` not `id`): that work is that item's (#578).
pub(crate) fn on_others_branch(f: &str, id: &str) -> bool {
    if !std::path::Path::new(f).is_dir() {
        return false;
    }
    let branch = git("", f, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_default();
    let Some(rest) = branch.strip_prefix("desk/") else { return false };
    let n: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    !n.is_empty() && n != id
}

/// `f` is in one of `copies` (its folder or one inside it).
pub(crate) fn in_copies(f: &str, copies: &[(String, String)]) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| p.trim_end_matches('/').to_string());
    let f = canon(f);
    copies.iter().any(|(p, _)| {
        let p = canon(p);
        f == p || f.starts_with(&format!("{p}/"))
    })
}

fn add_repo(m: &str, repos: &mut Vec<String>) {
    if !repos.iter().any(|r| same_dir(r, m)) {
        repos.push(m.to_string());
    }
}

impl Hub {
    /// Item `it`'s holder's own work outside its copy that main doesn't have yet (none for a crew's or a remote
    /// session's: those land as before; a repository landing by pull request left out, its branches being GitHub's).
    pub(crate) fn holder_work(&self, it: &DeskItem) -> OwnWork {
        let mut out = OwnWork::default();
        let Some(own) = self.read(|st| own_log(st, it)) else { return out };
        let worked = crate::logs::steps::worked_between(&own.log, own.from, own.until);
        let others = self.read(|st| other_copies(st, it));
        let skip = Skip { copy: it.tree.as_ref().map(|t| (t.path.clone(), t.branch.clone())), others, project: it.project.clone() };
        let mut repos: Vec<String> = Vec::new();
        for f in worked.folders.iter().chain(std::iter::once(&own.folder)) {
            worked_in(f, &skip, &mut repos, &mut out);
        }
        for (cwd, said_branch, sha) in &worked.commits {
            let at = if std::path::Path::new(cwd).is_dir() { cwd.as_str() } else { own.folder.as_str() };
            committed(at, said_branch, sha, &skip, &mut repos, &mut out);
        }
        let keep: Vec<String> = self.read(|st| st.desk.kept.iter().filter(|k| k.id == it.id).map(|k| k.branch.clone()).collect());
        for m in repos.iter().filter(|m| !skip.by_pr(m)) {
            own_branch_left(m, &it.id, &keep, &skip, &mut out);
        }
        out
    }
}

/// A folder it worked in: a worktree (not a main checkout) with work main lacks is merged.
fn worked_in(f: &str, skip: &Skip, repos: &mut Vec<String>, out: &mut OwnWork) {
    if !std::path::Path::new(f).is_dir() {
        return;
    }
    let Some((m, _)) = main_checkout(f) else { return };
    add_repo(&m, repos);
    let Some(top) = git("", f, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()) else { return };
    let branch = git("", &top, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_default();
    if same_dir(&top, &m) || skip.in_copy(&top, &branch) || skip.by_pr(&m) || out.folders.iter().any(|o| same_dir(o, &top)) {
        return;
    }
    if git("", &top, &[], &["rev-parse", "HEAD"]).is_some_and(|h| rev_left(&m, h.trim()) > 0) {
        out.folders.push(top);
    }
}

/// A commit it made (in `at`), wherever it is now: in a worktree, merged as one; on a branch alone, merged as that.
fn committed(at: &str, said_branch: &str, sha: &str, skip: &Skip, repos: &mut Vec<String>, out: &mut OwnWork) {
    let Some((m, _)) = main_checkout(at) else { return };
    let Some(full) = git("", &m, &[], &["rev-parse", "-q", "--verify", &format!("{sha}^{{commit}}")]).map(|s| s.trim().to_string()) else { return };
    add_repo(&m, repos);
    if skip.by_pr(&m) || rev_left(&m, &full) == 0 {
        return;
    }
    if let Some(t) = trees(&m).into_iter().find(|t| contains(&m, &t.head, &full)) {
        if skip.in_copy(&t.path, &t.branch) {
            return;
        }
        let on = if t.branch.is_empty() { String::new() } else { format!(" on {}", t.branch) };
        out.off.push(format!("{}{on} in {}", short(&full), t.path));
        if !same_dir(&t.path, &m) && !out.folders.iter().any(|o| same_dir(o, &t.path)) {
            out.folders.push(t.path);
        }
        return;
    }
    // On no worktree: the branch holding it (the one git named at the commit first). On none, it was let go.
    let on = git("", &m, &[], &["branch", "--contains", &full, "--format=%(refname:short)"]).unwrap_or_default();
    let mut branches: Vec<&str> = on.lines().map(str::trim).filter(|b| !b.is_empty()).collect();
    branches.sort_by_key(|b| *b != said_branch);
    let Some(b) = branches.first().map(|b| b.to_string()) else { return };
    if skip.in_copy("", &b) {
        return;
    }
    out.off.push(format!("{} on {b}", short(&full)));
    if let Some(tip) = git("", &m, &[], &["rev-parse", &format!("refs/heads/{b}")]).map(|t| t.trim().to_string()) {
        if !out.refs.iter().any(|(_, r, _)| *r == b) {
            out.refs.push((m.clone(), b, tip));
        }
    }
}

/// Its own branch (desk/<item>) with work main lacks and no worktree on it (#448's round 3); a dropped copy's work
/// you kept left out.
fn own_branch_left(m: &str, id: &str, keep: &[String], skip: &Skip, out: &mut OwnWork) {
    let all = trees(m);
    for b in own_branches(id) {
        if keep.contains(&b) || skip.in_copy("", &b) || all.iter().any(|t| t.branch == b) || out.refs.iter().any(|(_, r, _)| *r == b) {
            continue;
        }
        if crate::desk::hub::landing::unlanded::left_on(m, &b) == 0 {
            continue;
        }
        if let Some(tip) = git("", m, &[], &["rev-parse", &format!("refs/heads/{b}")]).map(|t| t.trim().to_string()) {
            out.refs.push((m.to_string(), b, tip));
        }
    }
}

/// `abs` as a path inside folder `top` (relative), as given or with either one's links read; None when outside it.
fn rel_in(abs: &str, top: &str) -> Option<String> {
    let real = |p: &str| -> String {
        let path = std::path::Path::new(p);
        match (path.parent().and_then(|d| std::fs::canonicalize(d).ok()), path.file_name()) {
            (Some(d), Some(n)) => d.join(n).to_string_lossy().to_string(),
            _ => p.to_string(),
        }
    };
    let tops = [top.trim_end_matches('/').to_string(), std::fs::canonicalize(top).map(|t| t.to_string_lossy().to_string()).unwrap_or_default()];
    for a in [abs.to_string(), real(abs)] {
        for t in tops.iter().filter(|t| !t.is_empty()) {
            if let Some(rel) = a.strip_prefix(&format!("{t}/")) {
                return Some(rel.to_string());
            }
        }
    }
    None
}

impl Hub {
    /// The files item `it`'s holder changed in its repository's main folder while it held it, not in its copy `t`
    /// (#653), that its copy's landing would leave behind: still changed there (git says so; ignored ones, build
    /// output, aren't), not as its copy has them, and what its own Edit or Write put there still in them (#770: a
    /// file only another session changed, a shell command's guessed path, or an edit Cue stopped is never its).
    /// Relative to the main folder. #598's holder worked in the main folder before its copy was free and moved only
    /// part of it over; the rest stayed there, uncommitted. None for a branch in the folder itself (everything there
    /// is in its snapshot), a branch the lead named, or another machine.
    pub(crate) fn main_leftovers(&self, it: &DeskItem, t: &crate::desk::ItemTree) -> Vec<String> {
        if t.in_place || t.keep || it.remote.is_some() {
            return Vec::new();
        }
        let Some(own) = self.read(|st| own_log(st, it)) else { return Vec::new() };
        let others = self.read(|st| other_copies(st, it));
        let own_copy = [(t.path.clone(), t.branch.clone())];
        let writes = crate::logs::steps::writes_between(&own.log, own.from, own.until);
        let mut out: Vec<String> = Vec::new();
        for f in writes.iter().map(|w| w.file.clone()) {
            let abs = if f.starts_with('/') { f.clone() } else { format!("{}/{}", own.folder.trim_end_matches('/'), f.trim_start_matches("./")) };
            if in_copies(&abs, &own_copy) || in_copies(&abs, &others) {
                continue;
            }
            let Some(rel) = rel_in(&abs, &t.repo) else { continue };
            if rel.is_empty() || out.contains(&rel) || rel == ".git" || rel.starts_with(".git/") {
                continue;
            }
            let changed = git("", &t.repo, &[], &["status", "--porcelain", "--untracked-files=all", "--", &rel]).unwrap_or_default();
            if changed.trim().is_empty() {
                continue;
            }
            let in_main = std::fs::read(std::path::Path::new(&t.repo).join(&rel)).ok();
            let in_copy = std::fs::read(std::path::Path::new(&t.path).join(&rel)).ok();
            if in_main == in_copy {
                continue;
            }
            // Its own write still there (#770): uncommitted when it lands isn't enough, another session may have
            // changed it, and an edit Cue stopped or one it undid isn't its.
            let text = in_main.map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
            if writes.iter().any(|w| w.file == f && w.still_in(&text)) {
                out.push(rel);
            }
        }
        out
    }
}

impl Hub {
    /// Item `id`'s holder changed files in the main folder that its copy `t` doesn't have (#653): any not named to it
    /// yet, it's sent back naming those (its turn ends); all named before, it lands without them and the lead is told
    /// which (they stay in the folder, uncommitted). Each file is named to it once and to the lead once (#770: a
    /// landing tried again after a wait or a failed check said them again). None: nothing left there, it lands.
    pub(super) fn land_main_left(&self, id: &str, it: &DeskItem, t: &ItemTree) -> Option<Landed> {
        let left = self.main_leftovers(it, t);
        if left.is_empty() {
            return None;
        }
        let unnamed: Vec<String> = left.iter().filter(|p| !it.main_left.contains(p)).cloned().collect();
        if !unnamed.is_empty() {
            self.write(|st| {
                if let Some(i) = st.desk.item_mut(id) {
                    i.main_left.extend(unnamed.iter().cloned());
                }
            });
            let words = crate::desk::hub::landing::own_work::main_left_words(&unnamed, t);
            self.finish_back(it, &words, "main_left", "", true);
            return Some(Landed::Done);
        }
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let Some(i) = st.desk.item_mut(id) else { return fx };
            let title = i.title.clone();
            if let Some(f) = i.finish.as_mut() {
                for p in left.iter().map(|p| format!("{}/{p}", t.repo.trim_end_matches('/'))) {
                    if !f.without.contains(&p) {
                        f.without.push(p);
                    }
                }
            }
            let untold: Vec<String> = left.iter().filter(|p| !i.main_left_told.contains(p)).cloned().collect();
            if untold.is_empty() {
                return fx;
            }
            i.main_left_told.extend(untold.iter().cloned());
            let project = i.project.clone();
            if let Some(lead) = st.desk.lead_sid(&project) {
                let words = format!(
                    "#{id} ({title}) lands from its copy without these files its session changed in the main folder ({}), named to it once already: {}. They stay there, uncommitted; tell the user whose they are.",
                    t.repo,
                    untold.join(", ")
                );
                fx.sends.push(crate::desk::hub::core::desk::kinded((lead.to_string(), words), crate::sessions::Kind::Desk));
            }
            fx
        });
        self.desk_done(fx);
        None
    }
}

/// What item `id`'s holder is told when files it changed are in the main folder, not in its copy (#653).
pub(crate) fn main_left_words(left: &[String], t: &crate::desk::ItemTree) -> String {
    format!(
        "Not landed: these files you changed for it are in the main folder ({}), not in its copy, and landing the copy would leave them behind: {}. Move this item's changes in them into its copy at {} (leave any that are another session's), then say DONE. Files still there at that DONE land without them, named to the lead.",
        t.repo,
        left.join(", "),
        t.path
    )
}

/// What its session is told when its work isn't all on main after its DONE: which commits, and where.
pub(crate) fn off_main_words(off: &[String], unlanded: usize, id: &str) -> String {
    let mut parts: Vec<String> = off.to_vec();
    if unlanded > 0 && !parts.iter().any(|p| p.contains(&format!("desk/{id}"))) {
        parts.push(format!("{} on desk/{id}", crate::desk::hub::landing::unlanded::unlanded_words(unlanded)));
    }
    format!("Its work isn't on main yet, so it isn't ready to check: {}. Cue couldn't land it. Put it on main (or say why it shouldn't be there), then say DONE", parts.join("; "))
}
