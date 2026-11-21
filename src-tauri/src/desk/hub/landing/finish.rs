//! Done means it's in your app: once its holder says DONE, an item is "finishing" (`DeskItem::finish`, saved with it,
//! so across the restart an install makes too). Its files are committed at once (Looks right's commit code: only its
//! files, the word check, a wait while another session at work edits one of them, never pushed); its crew's branch,
//! when its work sits on one, is merged into the main checkout's branch, the merge word-checked too and never forced.
//! Its work must then be on main (the branch, not just the main checkout: "landing" while it isn't, saying where); an
//! item that changed the Cue this app is built from is then in the next install; any other goes to check.
//! Installs run one at a time, once for everything waiting (an item waiting while one runs that has its commit goes
//! with that one): half an hour after the first of it, built from the main
//! line's HEAD, and not while a session that ends with Cue is mid-turn (unless you say Install now). Once the running
//! Cue has an item's commit, the item is to check. A build that fails, or a new Cue that doesn't start, stops on a
//! question for you, and its holder is sent the log. Work its holder committed itself waits for the app all the same,
//! and a later commit on the main line that takes a finished item's work back out sends it back to its holder (#213).

use crate::desk::hub::landing::commit::{commit_each, shas_of, words_checked};
use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::install::{installing, stopped_install, tail};
use crate::desk::hub::landing::main_line::{main_checkout, merge_ref, merge_to_main, same_dir, Merge};
use crate::desk::hub::theirs::theirs::Part;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, Finish};

/// How long an item waits for its work to be on main before you're asked about it.
pub(crate) const WAIT_ASK_MS: u64 = 15 * 60_000;
/// How long the first item waiting for an install waits, so everything done in that time goes in one build.
pub(crate) const INSTALL_EVERY_MS: u64 = 30 * 60_000;

/// The questions a finishing item can stop on.
pub(crate) const BUILD_FAILED: &str = "The build failed";
/// The window's check (scripts/check-window.mjs) refused the commit: its scripts call a name none declares, or
/// declare one twice. Nothing was installed or served.
pub(crate) const WINDOW_BROKEN: &str = "The window doesn't load at this commit, so it wasn't installed or served";
pub(crate) const ROLLED_BACK: &str = "The new Cue didn't start; the old one is back";
/// The installer wouldn't begin (it can't install this Cue, or another install is running), its reason after it.
pub(crate) const NOT_STARTED: &str = "The install didn't start";

/// One run of `key`'s finishing work on a hub (an item's commit, or "tick"), let go as it ends, a panic in it too.
pub(crate) struct Claim<'a> {
    hub: &'a Hub,
    key: String,
}

impl<'a> Claim<'a> {
    /// The claim, unless a run of `key` holds it now.
    pub(crate) fn take(hub: &'a Hub, key: &str) -> Option<Self> {
        let claimed = hub.write(|st| st.desk.run.finish_claims.insert(key.to_string()));
        if claimed && key == "landing" {
            LANDING_HERE.with(|n| n.set(n.get() + 1));
        }
        claimed.then(|| Claim { hub, key: key.to_string() })
    }

    /// The landing line's claim for work that mustn't run beside a landing (an item's folder switched onto its branch
    /// or back, desk/hub/landing/in_place.rs): Ok(None) when this thread holds it already (a landing that goes on to
    /// start the next item), Err when another run holds it after a short wait (tried again later).
    pub(crate) fn landing(hub: &'a Hub) -> Result<Option<Self>, ()> {
        if LANDING_HERE.with(|n| n.get() > 0) {
            return Ok(None);
        }
        for _ in 0..10 {
            if let Some(c) = Claim::take(hub, "landing") {
                return Ok(Some(c));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(())
    }
}

thread_local! {
    /// How many landing claims this thread holds.
    static LANDING_HERE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if self.key == "landing" {
            LANDING_HERE.with(|n| n.set(n.get().saturating_sub(1)));
        }
        let key = std::mem::take(&mut self.key);
        self.hub.write(|st| st.desk.run.finish_claims.remove(&key));
    }
}

/// Why finishing item `id` can't be taken off its finish now, as the lead is told (#248): its commit is being made,
/// it waits on files or on main, or it waits for or is in an install. None when it stopped on a question: nothing of
/// its finish is under way then (a merge that clashed was aborted; commits it made stay, as Send back leaves them), so
/// it can go back to work. A commit that waited long enough to ask is still tried each tick: under way while it runs.
pub(crate) fn under_way(st: &Store, id: &str) -> Option<String> {
    let it = st.desk.item(id).filter(|i| i.state == "finishing")?;
    let f = it.finish.as_ref()?;
    if st.desk.run.finish_claims.contains(id) {
        return Some("is being committed now".to_string());
    }
    if !f.question.is_empty() {
        return None;
    }
    Some(match f.step.as_str() {
        "commit" => "is being committed now".to_string(),
        "commit_wait" => format!("is waiting to be committed ({})", f.wait),
        "landing" => format!("is committed and waiting to be on main ({})", f.wait),
        "queued" => "is committed and waiting for the next install".to_string(),
        s if installing(s) => "is being installed now".to_string(),
        _ => "is finishing".to_string(),
    })
}

impl Hub {
    /// Session `sid` as one on `machine` (a test's).
    #[cfg(test)]
    pub(crate) fn desk_session_on(&self, sid: &str, machine: &str) {
        self.write(|st| {
            if let Some(s) = st.sessions.get_mut(sid) {
                s.origin.machine = machine.to_string();
            }
        });
    }

    /// `f` done to item `id` while it's still finishing, saved; the item as it is then.
    pub(crate) fn finish_set(&self, id: &str, f: impl FnOnce(&mut DeskItem)) -> Option<DeskItem> {
        let it = self.desk_saving(|| {
            let it = self.write(|st| {
                let i = st.desk.item_mut(id).filter(|i| i.state == "finishing" && i.finish.is_some())?;
                f(i);
                Some(i.clone())
            });
            it.inspect(crate::db::desk_put_item)
        });
        if it.is_some() {
            self.changed();
        }
        it
    }

    /// Item `id` to check: nothing (more) to install for it. Closed then when the lead said DONE on it for you.
    pub(crate) fn finish_checked(&self, id: &str) {
        let now = now_ms();
        let checked = self.finish_set(id, |i| {
            i.state = "to_check".into();
            i.updated = crate::desk::hub::core::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.wait) = ("checked".into(), now, String::new(), String::new());
            }
        });
        if checked.is_some() {
            self.desk_finished(id, "check");
        }
        self.desk_close_on_check(id);
    }

    /// Item `id`, to check, closed as Looks right would when the Desk lead said DONE on it for you (#337,
    /// `DeskItem::close_on_check`); one still finishing waits for `finish_checked`.
    pub(crate) fn desk_close_on_check(&self, id: &str) {
        // Stopped on a failed install: its landing run again on the main line as it is now (#343), closed once that's in.
        let stopped = self.read(|st| st.desk.item(id).is_some_and(|i| i.close_on_check && i.state == "finishing" && i.finish.as_ref().is_some_and(|f| stopped_install(&f.step) && !f.repo.is_empty())));
        // A stop the lead has (#483): its DONE says the way is clear. Landed again, it comes to your check.
        let cleared = self.read(|st| st.desk.item(id).is_some_and(|i| i.close_on_check && i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.lead && !f.question.is_empty())));
        if cleared && !stopped {
            self.finish_set(id, |i| i.close_on_check = false);
            if let Err(e) = self.desk_rerun_landing(id) {
                eprintln!("cue: desk: #{id}: the lead's DONE couldn't run its landing again: {e}");
            }
            return;
        }
        if stopped {
            if let Err(e) = self.desk_rerun_landing(id) {
                eprintln!("cue: desk: #{id}: the lead's DONE couldn't run its landing again: {e}");
            }
            return;
        }
        if !self.read(|st| st.desk.item(id).is_some_and(|i| i.close_on_check && i.state == "to_check")) {
            return;
        }
        if let Err(e) = self.desk_looks_right(id) {
            eprintln!("cue: desk: #{id}: the lead's DONE couldn't close it: {e}");
        }
    }

    /// `f` done to item `id`'s `finish` while it's still finishing, saved.
    pub(crate) fn finish_fin(&self, id: &str, f: impl FnOnce(&mut Finish)) {
        self.finish_set(id, |i| {
            if let Some(fin) = i.finish.as_mut() {
                f(fin);
            }
        });
    }

    /// Item `id` at finishing step `step` from now.
    pub(crate) fn finish_step(&self, id: &str, step: &str) {
        let now = now_ms();
        self.finish_fin(id, |f| (f.step, f.at, f.wait) = (step.to_string(), now, String::new()));
    }

    /// Item `id` stopped on `question` at `step`: its Desk lead's to settle when it has a live one, else yours
    /// (desk/hub/lead/stop_lead.rs, #483).
    pub(crate) fn finish_ask(&self, id: &str, step: &str, question: &str, log: &str) {
        let said = self.finish_question(id, step, question, log);
        self.finish_to_lead(id, &said);
    }

    /// Item `id` stopped on `question` at `step`, its first line kept as what's wrong and git's words after it with
    /// its log; returns git's words.
    pub(crate) fn finish_question(&self, id: &str, step: &str, question: &str, log: &str) -> String {
        let now = now_ms();
        let (plain, said) = crate::desk::hub::lead::stop_lead::plain_and_said(question);
        let log = [said.as_str(), log].iter().filter(|l| !l.trim().is_empty()).copied().collect::<Vec<_>>().join("\n");
        self.finish_set(id, |i| {
            i.updated = crate::desk::hub::core::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.log, f.now, f.lead) = (step.into(), now, plain, tail(&log), false, false);
            }
        });
        said
    }

    /// Item `id` just said DONE: in its own copy, it joins the landing line (desk/hub/landing/trees.rs); else its commit made (with its crew's branch merged), then queued
    /// for an install when it changed this Cue, else to check. One run per item at a time.
    pub(crate) fn desk_finish_commit(&self, id: &str) {
        let Some(_claim) = Claim::take(self, id) else { return };
        self.finish_commit(id);
    }

    fn finish_commit(&self, id: &str) {
        // Worked on another machine: its branch pushed from there and fetched here first.
        if self.desk_remote_done(id) {
            return;
        }
        // In its own copy: it lands through the landing line, one item at a time.
        if self.read(|st| st.desk.item(id).is_some_and(|i| i.tree.is_some())) {
            return self.desk_land_line();
        }
        self.desk_finish_shared(id);
    }

    /// Item `id`, worked on in its holder's own folder (no copy of its own): its files there committed (those
    /// another session is editing or changed too left out, in the folder for that session), its crew's branch merged.
    pub(crate) fn desk_finish_shared(&self, id: &str) {
        let Ok((it, machine, sources)) = self.desk_change_sources(id) else { return };
        let Some(_fin) = it.finish.clone().filter(|f| it.state == "finishing" && f.step == "commit") else { return };
        // A session on another machine: nothing to install here, and its work never closes uncommitted: to check with
        // no `finish`, so Looks right commits it there and Undo takes that back, as before. A project whose work you
        // commit yourself (desk/lands.rs) goes the same way, and Looks right then commits nothing.
        if !machine.is_empty() || self.item_lands(&it).how == "owner" {
            let now = now_ms();
            if self.finish_set(id, |i| (i.state, i.finish, i.updated) = ("to_check".into(), None, crate::desk::hub::core::desk::secs(now))).is_some() {
                self.desk_finished(id, "check");
            }
            self.desk_close_on_check(id);
            return;
        }
        let (todo, parts, left) = self.to_commit(&it, &sources);
        if !left.is_empty() {
            self.finish_fin(id, |f| {
                for p in left {
                    if !f.without.contains(&p) {
                        f.without.push(p);
                    }
                }
            });
        }
        let Some(tops) = self.finish_land(&it, &todo, &parts, &sources) else { return };
        // Its helpers' copies whose work is on the main line now (#376).
        for top in &tops {
            self.desk_clear_helpers(id, top, None);
        }
        self.finish_after(id, &tops);
    }

    /// Item `it`'s folders on this Mac: its crew's (desk/hub/core/desk.rs `item_sources`), then each folder of `sources`.
    pub(crate) fn item_folders(&self, it: &DeskItem, sources: &[(String, Vec<String>)]) -> Vec<String> {
        let mut out: Vec<String> = self.read(|st| crate::desk::hub::core::desk::item_sources(st, it).1.into_iter().map(|(f, _)| f).collect());
        for (f, _) in sources {
            if !out.contains(f) {
                out.push(f.clone());
            }
        }
        // A worktree its holder made itself on the item's own branch (started without a copy of its own: #417's
        // rounds 2 and 3, #448), merged as its crew's is.
        let holder = self.read(|st| it.holder.as_ref().and_then(|h| st.sessions.get(h.to())).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone()));
        let mut repos: Vec<String> = Vec::new();
        for f in out.iter().cloned().chain(holder) {
            if let Some((m, _)) = main_checkout(&f).filter(|(m, _)| !repos.iter().any(|r| same_dir(r, m))) {
                repos.push(m);
            }
        }
        let skip = it.tree.as_ref().map(|t| t.path.clone());
        for r in repos {
            for t in crate::desk::hub::landing::unlanded::branch_trees(&r, &it.id, skip.as_deref()) {
                if !out.iter().any(|o| same_dir(o, &t)) {
                    out.push(t);
                }
            }
        }
        // Any other worktree its holder worked in or committed in, whatever its branch is called (#481).
        for t in self.holder_work(it).folders {
            if !out.iter().any(|o| same_dir(o, &t)) {
                out.push(t);
            }
        }
        // Never another item's copy, wherever its holder looked: that work lands with that item (#551).
        let others = self.read(|st| crate::desk::hub::landing::own_work::other_copies(st, it));
        out.retain(|f| !crate::desk::hub::landing::own_work::in_copies(f, &others));
        // Nor a worktree on another item's branch (desk/<n>), its item done and its copy not yet handed on (#578).
        out.retain(|f| !crate::desk::hub::landing::own_work::on_others_branch(f, &it.id));
        out
    }

    /// Item `id`'s work git says isn't committed (each path in the folder it was changed in), asked as its holder's
    /// turn ends on it (#213). None on another machine: its folders aren't looked at here.
    pub(crate) fn desk_uncommitted(&self, id: &str) -> Vec<String> {
        let Ok((_, machine, sources)) = self.desk_change_sources(id) else { return Vec::new() };
        if !machine.is_empty() {
            return Vec::new();
        }
        sources.iter().flat_map(|(folder, paths)| crate::desk::hub::landing::commit::changed_of("", folder, paths).unwrap_or_default()).collect()
    }

    /// Item `it`'s files git says changed, by folder (one not in a repository left out), with only its own part of a
    /// file another session changed too (desk/hub/theirs/theirs.rs), and the files left out (absolute): a session at work is
    /// editing it now, or another's changes in it can't be told apart from the item's. They stay in the folder,
    /// uncommitted, for that session: an item's commit never waits on another's.
    fn to_commit(&self, it: &DeskItem, sources: &[(String, Vec<String>)]) -> (Vec<(String, Vec<String>)>, Vec<Part>, Vec<String>) {
        let busy = self.busy_by(it);
        let mut todo: Vec<(String, Vec<String>)> = Vec::new();
        let mut left: Vec<String> = Vec::new();
        for (folder, paths) in sources {
            let Some(changed) = crate::desk::hub::landing::commit::changed_of("", folder, paths) else { continue };
            let abs = |p: &str| format!("{}/{p}", folder.trim_end_matches('/'));
            let (editing, changed): (Vec<String>, Vec<String>) = changed.into_iter().partition(|p| busy.iter().any(|(f, _, _)| *f == abs(p)));
            left.extend(editing.iter().map(|p| abs(p)));
            if !changed.is_empty() {
                todo.push((folder.clone(), changed));
            }
        }
        let folders: Vec<(String, String)> = todo.iter().flat_map(|(f, ps)| ps.iter().map(|p| (p.clone(), f.clone())).collect::<Vec<_>>()).collect();
        let (todo, parts, mixed) = self.own_of(it, todo);
        for (p, _) in mixed {
            let folder = folders.iter().find(|(q, _)| *q == p).map(|(_, f)| f.clone()).unwrap_or_default();
            left.push(format!("{}/{p}", folder.trim_end_matches('/')));
        }
        (todo, parts, left)
    }

    /// Item `it`'s `todo` committed, and its work on a branch of its own (its crew's, or its holder's own worktree:
    /// each folder of `sources`) merged into the main checkout's branch: each main checkout its work is on, those of
    /// work its holder committed itself too (#213). None when it stopped on a question (a word, a conflict).
    fn finish_land(&self, it: &DeskItem, todo: &[(String, Vec<String>)], parts: &[Part], sources: &[(String, Vec<String>)]) -> Option<Vec<String>> {
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        let crew = self.item_folders(it, sources);
        if !todo.is_empty() {
            if let Err(e) = words_checked(&project, "", todo, parts, &it.title) {
                self.finish_ask(&it.id, if e.contains("\" is in ") { "word" } else { "error" }, &e, "");
                return None;
            }
        }
        // Each commit made is kept, a later one failing or not (its card, diff and Undo have it).
        let (made, failed) = commit_each("", todo, parts, &it.title);
        self.save_made(&it.id, &made);
        if let Some(e) = failed {
            self.finish_ask(&it.id, "error", &e, "");
            return None;
        }
        let mut merged: Vec<String> = Vec::new();
        // Branches holding its holder's commits that no worktree has (#481), merged as a worktree's are.
        let refs: Vec<Merge> = self.holder_work(it).refs.into_iter().map(|(main, branch, sha)| Merge::Ref { main, branch, sha }).collect();
        for m in crew.iter().map(|f| Merge::Folder(f.clone())).chain(refs) {
            let tried = match &m {
                Merge::Folder(folder) => merge_to_main(&project, folder, &it.title),
                Merge::Ref { main, branch, sha } => merge_ref(&project, main, branch, sha, &it.title),
            };
            match tried {
                Ok(Some(main)) => {
                    // Its main line's tip, not the folder's HEAD (on another item's branch there, in_place.rs).
                    if let Some(sha) = crate::desk::hub::landing::in_place::main_head(&main) {
                        let m = main.clone();
                        self.finish_fin(&it.id, |f| f.merges.push((m, sha)));
                    }
                    merged.push(main)
                }
                Ok(None) => {}
                // A clash in its work: back to the session that did it (#371); git refusing or a word: waits on you.
                Err(("clash", question)) => {
                    self.finish_clash(it, &question);
                    return None;
                }
                Err((step, question)) => {
                    let main = match &m {
                        Merge::Folder(folder) => main_checkout(folder).map(|(main, _)| main).unwrap_or_default(),
                        Merge::Ref { main, .. } => main.clone(),
                    };
                    let question = self.main_edits_named(it, &main, &question);
                    self.finish_ask(&it.id, step, &question, "");
                    return None;
                }
            }
        }
        let mut tops: Vec<String> = Vec::new();
        // Its own folders last: work its holder committed there itself is on their main line all the same.
        for folder in made.iter().map(|(f, _)| f.clone()).chain(merged).chain(crew) {
            if let Some((main, _)) = main_checkout(&folder).filter(|(m, _)| !tops.contains(m)) {
                tops.push(main);
            }
        }
        Some(tops)
    }

    /// Item `id`, its work on the main line in `tops`: queued for an install when one of them is this Cue's source,
    /// else to check.
    /// Its work left on the add-on's branches (#582's) is merged into the add-on's main line first, Cue installed with it.
    pub(crate) fn finish_after(&self, id: &str, tops: &[String]) {
        if self.land_addon_left(id) {
            return;
        }
        let now = now_ms();
        // Nothing new this round (its fix after a failed install was already committed): the main line it went to
        // before, so it's installed all the same.
        let before = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).map(|f| f.repo.clone())).unwrap_or_default();
        let tops: Vec<String> = if tops.is_empty() && !before.is_empty() { vec![before] } else { tops.to_vec() };
        // Its project set to nothing after landing (desk/lands.rs): Cue's own work isn't installed or served.
        let project = self.read(|st| st.desk.item(id).map(|i| i.project.clone())).unwrap_or_default();
        let cue = tops.iter().find(|t| crate::desk::hub::lead::checks::builds_this_cue(t) && crate::desk::lands::lands_of(&project, t).after == "install");
        let top = cue.or(tops.first()).cloned().unwrap_or_default();
        // The main line's tip: the folder's HEAD, or, while another item works on its own branch there, the main line it
        // came from (desk/hub/landing/in_place.rs).
        let head = if top.is_empty() { String::new() } else { crate::desk::hub::landing::in_place::main_head(&top).unwrap_or_default() };
        // Not on main yet (the main checkout is on another branch, or a worktree's work wasn't merged): it waits
        // there, saying where, and goes on once it is (the tick looks again). Never to check, nor served, before.
        if !top.is_empty() {
            if let Some(at) = self.not_on_main(id, &top, &head) {
                return self.finish_landing(id, &top, &head, at);
            }
        }
        // Landed, but not all of its work (#448, #481): back to its session, never to check.
        if self.sent_back_off_main(id) {
            return;
        }
        // Its steps after landing, and the window's files, run in the project's folder: never on another item's half-done
        // files while that item works on its own branch there. It waits, saying so, till the folder is back.
        if !top.is_empty() && (cue.is_some() || crate::desk::lands::lands_of(&project, &top).after == "command") {
            if let Some((b, line)) = crate::desk::hub::landing::in_place::on_item_branch(&top) {
                let why = format!("it's on {line}; its steps after landing wait till the project's folder is back on {line} (it's on {b}, another item's own branch, now)");
                return self.finish_wait_folder(id, &top, &head, why);
            }
        }
        // On main: pushed now when its project pushes after each landing (desk/hub/landing/pr.rs).
        if !top.is_empty() {
            self.desk_push_after(id, &project, &top);
        }
        // After landing, your own steps (desk/lands.rs): in order in the project's main folder, each stopped past its cap;
        // the first that fails stops the rest, and the item waits for you, saying which. No lock is held here.
        if cue.is_none() && !top.is_empty() && self.finish_after_steps(id, &project, &top, &head) {
            return;
        }
        if self.finish_in_place(id, &top, &head, cue.is_some()) {
            return;
        }
        self.finish_queue_or_check(id, top, head, cue.is_some(), now);
    }

    /// Item `id`, landed on main line `top` at `head`: `queued` for an install (`now`) when it's this Cue's source,
    /// else to check.
    fn finish_queue_or_check(&self, id: &str, top: String, head: String, queued: bool, now: u64) {
        // Nothing of it was ever on a main line (no commit, nothing merged: a plan, a doc, a decision): to check with
        // nothing to install, as an item whose work changed nothing the app runs (#543).
        let nothing = top.is_empty() && self.read(|st| st.desk.item(id).is_some_and(|i| i.commit_in.is_empty() && i.finish.as_ref().is_none_or(|f| f.merges.is_empty())));
        self.finish_set(id, |i| {
            if nothing {
                i.mocks_only = true;
            }
            if let Some(f) = i.finish.as_mut() {
                f.mocks |= nothing;
                (f.repo, f.commit) = (top, head);
                if queued {
                    (f.step, f.at, f.wait, f.wait_from, f.question) = ("queued".into(), now, String::new(), 0, String::new());
                }
            }
        });
        if queued {
            // Due once the first waiting item's half hour is over; the tick looks too.
            self.later(INSTALL_EVERY_MS, |h| h.desk_finish_tick());
        } else {
            self.finish_checked(id);
        }
    }

    /// After landing, item `id`'s project's own steps in `top`: true when one failed (it waits for you, saying which).
    /// Never while an item's branch is put in that folder ("A branch", in_place.rs): placing one waits while they run.
    fn finish_after_steps(&self, id: &str, project: &str, top: &str, head: &str) -> bool {
        let l = crate::desk::lands::lands_of(project, top);
        if l.after == "command" {
            // Beside no placement: checked holding the landing line's claim, and marked running before it's let go.
            let Ok(guard) = Claim::landing(self) else {
                self.finish_wait_folder(id, top, head, "it's on main; its steps after landing wait for a landing under way in the project's folder".into());
                return true;
            };
            if let Some((b, line)) = crate::desk::hub::landing::in_place::on_item_branch(top) {
                let why = format!("it's on {line}; its steps after landing wait till the project's folder is back on {line} (it's on {b}, another item's own branch, now)");
                self.finish_wait_folder(id, top, head, why);
                return true;
            }
            let running = Claim::take(self, &crate::desk::hub::landing::in_place::steps_key(top));
            drop(guard);
            let ran = crate::desk::lands::run_steps(top, &l.after_steps);
            if running.is_some() {
                drop(running);
                // An item waiting for the folder may take it now.
                let _ = self.desk_trees_free(top);
            }
            if let Err(e) = ran {
                self.finish_ask(id, "after_failed", &format!("It landed, but after landing \"{}\" {}", e.step, e.why), &e.said);
                return true;
            }
        }
        false
    }

    /// Item `id` on main at `top` (`head`), its work changing nothing the app runs, or only the window's files (`cue`:
    /// this Cue's source): to check with no install, or back to its holder when its window doesn't load. True when so.
    fn finish_in_place(&self, id: &str, top: &str, head: &str, cue: bool) -> bool {
        // Its own work changed nothing the app runs (mocks, notes, plans, tests): to check now, no install, whatever
        // else on the main line waits for one (#408).
        if cue && self.runs_nothing(id, top) {
            self.finish_set(id, |i| {
                if let Some(f) = i.finish.as_mut() {
                    (f.repo, f.commit, f.mocks) = (top.to_string(), head.to_string(), true);
                }
                // Kept past a reopen, so its next DONE isn't held for something new on main (#549).
                i.mocks_only = true;
            });
            self.finish_checked(id);
            return true;
        }
        // Only the window's files changed since the running Cue's build: they're served in place, no install.
        let in_place = if cue { self.ui_in_place(top, head) } else { Ok(false) };
        if !matches!(in_place, Ok(false)) {
            self.finish_set(id, |i| {
                if let Some(f) = i.finish.as_mut() {
                    (f.repo, f.commit) = (top.to_string(), head.to_string());
                }
            });
            // Its window doesn't load: back to its holder with what's wrong, as a failed install is.
            if let Err(said) = in_place {
                if let Some(it) = self.read(|st| st.desk.item(id).cloned()) {
                    self.finish_failed(&it, WINDOW_BROKEN, "failed", &said);
                }
                return true;
            }
            self.finish_checked(id);
            return true;
        }
        false
    }

    /// Item `id` sent back to its session, not to check, when its work isn't all on main: reopened and nothing new of
    /// it landed since (#448); or a commit of its holder's Cue couldn't land, changes left on its own branch (#481).
    fn sent_back_off_main(&self, id: &str) -> bool {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return false };
        if self.desk_nothing_new(id) {
            let words = crate::desk::hub::landing::unlanded::nothing_new_words(id, self.desk_unlanded(id));
            self.finish_back(&it, &words, "nothing_new", "", true);
            return true;
        }
        let left = self.desk_unlanded(id);
        let off = self.holder_work(&it).off;
        if left == 0 && off.is_empty() {
            return false;
        }
        self.finish_back(&it, &crate::desk::hub::landing::own_work::off_main_words(&off, left, id), "unlanded", "", true);
        true
    }

    /// Item `id` waits for its work to be on main (`at` says where it is), with the main line's `top` and `head` kept;
    /// asked of you once it has waited `WAIT_ASK_MS`.
    fn finish_landing(&self, id: &str, top: &str, head: &str, at: String) {
        let now = now_ms();
        let Some(fin) = self.read(|st| st.desk.item(id).and_then(|i| i.finish.clone())) else { return };
        let from = if fin.step == "landing" && fin.wait_from > 0 { fin.wait_from } else { now };
        let ask = now.saturating_sub(from) >= WAIT_ASK_MS;
        let question = if ask { format!("Its work isn't on main after 15 minutes: it's {at}") } else { String::new() };
        if fin.step == "landing" && fin.wait == at && fin.question == question && fin.commit == head {
            return;
        }
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                if f.step != "landing" {
                    f.at = now;
                }
                (f.step, f.repo, f.commit, f.wait, f.wait_from) = ("landing".into(), top.into(), head.into(), at, from);
                if f.question != question {
                    (f.question, f.lead) = (question.clone(), false);
                }
            }
        });
        if ask && fin.question.is_empty() {
            self.finish_to_lead(id, "");
        }
    }

    /// Item `id`, on main at `top` (`head`), waits for the project's folder (`why`): never asked of you, it goes on by
    /// itself once the folder is back (the tick looks again, as for "landing").
    fn finish_wait_folder(&self, id: &str, top: &str, head: &str, why: String) {
        let now = now_ms();
        let same = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).is_some_and(|f| f.step == "landing" && f.wait == why && f.commit == head));
        if same {
            return;
        }
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                if f.step != "landing" {
                    (f.at, f.wait_from) = (now, now);
                }
                (f.step, f.repo, f.commit, f.wait) = ("landing".into(), top.into(), head.into(), why);
            }
        });
    }

    /// Whether `head` of `repo` is in the running Cue without an install: what changed since the commit it was built
    /// from is only the window's files (then served from `head`, and the window told to reload: "ui-reload") or
    /// nothing it shows. Not while an install is under way or queued: the main line then has more than the window's
    /// files, or it soon will be in the app anyway. Err: the window at `head` doesn't pass its check
    /// (live_ui::check), what's wrong said; nothing is served.
    fn ui_in_place(&self, repo: &str, head: &str) -> Result<bool, String> {
        let Some(running) = crate::install::live_ui::running() else { return Ok(false) };
        let busy = self.read(|st| st.desk.items.iter().any(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "queued" || installing(&f.step))));
        if busy {
            return Ok(false);
        }
        let served = crate::install::live_ui::current().filter(|c| c.base == running).map(|c| c.commit);
        match crate::install::live_ui::plan(repo, &running, served.as_deref(), head) {
            crate::install::live_ui::Plan::Install => Ok(false),
            crate::install::live_ui::Plan::Nothing => Ok(true),
            crate::install::live_ui::Plan::Reload => {
                crate::install::live_ui::check(repo, head)?;
                match crate::install::live_ui::take(repo, head, &running) {
                    Ok(()) => {
                        self.ports.window.tell("ui-reload", serde_json::json!({ "commit": head }));
                        Ok(true)
                    }
                    Err(e) => {
                        eprintln!("cue: desk: the window's files of {head} couldn't be served, installing instead: {e}");
                        Ok(false)
                    }
                }
            }
        }
    }

    /// The commits made for item `id`, added to those it has (one per repository and round).
    pub(crate) fn save_made(&self, id: &str, made: &[(String, String)]) {
        if made.is_empty() {
            return;
        }
        self.finish_set(id, |i| {
            i.commit_in.extend(made.iter().cloned());
            i.commit = shas_of(&i.commit_in);
        });
    }

    /// Cue's tick for finishing items (every ~10 s and as a turn ends; the first after a restart too): commits that
    /// wait tried again, items the running Cue now has to check, installs followed, and the next install started
    /// once it's due.
    pub(crate) fn desk_finish_tick(&self) {
        let Some(_claim) = Claim::take(self, "tick") else { return };
        self.finish_tick();
        // Pull requests open for review: read once a minute (desk/hub/landing/pr.rs).
        self.desk_pr_tick();
        // Folders left on an item's branch with nobody at work on it put back first, so one waiting may start there.
        self.desk_in_place_tick();
        self.desk_trees_catch_up();
        self.desk_old_copies_tick();
        self.taken_out_tick();
        // Circles the waits list has now told to their lead, gone ones let go (#250).
        self.circle_check();
        // Desks' leads that can't settle it: a band unanswered 15 minutes, a hold an hour past its till (#540).
        self.desk_to_user_tick();
        // The change feed: what time raises (a finishing stuck, a holder gone), then its backstop: rows past "sent" go
        // once the lead is free (desk/hub/lead/feed.rs).
        self.desk_feed_watch();
        self.desk_feed_tick();
        // Items queued for a session whose wait is up (desk/hub/turn/queue.rs), should its own timer have been lost.
        self.desk_queue_tick();
    }
}

/// Whether `path` (relative to `folder`) was never in its main line: in no commit of it (a scratch or ignored file
/// lands nothing). False when git can't say.
pub(crate) fn never_in(folder: &str, path: &str) -> bool {
    git("", folder, &[], &["log", "-1", "--format=%H", "HEAD", "--", path]).is_some_and(|o| o.trim().is_empty())
}

/// A finishing item as it's said DONE (or retried): its commit to be made, the main line and commit of an earlier
/// round (`before`) kept, so a round with nothing new still installs them.
pub(crate) fn begun(now: u64, before: Option<&Finish>) -> Finish {
    let (repo, commit) = before.map(|f| (f.repo.clone(), f.commit.clone())).unwrap_or_default();
    Finish { step: "commit".into(), at: now, repo, commit, ..Default::default() }
}
