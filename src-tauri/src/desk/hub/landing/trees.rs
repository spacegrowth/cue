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
//! is, and lands as before (desk/hub/landing/finish.rs). An item whose copy has nothing in it at DONE (its holder worked in
//! the main checkout after all) lands as before too.

use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::landing::git::{git, git_said, word_in};
use crate::desk::hub::landing::finish::Claim;
use crate::desk::hub::landing::main_line::{in_progress, main_checkout};
use crate::desk::hub::landing::tree_files::*;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree, KeptTree};

/// Where a repository's copies are, under its main checkout.
pub(crate) const TREES: &str = ".claude/worktrees";

/// The questions landing can stop on.
pub(crate) const CHECK_FAILED: &str = "Its check failed";

/// What a holder is told of its item's copy.
pub(crate) fn tree_line(t: &ItemTree) -> String {
    if t.keep {
        return crate::desk::hub::landing::named_copy::named_line(t);
    }
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
    /// instead (its message held till then). Run as the Desk's changes are saved (desk/hub/core/desk.rs `desk_done`).
    pub(crate) fn desk_trees(&self, fx: &mut Effects) {
        // Dropped items' copies first: one with work is kept before its slot can go to another item.
        for it in std::mem::take(&mut fx.trees_gone) {
            self.desk_tree_dropped(it);
        }
        if fx.items.is_empty() {
            return;
        }
        let wanted = self.desk_trees_wanted(fx);
        for (id, folder, to, at_once) in wanted {
            let Some(main) = tree_repo(&folder) else { continue };
            let claimed = self.write(|st| claim_tree(st, &id, &main, at_once));
            self.desk_tree_back(fx, &id, &to, &claimed);
            self.desk_tree_placed(fx, &id, &to, claimed);
        }
    }

    /// (item, the holder's folder, whom its message goes to, how many of its project's items have copies at once).
    /// A project set to the shared folder, or to commit its work yourself, gets none (desk/lands.rs). One the lead
    /// gave a copy of another repository (COPY, #557) gets that, wherever its holder works.
    fn desk_trees_wanted(&self, fx: &Effects) -> Vec<(String, String, String, u32)> {
        // (item, its project, the folder that decides, whom its message goes to, whether the lead gave it a copy). Read
        // under the store's lock; how each project lands is read after, with it let go (the folder may need a look).
        let asked: Vec<(String, String, String, String, bool)> = self.read(|st| {
            fx.items
                .iter()
                .filter_map(|id| {
                    let it = st.desk.item(id).filter(|i| i.state == "working" && i.tree.is_none())?;
                    let h = it.holder.as_ref()?;
                    let s = st.sessions.get(h.to()).filter(|s| s.origin.machine.is_empty() && !s.origin.cwd.is_empty())?;
                    // No code at all (its "done when" says so: a plan, a doc, mocks): no copy, nor a wait for one; it
                    // works where its holder is (#543). A copy the lead gave still goes.
                    if it.copy_on.is_none() && crate::desk::hub::landing::unlanded::rules_out_code(&it.done_when) {
                        return None;
                    }
                    Some(match &it.copy_on {
                        Some(c) => (id.clone(), it.project.clone(), c.repo.clone(), h.to().to_string(), true),
                        None => (id.clone(), it.project.clone(), s.origin.cwd.clone(), h.to().to_string(), false),
                    })
                })
                .collect()
        });
        asked
            .into_iter()
            .filter_map(|(id, project, folder, to, given)| {
                let l = crate::desk::lands::lands_of(&project, &folder);
                if given {
                    return Some((id, folder, to, l.at_once.max(1)));
                }
                (l.at == "copy").then_some((id, folder, to, l.at_once))
            })
            .collect()
    }

    /// Going now (a copy, or none needed): your Send back kept while it waited goes with its message (#396).
    fn desk_tree_back(&self, fx: &mut Effects, id: &str, to: &str, claimed: &Option<Option<ItemTree>>) {
        if !matches!(claimed, Some(None)) {
            let back = self.write(|st| st.desk.item_mut(id).map(|i| std::mem::take(&mut i.back_waiting)).unwrap_or_default());
            if !back.is_empty() {
                match fx.sends.iter_mut().find(|(s, text)| *s == to && names_item(text, id)) {
                    Some((_, text)) => {
                        text.push_str("\n\n");
                        text.push_str(&back);
                    }
                    None => fx.sends.push((to.to_string(), back)),
                }
            }
        }
    }

    /// Item `id` as its claim went: its message waits for a copy, or says where its copy is.
    fn desk_tree_placed(&self, fx: &mut Effects, id: &str, to: &str, claimed: Option<Option<ItemTree>>) {
        match claimed {
            None => {}
            // None free: its message waits for its copy. The item's own words go then (`desk_trees_free`), and your
            // Send back's message with them: kept on it, as only it has your words (#396).
            Some(None) => {
                // Your words (a Send back, a question) and the lead's notes go now all the same (#567): its holder reads
                // them while it waits, told not to change files yet, and the lead hears your words at once.
                let now_ones = self.write(|st| words_now(st, fx, id, to));
                fx.sends.retain(|(s, text)| !(*s == to && names_item(text, id)) || now_ones.contains(text));
                let backs: Vec<String> = fx.backs.iter().filter(|(b, _)| *b == id).map(|(_, m)| m.clone()).collect();
                if !backs.is_empty() {
                    self.write(|st| {
                        if let Some(i) = st.desk.item_mut(id) {
                            i.back_waiting = std::iter::once(std::mem::take(&mut i.back_waiting)).chain(backs).filter(|m| !m.is_empty()).collect::<Vec<_>>().join("\n\n");
                        }
                    });
                }
            }
            Some(Some(mut t)) => match prepare(&t, id) {
                Ok((base, path)) => {
                    t.path = path;
                    self.write(|st| {
                        if let Some(i) = st.desk.item_mut(id).and_then(|i| i.tree.as_mut()) {
                            (i.base, i.path) = (base.clone(), t.path.clone());
                        }
                    });
                    let line = tree_line(&t);
                    for (_, text) in fx.sends.iter_mut().filter(|(s, text)| *s == to && names_item(text, id) && !text.contains(&t.path)) {
                        text.push('\n');
                        text.push_str(&line);
                    }
                }
                Err(e) => {
                    // Its holder works in the shared folder, as before.
                    eprintln!("cue: desk: couldn't make #{id} its own copy: {e}");
                    // One the lead asked for (COPY, #557): it and the holder are told, so the work isn't done elsewhere unknowingly.
                    if t.keep {
                        let words = format!("Desk item #{id}: Cue couldn't make its copy of {} on branch {}: {e}. Say where to work before changing files.", t.repo, t.branch);
                        fx.sends.push((to.to_string(), words.clone()));
                        if let Some(lead) = self.read(|st| st.desk.item(id).and_then(|i| st.desk.lead_sid(&i.project)).map(str::to_string)) {
                            fx.sends.push((lead, words));
                        }
                    }
                    // Not in line again: a copy that can't be made would be tried at every free one (#566).
                    self.write(|st| {
                        if let Some(i) = st.desk.item_mut(id) {
                            (i.tree, i.no_copy_wait) = (None, false);
                        }
                    });
                }
            },
        }
    }

    /// A dropped item's copy: kept and listed when it has work in it, else removed (its slot free for the next).
    fn desk_tree_dropped(&self, it: DeskItem) {
        let Some(t) = it.tree.clone() else { return };
        if t.keep {
            // A branch the lead named (#557): its work stays on it, the copy goes.
            self.desk_named_dropped(&it.id, &it.title, &t);
        } else if has_work(&t) {
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
    pub(crate) fn desk_trees_free(&self, repo: &str) -> bool {
        let now = now_ms();
        // How many copies of it each project waiting for one may have at once, read before the store is taken.
        let projects: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| in_line(i) && i.tree_wait_repo == repo).map(|i| i.project.clone()).collect());
        let at_once: HashMap<String, u32> = projects.into_iter().map(|p| { let n = crate::desk::lands::lands_of(&p, repo).at_once; (p, n) }).collect();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            // The oldest waiting whose own project's pool (desk/lands.rs) has room among the repository's copies.
            let used = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == repo)).count() as u32;
            // Never one still queued after another item (#512): that one isn't done, so it waits on, copy or not.
            let mut waiting: Vec<&DeskItem> = st.desk.items.iter().filter(|i| in_line(i) && i.tree_wait_repo == repo).collect();
            // Your order first (#387), then the one queued longest.
            waiting.sort_by_key(|i| (i.queue_order.unwrap_or(u32::MAX), i.updated));
            let Some(id) = waiting.into_iter().find(|i| used < at_once.get(&i.project).copied().unwrap_or_else(|| crate::desk::lands::lands_known(&i.project, repo).at_once)).map(|i| i.id.clone()) else { return fx };
            let rule = st.desk.item(&id).map(|i| crate::desk::hub::core::desk::rule_of(st, &i.project)).unwrap_or_default();
            let Some(it) = st.desk.item_mut(&id) else { return fx };
            // Already at work without one (started now with every copy in use, #566): told the copy is its now.
            let at_work = it.state == "working";
            (it.state, it.tree_wait, it.updated, it.queue_order) = ("working".into(), false, crate::desk::hub::core::desk::secs(now), None);
            it.tree_wait_repo.clear();
            if let Some(h) = &it.holder {
                let words = if at_work {
                    format!("Desk item #{id} ({}): a copy of its repository is free now, and it's yours. Move any work you've done for it so far into it, then carry on there.", it.title)
                } else {
                    crate::desk::handed_words(it, "", &rule)
                };
                fx.sends.push((h.to().to_string(), words));
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
    pub(crate) fn desk_trees_catch_up(&self) {
        self.desk_trees_line_up();
        let mut repos: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| in_line(i)).map(|i| i.tree_wait_repo.clone()).collect());
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

    /// Items at work without a copy, started now with every copy in use (START, Start now), put in line for the next
    /// free one (#566): saved before they were, nothing ever gave them one. Only those whose holder would get a copy.
    fn desk_trees_line_up(&self) {
        let items: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "working" && i.no_copy_wait && i.tree.is_none() && !i.tree_wait).map(|i| i.id.clone()).collect());
        if items.is_empty() {
            return;
        }
        let fx = Effects { items, ..Default::default() };
        for (id, folder, _, _) in self.desk_trees_wanted(&fx) {
            let Some(main) = tree_repo(&folder) else { continue };
            self.write(|st| {
                if let Some(i) = st.desk.item_mut(&id).filter(|i| i.state == "working" && i.tree.is_none()) {
                    (i.tree_wait, i.tree_wait_repo) = (true, main);
                }
            });
        }
    }

    /// The landing line: items said DONE in their own copies landed one at a time, oldest DONE first (those the lead
    /// moved up, LAND FIRST, before them: the latest moved first). Whoever runs
    /// it takes everything waiting; an item that has to wait lets the next go first.
    pub(crate) fn desk_land_line(&self) {
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
        // On a branch the lead named (#557): committed there, never on a main line.
        if t.keep {
            self.land_on_branch(id, &t);
            return Landed::Done;
        }
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
            return self.land_nothing(id, &t);
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
            let merged = match self.land_merged(id, &it, &t, &onto, &base, &main, &work) {
                Ok(merged) => merged,
                Err(landed) => return landed,
            };
            let diff = git("", &t.repo, &[], &["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", &main, &merged]).unwrap_or_default();
            if let Some((w, at)) = word_in(&diff, &words) {
                self.finish_ask(id, "word", &format!("Not landed: \"{w}\" is in {at}"), "");
                return Landed::Done;
            }
            let Some(squash) = git("", &t.repo, &[], &["commit-tree", &merged, "-p", &main, "-m", &message]).map(|c| c.trim().to_string()) else {
                return self.land_wait(id, "git couldn't make its commit");
            };
            if let Some(landed) = self.land_check(id, &it, &t, &merged, &squash) {
                return landed;
            }
            // The main line moved while it was checked: brought up to it again.
            if rev(&t.repo, &onto).as_deref() != Some(main.as_str()) {
                continue;
            }
            return self.land_onto(id, &it, &t, &onto, &main, &squash, &message);
        }
        self.land_wait(id, "The main line kept moving")
    }

    /// Nothing in item `id`'s copy `t`: its holder worked where it was. That's landed as before; the copy goes.
    fn land_nothing(&self, id: &str, t: &ItemTree) -> Landed {
        remove(t);
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.tree = None;
            }
        });
        let _ = self.desk_trees_free(&t.repo);
        // Committed where it was, as an item without a copy is (that path takes it at "commit").
        self.finish_fin(id, |f| (f.step, f.wait) = ("commit".into(), String::new()));
        self.desk_finish_shared(id);
        Landed::Done
    }

    /// Item `id`'s work (the tree `work`, from `base`) brought up to the main line at `main`: the merged tree, or how
    /// its turn ended (a clash asks its holder; git failing waits).
    #[allow(clippy::too_many_arguments)]
    fn land_merged(&self, id: &str, it: &DeskItem, t: &ItemTree, onto: &str, base: &str, main: &str, work: &str) -> Result<String, Landed> {
        if main == base {
            return Ok(work.to_string());
        }
        let Some(c) = git("", &t.repo, &[], &["commit-tree", work, "-p", base, "-m", "desk work"]).map(|c| c.trim().to_string()) else {
            return Err(self.land_wait(id, "git couldn't make its commit"));
        };
        let filters = crate::safe_git::no_repo_filters("", &t.repo);
        let tried = crate::safe_git::run("", &t.repo, &filters, &["merge-tree", "--write-tree", "--name-only", "--merge-base", base, main, &c]);
        let Ok((out, code)) = tried else { return Err(self.land_wait(id, "git couldn't bring it up to the main line")) };
        if code != 0 {
            let files: Vec<&str> = out.lines().skip(1).take_while(|l| !l.is_empty()).collect();
            let q = format!(
                "Its work clashes with {onto} in {}: in {}, merge {onto} into {}, settle the clash there, and say DONE again",
                files.join(", "),
                t.path,
                t.branch
            );
            self.finish_clash(it, &q);
            return Err(Landed::Done);
        }
        Ok(out.lines().next().unwrap_or("").trim().to_string())
    }

    /// Its check, once per state of the main line it's checked on (a wait doesn't run it again): Some when its turn
    /// ends here (the check failed and it went back).
    fn land_check(&self, id: &str, it: &DeskItem, t: &ItemTree, merged: &str, squash: &str) -> Option<Landed> {
        let checked = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).map(|f| f.checked.clone())).unwrap_or_default();
        let mut unchecked = false;
        if checked != merged {
            // Its `at` kept: cut short, it's back in the line in its place.
            self.finish_fin(id, |f| (f.step, f.wait) = ("checking".into(), String::new()));
            // Its project's check (desk/lands.rs), on the same tree: main plus the item (`squash`). Your own steps run
            // in a clean copy of it; set to none, it lands "not checked"; else the repository's own (Cue's).
            let check = match crate::desk::lands::check_of(&it.project, &t.repo) {
                crate::desk::lands::Check::Steps(steps) => Some(check_steps_on(&t.repo, squash, &steps)),
                crate::desk::lands::Check::Off => None,
                crate::desk::lands::Check::Repo => self.installer().check(&t.repo, squash),
            };
            match check {
                None => unchecked = true,
                Some(Ok(())) => {}
                Some(Err(log)) => {
                    self.desk_land_back(it, CHECK_FAILED, "check_failed", &log);
                    return Some(Landed::Done);
                }
            }
            let m = merged.to_string();
            self.finish_fin(id, |f| (f.checked, f.unchecked) = (m, unchecked));
        }
        None
    }

    /// Item `id`, checked on the main line as it still is: landed on it (`squash`), or its pull request opened.
    #[allow(clippy::too_many_arguments)]
    fn land_onto(&self, id: &str, it: &DeskItem, t: &ItemTree, onto: &str, main: &str, squash: &str, message: &str) -> Landed {
        // Dropped or sent back while it was checked: not landed.
        if !self.read(|st| still_landing(st.desk.item(id), &t.path)) {
            return Landed::Done;
        }
        // Its project lands by pull request: checked, its branch pushed and a pull request opened; it lands when that's
        // merged (desk/hub/landing/pr.rs). Main isn't touched here.
        if crate::desk::lands::lands_of(&it.project, &t.repo).how == "pr" {
            self.desk_open_pr(it, t, onto, squash, message);
            return Landed::Done;
        }
        if let Some(what) = in_progress(&t.repo) {
            return self.land_wait(id, &format!("The main checkout is in the middle of {what}"));
        }
        // Others' unsaved edits in the main checkout stay as they are, the item's changes merged into them (#350).
        if let Err(why) = crate::desk::hub::landing::land_keep::fast_forward_keeping_edits(&t.repo, main, squash) {
            return self.land_wait(id, &crate::desk::hub::landing::git::because(format!("git wouldn't move {onto} in the main checkout"), &why));
        }
        let short = git("", &t.repo, &[], &["rev-parse", "--short", squash]).map(|s| s.trim().to_string()).unwrap_or_default();
        self.finish_landed(id, t, &short);
        Landed::Done
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
    pub(crate) fn finish_landed(&self, id: &str, t: &ItemTree, short: &str) {
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
        self.desk_finished(id, "landed");
        self.finish_after(id, std::slice::from_ref(&t.repo));
        clear_build_leftovers(&std::path::Path::new(&t.repo).join(TREES).join(SHARED_TARGET));
    }
}

/// Item `i` in line for a free copy of `tree_wait_repo`: queued for one (and not after another item, #512), or at
/// work without one till then (#566).
pub(crate) fn in_line(i: &DeskItem) -> bool {
    i.tree_wait && i.after.is_none() && (i.state == "queued" || (i.state == "working" && i.tree.is_none()))
}

/// What an item of `project` queued for one of `repo`'s copies waits on, as the Desk lead's summary says it (#340).
pub(crate) fn copy_wait(st: &Store, project: &str, repo: &str) -> String {
    let mut held: Vec<String> = st.desk.items.iter().filter(|i| i.tree.as_ref().is_some_and(|t| t.repo == repo)).map(|i| format!("#{}", i.id)).collect();
    held.sort();
    let at_once = crate::desk::lands::lands_known(project, repo).at_once;
    format!(
        "waiting for a free copy of its repository ({} of {at_once} held, by {}): it starts by itself once one lands or is dropped",
        held.len(),
        if held.is_empty() { "none".to_string() } else { held.join(", ") }
    )
}

/// Item `id` just queued for a copy (`claim_tree` found none free), held by session `to`: the messages in `fx.sends`
/// that carry someone's words on it (your Send back or question, the lead's note: steps in `fx.went`) still go now
/// (#567), each told it waits for a copy, so nobody's words sit unseen in the line. Your words wake its lead too.
/// The messages that go, as they now read.
fn words_now(st: &mut Store, fx: &mut Effects, id: &str, to: &str) -> Vec<String> {
    let Some(it) = st.desk.item(id) else { return Vec::new() };
    let (project, title, repo) = (it.project.clone(), it.title.clone(), it.tree_wait_repo.clone());
    let wait = format!("\nIt waits for a free copy of {repo}: every copy is in use. Don't change any files until Cue sends you where its copy is; reading, planning and answering now are fine.");
    let steps: Vec<String> = fx.went.iter().filter(|(_, s, i)| s == to && i == id).map(|(step, ..)| step.clone()).collect();
    let mut now_ones = Vec::new();
    let mut yours = Vec::new();
    for d in st.desk.decisions.iter_mut().filter(|d| steps.contains(&d.id)) {
        let Some(del) = d.delivery.as_mut() else { continue };
        let Some((_, text)) = fx.sends.iter_mut().find(|(s, text)| s == to && *text == del.text) else { continue };
        if !text.contains(&wait) {
            text.push_str(&wait);
        }
        del.text = text.clone();
        now_ones.push(text.clone());
        if d.by == "You" {
            yours.push((d.id.clone(), d.why.clone(), d.text.clone()));
        }
    }
    for (step, why, words) in yours {
        let row = format!(
            "#{id} ({title}): the user's words, {why}: \"{}\". It waits for a free copy of {repo} (every copy is in use); Cue sent its holder the words now, told not to change files until its copy is ready. If it can't wait, free a copy or start it now.",
            words.trim()
        );
        if crate::desk::hub::lead::feed::note(st, &project, id, crate::desk::feed::ChangeKind::WordsWait, row, &step, now_ms()) {
            fx.items.push(id.to_string());
        }
    }
    now_ones
}

/// Item `id`'s copy of `main`, in a slot free of the `at_once` its project allows: Some(Some) claimed, Some(None) none
/// free (it's queued for one), None nothing to claim (not at work, has one, or started by the lead to go without).
fn claim_tree(st: &mut Store, id: &str, main: &str, at_once: u32) -> Option<Option<ItemTree>> {
    let used: Vec<u32> = st.desk.items.iter().filter_map(|i| i.tree.as_ref()).filter(|t| t.repo == main).map(|t| t.slot).collect();
    // Its copy taken back earlier (TAKE COPY, #347) is kept on its branch: this one's is another.
    let again = if st.desk.kept.iter().any(|k| k.id == id) { "-again" } else { "" };
    let it = st.desk.item_mut(id).filter(|i| i.state == "working" && i.tree.is_none())?;
    // A branch the lead named (COPY, #557): that one, kept.
    let (branch, keep) = match &it.copy_on {
        Some(c) => (c.branch.clone(), true),
        None => (format!("desk/{id}{again}"), false),
    };
    match (1..=at_once).find(|n| !used.contains(n)) {
        Some(n) => {
            let t = ItemTree { repo: main.to_string(), path: format!("{main}/{TREES}/desk-{n}"), branch, base: String::new(), slot: n, keep };
            it.tree = Some(t.clone());
            // Started another way (Start it now, or freed by its `after`) while it waited for a copy: that wait is over.
            it.tree_wait = false;
            it.tree_wait_repo.clear();
            Some(Some(t))
        }
        // Started now by the lead (START): it works where its holder is, message and all, in line for the next free
        // copy all the same (#566).
        None if it.no_copy_wait => {
            (it.tree_wait, it.tree_wait_repo) = (true, main.to_string());
            None
        }
        None => {
            (it.state, it.tree_wait, it.tree_wait_repo) = ("queued".into(), true, main.to_string());
            Some(None)
        }
    }
}

#[cfg(test)]
impl Hub {
    /// `repo`'s copies `slots` taken by items of another Desk (a test's), as if they were at work.
    pub(crate) fn desk_hold_copies(&self, repo: &str, slots: &[u32]) {
        self.write(|st| {
            for n in slots {
                let tree = ItemTree { repo: repo.to_string(), path: format!("{repo}/{TREES}/held-{n}"), branch: format!("held/{n}"), base: String::new(), slot: *n, keep: false };
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

    /// Item `id` as an older Cue saved one started now with every copy in use: at work, not in line for one (#566).
    pub(crate) fn desk_out_of_copy_line(&self, id: &str) {
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                (i.tree_wait, i.no_copy_wait) = (false, true);
                i.tree_wait_repo.clear();
            }
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
