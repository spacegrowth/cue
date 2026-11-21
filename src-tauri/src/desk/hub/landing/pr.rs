//! PR landing and the push choices (docs/desk-landing-design.md part 2, step 8). A project whose work lands by pull
//! request (desk/lands.rs `how` = "pr") has each item checked in the landing line as any other, then, in place of moving
//! main, its one commit pushed as the item's own branch (never main) and a pull request opened with GitHub's gh. The
//! item shows "In review" with the pull request's link, and Cue looks at the pull request once a minute: merged, and
//! its commit seen on main, it's landed (its copy removed, as any landing); closed unmerged, it goes back to its
//! holder. A push or a pull request that fails says why on the item and waits: its copy and its check are kept.
//!
//! Push to main (desk/lands.rs `push`): "ask" is the rule from before (a session's push to main asks you, hub/guard.rs;
//! the Desk pushes nothing), "never" the same with nothing ever pushed by the Desk, and "after" has the Desk push
//! main after each landing, without asking. Sessions never push main in any of them.

use crate::desk::hub::landing::git::{because, git};
use crate::desk::hub::landing::finish::Claim;
use crate::desk::hub::landing::main_line::{in_progress, main_checkout, main_ref};
use crate::desk::hub::landing::remote_item::remote_of;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree};
use std::process::{Command, Stdio};
use std::time::Duration;

/// What an item says when its pull request was closed without being merged.
pub(crate) const PR_CLOSED: &str = "Its pull request was closed without merging";
/// What an item says when its pull request was merged but its commit never showed on main.
pub(crate) const PR_UNSEEN: &str = "Merged, but Cue can't see it on main: run the landing again.";
/// How often Cue reads an open pull request.
const LOOK_EVERY_MS: u64 = 60_000;
/// How long a merged pull request may stay unseen on main before the item stops and says so.
const UNSEEN_ASK_MS: u64 = 30 * 60_000;
/// Failed reads of a pull request in a row before the item says Cue can't read it.
const MISSES_SAID: u32 = 3;
/// How long a push, a fetch or a gh call may take before it's cut off: a hang never holds the landing line.
const NET_CAP: Duration = Duration::from_secs(60);

/// Why a merged pull request isn't on main yet: its commit not seen there, or something Cue can name.
enum Onto {
    Unseen,
    Wait(String),
}

impl Hub {
    /// A test's fake gh in place of GitHub's.
    #[cfg(test)]
    pub(crate) fn desk_set_gh(&self, program: &str) {
        self.write(|st| st.desk.gh = program.to_string());
    }

    /// A test's cap on pushes, fetches and gh calls, in place of a minute.
    #[cfg(test)]
    pub(crate) fn desk_set_net_cap(&self, ms: u64) {
        self.write(|st| st.desk.net_cap_ms = ms);
    }

    /// Item `it`, checked in its copy `t`: its commit `squash` (on the main line `onto`) pushed as its branch, and a
    /// pull request titled `title` opened from it (or the one open for that branch already). In review then, or
    /// stopped saying why not.
    pub(crate) fn desk_open_pr(&self, it: &DeskItem, t: &ItemTree, onto: &str, squash: &str, title: &str) {
        let opened = (|| {
            // Its branch is forced to its commit: only ever an item's own branch, never the main line.
            if !t.branch.starts_with("desk/") || t.branch == onto {
                return Err(format!("Its branch is {}, not one of the Desk's own, so nothing was pushed", t.branch));
            }
            let remote = remote_of("", &t.repo, onto).ok_or("This project has no remote to push to, so no pull request was opened")?;
            // Nothing goes out before the check of every commit the remote doesn't have yet (push_check.rs).
            let project = self.read(|st| st.desk.project_of(&it.project).to_string());
            crate::desk::hub::landing::push_check::push_allowed("", &t.repo, &project, &remote, squash).map_err(|e| format!("{e}. So no pull request was opened"))?;
            self.net_git(&t.repo, &["push", "-q", &remote, &format!("+{squash}:refs/heads/{}", t.branch)])
                .map_err(|e| because(format!("Its branch couldn't be pushed to {remote}, so no pull request was opened"), &e))?;
            // Opened before (the landing run again): that one.
            let open = self.gh(&t.repo, &["pr", "list", "--head", &t.branch, "--state", "open", "--json", "url"]).map_err(|e| because("Cue couldn't look for its pull request".into(), &e))?;
            let found = serde_json::from_str::<Value>(&open).ok().and_then(|v| v.pointer("/0/url").and_then(Value::as_str).map(str::to_string)).filter(|u| !u.is_empty());
            if let Some(url) = found {
                return Ok(url);
            }
            let body = format!("Desk item #{}.{}", it.id, if it.done_when.trim().is_empty() { String::new() } else { format!("\n\nDone when: {}", it.done_when.trim()) });
            match self.gh(&t.repo, &["pr", "create", "--head", &t.branch, "--base", onto, "--title", title, "--body", &body]) {
                Ok(out) => link_in(&out).ok_or_else(|| "GitHub opened no pull request that Cue could see".to_string()),
                Err(e) => Err(because("The pull request couldn't be opened".into(), &e)),
            }
        })();
        let now = now_ms();
        match opened {
            Ok(url) => {
                let (repo, squash, base) = (t.repo.clone(), squash.to_string(), onto.to_string());
                self.finish_set(&it.id, |i| {
                    i.updated = crate::desk::hub::core::desk::secs(now);
                    if let Some(f) = i.finish.as_mut() {
                        (f.step, f.at, f.pr, f.base, f.commit, f.repo, f.wait, f.wait_from, f.question) = ("review".into(), now, url, base, squash, repo, String::new(), 0, String::new());
                    }
                });
                self.write(|st| {
                    st.desk.run.pr_looked.insert(it.id.clone(), now);
                    st.desk.run.pr_misses.remove(&it.id);
                });
            }
            Err(why) => self.finish_ask(&it.id, "pr_failed", &why, ""),
        }
    }

    /// Items in review whose pull request Cue hasn't read for a minute, read now (the finishing tick's). What's kept
    /// for an item no longer in review (dropped, sent back, landed) goes.
    pub(crate) fn desk_pr_tick(&self) {
        let now = now_ms();
        let due: Vec<String> = self.write(|st| {
            let review: Vec<String> = st.desk.items.iter().filter(|i| in_review(i)).map(|i| i.id.clone()).collect();
            st.desk.run.pr_looked.retain(|id, _| review.contains(id));
            st.desk.run.pr_misses.retain(|id, _| review.contains(id));
            let ids: Vec<String> = review.into_iter().filter(|id| st.desk.run.pr_looked.get(id).is_none_or(|at| now.saturating_sub(*at) >= LOOK_EVERY_MS)).collect();
            for id in &ids {
                st.desk.run.pr_looked.insert(id.clone(), now);
            }
            ids
        });
        for id in due {
            self.desk_pr_look(&id);
        }
    }

    /// Item `id`'s pull request read now: merged and on main, landed; closed, back to its holder; open, as it was.
    pub(crate) fn desk_pr_look(&self, id: &str) {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return };
        let Some(f) = it.finish.clone().filter(|_| in_review(&it)) else { return };
        let t = it.tree.clone().unwrap_or_else(|| ItemTree { repo: f.repo.clone(), ..Default::default() });
        let said = self.gh(&t.repo, &["pr", "view", &f.pr, "--json", "state,mergeCommit"]);
        let v: Value = match said.and_then(|o| serde_json::from_str(&o).map_err(|e| e.to_string())) {
            Ok(v) => v,
            // Said only once it fails a few times in a row: one slow answer isn't news.
            Err(e) => {
                let misses = self.write(|st| {
                    let n = st.desk.run.pr_misses.entry(id.to_string()).or_insert(0);
                    *n += 1;
                    *n
                });
                if misses >= MISSES_SAID {
                    self.pr_wait(id, &because("Cue can't read its pull request".into(), &e));
                }
                return;
            }
        };
        self.write(|st| st.desk.run.pr_misses.remove(id));
        match v["state"].as_str().unwrap_or("") {
            "MERGED" => {
                let oid = v.pointer("/mergeCommit/oid").and_then(Value::as_str).unwrap_or("").to_string();
                let base = if f.base.is_empty() { main_ref(&t.repo).unwrap_or_default() } else { f.base.clone() };
                match self.bring_onto_main(&t.repo, &base, &oid) {
                    Ok(()) => {}
                    Err(Onto::Wait(why)) => return self.pr_wait(id, &why),
                    Err(Onto::Unseen) => return self.pr_unseen(id, f.wait_from),
                }
                self.write(|st| st.desk.run.pr_looked.remove(id));
                let short = git("", &t.repo, &[], &["rev-parse", "--short", &oid]).map(|s| s.trim().to_string()).unwrap_or_default();
                if t.path.is_empty() {
                    self.save_made(id, &[(t.repo.clone(), short)]);
                    self.finish_after(id, std::slice::from_ref(&t.repo));
                } else {
                    self.finish_landed(id, &t, &short);
                }
            }
            "CLOSED" => {
                self.write(|st| st.desk.run.pr_looked.remove(id));
                self.finish_failed(&it, PR_CLOSED, "pr_closed", "");
            }
            _ => self.pr_wait(id, ""),
        }
    }

    /// Item `id`, in review, says `why` it's still there ("" for nothing more than the review).
    fn pr_wait(&self, id: &str, why: &str) {
        let same = self.read(|st| st.desk.item(id).and_then(|i| i.finish.as_ref()).is_some_and(|f| f.wait == why));
        if !same {
            let why = why.to_string();
            self.finish_fin(id, |f| f.wait = why);
        }
    }

    /// Item `id`'s pull request merged, its commit not seen on main since `from` (0: not before now): it waits, and
    /// after half an hour stops and says so.
    fn pr_unseen(&self, id: &str, from: u64) {
        let now = now_ms();
        if from == 0 {
            self.finish_fin(id, |f| f.wait_from = now);
        } else if now.saturating_sub(from) >= UNSEEN_ASK_MS {
            self.write(|st| st.desk.run.pr_looked.remove(id));
            return self.finish_ask(id, "pr_failed", PR_UNSEEN, "");
        }
        self.pr_wait(id, "Merged; waiting to see it on main");
    }

    /// Commit `oid` (a merged pull request's) on `base` in `repo`: there already, or on the remote's (fetched now)
    /// and the main checkout brought up to it, its edits kept (desk/hub/landing/land_keep.rs). Only while the main checkout is on
    /// `base`, and never while an item lands (the landing line's claim).
    fn bring_onto_main(&self, repo: &str, base: &str, oid: &str) -> Result<(), Onto> {
        if oid.is_empty() || base.is_empty() {
            return Err(Onto::Unseen);
        }
        let has = |of: &str| matches!(crate::safe_git::run("", repo, &[], &["merge-base", "--is-ancestor", oid, of]), Ok((_, 0)));
        let local = format!("refs/heads/{base}");
        if has(&local) {
            return Ok(());
        }
        let Some(remote) = remote_of("", repo, base) else { return Err(Onto::Unseen) };
        let theirs = format!("refs/remotes/{remote}/{base}");
        if let Err(e) = self.net_git(repo, &["fetch", "-q", &remote, &format!("+{local}:{theirs}")]) {
            return Err(Onto::Wait(because(format!("Merged; Cue couldn't fetch {base} from {remote}"), &e)));
        }
        if !has(&theirs) {
            return Err(Onto::Unseen);
        }
        let waiting = format!("Merged; waiting to bring it onto {base}");
        let on = main_checkout(repo).map(|(_, b)| b).unwrap_or_default();
        if on != base {
            return Err(Onto::Wait(if on.is_empty() { format!("{waiting}: the main checkout isn't on a branch") } else { format!("{waiting}: the main checkout is on {on}, not {base}") }));
        }
        let Some(_claim) = Claim::take(self, "landing") else { return Err(Onto::Wait(format!("{waiting}: another item is landing now"))) };
        if let Some(what) = in_progress(repo) {
            return Err(Onto::Wait(format!("{waiting}: the main checkout is in the middle of {what}")));
        }
        let Some(main) = git("", repo, &[], &["rev-parse", &local]).map(|s| s.trim().to_string()) else {
            return Err(Onto::Wait(format!("{waiting}: Cue can't read {base}")));
        };
        crate::desk::hub::landing::land_keep::fast_forward_keeping_edits(repo, &main, oid).map_err(|e| Onto::Wait(because(format!("{waiting}: git wouldn't move {base} in the main checkout"), &e)))
    }

    /// Item `id` landed on `top`'s main line: pushed there now when its project pushes after each landing (not one
    /// whose pull request was merged: that's on the remote already), once the check before a push passes
    /// (push_check.rs). What happened is kept on the item, a refusal naming each find.
    pub(crate) fn desk_push_after(&self, id: &str, project: &str, top: &str) {
        let l = crate::desk::lands::lands_of(project, top);
        if l.push != "after" || l.how == "pr" {
            return;
        }
        let Some((_, onto)) = main_checkout(top).filter(|(_, b)| !b.is_empty()) else { return };
        let said = match remote_of("", top, &onto) {
            None => "Not pushed: this project has no remote".to_string(),
            Some(r) => match crate::desk::hub::landing::push_check::push_allowed("", top, project, &r, &format!("refs/heads/{onto}")) {
                Err(refused) => refused,
                Ok(()) => match self.net_git(top, &["push", "-q", &r, &format!("refs/heads/{onto}:refs/heads/{onto}")]) {
                    Ok(_) => format!("Pushed to {r}"),
                    Err(e) => because(format!("Not pushed to {r}"), &e),
                },
            },
        };
        self.finish_fin(id, |f| f.pushed = said);
    }

    /// gh run in `dir` with `args`: what it printed, or why it failed in its own words.
    fn gh(&self, dir: &str, args: &[&str]) -> Result<String, String> {
        let fake = self.read(|st| st.desk.gh.clone());
        let program = if fake.is_empty() { gh_bin() } else { std::path::PathBuf::from(fake) };
        let mut cmd = Command::new(&program);
        cmd.args(args).current_dir(dir).env("NO_COLOR", "1");
        let out = self.net_run(&mut cmd).map_err(|e| if e.contains("No such file") { "GitHub's gh tool isn't installed on this Mac".to_string() } else { e })?;
        let said = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
        if out.status.success() {
            return Ok(said(&out.stdout));
        }
        let err = said(&out.stderr);
        if err.contains("gh auth login") {
            return Err("GitHub's gh tool isn't signed in: run gh auth login in a terminal".into());
        }
        Err(if err.is_empty() { said(&out.stdout) } else { err })
    }

    /// git over the network (a push, a fetch) in `repo`, the repository's own programs off as every run of Cue's
    /// (safe_git.rs): what it printed, or why it failed in git's own words.
    pub(crate) fn net_git(&self, repo: &str, args: &[&str]) -> Result<String, String> {
        let filters = crate::safe_git::no_repo_filters("", repo);
        let w = crate::safe_git::words(repo, &filters, args);
        let mut cmd = Command::new(crate::safe_git::git_bin());
        cmd.args(&w[1..]);
        let out = self.net_run(&mut cmd)?;
        let text = |b: &[u8]| String::from_utf8_lossy(b).to_string();
        if out.status.success() {
            return Ok(text(&out.stdout));
        }
        Err(crate::safe_git::last_words(&format!("{}\n{}", text(&out.stderr), text(&out.stdout))))
    }

    /// `cmd` run with nothing to ask on (no password prompt from git, ssh or gh) and cut off past the cap, killed by
    /// its own PID: a remote that doesn't answer is said as such, never waited on for good.
    fn net_run(&self, cmd: &mut Command) -> Result<std::process::Output, String> {
        let ms = self.read(|st| st.desk.net_cap_ms);
        let cap = if ms == 0 { NET_CAP } else { Duration::from_millis(ms) };
        cmd.env("GIT_TERMINAL_PROMPT", "0").env("GH_PROMPT_DISABLED", "1").stdin(Stdio::null());
        if std::env::var_os("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
        crate::terminal::focus::capped(cmd, cap).map_err(|e| e.to_string())
    }
}

/// Whether item `i` is in review: finishing, its pull request open.
fn in_review(i: &DeskItem) -> bool {
    i.state == "finishing" && i.finish.as_ref().is_some_and(|f| f.step == "review" && !f.pr.is_empty())
}

/// The first web link in `text`.
fn link_in(text: &str) -> Option<String> {
    text.split_whitespace().find(|w| w.starts_with("https://") || w.starts_with("http://")).map(|w| w.trim_end_matches(['.', ',', ')']).to_string())
}

/// GitHub's gh: on the PATH, or where Homebrew puts it.
pub(crate) fn gh_bin() -> std::path::PathBuf {
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path.split(':').chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]).map(|d| std::path::Path::new(d).join("gh")).find(|p| p.is_file());
    found.unwrap_or_else(|| std::path::PathBuf::from("gh"))
}

#[cfg(test)]
mod tests {
    use super::link_in;

    #[test]
    fn the_link_is_found_in_what_gh_says() {
        assert_eq!(link_in("https://example.test/o/r/pull/7\n").as_deref(), Some("https://example.test/o/r/pull/7"));
        assert_eq!(link_in("a pull request for branch \"desk/3\" into branch \"main\" already exists:\nhttps://example.test/o/r/pull/3").as_deref(), Some("https://example.test/o/r/pull/3"));
        assert_eq!(link_in("nothing here"), None);
    }
}
