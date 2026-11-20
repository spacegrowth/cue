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

use super::desk_commit::{commit_each, git, git_said, shas_of, word_in, words_checked};
use super::desk_theirs::Part;
use super::*;
use crate::desk::{DeskItem, Finish, InstallStatus, Installer};

/// How long an item waits for its work to be on main before you're asked about it.
pub(super) const WAIT_ASK_MS: u64 = 15 * 60_000;
/// How long the first item waiting for an install waits, so everything done in that time goes in one build.
pub(super) const INSTALL_EVERY_MS: u64 = 30 * 60_000;
/// How much of an install's log an item keeps (lines, from its end).
const LOG_LINES: usize = 40;

/// The questions a finishing item can stop on.
pub(crate) const BUILD_FAILED: &str = "The build failed";
/// The window's check (scripts/check-window.mjs) refused the commit: its scripts call a name none declares, or
/// declare one twice. Nothing was installed or served.
pub(crate) const WINDOW_BROKEN: &str = "The window doesn't load at this commit, so it wasn't installed or served";
pub(crate) const ROLLED_BACK: &str = "The new Cue didn't start; the old one is back";
/// The installer wouldn't begin (it can't install this Cue, or another install is running), its reason after it.
pub(crate) const NOT_STARTED: &str = "The install didn't start";

/// The app's own installer: self_install.rs.
struct AppInstaller;

impl Installer for AppInstaller {
    fn has(&self, repo: &str, commit: &str) -> bool {
        crate::self_install::has(repo, commit)
    }

    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String> {
        crate::self_install::start(repo, commit, tag)
    }

    fn status(&self) -> Option<InstallStatus> {
        crate::self_install::status().map(|s| InstallStatus { tag: s.tag, commit: s.commit, step: s.step, log: s.log, at: s.at })
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::self_install::is_ancestor(repo, commit, of)
    }

    /// Cue's own repository is checked before a landing (the step 1 check, docs/desk-landing-design.md part 7): its
    /// window loaded from a clean copy of the commit, failing on a name missing or declared twice. Others have none.
    fn check(&self, repo: &str, commit: &str) -> Option<Result<(), String>> {
        super::desk_checks::builds_this_cue(repo).then(|| crate::live_ui::check(repo, commit))
    }
}

/// Notes which Cue is running, once, as the hub starts (the app's only: a test's hub has its own installer).
pub(super) fn note_running() {
    #[cfg(not(test))]
    crate::self_install::note_running();
}

/// One run of `key`'s finishing work on a hub (an item's commit, or "tick"), let go as it ends, a panic in it too.
pub(super) struct Claim<'a> {
    hub: &'a Hub,
    key: String,
}

impl<'a> Claim<'a> {
    /// The claim, unless a run of `key` holds it now.
    pub(super) fn take(hub: &'a Hub, key: &str) -> Option<Self> {
        let claimed = hub.write(|st| st.desk.finish_claims.insert(key.to_string()));
        claimed.then(|| Claim { hub, key: key.to_string() })
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let key = std::mem::take(&mut self.key);
        self.hub.write(|st| st.desk.finish_claims.remove(&key));
    }
}

/// Whether an item at `step` is in an install (begun, not yet seen to have landed).
fn installing(step: &str) -> bool {
    matches!(step, "building" | "installing" | "starting")
}

/// A finishing step stopped on its install: the build failed, or the new Cue didn't start.
fn stopped_install(step: &str) -> bool {
    matches!(step, "failed" | "rolled_back")
}

/// Why finishing item `id` can't be taken off its finish now, as the lead is told (#248): its commit is being made,
/// it waits on files or on main, or it waits for or is in an install. None when it stopped on a question: nothing of
/// its finish is under way then (a merge that clashed was aborted; commits it made stay, as Send back leaves them), so
/// it can go back to work. A commit that waited long enough to ask is still tried each tick: under way while it runs.
pub(super) fn under_way(st: &Store, id: &str) -> Option<String> {
    let it = st.desk.item(id).filter(|i| i.state == "finishing")?;
    let f = it.finish.as_ref()?;
    if st.desk.finish_claims.contains(id) {
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

/// The last `LOG_LINES` lines of `log`.
fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.trim_end().lines().collect();
    lines[lines.len().saturating_sub(LOG_LINES)..].join("\n")
}

impl Hub {
    /// The installer this hub's Desk uses.
    pub(super) fn installer(&self) -> std::sync::Arc<dyn Installer> {
        self.read(|st| st.desk.installer.0.clone()).unwrap_or_else(|| std::sync::Arc::new(AppInstaller))
    }

    /// Another installer in place of the app's own (a test's).
    #[cfg(test)]
    pub(crate) fn desk_set_installer(&self, i: std::sync::Arc<dyn Installer>) {
        self.write(|st| st.desk.installer.0 = Some(i));
    }

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
    pub(super) fn finish_set(&self, id: &str, f: impl FnOnce(&mut DeskItem)) -> Option<DeskItem> {
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
    fn finish_checked(&self, id: &str) {
        let now = now_ms();
        self.finish_set(id, |i| {
            i.state = "to_check".into();
            i.updated = super::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.wait) = ("checked".into(), now, String::new(), String::new());
            }
        });
        self.desk_close_on_check(id);
    }

    /// Item `id`, to check, closed as Looks right would when the Desk lead said DONE on it for you (#337,
    /// `DeskItem::close_on_check`); one still finishing waits for `finish_checked`.
    pub(super) fn desk_close_on_check(&self, id: &str) {
        // Stopped on a failed install: its landing run again on the main line as it is now (#343), closed once that's in.
        let stopped = self.read(|st| st.desk.item(id).is_some_and(|i| i.close_on_check && i.state == "finishing" && i.finish.as_ref().is_some_and(|f| stopped_install(&f.step) && !f.repo.is_empty())));
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
    pub(super) fn finish_fin(&self, id: &str, f: impl FnOnce(&mut Finish)) {
        self.finish_set(id, |i| {
            if let Some(fin) = i.finish.as_mut() {
                f(fin);
            }
        });
    }

    /// Item `id` at finishing step `step` from now.
    pub(super) fn finish_step(&self, id: &str, step: &str) {
        let now = now_ms();
        self.finish_fin(id, |f| (f.step, f.at, f.wait) = (step.to_string(), now, String::new()));
    }

    /// Item `id` stopped on `question` at `step`: shown as needing you.
    pub(super) fn finish_ask(&self, id: &str, step: &str, question: &str, log: &str) {
        let now = now_ms();
        self.finish_set(id, |i| {
            i.updated = super::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.log, f.now) = (step.into(), now, question.into(), tail(log), false);
            }
        });
    }

    /// Items queued after `id` start now (its commit is made, or there was none to make).
    pub(super) fn finish_release(&self, id: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            super::desk::release(st, id, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Item `id` just said DONE: in its own copy, it joins the landing line (hub/desk_trees.rs); else its commit made (with its crew's branch merged), then queued
    /// for an install when it changed this Cue, else to check. One run per item at a time.
    pub(super) fn desk_finish_commit(&self, id: &str) {
        let Some(_claim) = Claim::take(self, id) else { return };
        self.finish_commit(id);
    }

    fn finish_commit(&self, id: &str) {
        // In its own copy: it lands through the landing line, one item at a time.
        if self.read(|st| st.desk.item(id).is_some_and(|i| i.tree.is_some())) {
            return self.desk_land_line();
        }
        self.desk_finish_shared(id);
    }

    /// Item `id`, worked on in its holder's own folder (no copy of its own): its files there committed (those
    /// another session is editing or changed too left out, in the folder for that session), its crew's branch merged.
    pub(super) fn desk_finish_shared(&self, id: &str) {
        let Ok((it, machine, sources)) = self.desk_change_sources(id) else { return };
        let Some(_fin) = it.finish.clone().filter(|f| it.state == "finishing" && f.step == "commit") else { return };
        // A session on another machine: nothing to install here, and its work never closes uncommitted: to check with
        // no `finish`, so Looks right commits it there and Undo takes that back, as before. A project whose work you
        // commit yourself (lands.rs) goes the same way, and Looks right then commits nothing.
        if !machine.is_empty() || self.item_lands(&it).how == "owner" {
            self.finish_release(id);
            let now = now_ms();
            self.finish_set(id, |i| {
                (i.state, i.finish, i.updated) = ("to_check".into(), None, super::desk::secs(now));
            });
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
        self.finish_release(id);
        self.finish_after(id, &tops);
    }

    /// Item `it`'s folders on this Mac: its crew's (hub/desk.rs `item_sources`), then each folder of `sources`.
    fn item_folders(&self, it: &DeskItem, sources: &[(String, Vec<String>)]) -> Vec<String> {
        let mut out: Vec<String> = self.read(|st| super::desk::item_sources(st, it).1.into_iter().map(|(f, _)| f).collect());
        for (f, _) in sources {
            if !out.contains(f) {
                out.push(f.clone());
            }
        }
        out
    }

    /// Item `id`'s work git says isn't committed (each path in the folder it was changed in), asked as its holder's
    /// turn ends on it (#213). None on another machine: its folders aren't looked at here.
    pub(super) fn desk_uncommitted(&self, id: &str) -> Vec<String> {
        let Ok((_, machine, sources)) = self.desk_change_sources(id) else { return Vec::new() };
        if !machine.is_empty() {
            return Vec::new();
        }
        sources.iter().flat_map(|(folder, paths)| super::desk_commit::changed_of("", folder, paths).unwrap_or_default()).collect()
    }

    /// Item `it`'s files git says changed, by folder (one not in a repository left out), with only its own part of a
    /// file another session changed too (hub/desk_theirs.rs), and the files left out (absolute): a session at work is
    /// editing it now, or another's changes in it can't be told apart from the item's. They stay in the folder,
    /// uncommitted, for that session: an item's commit never waits on another's.
    fn to_commit(&self, it: &DeskItem, sources: &[(String, Vec<String>)]) -> (Vec<(String, Vec<String>)>, Vec<Part>, Vec<String>) {
        let busy = self.busy_by(it);
        let mut todo: Vec<(String, Vec<String>)> = Vec::new();
        let mut left: Vec<String> = Vec::new();
        for (folder, paths) in sources {
            let Some(changed) = super::desk_commit::changed_of("", folder, paths) else { continue };
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
        for folder in &crew {
            match merge_to_main(&project, folder, &it.title) {
                Ok(Some(main)) => {
                    if let Some(sha) = git("", &main, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) {
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
    pub(super) fn finish_after(&self, id: &str, tops: &[String]) {
        let now = now_ms();
        // Nothing new this round (its fix after a failed install was already committed): the main line it went to
        // before, so it's installed all the same.
        let before = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).map(|f| f.repo.clone())).unwrap_or_default();
        let tops: Vec<String> = if tops.is_empty() && !before.is_empty() { vec![before] } else { tops.to_vec() };
        // Its project set to nothing after landing (lands.rs): Cue's own work isn't installed or served.
        let project = self.read(|st| st.desk.item(id).map(|i| i.project.clone())).unwrap_or_default();
        let cue = tops.iter().find(|t| super::desk_checks::builds_this_cue(t) && crate::lands::lands_of(&project, t).after == "install");
        let top = cue.or(tops.first()).cloned().unwrap_or_default();
        let head = if top.is_empty() { String::new() } else { git("", &top, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()).unwrap_or_default() };
        // Not on main yet (the main checkout is on another branch, or a worktree's work wasn't merged): it waits
        // there, saying where, and goes on once it is (the tick looks again). Never to check, nor served, before.
        if !top.is_empty() {
            if let Some(at) = self.not_on_main(id, &top, &head) {
                return self.finish_landing(id, &top, &head, at);
            }
        }
        // On main: pushed now when its project pushes after each landing (hub/desk_pr.rs).
        if !top.is_empty() {
            self.desk_push_after(id, &project, &top);
        }
        // After landing, your own steps (lands.rs): in order in the project's main folder, each stopped past its cap;
        // the first that fails stops the rest, and the item waits for you, saying which. No lock is held here.
        if cue.is_none() && !top.is_empty() {
            let l = crate::lands::lands_of(&project, &top);
            if l.after == "command" {
                if let Err(e) = crate::lands::run_steps(&top, &l.after_steps) {
                    return self.finish_ask(id, "after_failed", &format!("It landed, but after landing \"{}\" {}", e.step, e.why), &e.said);
                }
            }
        }
        // Its own work changed nothing the app runs (mocks, notes, plans, tests): to check now, no install, whatever
        // else on the main line waits for one (#408).
        if cue.is_some() && self.runs_nothing(id, &top) {
            self.finish_set(id, |i| {
                if let Some(f) = i.finish.as_mut() {
                    (f.repo, f.commit) = (top.clone(), head.clone());
                }
            });
            return self.finish_checked(id);
        }
        // Only the window's files changed since the running Cue's build: they're served in place, no install.
        let in_place = if cue.is_some() { self.ui_in_place(&top, &head) } else { Ok(false) };
        if !matches!(in_place, Ok(false)) {
            self.finish_set(id, |i| {
                if let Some(f) = i.finish.as_mut() {
                    (f.repo, f.commit) = (top.clone(), head.clone());
                }
            });
            // Its window doesn't load: back to its holder with what's wrong, as a failed install is.
            if let Err(said) = in_place {
                if let Some(it) = self.read(|st| st.desk.item(id).cloned()) {
                    self.finish_failed(&it, WINDOW_BROKEN, "failed", &said);
                }
                return;
            }
            return self.finish_checked(id);
        }
        let queued = cue.is_some();
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
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

    /// Whether item `id`'s own commits on `top`'s main line (those Cue made for it, its crew's merges) changed nothing
    /// that goes into the app (live_ui::outside_app). With none there, its files in that repository say: each outside
    /// the app or never in it (a mocks item's scratch files, read as the repository's, #432). False when git can't
    /// say, or it has a file in the app (work its holder committed itself): it's installed as before.
    fn runs_nothing(&self, id: &str, top: &str) -> bool {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return false };
        let merges = it.finish.as_ref().map(|f| f.merges.clone()).unwrap_or_default();
        let here = |folder: &str| main_checkout(folder).is_some_and(|(m, _)| same_dir(&m, top));
        let own: Vec<String> = it.commit_in.iter().chain(&merges).filter(|(folder, _)| here(folder)).map(|(_, sha)| sha.clone()).collect();
        if !own.is_empty() {
            return own.iter().all(|sha| crate::live_ui::changed_in(top, sha).is_some_and(|paths| paths.iter().all(|p| crate::live_ui::outside_app(p))));
        }
        let Ok((_, machine, sources)) = self.desk_change_sources(id) else { return false };
        let files: Vec<(String, String)> = sources.into_iter().filter(|(folder, _)| machine.is_empty() && here(folder)).flat_map(|(folder, paths)| paths.into_iter().map(move |p| (folder.clone(), p))).collect();
        !files.is_empty() && files.iter().all(|(folder, p)| crate::live_ui::outside_app(p) || never_in(folder, p))
    }

    /// Where item `id`'s work is, when it isn't all on `top`'s main yet ("on fix-header, not main"): the main line's
    /// `head`, each commit Cue made for it in that repository, and the HEAD of each other worktree of it the item
    /// worked in (merged into the main checkout as it finished). None once all are on main, or when the repository
    /// has no main to look at (no main or master branch, and its main checkout on none).
    fn not_on_main(&self, id: &str, top: &str, head: &str) -> Option<String> {
        let main = main_ref(top)?;
        let Ok((it, _, sources)) = self.desk_change_sources(id) else { return None };
        let mut shas: Vec<String> = Vec::new();
        for (folder, sha) in &it.commit_in {
            if main_checkout(folder).is_some_and(|(m, _)| same_dir(&m, top)) {
                shas.push(sha.clone());
            }
        }
        for folder in self.item_folders(&it, &sources) {
            let Some((m, _)) = main_checkout(&folder).filter(|(m, _)| same_dir(m, top)) else { continue };
            let own = git("", &folder, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()).unwrap_or_default();
            if own.is_empty() || same_dir(&own, &m) {
                continue;
            }
            if let Some(h) = git("", &own, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) {
                shas.push(h);
            }
        }
        if !head.is_empty() {
            shas.push(head.to_string());
        }
        let off = shas.iter().find(|s| !matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", s, &main]), Ok((_, 0))))?;
        let on = git("", top, &[], &["branch", "--contains", off, "--format=%(refname:short)"]).unwrap_or_default();
        let branch = on.lines().map(str::trim).find(|b| !b.is_empty() && *b != main).map(str::to_string);
        Some(match branch {
            Some(b) => format!("on {b}, not {main}"),
            None => format!("not on {main} or any branch"),
        })
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
                (f.step, f.repo, f.commit, f.wait, f.wait_from, f.question) = ("landing".into(), top.into(), head.into(), at, from, question);
            }
        });
    }

    /// Whether `head` of `repo` is in the running Cue without an install: what changed since the commit it was built
    /// from is only the window's files (then served from `head`, and the window told to reload: "ui-reload") or
    /// nothing it shows. Not while an install is under way or queued: the main line then has more than the window's
    /// files, or it soon will be in the app anyway. Err: the window at `head` doesn't pass its check
    /// (live_ui::check), what's wrong said; nothing is served.
    fn ui_in_place(&self, repo: &str, head: &str) -> Result<bool, String> {
        let Some(running) = crate::live_ui::running() else { return Ok(false) };
        let busy = self.read(|st| st.desk.items.iter().any(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "queued" || installing(&f.step))));
        if busy {
            return Ok(false);
        }
        let served = crate::live_ui::current().filter(|c| c.base == running).map(|c| c.commit);
        match crate::live_ui::plan(repo, &running, served.as_deref(), head) {
            crate::live_ui::Plan::Install => Ok(false),
            crate::live_ui::Plan::Nothing => Ok(true),
            crate::live_ui::Plan::Reload => {
                crate::live_ui::check(repo, head)?;
                match crate::live_ui::take(repo, head, &running) {
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
    pub(super) fn save_made(&self, id: &str, made: &[(String, String)]) {
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
    pub(super) fn desk_finish_tick(&self) {
        let Some(_claim) = Claim::take(self, "tick") else { return };
        self.finish_tick();
        // Pull requests open for review: read once a minute (hub/desk_pr.rs).
        self.desk_pr_tick();
        self.desk_trees_catch_up();
        self.desk_old_copies_tick();
        self.taken_out_tick();
        // Circles the waits list has now told to their lead, gone ones let go (#250).
        self.circle_check();
    }

    /// Commits new on a main line since Cue last looked, each checked against the finished items whose commits are
    /// on it (to check, finishing, or done this week): one that takes most of an item's added lines back out (a
    /// revert, a file put back as it was) sends the item back to its holder, saying which commit did (#213).
    fn taken_out_tick(&self) {
        let now = now_ms();
        let week_ago = super::desk::secs(now).saturating_sub(7 * 24 * 3600);
        // Only items finished here at a DONE (`finish`): their commits are in this Mac's folders.
        let items: Vec<DeskItem> = self.read(|st| {
            let open = |i: &DeskItem| matches!(i.state.as_str(), "to_check" | "finishing") || (i.state == "done" && i.done_at.is_some_and(|d| d >= week_ago));
            st.desk.items.iter().filter(|i| i.finish.is_some() && !i.commit_in.is_empty() && open(i)).cloned().collect()
        });
        if items.is_empty() {
            return;
        }
        // Each item's main checkouts, and each main checkout's HEAD now.
        let mut tops: Vec<(String, String)> = Vec::new();
        let mut of_folder: Vec<(String, Option<String>)> = Vec::new();
        let mut of_item: Vec<(&DeskItem, Vec<String>)> = Vec::new();
        for it in &items {
            let mut mine: Vec<String> = Vec::new();
            for (folder, _) in &it.commit_in {
                if !of_folder.iter().any(|(f, _)| f == folder) {
                    of_folder.push((folder.clone(), main_checkout(folder).map(|(t, _)| t)));
                }
                let Some(top) = of_folder.iter().find(|(f, _)| f == folder).and_then(|(_, t)| t.clone()) else { continue };
                if !tops.iter().any(|(t, _)| *t == top) {
                    let Some(head) = git("", &top, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) else { continue };
                    tops.push((top.clone(), head));
                }
                if !mine.contains(&top) {
                    mine.push(top);
                }
            }
            of_item.push((it, mine));
        }
        let seen: Vec<(String, String, Option<String>)> = self.write(|st| {
            tops.iter().map(|(top, head)| (top.clone(), head.clone(), st.desk.seen_heads.insert(top.clone(), head.clone()))).collect()
        });
        let mut taken: Vec<(String, String)> = Vec::new();
        for (top, head, before) in seen {
            let Some(before) = before.filter(|b| *b != head) else { continue };
            let list = git("", &top, &[], &["rev-list", "--no-merges", "--reverse", "--max-count=50", &format!("{before}..{head}")]).unwrap_or_default();
            let new: Vec<&str> = list.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if new.is_empty() {
                continue;
            }
            for (it, _) in of_item.iter().filter(|(_, m)| m.contains(&top)) {
                if taken.iter().any(|(id, _)| *id == it.id) {
                    continue;
                }
                if let Some(by) = taken_out_by(&top, &head, it, &new) {
                    taken.push((it.id.clone(), by));
                }
            }
        }
        for (id, by) in taken {
            self.desk_taken_out(&id, &by);
        }
    }

    /// Item `id`'s work was taken back out by commit `by` ("<short> (<its subject>)"): sent back to its holder, as
    /// Send back does, saying so on its page and to its holder.
    fn desk_taken_out(&self, id: &str, by: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            let Some(i) = st.desk.item_mut(id) else { return fx };
            let h = i.holder.clone();
            let what = format!("Commit {by} took its work back out");
            (i.state, i.done_at, i.finish, i.updated) = (if h.is_some() { "working" } else { "new" }.into(), None, None, super::desk::secs(now));
            if let Some(c) = i.checks.last_mut() {
                c.back = what.clone();
            }
            let (title, project) = (i.title.clone(), i.project.clone());
            let whom = match &h {
                Some(h) => {
                    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
                    fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({title}) is open again: {what}. Put it back, then say DONE: #{id}.\nIts fixes stay in #{id}; don't add new items for them.")));
                    format!("sent back to {}", h.name)
                }
                None => "open again".to_string(),
            };
            super::desk::decide(st, &project, Some(id.to_string()), "Cue", "", &what, &whom, now, &mut fx);
            if let Some(d) = st.desk.decisions.last_mut() {
                d.undo = "none".into();
            }
            fx.items.push(id.to_string());
            fx
        });
        self.desk_done(fx);
    }

    fn finish_tick(&self) {
        // An older Cue's commit wait (gone): committed as any item said DONE.
        let old: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "commit_wait")).map(|i| i.id.clone()).collect());
        for id in old {
            self.finish_fin(&id, |f| (f.step, f.wait) = ("commit".into(), String::new()));
        }
        let items: Vec<DeskItem> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.finish.is_some()).cloned().collect());
        if items.is_empty() {
            return;
        }
        let step = |i: &DeskItem| i.finish.as_ref().map(|f| f.step.clone()).unwrap_or_default();
        for it in &items {
            match step(it).as_str() {
                "commit" if it.tree.is_none() => self.desk_finish_commit(&it.id),
                // In its own copy: the landing line takes it (below).
                "commit" | "line" | "checking" => {}
                // Waiting to be on main: on its way once it is (its main line as it is now).
                "landing" => drop(Claim::take(self, &it.id).map(|_claim| self.finish_after(&it.id, &[]))),
                _ => {}
            }
        }
        if items.iter().any(|i| i.tree.is_some() && matches!(step(i).as_str(), "commit" | "line" | "checking")) {
            self.desk_land_line();
        }
        let inst = self.installer();
        let status = inst.status();
        let now = now_ms();
        let mut running = status.as_ref().is_some_and(|s| installing(&s.step));
        for it in &items {
            // Stopped on a failed install too: a later install (main fixed, another item's) brought its work into the
            // running Cue, so nothing of its is left to install (#343).
            let Some(f) = it.finish.as_ref().filter(|f| f.step == "queued" || installing(&f.step) || stopped_install(&f.step)) else { continue };
            if !f.commit.is_empty() && inst.has(&f.repo, &f.commit) {
                self.finish_checked(&it.id);
                continue;
            }
            if stopped_install(&f.step) {
                continue;
            }
            if running && f.step == "queued" && self.finish_join(&it.id, f, status.as_ref(), now) {
                continue;
            }
            if !installing(&f.step) {
                continue;
            }
            self.finish_follow(it, f, status.as_ref(), running, now);
        }
        // Still building one of ours (or anything): the next waits for it.
        running = running || self.read(|st| st.desk.items.iter().any(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| installing(&f.step))));
        if !running {
            self.finish_install_due(now);
        }
    }

    /// Item `it`, in an install (`f`): its step as the installer says it (`status`), stopping on a question when it
    /// failed; queued again when the installer has no word of it and nothing is `running`.
    fn finish_follow(&self, it: &DeskItem, f: &Finish, status: Option<&InstallStatus>, running: bool, now: u64) {
        match status.filter(|s| s.tag == f.tag) {
            Some(s) if s.step == "failed" && s.log.contains("refused: the window") => self.finish_failed(it, WINDOW_BROKEN, "failed", &s.log),
            Some(s) if s.step == "failed" => self.finish_failed(it, BUILD_FAILED, "failed", &s.log),
            Some(s) if s.step == "rolled_back" => self.finish_failed(it, ROLLED_BACK, "rolled_back", &s.log),
            // Installed, and this Cue doesn't have it yet: the new one is starting.
            Some(s) => {
                let to = if s.step == "installed" { "starting" } else { s.step.as_str() };
                if f.step != to {
                    let to = to.to_string();
                    self.finish_set(&it.id, |i| {
                        if let Some(f) = i.finish.as_mut() {
                            (f.step, f.at) = (to, now);
                        }
                    });
                }
            }
            // The installer has no word of its install (it was lost, or another began since): queued again.
            None if !running => {
                self.finish_set(&it.id, |i| {
                    if let Some(f) = i.finish.as_mut() {
                        (f.step, f.tag) = ("queued".into(), String::new());
                    }
                });
            }
            None => {}
        }
    }

    /// Item `id`, waiting (`f`) while the install `status` runs, goes with that install when its commit has the item's
    /// (the Desk's own, or one a session began): never waiting for it to end to start another. Whether it did.
    fn finish_join(&self, id: &str, f: &Finish, status: Option<&InstallStatus>, now: u64) -> bool {
        let Some(s) = status.filter(|s| installing(&s.step) && !f.commit.is_empty()) else { return false };
        if !self.installer().contains(&f.repo, &f.commit, &s.commit) {
            return false;
        }
        let tag = s.tag.clone();
        let to = s.step.clone();
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.tag, f.now) = (to, now, tag, false);
            }
        });
        true
    }

    /// The next install, when items are queued and it's due: half an hour after the first of them, and with no
    /// session that ends with Cue mid-turn; at once when you said Install now.
    fn finish_install_due(&self, now: u64) {
        let queued: Vec<DeskItem> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "queued")).cloned().collect());
        let Some(repo) = queued.iter().filter_map(|i| i.finish.as_ref()).map(|f| f.repo.clone()).find(|r| !r.is_empty()) else { return };
        let batch: Vec<&DeskItem> = queued.iter().filter(|i| i.finish.as_ref().is_some_and(|f| f.repo == repo)).collect();
        let fin = |i: &&DeskItem| i.finish.clone().unwrap_or_default();
        let now_said = batch.iter().any(|i| fin(i).now);
        let first = batch.iter().map(|i| fin(i).at).min().unwrap_or(0);
        if !now_said {
            if now.saturating_sub(first) < INSTALL_EVERY_MS {
                return;
            }
            if self.ends_with_cue().iter().any(|e| e.mid_turn) {
                return;
            }
        }
        let Some(head) = git("", &repo, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) else { return };
        let tag = format!("desk-{now}");
        let started = self.installer().start(&repo, &head, &tag);
        for it in batch {
            match &started {
                Ok(()) => {
                    let tag = tag.clone();
                    let head = head.clone();
                    // The commit built is the item's now: main as it is, a fix made since it last failed in it (#343).
                    self.finish_set(&it.id, |i| {
                        if let Some(f) = i.finish.as_mut() {
                            (f.step, f.at, f.tag, f.now, f.commit) = ("building".into(), now, tag, false, head);
                        }
                    });
                }
                Err(e) => self.finish_failed(it, &format!("{NOT_STARTED}: {e}"), "failed", e),
            }
        }
    }

    /// Item `it`'s install failed (`step`): it stops on `question` with the log's end kept, and goes back to its
    /// holder with the log, as Send back does (its next DONE finishes it again).
    pub(super) fn finish_failed(&self, it: &DeskItem, question: &str, step: &str, log: &str) {
        self.finish_back(it, question, step, log, false);
    }

    /// A clash with the main line in item `it`'s work (#371): handed back to the session that did it, with what
    /// clashed and how to settle it: at work again, nothing waiting on you. Without a holder it waits on you.
    pub(super) fn finish_clash(&self, it: &DeskItem, question: &str) {
        self.finish_back(it, question, "conflict", "", true);
    }

    /// `finish_failed`, and with `hand_back` the item at work again (as Send back leaves it) instead of waiting on you.
    fn finish_back(&self, it: &DeskItem, question: &str, step: &str, log: &str, hand_back: bool) {
        self.finish_ask(&it.id, step, question, log);
        let Some(h) = it.holder.clone() else { return };
        let now = now_ms();
        let log = tail(log);
        let id = it.id.clone();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            let Some(i) = st.desk.item_mut(&id) else { return fx };
            if let Some(c) = i.checks.last_mut() {
                c.back = question.to_string();
            }
            if hand_back {
                (i.state, i.done_at, i.finish, i.close_on_check) = ("working".into(), None, None, false);
            }
            let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
            let log = if log.is_empty() { String::new() } else { format!("\nThe end of its log:\n{log}") };
            fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({}) isn't done: {question}.{log}\nIts fixes stay in #{id}; don't add new items for them. Say DONE: #{id} when it is.", i.title)));
            let project = i.project.clone();
            super::desk::decide(st, &project, Some(id.clone()), "Cue", "", question, &format!("sent back to {}", h.name), now, &mut fx);
            if let Some(d) = st.desk.decisions.last_mut() {
                d.undo = "none".into();
            }
            fx.items.push(id.clone());
            fx
        });
        self.desk_done(fx);
    }

    /// Install now (the Desk's, or on item `id`, in the next install): the install of everything waiting starts
    /// without waiting for its half hour or for sessions mid-turn.
    pub(crate) fn desk_install_now(&self, id: &str) -> Result<DeskItem, String> {
        let queued = self.read(|st| st.desk.item(id).map(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "queued"))).ok_or("no such item")?;
        if !queued {
            return Err("It isn't waiting for an install".into());
        }
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                f.now = true;
            }
        });
        self.desk_finish_tick();
        let it = self.read(|st| st.desk.item(id).map(|i| super::desk::shown(st, i)));
        it.ok_or_else(|| "no such item".into())
    }
}

impl Hub {
    /// Run the landing again on item `id`, stopped on a question while finishing: a plain retry, offered only when
    /// Cue can name nothing in its way (#250; what it can name has its own offer, desk_waits.rs): an install that
    /// failed is queued again; anything else is committed again, what an earlier round made kept.
    pub(crate) fn desk_rerun_landing(&self, id: &str) -> Result<DeskItem, String> {
        let at = self.read(|st| st.desk.item(id).map(|i| i.finish.clone().filter(|_| i.state == "finishing"))).ok_or("no such item")?;
        // In review: its pull request is open, so nothing is pushed or opened again; it lands once that's merged.
        if at.as_ref().is_some_and(|f| f.step == "review") {
            return Err("Its pull request is open for review: it lands once that's merged".into());
        }
        let at = at.filter(|f| !f.question.is_empty());
        let Some(fin) = at else { return Err("It isn't stopped on a question".into()) };
        let now = now_ms();
        let install = stopped_install(&fin.step) && !fin.repo.is_empty();
        self.finish_set(id, |i| {
            i.updated = super::desk::secs(now);
            i.finish = Some(match install {
                true => Finish { step: "queued".into(), at: now, ..begun(now, Some(&fin)) },
                // A failed check retried: back in the landing line in its place.
                false if i.tree.is_some() && fin.step == "check_failed" => Finish { step: "line".into(), at: fin.at, ..begun(now, Some(&fin)) },
                // Its branch not pushed or its pull request not opened: back in the line, its check kept; a pull
                // request closed unmerged: opened again from its copy as it is.
                false if i.tree.is_some() && fin.step == "pr_failed" => Finish { step: "line".into(), at: fin.at, checked: fin.checked.clone(), unchecked: fin.unchecked, ..begun(now, Some(&fin)) },
                false if i.tree.is_some() && fin.step == "pr_closed" => Finish { step: "line".into(), at: fin.at, ..begun(now, Some(&fin)) },
                false => begun(now, Some(&fin)),
            });
        });
        if install {
            self.desk_finish_tick();
        } else {
            self.desk_finish_commit(id);
        }
        let it = self.read(|st| st.desk.item(id).map(|i| super::desk::shown(st, i)));
        it.ok_or_else(|| "no such item".into())
    }
}

/// A finishing item as it's said DONE (or retried): its commit to be made, the main line and commit of an earlier
/// round (`before`) kept, so a round with nothing new still installs them.
/// When the next install is due (ms), the first item waiting for it plus half an hour; 0 when none waits.
pub(super) fn install_at(st: &Store) -> u64 {
    let first = st.desk.items.iter().filter(|i| i.state == "finishing").filter_map(|i| i.finish.as_ref()).filter(|f| f.step == "queued").map(|f| f.at).min();
    first.map_or(0, |at| at + INSTALL_EVERY_MS)
}

/// Whether `path` (relative to `folder`) was never in its main line: in no commit of it (a scratch or ignored file
/// lands nothing). False when git can't say.
fn never_in(folder: &str, path: &str) -> bool {
    git("", folder, &[], &["log", "-1", "--format=%H", "HEAD", "--", path]).is_some_and(|o| o.trim().is_empty())
}

pub(super) fn begun(now: u64, before: Option<&Finish>) -> Finish {
    let (repo, commit) = before.map(|f| (f.repo.clone(), f.commit.clone())).unwrap_or_default();
    Finish { step: "commit".into(), at: now, repo, commit, ..Default::default() }
}

/// The branch that is main in `top`'s repository: "main", else "master", else the branch its main checkout is on.
/// None when there's none of those (a detached main checkout in a repository with neither).
pub(super) fn main_ref(top: &str) -> Option<String> {
    for b in ["main", "master"] {
        if git("", top, &[], &["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")]).is_some() {
            return Some(b.to_string());
        }
    }
    main_checkout(top).map(|(_, b)| b).filter(|b| !b.is_empty())
}

/// `folder`'s repository's main checkout (its first worktree) and the branch it's on ("" for a detached HEAD).
pub(super) fn main_checkout(folder: &str) -> Option<(String, String)> {
    let list = git("", folder, &[], &["worktree", "list", "--porcelain"])?;
    let mut lines = list.lines();
    let top = lines.next()?.strip_prefix("worktree ")?.to_string();
    let branch = lines.take_while(|l| !l.is_empty()).find_map(|l| l.strip_prefix("branch refs/heads/")).unwrap_or("").to_string();
    Some((top, branch))
}

/// `folder`'s work merged into its repository's main checkout, when it's another worktree on a branch with commits
/// the main checkout's branch doesn't have: tried first without touching any file (`git merge-tree`), its added
/// lines word-checked, then merged there (`--no-ff`). The main checkout, when merged; None when there was nothing
/// to merge; else the step and question to stop on ("clash": its work clashes with the main line, back to its
/// session; a word, or git refusing: nothing merged).
fn merge_to_main(project: &str, folder: &str, title: &str) -> Result<Option<String>, (&'static str, String)> {
    let Some(top) = git("", folder, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()) else { return Ok(None) };
    let Some((main, onto)) = main_checkout(folder) else { return Ok(None) };
    if same_dir(&top, &main) || onto.is_empty() {
        return Ok(None);
    }
    let Some(sha) = git("", &top, &[], &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()) else { return Ok(None) };
    let branch = git("", &top, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty() && *b != onto).unwrap_or_else(|| sha[..sha.len().min(8)].to_string());
    let ahead = git("", &main, &[], &["rev-list", "--count", &format!("{onto}..{sha}")]).and_then(|n| n.trim().parse::<u64>().ok()).unwrap_or(0);
    if ahead == 0 {
        return Ok(None);
    }
    let filters = crate::safe_git::no_repo_filters("", &main);
    let tried = crate::safe_git::run_env("", &main, &filters, &[], &["merge-tree", "--write-tree", "--name-only", &onto, &sha]);
    let (out, code) = tried.map_err(|e| ("conflict", format!("Not merged: git couldn't try the merge of {branch} into {onto}: {e}")))?;
    if code != 0 {
        let files: Vec<&str> = out.lines().skip(1).take_while(|l| !l.is_empty()).collect();
        return Err(("clash", format!("Its work clashes with {onto} in {}: in {top}, merge {onto} into {branch}, settle the clash there, and say DONE again", files.join(", "))));
    }
    let tree = out.lines().next().unwrap_or("").trim().to_string();
    let diff = git("", &main, &[], &["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", &onto, &tree]).ok_or(("conflict", format!("Not merged: git couldn't read the merge of {branch}")))?;
    let words = crate::config::never_words(project);
    if let Some((w, at)) = word_in(&diff, &words) {
        return Err(("word", format!("Not merged: \"{w}\" is in {at}")));
    }
    let message = format!("Merge {branch}: {title}");
    if let Some(w) = words.iter().find(|w| message.to_lowercase().contains(&w.to_lowercase())) {
        return Err(("word", format!("Not merged: \"{w}\" is in the merge's message")));
    }
    if let Some(what) = in_progress(&main) {
        return Err(("conflict", format!("Not merged: the main checkout is in the middle of {what}; finish it there first")));
    }
    if let Err(why) = git_said("", &main, &[], &["merge", "--no-ff", "--no-edit", "-q", "-m", &message, &sha]) {
        // Refused before it began (a file there with changes of its own, a hook), or stopped: left as it was. Only
        // this merge is undone (none was in progress before it). git's own words say which.
        if in_progress(&main).is_some() {
            git("", &main, &[], &["merge", "--abort"]);
        }
        let what = format!("Not merged: git wouldn't merge {branch} into {onto} in the main checkout");
        return Err(("conflict", super::desk_commit::because(what, &why)));
    }
    Ok(Some(main))
}

/// Lines worth matching in a diff: with a letter or digit, four characters or more once trimmed (not a lone brace).
fn counts(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() >= 4 && t.chars().any(char::is_alphanumeric)
}

/// Commit `sha`'s added ('+') or removed ('-') lines, by file (its path after the change), trimmed.
fn lines_of(top: &str, sha: &str, sign: char) -> Vec<(String, String)> {
    let diff = git("", top, &[], &["show", "-U0", "--format=", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", sha]).unwrap_or_default();
    let mut file = String::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("diff --git ") {
            file = rest.rsplit_once(" b/").map(|(_, b)| b.to_string()).unwrap_or_default();
        } else if l.starts_with("+++ ") || l.starts_with("--- ") {
            continue;
        } else if let Some(body) = l.strip_prefix(sign) {
            if counts(body) {
                out.push((file.clone(), body.trim().to_string()));
            }
        }
    }
    out
}

/// The first of `new` (commits on `top`'s main line, oldest first; `head` the newest) that takes item `it`'s work
/// back out: made after it, not its own, removing at least half its added lines, which `head` no longer has.
/// "<short> (<subject>)", when one does.
fn taken_out_by(top: &str, head: &str, it: &DeskItem, new: &[&str]) -> Option<String> {
    let shas: Vec<&str> = it.commit_in.iter().map(|(_, s)| s.as_str()).collect();
    let is_ancestor = |a: &str, b: &str| matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", a, b]), Ok((_, 0)));
    // Its commits on this main line (a commit taken back by Undo, or in another repository, isn't).
    let on: Vec<&str> = shas.iter().copied().filter(|s| is_ancestor(s, head)).collect();
    if on.is_empty() {
        return None;
    }
    let added: Vec<(String, String)> = on.iter().flat_map(|s| lines_of(top, s, '+')).collect();
    if added.is_empty() {
        return None;
    }
    for c in new {
        if shas.iter().any(|s| c.starts_with(s)) || !on.iter().all(|s| is_ancestor(s, c)) {
            continue;
        }
        let removed = lines_of(top, c, '-');
        if removed.is_empty() {
            continue;
        }
        let mut now_in: Vec<(String, Vec<String>)> = Vec::new();
        let mut gone = 0;
        for (file, line) in &added {
            if !removed.iter().any(|(f, l)| f == file && l == line) {
                continue;
            }
            if !now_in.iter().any(|(f, _)| f == file) {
                let text = git("", top, &[], &["show", &format!("{head}:{file}")]).unwrap_or_default();
                now_in.push((file.clone(), text.lines().map(|l| l.trim().to_string()).collect()));
            }
            if now_in.iter().find(|(f, _)| f == file).is_some_and(|(_, ls)| !ls.contains(line)) {
                gone += 1;
            }
        }
        if gone > 0 && gone * 2 >= added.len() {
            let short = git("", top, &[], &["log", "-1", "--format=%h (%s)", c]).map(|l| l.trim().to_string()).unwrap_or_else(|| c[..c.len().min(8)].to_string());
            return Some(short);
        }
    }
    None
}

/// What `main`'s checkout is in the middle of, when it is: a merge, a rebase, a cherry-pick or a revert.
pub(super) fn in_progress(main: &str) -> Option<&'static str> {
    let path = |p: &str| -> Option<std::path::PathBuf> {
        let at = git("", main, &[], &["rev-parse", "--git-path", p])?.trim().to_string();
        let at = std::path::PathBuf::from(at);
        Some(if at.is_absolute() { at } else { std::path::Path::new(main).join(at) })
    };
    let marks = [("MERGE_HEAD", "a merge"), ("rebase-merge", "a rebase"), ("rebase-apply", "a rebase"), ("CHERRY_PICK_HEAD", "a cherry-pick"), ("REVERT_HEAD", "a revert")];
    marks.into_iter().find(|(p, _)| path(p).is_some_and(|f| f.exists())).map(|(_, what)| what)
}

/// Whether two folders are the same, once their links are followed.
pub(super) fn same_dir(a: &str, b: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| p.trim_end_matches('/').to_string());
    canon(a) == canon(b)
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn a_logs_tail_keeps_its_last_lines() {
        let log: String = (1..=100).map(|n| format!("line {n}\n")).collect();
        let t = tail(&log);
        assert_eq!((t.lines().count(), t.lines().next(), t.lines().last()), (40, Some("line 61"), Some("line 100")));
        assert_eq!(tail("one\ntwo\n"), "one\ntwo");
    }
}
