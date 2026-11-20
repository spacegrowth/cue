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
use super::desk_git::{git, git_said, word_in};
use super::desk_finish::Claim;
use super::desk_main_line::{in_progress, main_checkout};
use super::desk_tree_files::*;
use super::*;
use crate::desk::{DeskItem, ItemTree, KeptTree};

/// Where a repository's copies are, under its main checkout.
pub(super) const TREES: &str = ".claude/worktrees";

/// The questions landing can stop on.
pub(crate) const CHECK_FAILED: &str = "Its check failed";

/// What a holder is told of its item's copy.
pub(crate) fn tree_line(t: &ItemTree) -> String {
    format!(
        "Work in its own copy of the repository: {} (branch {}). Change files only there, never in the main checkout \
         or another folder, make no worktree or branch of your own, and don't merge or push: at your DONE, Cue lands \
         everything in it on the main line as one commit. Cue stops an edit or git command outside it.",
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
            // Going now (a copy, or none needed): your Send back kept while it waited goes with its message (#396).
            if !matches!(claimed, Some(None)) {
                let back = self.write(|st| st.desk.item_mut(&id).map(|i| std::mem::take(&mut i.back_waiting)).unwrap_or_default());
                if !back.is_empty() {
                    match fx.sends.iter_mut().find(|(s, text)| *s == to && names_item(text, &id)) {
                        Some((_, text)) => {
                            text.push_str("\n\n");
                            text.push_str(&back);
                        }
                        None => fx.sends.push((to.clone(), back)),
                    }
                }
            }
            match claimed {
                None => {}
                // None free: its message waits for its copy. The item's own words go then (`desk_trees_free`), and your
                // Send back's message with them: kept on it, as only it has your words (#396).
                Some(None) => {
                    fx.sends.retain(|(s, text)| !(*s == to && names_item(text, &id)));
                    let backs: Vec<String> = fx.backs.iter().filter(|(b, _)| *b == id).map(|(_, m)| m.clone()).collect();
                    if !backs.is_empty() {
                        self.write(|st| {
                            if let Some(i) = st.desk.item_mut(&id) {
                                i.back_waiting = std::iter::once(std::mem::take(&mut i.back_waiting)).chain(backs).filter(|m| !m.is_empty()).collect::<Vec<_>>().join("\n\n");
                            }
                        });
                    }
                }
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

    /// One of `repo`'s copies is free: the top one you dragged up (#387), else the item queued longest for one, starts in it. Whether one started.
    pub(super) fn desk_trees_free(&self, repo: &str) -> bool {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            // The oldest waiting whose own project's pool (lands.rs) has room among the repository's copies.
            let used = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == repo)).count() as u32;
            let mut waiting: Vec<&DeskItem> = st.desk.items.iter().filter(|i| i.tree_wait && i.state == "queued" && i.tree_wait_repo == repo).collect();
            // Your order first (#387), then the one queued longest.
            waiting.sort_by_key(|i| (i.queue_order.unwrap_or(u32::MAX), i.updated));
            let Some(id) = waiting.into_iter().find(|i| used < crate::lands::lands_of(&i.project, repo).at_once).map(|i| i.id.clone()) else { return fx };
            let rule = st.desk.item(&id).map(|i| super::desk::rule_of(st, &i.project)).unwrap_or_default();
            let Some(it) = st.desk.item_mut(&id) else { return fx };
            (it.state, it.tree_wait, it.updated, it.queue_order) = ("working".into(), false, super::desk::secs(now), None);
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
                // Its project's check (lands.rs), on the same tree: main plus the item (`squash`). Your own steps run
                // in a clean copy of it; set to none, it lands "not checked"; else the repository's own (Cue's).
                let check = match crate::lands::check_of(&it.project, &t.repo) {
                    crate::lands::Check::Steps(steps) => Some(check_steps_on(&t.repo, &squash, &steps)),
                    crate::lands::Check::Off => None,
                    crate::lands::Check::Repo => self.installer().check(&t.repo, &squash),
                };
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
            // Its project lands by pull request: checked, its branch pushed and a pull request opened; it lands when that's
            // merged (hub/desk_pr.rs). Main isn't touched here.
            if crate::lands::lands_of(&it.project, &t.repo).how == "pr" {
                self.desk_open_pr(&it, &t, &onto, &squash, &message);
                return Landed::Done;
            }
            if let Some(what) = in_progress(&t.repo) {
                return self.land_wait(id, &format!("The main checkout is in the middle of {what}"));
            }
            // Others' unsaved edits in the main checkout stay as they are, the item's changes merged into them (#350).
            if let Err(why) = super::land_keep::fast_forward_keeping_edits(&t.repo, &main, &squash) {
                return self.land_wait(id, &super::desk_git::because(format!("git wouldn't move {onto} in the main checkout"), &why));
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
    pub(super) fn finish_landed(&self, id: &str, t: &ItemTree, short: &str) {
        let made = vec![(t.repo.clone(), short.to_string())];
        self.save_made(id, &made);
        // Its helpers' copies whose work is on the main line now go too; any other is moved out of its copy (#376).
        self.desk_clear_helpers(id, &t.repo, Some(&t.path));
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
