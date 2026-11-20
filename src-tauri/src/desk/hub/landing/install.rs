//! A finishing item's install (split from desk/hub/landing/finish.rs): the app's installer, the tick that follows installs and
//! starts the next once it's due, Install now, a stopped install sent back to its holder, and Run again.

use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::finish::*;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, Finish, InstallStatus, Installer};

/// How much of an install's log an item keeps (lines, from its end).
const LOG_LINES: usize = 40;

/// The app's own installer: self_install.rs.
struct AppInstaller;

impl Installer for AppInstaller {
    fn has(&self, repo: &str, commit: &str) -> bool {
        crate::install::self_install::has(repo, commit)
    }

    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String> {
        crate::install::self_install::start(repo, commit, tag)
    }

    fn status(&self) -> Option<InstallStatus> {
        crate::install::self_install::status().map(|s| InstallStatus { tag: s.tag, commit: s.commit, step: s.step, log: s.log, at: s.at })
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::install::self_install::is_ancestor(repo, commit, of)
    }

    /// Cue's own repository is checked before a landing (the step 1 check, docs/desk-landing-design.md part 7): its
    /// window loaded from a clean copy of the commit, failing on a name missing or declared twice. Others have none.
    fn check(&self, repo: &str, commit: &str) -> Option<Result<(), String>> {
        crate::desk::hub::lead::checks::builds_this_cue(repo).then(|| crate::install::live_ui::check(repo, commit))
    }
}

/// Notes which Cue is running, once, as the hub starts (the app's only: a test's hub has its own installer).
pub(crate) fn note_running() {
    #[cfg(not(test))]
    crate::install::self_install::note_running();
}

/// Whether an item at `step` is in an install (begun, not yet seen to have landed).
pub(crate) fn installing(step: &str) -> bool {
    matches!(step, "building" | "installing" | "starting")
}

/// A finishing step stopped on its install: the build failed, or the new Cue didn't start.
pub(crate) fn stopped_install(step: &str) -> bool {
    matches!(step, "failed" | "rolled_back")
}

/// The last `LOG_LINES` lines of `log`.
pub(crate) fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.trim_end().lines().collect();
    lines[lines.len().saturating_sub(LOG_LINES)..].join("\n")
}


impl Hub {
    /// Run the landing again on item `id`, stopped on a question while finishing: a plain retry, offered only when
    /// Cue can name nothing in its way (#250; what it can name has its own offer, desk/hub/core/waits.rs): an install that
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
            i.updated = crate::desk::hub::core::desk::secs(now);
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
        let it = self.read(|st| st.desk.item(id).map(|i| crate::desk::hub::core::desk::shown(st, i)));
        it.ok_or_else(|| "no such item".into())
    }
}

/// When the next install is due (ms), the first item waiting for it plus half an hour; 0 when none waits.
pub(crate) fn install_at(st: &Store) -> u64 {
    let first = st.desk.items.iter().filter(|i| i.state == "finishing").filter_map(|i| i.finish.as_ref()).filter(|f| f.step == "queued").map(|f| f.at).min();
    first.map_or(0, |at| at + INSTALL_EVERY_MS)
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

impl Hub {
    /// The installer this hub's Desk uses.
    pub(crate) fn installer(&self) -> std::sync::Arc<dyn Installer> {
        self.read(|st| st.desk.installer.0.clone()).unwrap_or_else(|| std::sync::Arc::new(AppInstaller))
    }

    /// Another installer in place of the app's own (a test's).
    #[cfg(test)]
    pub(crate) fn desk_set_installer(&self, i: std::sync::Arc<dyn Installer>) {
        self.write(|st| st.desk.installer.0 = Some(i));
    }

    pub(crate) fn finish_tick(&self) {
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
    pub(crate) fn finish_failed(&self, it: &DeskItem, question: &str, step: &str, log: &str) {
        self.finish_back(it, question, step, log, false);
    }

    /// A clash with the main line in item `it`'s work (#371): handed back to the session that did it, with what
    /// clashed and how to settle it: at work again, nothing waiting on you. Without a holder it waits on you.
    pub(crate) fn finish_clash(&self, it: &DeskItem, question: &str) {
        self.finish_back(it, question, "conflict", "", true);
    }

    /// `finish_failed`, and with `hand_back` the item at work again (as Send back leaves it) instead of waiting on you.
    pub(crate) fn finish_back(&self, it: &DeskItem, question: &str, step: &str, log: &str, hand_back: bool) {
        // Closed while its landing ran (#406's, closed as on main mid-bounce): nothing goes back, it's done (#551).
        if self.read(|st| st.desk.item(&it.id).is_none_or(|i| i.state == "done")) {
            return;
        }
        // Handed back, it's at work again: nothing for the lead to settle.
        if hand_back && it.holder.is_some() {
            self.finish_question(&it.id, step, question, log);
        } else {
            self.finish_ask(&it.id, step, question, log);
        }
        let question = &crate::desk::hub::lead::stop_lead::plain_and_said(question).0;
        let Some(h) = it.holder.clone() else { return };
        let now = now_ms();
        let log = tail(log);
        let id = it.id.clone();
        let fx = self.write(|st| {
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            let Some(i) = st.desk.item_mut(&id) else { return fx };
            if let Some(c) = i.checks.last_mut() {
                c.back = question.to_string();
            }
            if hand_back {
                (i.state, i.done_at, i.finish, i.close_on_check) = ("working".into(), None, None, false);
                i.check = None;
            }
            let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
            let log = if log.is_empty() { String::new() } else { format!("\nThe end of its log:\n{log}") };
            fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({}) isn't done: {question}.{log}\nIts fixes stay in #{id}; don't add new items for them. Say DONE: #{id} when it is.\n{}", i.title, crate::desk::check_words(&id))));
            let project = i.project.clone();
            crate::desk::hub::core::desk::decide(st, &project, Some(id.clone()), "Cue", "", question, &format!("sent back to {}", h.name), now, &mut fx);
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
        let it = self.read(|st| st.desk.item(id).map(|i| crate::desk::hub::core::desk::shown(st, i)));
        it.ok_or_else(|| "no such item".into())
    }
}
