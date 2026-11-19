//! Each item in its own copy, landed as one squash (docs/desk-landing-design.md, step 6). As an item's holder starts
//! on it, Cue makes it a copy of its repository: a git worktree in `<main checkout>/.claude/worktrees/desk-<n>` on a
//! branch of its own (`desk/<item>`) from the main line, and tells the holder to work there. A repository has three
//! such copies at most; an item past that is queued until one is free. Cue's own repository's copies share its
//! `node_modules` and one build folder (`.claude/worktrees/desk-target`), so a build in one warms the others.
//!
//! At the item's DONE it joins the landing line, which lands one item at a time, oldest DONE first: everything in
//! its copy (committed there or not) is squashed into one commit on top of the main line, brought up to the main
//! line when it moved (a clash goes back to its holder), word-checked, and checked (the repository's check, when it
//! has one: Cue's loads its window from a clean copy). Only then is the main checkout's branch moved to it, a
//! fast-forward, never forced. The copy is removed once it landed. A dropped item's copy with work in it is kept
//! (moved to `kept-<item>`, its branch kept) and listed on its Desk; one with none is removed.
//!
//! Only an item whose holder works here in a repository's main checkout, on a branch, gets a copy: a session on
//! another machine, in a folder that isn't a repository, or already in a worktree of its own keeps working where it
//! is, and lands as before (hub/desk_finish.rs). An item whose copy has nothing in it at DONE (its holder worked in
//! the main checkout after all) lands as before too.

use super::desk::Effects;
use super::desk_commit::{git, git_said, word_in};
use super::desk_finish::{in_progress, main_checkout, same_dir, Claim};
use super::*;
use crate::desk::{DeskItem, ItemTree, KeptTree};

/// Where a repository's copies are, under its main checkout.
const TREES: &str = ".claude/worktrees";
/// The build folder Cue's copies share, under `TREES`.
const SHARED_TARGET: &str = "desk-target";

/// The questions landing can stop on.
pub(crate) const CHECK_FAILED: &str = "Its check failed";

/// What a holder is told of its item's copy.
pub(crate) fn tree_line(t: &ItemTree) -> String {
    format!(
        "Work in its own copy of the repository: {} (branch {}). Change files only there, never in the main checkout, \
         and don't merge or push: at your DONE, Cue lands everything in it on the main line as one commit.",
        t.path, t.branch
    )
}

/// How a turn in the landing line went.
enum Landed {
    /// On the main line, or stopped on a question: the line goes on.
    Done,
    /// Waits (the main checkout busy): the next item in the line goes first; the tick tries this one again.
    Wait,
}

impl Hub {
    /// Copies for `fx`'s items that just went to work: each item at work with a holder here gets its copy, its
    /// holder's message saying where it is. An item whose repository's copies are all in use is queued for one
    /// instead (its message held till then). Run as the Desk's changes are saved (hub/desk.rs `desk_done`).
    pub(super) fn desk_trees(&self, fx: &mut Effects) {
        // Dropped items' copies first: one with work is kept before its slot can go to another item.
        for it in std::mem::take(&mut fx.trees_gone) {
            self.desk_tree_dropped(it);
        }
        if fx.items.is_empty() {
            return;
        }
        // (item, the holder's folder, whom its message goes to, how many of its project's items have copies at once).
        // A project set to the shared folder, or to commit its work yourself, gets none (lands.rs).
        let wanted: Vec<(String, String, String, u32)> = self.read(|st| {
            fx.items
                .iter()
                .filter_map(|id| {
                    let it = st.desk.item(id).filter(|i| i.state == "working" && i.tree.is_none())?;
                    let h = it.holder.as_ref()?;
                    let s = st.sessions.get(h.to()).filter(|s| s.origin.machine.is_empty() && !s.origin.cwd.is_empty())?;
                    let l = crate::lands::lands_of(&it.project, &s.origin.cwd);
                    (l.at == "copy").then(|| (id.clone(), s.origin.cwd.clone(), h.to().to_string(), l.at_once))
                })
                .collect()
        });
        for (id, folder, to, at_once) in wanted {
            let Some(main) = tree_repo(&folder) else { continue };
            let claimed = self.write(|st| {
                let used: Vec<u32> = st.desk.items.iter().filter_map(|i| i.tree.as_ref()).filter(|t| t.repo == main).map(|t| t.slot).collect();
                // Its copy taken back earlier (TAKE COPY, #347) is kept on its branch: this one's is another.
                let again = if st.desk.kept.iter().any(|k| k.id == id) { "-again" } else { "" };
                let it = st.desk.item_mut(&id).filter(|i| i.state == "working" && i.tree.is_none())?;
                match (1..=at_once).find(|n| !used.contains(n)) {
                    Some(n) => {
                        let t = ItemTree { repo: main.clone(), path: format!("{main}/{TREES}/desk-{n}"), branch: format!("desk/{id}{again}"), base: String::new(), slot: n };
                        it.tree = Some(t.clone());
                        // Started another way (Start it now, or freed by its `after`) while it waited for a copy: that wait is over.
                        it.tree_wait = false;
                        it.tree_wait_repo.clear();
                        Some(Some(t))
                    }
                    // Started now by the lead (START): it works where its holder is, message and all.
                    None if it.no_copy_wait => None,
                    None => {
                        (it.state, it.tree_wait, it.tree_wait_repo) = ("queued".into(), true, main.clone());
                        Some(None)
                    }
                }
            });
            match claimed {
                None => {}
                // None free: its message waits for its copy.
                Some(None) => fx.sends.retain(|(s, text)| !(*s == to && names_item(text, &id))),
                Some(Some(mut t)) => match prepare(&t, &id) {
                    Ok((base, path)) => {
                        t.path = path;
                        self.write(|st| {
                            if let Some(i) = st.desk.item_mut(&id).and_then(|i| i.tree.as_mut()) {
                                (i.base, i.path) = (base.clone(), t.path.clone());
                            }
                        });
                        let line = tree_line(&t);
                        for (_, text) in fx.sends.iter_mut().filter(|(s, text)| *s == to && names_item(text, &id) && !text.contains(&t.path)) {
                            text.push('\n');
                            text.push_str(&line);
                        }
                    }
                    Err(e) => {
                        // Its holder works in the shared folder, as before.
                        eprintln!("cue: desk: couldn't make #{id} its own copy: {e}");
                        self.write(|st| {
                            if let Some(i) = st.desk.item_mut(&id) {
                                i.tree = None;
                            }
                        });
                    }
                },
            }
        }
    }

    /// A dropped item's copy: kept and listed when it has work in it, else removed (its slot free for the next).
    fn desk_tree_dropped(&self, it: DeskItem) {
        let Some(t) = it.tree.clone() else { return };
        if has_work(&t) {
            let to = format!("{}/{TREES}/kept-{}", t.repo, it.id);
            unlink_shared(&t.path);
            let moved = git_said("", &t.repo, &[], &["worktree", "move", &t.path, &to]);
            let path = if moved.is_ok() { to } else { t.path.clone() };
            if let Err(e) = moved {
                eprintln!("cue: desk: #{}'s copy stays at {}: {e}", it.id, t.path);
            }
            let k = KeptTree { id: it.id.clone(), title: it.title.clone(), project: it.project.clone(), repo: t.repo.clone(), path, branch: t.branch.clone(), at: now_ms() };
            crate::db::desk_put_kept(&k);
            self.write(|st| {
                st.desk.kept.retain(|x| x.id != k.id);
                st.desk.kept.push(k);
            });
        } else {
            remove(&t);
        }
        let _ = self.desk_trees_free(&t.repo);
    }

    /// Removes a dropped item's kept copy and its branch, and takes it off the list.
    pub(crate) fn desk_kept_remove(&self, id: &str) -> Result<(), String> {
        let k = self.read(|st| st.desk.kept.iter().find(|k| k.id == id).cloned()).ok_or("no such kept copy")?;
        let t = ItemTree { repo: k.repo.clone(), path: k.path.clone(), branch: k.branch.clone(), ..Default::default() };
        remove(&t);
        crate::db::desk_drop_kept(id);
        self.write(|st| st.desk.kept.retain(|x| x.id != id));
        self.changed();
        Ok(())
    }

    /// One of `repo`'s copies is free: the item queued longest for one starts in it. Whether one started.
    pub(super) fn desk_trees_free(&self, repo: &str) -> bool {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            // The oldest waiting whose own project's pool (lands.rs) has room among the repository's copies.
            let used = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == repo)).count() as u32;
            let mut waiting: Vec<&DeskItem> = st.desk.items.iter().filter(|i| i.tree_wait && i.state == "queued" && i.tree_wait_repo == repo).collect();
            waiting.sort_by_key(|i| i.updated);
            let Some(id) = waiting.into_iter().find(|i| used < crate::lands::lands_of(&i.project, repo).at_once).map(|i| i.id.clone()) else { return fx };
            let rule = st.desk.item(&id).map(|i| super::desk::rule_of(st, &i.project)).unwrap_or_default();
            let Some(it) = st.desk.item_mut(&id) else { return fx };
            (it.state, it.tree_wait, it.updated) = ("working".into(), false, super::desk::secs(now));
            it.tree_wait_repo.clear();
            if let Some(h) = &it.holder {
                fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "", &rule)));
            }
            fx.items.push(id);
            fx
        });
        let started = !fx.items.is_empty();
        self.desk_done(fx);
        started
    }

    /// Items waiting for a copy of a repository with one free start (#340): a copy let go of by any way that
    /// doesn't hand it on left them waiting for good. Run by the finishing tick (~10 s), so one starts within a minute.
    pub(super) fn desk_trees_catch_up(&self) {
        let mut repos: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.tree_wait && i.state == "queued").map(|i| i.tree_wait_repo.clone()).collect());
        repos.sort();
        repos.dedup();
        for repo in repos {
            // One per free copy; bounded, as a started item that got no copy leaves it free for the next.
            for _ in 0..self.read(|st| st.desk.items.len()) {
                if !self.desk_trees_free(&repo) {
                    break;
                }
            }
        }
    }

    /// The landing line: items said DONE in their own copies landed one at a time, oldest DONE first (those the lead
    /// moved up, LAND FIRST, before them: the latest moved first). Whoever runs
    /// it takes everything waiting; an item that has to wait lets the next go first.
    pub(super) fn desk_land_line(&self) {
        let Some(_claim) = Claim::take(self, "landing") else { return };
        // No run holds the line: an item left checking (Cue quit or crashed mid-check) is back in the line, in its place.
        let cut_short: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.tree.is_some() && i.finish.as_ref().is_some_and(|f| f.step == "checking")).map(|i| i.id.clone()).collect());
        for id in cut_short {
            self.finish_fin(&id, |f| f.step = "line".into());
        }
        let mut later: Vec<String> = Vec::new();
        loop {
            let next = self.read(|st| {
                st.desk
                    .items
                    .iter()
                    .filter(|i| i.state == "finishing" && i.tree.is_some() && !later.contains(&i.id))
                    .filter_map(|i| i.finish.as_ref().filter(|f| matches!(f.step.as_str(), "commit" | "line")).map(|f| (f.first == 0, std::cmp::Reverse(f.first), f.at, i.id.clone())))
                    .min()
                    .map(|(_, _, _, id)| id)
            });
            let Some(id) = next else { break };
            if let Landed::Wait = self.land(&id) {
                later.push(id);
            }
        }
    }

    /// Item `id`'s turn in the landing line.
    fn land(&self, id: &str) -> Landed {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return Landed::Done };
        let (Some(t), Some(fin)) = (it.tree.clone(), it.finish.clone()) else { return Landed::Done };
        let Some((_, onto)) = main_checkout(&t.repo) else { return self.land_wait(id, "Cue can't read the main checkout") };
        if onto.is_empty() {
            return self.land_wait(id, "The main checkout isn't on a branch");
        }
        let Some(work) = snapshot(&t.path) else {
            self.finish_ask(id, "error", &format!("Cue can't read its copy at {}", t.path), "");
            return Landed::Done;
        };
        let base = if t.base.is_empty() { rev(&t.repo, &onto) } else { Some(t.base.clone()) };
        let Some(base) = base else { return self.land_wait(id, "Cue can't read the main line") };
        // Its copy moved onto a main line whose history was rewritten (its branch rebased, the commit it started from
        // gone from it): measured from there, main's own changes would read as its, and clash. Where its branch
        // meets the main line now is where it starts.
        let base = moved_base_of(&t.repo, &t.path, &onto, &base).unwrap_or(base);
        if rev(&t.repo, &format!("{base}^{{tree}}")).as_deref() == Some(work.as_str()) {
            // Nothing in its copy: its holder worked where it was. That's landed as before; the copy goes.
            remove(&t);
            self.write(|st| {
                if let Some(i) = st.desk.item_mut(id) {
                    i.tree = None;
                }
            });
            let _ = self.desk_trees_free(&t.repo);
            // Committed where it was, as an item without a copy is (that path takes it at "commit").
            self.finish_fin(id, |f| (f.step, f.wait) = ("commit".into(), String::new()));
            self.desk_finish_shared(id);
            return Landed::Done;
        }
        if fin.step == "commit" {
            self.finish_step(id, "line");
        }
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        let words = crate::config::never_words(&project);
        let message = format!("{} (#{id})", it.title);
        if let Some(w) = words.iter().find(|w| message.to_lowercase().contains(&w.to_lowercase())) {
            self.finish_ask(id, "word", &format!("Not landed: \"{w}\" is in its title"), "");
            return Landed::Done;
        }
        for _ in 0..3 {
            let Some(main) = rev(&t.repo, &onto) else { return self.land_wait(id, "Cue can't read the main line") };
            let merged = if main == base {
                work.clone()
            } else {
                let Some(c) = git("", &t.repo, &[], &["commit-tree", &work, "-p", &base, "-m", "desk work"]).map(|c| c.trim().to_string()) else {
                    return self.land_wait(id, "git couldn't make its commit");
                };
                let filters = crate::safe_git::no_repo_filters("", &t.repo);
                let tried = crate::safe_git::run("", &t.repo, &filters, &["merge-tree", "--write-tree", "--name-only", "--merge-base", &base, &main, &c]);
                let Ok((out, code)) = tried else { return self.land_wait(id, "git couldn't bring it up to the main line") };
                if code != 0 {
                    let files: Vec<&str> = out.lines().skip(1).take_while(|l| !l.is_empty()).collect();
                    let q = format!(
                        "Its work clashes with {onto} in {}: in {}, merge {onto} into {}, settle the clash there, and say DONE again",
                        files.join(", "),
                        t.path,
                        t.branch
                    );
                    self.finish_clash(&it, &q);
                    return Landed::Done;
                }
                out.lines().next().unwrap_or("").trim().to_string()
            };
            let diff = git("", &t.repo, &[], &["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", &main, &merged]).unwrap_or_default();
            if let Some((w, at)) = word_in(&diff, &words) {
                self.finish_ask(id, "word", &format!("Not landed: \"{w}\" is in {at}"), "");
                return Landed::Done;
            }
            let Some(squash) = git("", &t.repo, &[], &["commit-tree", &merged, "-p", &main, "-m", &message]).map(|c| c.trim().to_string()) else {
                return self.land_wait(id, "git couldn't make its commit");
            };
            // Its check, once per state of the main line it's checked on (a wait doesn't run it again).
            let checked = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).map(|f| f.checked.clone())).unwrap_or_default();
            let mut unchecked = false;
            if checked != merged {
                // Its `at` kept: cut short, it's back in the line in its place.
                self.finish_fin(id, |f| (f.step, f.wait) = ("checking".into(), String::new()));
                // Its project set to no check: it lands "not checked" (lands.rs).
                let check = if crate::lands::check_off(&it.project, &t.repo) { None } else { self.installer().check(&t.repo, &squash) };
                match check {
                    None => unchecked = true,
                    Some(Ok(())) => {}
                    Some(Err(log)) => {
                        self.desk_land_back(&it, CHECK_FAILED, "check_failed", &log);
                        return Landed::Done;
                    }
                }
                let m = merged.clone();
                self.finish_fin(id, |f| (f.checked, f.unchecked) = (m, unchecked));
            }
            // The main line moved while it was checked: brought up to it again.
            if rev(&t.repo, &onto).as_deref() != Some(main.as_str()) {
                continue;
            }
            // Dropped or sent back while it was checked: not landed.
            if !self.read(|st| still_landing(st.desk.item(id), &t.path)) {
                return Landed::Done;
            }
            if let Some(what) = in_progress(&t.repo) {
                return self.land_wait(id, &format!("The main checkout is in the middle of {what}"));
            }
            // Others' unsaved edits in the main checkout stay as they are, the item's changes merged into them (#350).
            if let Err(why) = super::land_keep::fast_forward_keeping_edits(&t.repo, &main, &squash) {
                return self.land_wait(id, &super::desk_commit::because(format!("git wouldn't move {onto} in the main checkout"), &why));
            }
            let short = git("", &t.repo, &[], &["rev-parse", "--short", &squash]).map(|s| s.trim().to_string()).unwrap_or_default();
            self.finish_landed(id, &t, &short);
            return Landed::Done;
        }
        self.land_wait(id, "The main line kept moving")
    }

    /// Item `id` waits in the landing line on `why` (said on it; nobody asked).
    fn land_wait(&self, id: &str, why: &str) -> Landed {
        let same = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).is_some_and(|f| f.step == "line" && f.wait == why));
        if !same {
            let why = why.to_string();
            self.finish_fin(id, |f| (f.step, f.wait) = ("line".into(), why));
        }
        Landed::Wait
    }

    /// Item `it` goes back to its holder with `question` (a clash, a failed check), its copy kept for the fix.
    fn desk_land_back(&self, it: &DeskItem, question: &str, step: &str, log: &str) {
        self.finish_failed(it, question, step, log);
    }

    /// Item `id` landed as `short` on `t`'s main line: its copy removed (the next queued item starts in its
    /// place), what waited on it started, then installed or to check as any finished item.
    fn finish_landed(&self, id: &str, t: &ItemTree, short: &str) {
        let made = vec![(t.repo.clone(), short.to_string())];
        self.save_made(id, &made);
        remove(t);
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.tree = None;
                if let Some(f) = i.finish.as_mut() {
                    f.wait = String::new();
                }
            }
        });
        let _ = self.desk_trees_free(&t.repo);
        self.finish_release(id);
        self.finish_after(id, std::slice::from_ref(&t.repo));
        clear_build_leftovers(&std::path::Path::new(&t.repo).join(TREES).join(SHARED_TARGET));
    }
}

/// What an item of `project` queued for one of `repo`'s copies waits on, as the Desk lead's summary says it (#340).
pub(super) fn copy_wait(st: &Store, project: &str, repo: &str) -> String {
    let mut held: Vec<String> = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == repo)).map(|i| format!("#{}", i.id)).collect();
    held.sort();
    let at_once = crate::lands::lands_of(project, repo).at_once;
    format!(
        "waiting for a free copy of its repository ({} of {at_once} held, by {}): it starts by itself once one lands or is dropped",
        held.len(),
        if held.is_empty() { "none".to_string() } else { held.join(", ") }
    )
}

/// The main checkout `folder` is, when an item worked from it gets a copy: a repository's main checkout (not one of
/// its worktrees), on a branch with a commit.
pub(super) fn tree_repo(folder: &str) -> Option<String> {
    let top = git("", folder, &[], &["rev-parse", "--show-toplevel"])?.trim().to_string();
    let (main, onto) = main_checkout(&top)?;
    (same_dir(&top, &main) && !onto.is_empty() && rev(&main, &onto).is_some()).then_some(main)
}

/// Where copy `path`'s branch meets the main line `onto` now, when that's not the commit it was made from (`base`):
/// its branch moved onto a rewritten main line (`base` gone from its history), or main was merged into it or it was
/// rebased onto main (the meeting point is past `base`; measured from `base`, main's own changes since would clash
/// with its copies of them, #358). None while they meet at `base`.
fn moved_base_of(repo: &str, path: &str, onto: &str, base: &str) -> Option<String> {
    let head = rev(path, "HEAD")?;
    let main = rev(repo, onto)?;
    let meet = git("", repo, &[], &["merge-base", &head, &main]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())?;
    (meet != base).then_some(meet)
}

/// The full commit `what` names in `repo`.
pub(super) fn rev(repo: &str, what: &str) -> Option<String> {
    git("", repo, &[], &["rev-parse", "-q", "--verify", what]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Copy `t` made (or made again) for item `id` from the main line's tip, on its own branch: the commit it starts
/// from, and its folder. A copy left at `t.path` (Cue stopped as it removed it) is removed first only when it holds no
/// work; one with work stays, and this copy goes beside it (`desk-<n>-<id>`).
fn prepare(t: &ItemTree, id: &str) -> Result<(String, String), String> {
    let (_, onto) = main_checkout(&t.repo).ok_or("no main checkout")?;
    let base = rev(&t.repo, &onto).ok_or("no main line")?;
    exclude_trees(&t.repo);
    let _ = git("", &t.repo, &[], &["worktree", "prune"]);
    let mut path = t.path.clone();
    if std::path::Path::new(&path).exists() {
        if left_has_work(&t.repo, &path, &base) {
            eprintln!("cue: desk: {path} has work in it: left as it is");
            path = format!("{}-{id}", t.path);
            if std::path::Path::new(&path).exists() && left_has_work(&t.repo, &path, &base) {
                return Err(format!("{} and {path} both have work in them", t.path));
            }
        }
        if std::path::Path::new(&path).exists() {
            remove(&ItemTree { path: path.clone(), branch: String::new(), ..t.clone() });
        }
    }
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    git_said("", &t.repo, &[], &["worktree", "add", "-q", "-B", &t.branch, &path, &base]).map_err(|e| super::desk_commit::because("git worktree add failed".into(), &e))?;
    if super::desk_checks::builds_this_cue(&t.repo) {
        link_shared(&t.repo, &path);
    }
    Ok((base, path))
}

/// Whether a copy left at `path` (its item's start unknown) holds work: a change not committed, a commit not on the
/// main line (`main`), or a folder git can't read with anything in it.
fn left_has_work(repo: &str, path: &str, main: &str) -> bool {
    // Not a copy of its own (no `.git` there: git would read the main checkout around it instead).
    let head = std::path::Path::new(path).join(".git").exists().then(|| rev(path, "HEAD")).flatten();
    let Some(head) = head else {
        return std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false);
    };
    let changed = git("", path, &[], &["status", "--porcelain", "--", ".", ":(exclude)node_modules", ":(exclude)src-tauri/target"]);
    let Some(changed) = changed else { return true };
    let on_main = matches!(crate::safe_git::run("", repo, &[], &["merge-base", "--is-ancestor", &head, main]), Ok((_, 0)));
    !changed.trim().is_empty() || !on_main
}

/// Whether item `it` is still finishing in the landing line in its copy at `path`.
fn still_landing(it: Option<&DeskItem>, path: &str) -> bool {
    it.is_some_and(|i| i.state == "finishing" && i.tree.as_ref().is_some_and(|t| t.path == path) && i.finish.as_ref().is_some_and(|f| matches!(f.step.as_str(), "checking" | "line")))
}

/// Whether message `text` is about Desk item `id` ("Desk item #12" is not #1's).
fn names_item(text: &str, id: &str) -> bool {
    let at = format!("Desk item #{id}");
    text.match_indices(&at).any(|(i, _)| !text[i + at.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// The links a copy of Cue's repository shares: its main checkout's `node_modules`, and one build folder for all
/// its copies.
fn link_shared(repo: &str, path: &str) {
    let modules = std::path::Path::new(repo).join("node_modules");
    let target = std::path::Path::new(repo).join(TREES).join(SHARED_TARGET);
    let _ = std::fs::create_dir_all(&target);
    let links = [(modules, std::path::Path::new(path).join("node_modules")), (target, std::path::Path::new(path).join("src-tauri/target"))];
    for (from, at) in links {
        if from.exists() && std::fs::symlink_metadata(&at).is_err() {
            #[cfg(unix)]
            if let Err(e) = std::os::unix::fs::symlink(&from, &at) {
                eprintln!("cue: desk: couldn't link {}: {e}", at.display());
            }
        }
    }
}

/// What builds left in shared build folder `target` that no build reads again, cleared as an item lands (#377): the
/// object files in each profile's `deps` (`<target>/debug`, `<target>/<triple>/release`, …). A build of Cue no longer
/// leaves them (Cargo.toml, split-debuginfo), but one from before did, a set per rebuild, never removed: 87 GB in a
/// week. A program built before only loses its file and line numbers. A profile folder cargo holds locked (a build
/// under way) is left for the next landing; one cleared is held locked meanwhile, so a build waits for it.
pub(crate) fn clear_build_leftovers(target: &std::path::Path) {
    let mut profiles = Vec::new();
    for e in std::fs::read_dir(target).into_iter().flatten().flatten() {
        profiles.push(e.path());
        profiles.extend(std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|e| e.path()));
    }
    for dir in profiles.into_iter().filter(|d| d.join("deps").is_dir()) {
        let locks: Vec<std::fs::File> = [".cargo-lock", ".cargo-build-lock"].iter().filter_map(|n| std::fs::File::open(dir.join(n)).ok()).collect();
        if locks.iter().any(|l| l.try_lock().is_err()) {
            continue;
        }
        for e in std::fs::read_dir(dir.join("deps")).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().ends_with(".rcgu.o") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// The shared links taken out of copy `path` (only links: what they point at stays).
fn unlink_shared(path: &str) {
    for rel in ["node_modules", "src-tauri/target"] {
        let at = std::path::Path::new(path).join(rel);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            let _ = std::fs::remove_file(&at);
        }
    }
}

/// Copy `t` and its branch removed.
pub(super) fn remove(t: &ItemTree) {
    unlink_shared(&t.path);
    if git("", &t.repo, &[], &["worktree", "remove", "--force", "--force", &t.path]).is_none() && std::path::Path::new(&t.path).exists() {
        let _ = std::fs::remove_dir_all(&t.path);
    }
    let _ = git("", &t.repo, &[], &["worktree", "prune"]);
    if !t.branch.is_empty() {
        let _ = git("", &t.repo, &[], &["branch", "-q", "-D", &t.branch]);
    }
}

/// Whether copy `t` has work in it: a change not committed, or a commit past where it started.
fn has_work(t: &ItemTree) -> bool {
    let Some(work) = snapshot(&t.path) else { return std::path::Path::new(&t.path).exists() };
    let base = if t.base.is_empty() { None } else { rev(&t.repo, &format!("{}^{{tree}}", t.base)) };
    base.as_deref() != Some(work.as_str())
}

/// The tree of everything in copy `path` (committed or not, new files too, ignored ones and the shared links left
/// out), written with an index of its own so the copy's isn't touched.
pub(super) fn snapshot(path: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("cue-land-{}-{}", std::process::id(), now_ms()));
    let index = dir.to_string_lossy().to_string();
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let out = (|| {
        git("", path, &env, &["read-tree", "HEAD"])?;
        // Everything, then the shared links put back as the copy's HEAD has them (left out). Not an exclude in the
        // add: git takes a path there as named on purpose, and fails when the repository ignores it or a folder
        // above it.
        git("", path, &env, &["add", "-A", "--", "."])?;
        git("", path, &env, &["reset", "-q", "--", "node_modules", "src-tauri/target"])?;
        git("", path, &env, &["write-tree"]).map(|t| t.trim().to_string())
    })();
    let _ = std::fs::remove_file(&dir);
    out
}

/// `.claude/worktrees/` left out of `repo`'s changes (this checkout's own exclude list), so its copies never show
/// as new files in the main checkout or go into a commit there.
fn exclude_trees(repo: &str) {
    if git("", repo, &[], &["check-ignore", "-q", &format!("{TREES}/x")]).is_some() {
        return;
    }
    let Some(common) = git("", repo, &[], &["rev-parse", "--git-common-dir"]).map(|c| c.trim().to_string()) else { return };
    let common = if std::path::Path::new(&common).is_absolute() { std::path::PathBuf::from(common) } else { std::path::Path::new(repo).join(common) };
    let file = common.join("info/exclude");
    let _ = std::fs::create_dir_all(common.join("info"));
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("/{TREES}/\n"));
    let _ = std::fs::write(&file, text);
}

#[cfg(test)]
impl Hub {
    /// `repo`'s copies `slots` taken by items of another Desk (a test's), as if they were at work.
    pub(crate) fn desk_hold_copies(&self, repo: &str, slots: &[u32]) {
        self.write(|st| {
            for n in slots {
                let tree = ItemTree { repo: repo.to_string(), path: format!("{repo}/{TREES}/held-{n}"), branch: format!("held/{n}"), base: String::new(), slot: *n };
                st.desk.items.push(DeskItem { id: format!("held{n}"), project: "elsewhere".into(), state: "working".into(), tree: Some(tree), ..Default::default() });
            }
        });
    }

    /// Item `id` as if Cue stopped while its check ran.
    pub(crate) fn desk_cut_check_short(&self, id: &str) {
        self.desk_set_step(id, "checking");
    }

    /// Item `id` at finishing step `step` as saved (an older Cue's, say), its `at` kept.
    pub(crate) fn desk_set_step(&self, id: &str, step: &str) {
        self.finish_fin(id, |f| f.step = step.into());
    }

    /// An item of another Desk queued since long ago for one of `repo`'s copies.
    pub(crate) fn desk_queue_for(&self, repo: &str) {
        self.write(|st| {
            let it = DeskItem { id: "waits-elsewhere".into(), project: "elsewhere".into(), state: "queued".into(), tree_wait: true, tree_wait_repo: repo.to_string(), ..Default::default() };
            st.desk.items.push(it);
        });
    }

    /// The finishing tick, as on Cue's next start.
    pub(crate) fn desk_tick_now(&self) {
        self.desk_finish_tick();
    }

    /// The copy held as `slot` let go (its item done elsewhere): the next item waiting for one starts.
    /// A copy held by `desk_hold_copies` let go of without handing it to a waiting item (as when Start it now gets there first).
    pub(crate) fn desk_let_go_copy_quietly(&self, slot: u32) {
        self.write(|st| st.desk.items.retain(|i| i.id != format!("held{slot}")));
    }

    pub(crate) fn desk_let_go_copy(&self, repo: &str, slot: u32) {
        self.write(|st| st.desk.items.retain(|i| i.id != format!("held{slot}")));
        let _ = self.desk_trees_free(repo);
    }
}

#[cfg(test)]
mod tests {
    use super::names_item;

    /// A build under way holds its profile folder locked (cargo's `.cargo-lock`): nothing in it is cleared then (#377).
    #[test]
    fn a_build_folder_a_build_holds_locked_is_left_as_it_is() {
        let dir = std::env::temp_dir().join(format!("cue-test-leftovers-{}", std::process::id()));
        let deps = dir.join("debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        let left = deps.join("cue_lib-0123456789abcdef.0a1b2c.rcgu.o");
        std::fs::write(&left, "1\n").unwrap();
        let held = std::fs::File::create(dir.join("debug/.cargo-lock")).unwrap();
        held.lock().unwrap();
        super::clear_build_leftovers(&dir);
        assert!(left.exists(), "left while a build holds it");
        held.unlock().unwrap();
        super::clear_build_leftovers(&dir);
        assert!(!left.exists(), "cleared once it's free");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_items_message_is_told_by_its_whole_number() {
        assert!(names_item("Desk item #1: Header jumps", "1"));
        assert!(names_item("Desk item #1 (bug): Header jumps", "1"));
        assert!(!names_item("Desk item #12: Footer", "1"));
        assert!(!names_item("See #1 too", "1"));
        assert!(names_item("Desk item #12: x\nDesk item #1: y", "1"));
    }
}
