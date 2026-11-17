//! Done means it's in your app: once its holder says DONE, an item is "finishing" (`DeskItem::finish`, saved with it,
//! so across the restart an install makes too). Its files are committed at once (Looks right's commit code: only its
//! files, the word check, a wait while another session at work edits one of them, never pushed); its crew's branch,
//! when its work sits on one, is merged into the main checkout's branch, the merge word-checked too and never forced.
//! An item that changed the Cue this app is built from is then queued for an install; any other goes to check.
//! Installs run one at a time, for every item queued within a couple of minutes of the last, built from the main
//! line's HEAD, and not while a session that ends with Cue is mid-turn (unless you say Install now). Once the running
//! Cue has an item's commit, the item is to check. A build that fails, or a new Cue that doesn't start, stops on a
//! question for you, and its holder is sent the log.

use super::desk_commit::{commit_each, git, shas_of, word_in, words_checked};
use super::*;
use crate::desk::{DeskItem, Finish, InstallStatus, Installer};

/// How long a commit waits on files another session is editing before you're asked about it.
pub(super) const WAIT_ASK_MS: u64 = 15 * 60_000;
/// How long an install waits after the latest item queued for it, so items done close together go in one build.
pub(super) const BATCH_MS: u64 = 2 * 60_000;
/// How much of an install's log an item keeps (lines, from its end).
const LOG_LINES: usize = 40;

/// The questions a finishing item can stop on.
pub(crate) const BUILD_FAILED: &str = "The build failed";
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
}

/// Notes which Cue is running, once, as the hub starts (the app's only: a test's hub has its own installer).
pub(super) fn note_running() {
    #[cfg(not(test))]
    crate::self_install::note_running();
}

/// One run of `key`'s finishing work on a hub (an item's commit, or "tick"), let go as it ends, a panic in it too.
struct Claim<'a> {
    hub: &'a Hub,
    key: String,
}

impl<'a> Claim<'a> {
    /// The claim, unless a run of `key` holds it now.
    fn take(hub: &'a Hub, key: &str) -> Option<Self> {
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

/// The last `LOG_LINES` lines of `log`.
fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.trim_end().lines().collect();
    lines[lines.len().saturating_sub(LOG_LINES)..].join("\n")
}

impl Hub {
    /// The installer this hub's Desk uses.
    fn installer(&self) -> std::sync::Arc<dyn Installer> {
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
    fn finish_set(&self, id: &str, f: impl FnOnce(&mut DeskItem)) -> Option<DeskItem> {
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

    /// Item `id` to check: nothing (more) to install for it.
    fn finish_checked(&self, id: &str) {
        let now = now_ms();
        self.finish_set(id, |i| {
            i.state = "to_check".into();
            i.updated = super::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.wait) = ("checked".into(), now, String::new(), String::new());
            }
        });
    }

    /// Item `id` stopped on `question` at `step`: shown as needing you.
    fn finish_ask(&self, id: &str, step: &str, question: &str, log: &str) {
        let now = now_ms();
        self.finish_set(id, |i| {
            i.updated = super::desk::secs(now);
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.at, f.question, f.log, f.now) = (step.into(), now, question.into(), tail(log), false);
            }
        });
    }

    /// Items queued after `id` start now (its commit is made, or there was none to make).
    fn finish_release(&self, id: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            super::desk::release(st, id, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Item `id` just said DONE (or its commit waits): its commit made (with its crew's branch merged), then queued
    /// for an install when it changed this Cue, else to check. One run per item at a time.
    pub(super) fn desk_finish_commit(&self, id: &str) {
        let Some(_claim) = Claim::take(self, id) else { return };
        self.finish_commit(id);
    }

    fn finish_commit(&self, id: &str) {
        let Ok((it, machine, sources)) = self.desk_change_sources(id) else { return };
        let Some(fin) = it.finish.clone().filter(|f| it.state == "finishing" && matches!(f.step.as_str(), "commit" | "commit_wait")) else { return };
        // A session on another machine: nothing to install here, and its work never closes uncommitted: to check with
        // no `finish`, so Looks right commits it there and Undo takes that back, as before.
        if !machine.is_empty() {
            self.finish_release(id);
            let now = now_ms();
            self.finish_set(id, |i| {
                (i.state, i.finish, i.updated) = ("to_check".into(), None, super::desk::secs(now));
            });
            return;
        }
        let (todo, waiting) = self.to_commit(&it, &sources);
        if !waiting.is_empty() {
            return self.finish_wait(id, &fin, waiting.join(", "));
        }
        let Some(tops) = self.finish_land(&it, &todo) else { return };
        self.finish_release(id);
        self.finish_after(id, &tops);
    }

    /// Item `it`'s files git says changed, by folder (one not in a repository left out), and which of them a session
    /// at work is editing now ("<who> is editing <path>").
    fn to_commit(&self, it: &DeskItem, sources: &[(String, Vec<String>)]) -> (Vec<(String, Vec<String>)>, Vec<String>) {
        let busy = self.busy_files(it);
        let mut todo: Vec<(String, Vec<String>)> = Vec::new();
        let mut waiting: Vec<String> = Vec::new();
        for (folder, paths) in sources {
            let Some(changed) = super::desk_commit::changed_of("", folder, paths) else { continue };
            for p in &changed {
                let abs = format!("{}/{p}", folder.trim_end_matches('/'));
                if let Some((_, by)) = busy.iter().find(|(f, _)| *f == abs) {
                    waiting.push(format!("{by} is editing {p}"));
                }
            }
            if !changed.is_empty() {
                todo.push((folder.clone(), changed));
            }
        }
        (todo, waiting)
    }

    /// Item `id`'s commit waits on `wait`: kept so, with a question for you once it has waited 15 minutes.
    fn finish_wait(&self, id: &str, fin: &Finish, wait: String) {
        let now = now_ms();
        let from = if fin.wait_from > 0 { fin.wait_from } else { now };
        let ask = now.saturating_sub(from) >= WAIT_ASK_MS;
        if fin.step == "commit_wait" && fin.wait == wait && ask != fin.question.is_empty() {
            return;
        }
        let first = fin.step != "commit_wait";
        self.finish_set(id, |i| {
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.wait, f.wait_from) = ("commit_wait".into(), wait.clone(), from);
                f.question = if ask { format!("Its commit has waited 15 minutes: {wait}") } else { String::new() };
                if first {
                    f.at = now;
                }
            }
        });
    }

    /// Item `it`'s `todo` committed, and its crew's work on a branch of its own merged into the main checkout's
    /// branch: each main checkout its work went to. None when it stopped on a question (a word, a conflict).
    fn finish_land(&self, it: &DeskItem, todo: &[(String, Vec<String>)]) -> Option<Vec<String>> {
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        let crew: Vec<String> = self.read(|st| super::desk::item_sources(st, it).1.into_iter().map(|(f, _)| f).collect());
        if !todo.is_empty() {
            if let Err(e) = words_checked(&project, "", todo, &it.title) {
                self.finish_ask(&it.id, if e.contains("\" is in ") { "word" } else { "error" }, &e, "");
                return None;
            }
        }
        // Each commit made is kept, a later one failing or not (its card, diff and Undo have it).
        let (made, failed) = commit_each("", todo, &it.title);
        self.save_made(&it.id, &made);
        if let Some(e) = failed {
            self.finish_ask(&it.id, "error", &e, "");
            return None;
        }
        let mut merged: Vec<String> = Vec::new();
        for folder in &crew {
            match merge_to_main(&project, folder, &it.title) {
                Ok(Some(main)) => merged.push(main),
                Ok(None) => {}
                Err((step, question)) => {
                    self.finish_ask(&it.id, step, &question, "");
                    return None;
                }
            }
        }
        let mut tops: Vec<String> = Vec::new();
        for folder in made.iter().map(|(f, _)| f.clone()).chain(merged) {
            if let Some((main, _)) = main_checkout(&folder).filter(|(m, _)| !tops.contains(m)) {
                tops.push(main);
            }
        }
        Some(tops)
    }

    /// Item `id`, its work on the main line in `tops`: queued for an install when one of them is this Cue's source,
    /// else to check.
    fn finish_after(&self, id: &str, tops: &[String]) {
        let now = now_ms();
        // Nothing new this round (its fix after a failed install was already committed): the main line it went to
        // before, so it's installed all the same.
        let before = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).map(|f| f.repo.clone())).unwrap_or_default();
        let tops: Vec<String> = if tops.is_empty() && !before.is_empty() { vec![before] } else { tops.to_vec() };
        let cue = tops.iter().find(|t| super::desk_checks::builds_this_cue(t));
        let top = cue.or(tops.first()).cloned().unwrap_or_default();
        let head = if top.is_empty() { String::new() } else { git("", &top, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()).unwrap_or_default() };
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
            // Due once the batch's wait is over; the tick looks too.
            self.later(BATCH_MS, |h| h.desk_finish_tick());
        } else {
            self.finish_checked(id);
        }
    }

    /// The commits made for item `id`, added to those it has (one per repository and round).
    fn save_made(&self, id: &str, made: &[(String, String)]) {
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
    }

    fn finish_tick(&self) {
        let items: Vec<DeskItem> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.finish.is_some()).cloned().collect());
        if items.is_empty() {
            return;
        }
        let step = |i: &DeskItem| i.finish.as_ref().map(|f| f.step.clone()).unwrap_or_default();
        for it in items.iter().filter(|i| matches!(step(i).as_str(), "commit" | "commit_wait")) {
            self.desk_finish_commit(&it.id);
        }
        let inst = self.installer();
        let status = inst.status();
        let now = now_ms();
        let mut running = status.as_ref().is_some_and(|s| installing(&s.step));
        for it in &items {
            let Some(f) = it.finish.as_ref().filter(|f| f.step == "queued" || installing(&f.step)) else { continue };
            if !f.commit.is_empty() && inst.has(&f.repo, &f.commit) {
                self.finish_checked(&it.id);
                continue;
            }
            if !installing(&f.step) {
                continue;
            }
            match status.as_ref().filter(|s| s.tag == f.tag) {
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
        // Still building one of ours (or anything): the next waits for it.
        running = running || self.read(|st| st.desk.items.iter().any(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| installing(&f.step))));
        if !running {
            self.finish_install_due(now);
        }
    }

    /// The next install, when items are queued and it's due: two minutes after the latest of them, and with no
    /// session that ends with Cue mid-turn; at once for an item you said Install now on.
    fn finish_install_due(&self, now: u64) {
        let queued: Vec<DeskItem> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "queued")).cloned().collect());
        let Some(repo) = queued.iter().filter_map(|i| i.finish.as_ref()).map(|f| f.repo.clone()).find(|r| !r.is_empty()) else { return };
        let batch: Vec<&DeskItem> = queued.iter().filter(|i| i.finish.as_ref().is_some_and(|f| f.repo == repo)).collect();
        let fin = |i: &&DeskItem| i.finish.clone().unwrap_or_default();
        let now_said = batch.iter().any(|i| fin(i).now);
        let latest = batch.iter().map(|i| fin(i).at).max().unwrap_or(0);
        if !now_said {
            if now.saturating_sub(latest) < BATCH_MS {
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
                    self.finish_set(&it.id, |i| {
                        if let Some(f) = i.finish.as_mut() {
                            (f.step, f.at, f.tag, f.now) = ("building".into(), now, tag, false);
                        }
                    });
                }
                Err(e) => self.finish_failed(it, &format!("{NOT_STARTED}: {e}"), "failed", e),
            }
        }
    }

    /// Item `it`'s install failed (`step`): it stops on `question` with the log's end kept, and goes back to its
    /// holder with the log, as Send back does (its next DONE finishes it again).
    fn finish_failed(&self, it: &DeskItem, question: &str, step: &str, log: &str) {
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

    /// Install now on item `id`, queued for an install: it starts without waiting for more items or for sessions
    /// mid-turn.
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
    /// Try again on item `id`, stopped on a question while finishing (you fixed it by hand, or its holder is gone):
    /// an install that failed is queued again; anything else (a word, a conflict, a commit that waited too long) is
    /// committed again, what an earlier round made kept.
    pub(crate) fn desk_retry_finish(&self, id: &str) -> Result<DeskItem, String> {
        let at = self.read(|st| st.desk.item(id).map(|i| i.finish.clone().filter(|f| i.state == "finishing" && !f.question.is_empty()))).ok_or("no such item")?;
        let Some(fin) = at else { return Err("It isn't stopped on a question".into()) };
        let now = now_ms();
        let install = matches!(fin.step.as_str(), "failed" | "rolled_back") && !fin.repo.is_empty();
        self.finish_set(id, |i| {
            i.updated = super::desk::secs(now);
            i.finish = Some(match install {
                true => Finish { step: "queued".into(), at: now, ..begun(now, Some(&fin)) },
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
pub(super) fn begun(now: u64, before: Option<&Finish>) -> Finish {
    let (repo, commit) = before.map(|f| (f.repo.clone(), f.commit.clone())).unwrap_or_default();
    Finish { step: "commit".into(), at: now, repo, commit, ..Default::default() }
}

/// `folder`'s repository's main checkout (its first worktree) and the branch it's on ("" for a detached HEAD).
fn main_checkout(folder: &str) -> Option<(String, String)> {
    let list = git("", folder, &[], &["worktree", "list", "--porcelain"])?;
    let mut lines = list.lines();
    let top = lines.next()?.strip_prefix("worktree ")?.to_string();
    let branch = lines.take_while(|l| !l.is_empty()).find_map(|l| l.strip_prefix("branch refs/heads/")).unwrap_or("").to_string();
    Some((top, branch))
}

/// `folder`'s work merged into its repository's main checkout, when it's another worktree on a branch with commits
/// the main checkout's branch doesn't have: tried first without touching any file (`git merge-tree`), its added
/// lines word-checked, then merged there (`--no-ff`). The main checkout, when merged; None when there was nothing
/// to merge; else the step and question to stop on (a conflict, a word, or git refusing: nothing merged).
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
        return Err(("conflict", format!("Not merged: {branch} conflicts with {onto} in {}", files.join(", "))));
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
    if git("", &main, &[], &["merge", "--no-ff", "--no-edit", "-q", "-m", &message, &sha]).is_none() {
        // Refused before it began (a file there with changes of its own), or stopped: left as it was. Only this
        // merge is undone (none was in progress before it).
        if in_progress(&main).is_some() {
            git("", &main, &[], &["merge", "--abort"]);
        }
        return Err(("conflict", format!("Not merged: git wouldn't merge {branch} into {onto} in the main checkout (a file it changes has changes of its own there)")));
    }
    Ok(Some(main))
}

/// What `main`'s checkout is in the middle of, when it is: a merge, a rebase, a cherry-pick or a revert.
fn in_progress(main: &str) -> Option<&'static str> {
    let path = |p: &str| -> Option<std::path::PathBuf> {
        let at = git("", main, &[], &["rev-parse", "--git-path", p])?.trim().to_string();
        let at = std::path::PathBuf::from(at);
        Some(if at.is_absolute() { at } else { std::path::Path::new(main).join(at) })
    };
    let marks = [("MERGE_HEAD", "a merge"), ("rebase-merge", "a rebase"), ("rebase-apply", "a rebase"), ("CHERRY_PICK_HEAD", "a cherry-pick"), ("REVERT_HEAD", "a revert")];
    marks.into_iter().find(|(p, _)| path(p).is_some_and(|f| f.exists())).map(|(_, what)| what)
}

/// Whether two folders are the same, once their links are followed.
fn same_dir(a: &str, b: &str) -> bool {
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
