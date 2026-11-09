//! A hosted lead's team in the hub (docs/dev/cue-teams.md §3–§5): its teammates kept on its record from its stream
//! (sessions/team.rs), and what you do with each: Review (the lead reviews it through a fork, through Review's queue),
//! Diff, Commit (its files only, in its worktree when it has one), Send fixes (the findings back to it, through the
//! lead), Resume and Stop (through the lead), Set aside; and a teammate gone quiet (sessions/quiet.rs), seen to once.

use super::*;
use crate::hosted::HostedEvent;
use crate::sessions::quiet::Quiet;
use crate::sessions::team::{add_files, read_review, Teammate};

/// A teammate's review in Review's queue goes by this key: `team:<lead>:<agent id>`.
const KEY: &str = "team:";

/// How often the watcher's tick looks for quiet teammates.
const QUIET_CHECK_MS: u64 = 15_000;

/// What its lead is asked when you Resume a quiet teammate.
fn resume_words(name: &str) -> String {
    format!("Your teammate {name} has gone quiet. Ask it where it is and to carry on with its part, with SendMessage (to: \"{name}\").")
}

/// The most of a diff the window is handed.
const DIFF_LIMIT: usize = 200_000;

/// `key` as (lead, agent id) when it's a teammate's.
pub(super) fn teammate_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix(KEY)?.rsplit_once(':')
}

impl Hub {
    /// What one event of a hosted session's stream changes in its team; drawn when it changes anything. A teammate
    /// that just finished gets the size of what it changed, read off the stream's thread; one in a worktree of its
    /// own whose Bash call just ended gets what git says changed there (`team_scan`).
    pub(super) fn team_sees(&self, origin: &Origin, e: &HostedEvent) {
        let sid = origin.session_id.as_str();
        let now = now_ms();
        let finished = |t: &Teammate| matches!(t.state.as_str(), "done" | "stopped");
        let changed = self.write(|st| {
            let team = st.sessions.team(sid);
            let before: Vec<String> = team.iter().filter(|t| finished(t)).map(|t| t.call.clone()).collect();
            let bash_ended = match e {
                HostedEvent::ToolResult { id, agent: Some(call), .. } => team.iter().any(|t| t.call == *call && t.bash_calls.contains(id)).then(|| call.clone()),
                _ => None,
            };
            if !st.sessions.team_sees(sid, e, now) {
                return None;
            }
            let ended: Vec<Teammate> = st.sessions.team(sid).into_iter().filter(|t| finished(t) && !before.contains(&t.call)).collect();
            Some((ended, bash_ended))
        });
        let Some((ended, bash_ended)) = changed else { return };
        self.changed();
        // SHORTCUT: after each command here only; on a machine its files come at its finish (below), so a remote
        // worktree teammate's shell writes show late, not live. Upgrade: a throttled scan (one in flight a teammate).
        if let Some(call) = bash_ended.filter(|_| origin.machine.is_empty()) {
            self.team_scan(sid, &call);
        }
        if ended.is_empty() {
            return;
        }
        #[cfg(feature = "ext")]
        self.ext_mates_done(sid);
        let (sid, cwd, machine) = (sid.to_string(), origin.cwd.clone(), origin.machine.clone());
        self.later(0, move |h| {
            for t in ended {
                let c = changes(&machine, &cwd, &t).ok();
                h.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| finished_files(m, c.as_ref())));
            }
            h.changed();
        });
    }

    /// What git says changed in teammate `call`'s own worktree (its shell commands' changes too) added to its files,
    /// read off the caller's thread: git is never waited on here.
    fn team_scan(&self, sid: &str, call: &str) {
        let (sid, call) = (sid.to_string(), call.to_string());
        self.later(0, move |h| {
            if h.team_scan_now(&sid, &call) {
                h.changed();
            }
        });
    }

    /// The same, at once, on this thread: whether its files grew.
    fn team_scan_now(&self, sid: &str, call: &str) -> bool {
        let Some(origin) = self.session_origin(sid) else { return false };
        let Some(t) = self.read(|st| st.sessions.team(sid)).into_iter().find(|t| t.call == call) else { return false };
        let Some(files) = worktree_files(&origin.machine, &origin.cwd, &t) else { return false };
        self.write(|st| st.sessions.with_teammate(sid, call, |m| add_files(m, files, false))).unwrap_or(false)
    }

    /// Every teammate of `sid` in a worktree of its own (still in the team) with what git says it changed there, at
    /// once: for work that runs off the hub's threads (the optional add-on's).
    #[cfg_attr(not(feature = "ext"), allow(dead_code))]
    pub(super) fn team_rescan(&self, sid: &str) {
        let calls: Vec<String> = self.read(|st| st.sessions.team(sid)).into_iter().filter(|t| t.in_team() && has_worktree(t)).map(|t| t.call).collect();
        let mut grew = false;
        for call in calls {
            grew |= self.team_scan_now(sid, &call);
        }
        if grew {
            self.changed();
        }
    }

    /// The lead's turn ended with `text`: a reply to your Review is that teammate's review (the oldest under review; one
    /// review at a time goes to a lead). A turn nothing of yours started (a teammate finishing) isn't one.
    /// SHORTCUT: the next prompted reply is taken as the review; a lead that runs the fork in the background files its
    /// "started" line instead. Upgrade: match the fork's own Agent call (its task_notification) to the review sent.
    pub(super) fn team_reply(&self, lead: &str, text: &str, unprompted: bool) {
        if unprompted {
            return;
        }
        let filed = self.write(|st| {
            let Some(t) = st.sessions.team(lead).into_iter().filter(|t| t.state == "reviewing").min_by_key(|t| t.review_ms) else { return false };
            let (findings, recommendation) = read_review(text);
            st.sessions.with_teammate(lead, &t.call, |m| {
                m.state = "reviewed".into();
                m.findings = findings;
                m.recommendation = recommendation;
            });
            true
        });
        if filed {
            self.changed();
        }
    }

    /// One of a teammate's buttons: review, diff, commit, fixes (Send fixes), stop, aside (Set aside). What to say,
    /// or why not; Diff answers with the diff itself.
    pub fn team_action(&self, lead: &str, agent: &str, action: &str) -> Result<String, String> {
        let (origin, t) = self.read(|st| (st.sessions.origin(lead), st.sessions.team(lead).into_iter().find(|t| t.agent_id == agent || t.call == agent)));
        let origin = origin.ok_or("that session is gone")?;
        let t = t.ok_or("Cue doesn't know that teammate")?;
        match action {
            "review" => self.review_teammate(lead, &origin, &t),
            "diff" => changes(&origin.machine, &origin.cwd, &t).map(|c| c.diff),
            "commit" => self.commit_teammate(lead, &origin, &t, false),
            // Commit refused on main (or a detached HEAD): a new branch made for it there, then the commit.
            "branch" => self.commit_teammate(lead, &origin, &t, true),
            "fixes" if !t.from.is_empty() => {
                let text = format!(
                    "Your teammate {name} (carried over from the session before you) can't be messaged: start a new background teammate named \"{name}\" (Agent tool, `name: \"{name}\"`, `run_in_background: true`{iso}) to fix its review's findings in its files, and don't fix them yourself.\nWhat it was asked: {what}\nIts files: {files}\nThe findings:\n{}",
                    if t.findings.is_empty() { "(the review listed none: ask it to check its work against what it was asked)".to_string() } else { t.findings.join("\n") },
                    name = t.name,
                    iso = if t.worktree.is_empty() { "" } else { ", `isolation: \"worktree\"`" },
                    what = if t.what.is_empty() { first(&t.asked) } else { t.what.clone() },
                    files = if t.files.is_empty() { "(none)".to_string() } else { t.files.join(", ") },
                );
                self.send_to_session(lead, &text, &[], false)?;
                self.set_teammate(lead, &t.call, |m| {
                    m.step = "Fixes sent: its lead starts it again".into();
                    m.step_ms = now_ms();
                });
                Ok(format!("Asked its lead to start {} again with the fixes", t.name))
            }
            "fixes" => {
                let text = format!(
                    "Send your teammate {name} its review's findings with SendMessage (to: \"{name}\") and let it fix them; don't fix them yourself:\n{}",
                    if t.findings.is_empty() { "(the review listed none: ask it to check its work against what it was asked)".to_string() } else { t.findings.join("\n") },
                    name = t.name
                );
                self.send_to_session(lead, &text, &[], false)?;
                self.set_teammate(lead, &t.call, |m| {
                    crate::sessions::team::back_at_work(m, now_ms());
                    m.step = "Fixes sent through its lead".into();
                    m.step_ms = now_ms();
                });
                Ok(format!("Sent the fixes to {}", t.name))
            }
            // Carried over a handoff: its process ended with its old lead.
            "resume" | "stop" if !t.from.is_empty() => Err(format!("{} stopped with the lead it was carried over from", t.name)),
            // Gone quiet with nothing running: its lead asks it where it is, and to carry on.
            "resume" => {
                // At once: its lead's turn waits on this very teammate, so a message kept for the turn's end never goes.
                self.send_to_session(lead, &resume_words(&t.name), &[], true)?;
                Ok(format!("Asked its lead to wake {}", t.name))
            }
            "stop" => {
                // At once, as Resume: a stuck teammate holds its lead's turn open, so a kept message would never go.
                self.send_to_session(lead, &format!("Stop your teammate {} now (TaskStop, task id {}), and say so.", t.name, t.agent_id), &[], true)?;
                Ok(format!("Asked its lead to stop {}", t.name))
            }
            "aside" => {
                self.set_teammate(lead, &t.call, |m| m.state = "aside".into());
                Ok(format!("Set {} aside", t.name))
            }
            _ => Err(format!("Cue can't “{action}” a teammate")),
        }
    }

    /// The watcher's tick: each teammate at work that has gone quiet (sessions/quiet.rs), seen to once a quiet stretch.
    /// Finished but lingering: done. A long tool call open: left to it, its row says so. Stuck: its row says so, and
    /// its lead is nudged once (a hosted lead's: Cue never types into a terminal unasked). The optional add-on sees a
    /// stuck one first (one the add-on manages is the add-on's to see to). At most every QUIET_CHECK_MS.
    pub(crate) fn team_quiet_tick(&self) {
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let now = now_ms();
        let last = LAST.load(std::sync::atomic::Ordering::Relaxed);
        if now >= last && now - last < QUIET_CHECK_MS {
            return;
        }
        LAST.store(now, std::sync::atomic::Ordering::Relaxed);
        self.team_quiet_check(now);
    }

    /// The quiet teammates seen to as of `now` (the tick's work, unthrottled).
    pub(crate) fn team_quiet_check(&self, now: u64) {
        let leads: Vec<(Origin, Vec<Teammate>)> = self.read(|st| st.sessions.all().into_iter().filter(|s| s.team.iter().any(Teammate::working)).map(|s| (s.origin.clone(), s.team.clone())).collect());
        let mut changed = false;
        for (origin, team) in leads {
            let sid = origin.session_id.clone();
            for t in team.iter().filter(|t| t.working()) {
                match t.quiet_now(now) {
                    Quiet::Active => {}
                    Quiet::Finished => {
                        self.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| {
                            m.state = "done".into();
                            m.step.clear();
                            m.quiet.clear();
                            m.ended_ms = now;
                        }));
                        changed = true;
                    }
                    Quiet::Tool if t.quiet != "tool" => {
                        self.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| m.quiet = "tool".into()));
                        changed = true;
                    }
                    Quiet::Tool => {}
                    Quiet::Stuck => {
                        if t.quiet != "stuck" || t.nudged_ms != t.step_ms {
                            let stretch = t.step_ms;
                            self.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| {
                                m.quiet = "stuck".into();
                                m.nudged_ms = stretch;
                            }));
                            changed = true;
                        }
                        if t.nudged_ms != t.step_ms {
                            self.team_quiet_nudge(&origin, t, now);
                        }
                    }
                }
            }
        }
        if changed {
            self.changed();
        }
    }

    /// Stuck teammate `t` of lead `origin`, once for its quiet stretch: the add-on's to see to when it takes it, else
    /// a short line to its lead asking it to check on it (a hosted lead's only).
    fn team_quiet_nudge(&self, origin: &Origin, t: &Teammate, now: u64) {
        #[cfg(feature = "ext")]
        if self.ext_mate_quiet(&origin.session_id, t) {
            return;
        }
        if !origin.hosted || origin.harness != "claude" || !t.from.is_empty() {
            return;
        }
        let mins = now.saturating_sub(t.step_ms) / 60_000;
        let text = format!("(Cue) Your teammate {name} has been quiet for {mins} min with nothing running. Ask it where it is with SendMessage (to: \"{name}\"); if it doesn't answer, stop it.", name = t.name);
        // At once: its lead's turn waits on this teammate, so a message kept for that turn's end would never go.
        if let Err(e) = self.send_to_session(&origin.session_id, &text, &[], true) {
            eprintln!("cue: couldn't tell {} about its quiet teammate {}: {e}", origin.session_id, t.name);
        }
    }

    /// `f` on the teammate, saved with its lead's record and drawn.
    fn set_teammate(&self, lead: &str, call: &str, f: impl FnOnce(&mut Teammate)) {
        self.write(|st| st.sessions.with_teammate(lead, call, f));
        self.changed();
    }

    /// Review: the lead reviews the teammate's work through a fork, sent when the lead is free (Review's queue, one
    /// review at a time per lead). Its reply comes back as the teammate's review (`team_reply`).
    fn review_teammate(&self, lead: &str, origin: &Origin, t: &Teammate) -> Result<String, String> {
        if t.agent_id.is_empty() || t.working() {
            return Err(format!("{} is still working", t.name));
        }
        let files = changes(&origin.machine, &origin.cwd, t).map(|c| c.files).unwrap_or_else(|_| t.files.clone());
        let place = if t.worktree.is_empty() { String::new() } else { format!(" in its worktree {}", t.worktree) };
        let text = format!(
            "Review your teammate {name}'s work with a fork (one Agent call, not in the background): read the diff of its files{place} against what it was asked, run its tests, and reply with only numbered findings (`1. file:line — should-fix — what`) and a last line `Recommendation: commit` or `Recommendation: send fixes`. Don't change, commit or message anything.\nWhat it was asked: {what}\nIts files: {files}",
            name = t.name,
            what = if t.what.is_empty() { first(&t.asked) } else { t.what.clone() },
            files = if files.is_empty() { "(none yet)".to_string() } else { files.join(", ") },
        );
        self.set_teammate(lead, &t.call, |m| m.state = "queued".into());
        let key = format!("{KEY}{lead}:{}", t.agent_id);
        let r = self.review(lead, &key, &text);
        if r.is_err() {
            self.set_teammate(lead, &t.call, |m| m.state = "done".into());
        }
        r
    }

    /// A teammate's review typed into its lead (now, or from the queue): it's under review from here.
    pub(super) fn team_review_sent(&self, key: &str, sent: bool) {
        let Some((lead, agent)) = teammate_key(key) else { return };
        self.set_teammate(lead, agent, |m| {
            m.state = if sent { "reviewing" } else { "done" }.into();
            m.review_ms = if sent { now_ms() } else { 0 };
        });
    }

    /// Commit: the teammate's files only, `git commit --only -- <its files>`, in its worktree when it has one; held
    /// while another teammate in the same folder has uncommitted changes in one of them. Nothing of any other
    /// teammate's goes in, staged or not.
    /// `branch`: on main, master or a detached HEAD, the commit goes on a new branch team/<name> made at HEAD
    /// (`commit_to_new_branch`); the folder stays where it is.
    fn commit_teammate(&self, lead: &str, origin: &Origin, t: &Teammate, branch: bool) -> Result<String, String> {
        if t.working() || t.agent_id.is_empty() {
            return Err(format!("{} is still working", t.name));
        }
        if let Some(other) = t.also.first() {
            return Err(format!("Held: {other} has uncommitted changes in one of {}'s files", t.name));
        }
        let c = changes(&origin.machine, &origin.cwd, t)?;
        if c.files.is_empty() {
            return Err(format!("{} has nothing to commit", t.name));
        }
        let dir = t.folder(&origin.cwd);
        let m = origin.machine.as_str();
        let filters = crate::safe_git::no_repo_filters(m, dir);
        let git = |args: &[&str]| crate::safe_git::run(m, dir, &filters, args);
        // Never on the main line: a teammate's work goes on a branch of its own (its worktree's, or one you made).
        let on = git(&["symbolic-ref", "--short", "-q", "HEAD"]).map(|(o, _)| o.trim().to_string()).unwrap_or_default();
        if let Some(why) = not_on(&on) {
            if !branch {
                return Err(why);
            }
            return self.commit_to_new_branch(lead, origin, t, &c.files, &on);
        }
        let made = if branch { on } else { String::new() };
        let paths: Vec<&str> = c.files.iter().map(String::as_str).collect();
        let add: Vec<&str> = ["add", "-A", "--"].into_iter().chain(paths.iter().copied()).collect();
        let (_, code) = git(&add)?;
        if code != 0 {
            return Err(format!("git couldn't add {}'s files", t.name));
        }
        let subject = if t.what.is_empty() { format!("{}'s work", t.name) } else { t.what.clone() };
        let message = if t.summary.is_empty() { subject } else { format!("{subject}\n\n{}", t.summary) };
        let commit: Vec<&str> = ["commit", "--only", "-q", "-m", message.as_str(), "--"].into_iter().chain(paths.iter().copied()).collect();
        let (_, code) = git(&commit)?;
        if code != 0 {
            return Err(format!("git didn't commit {}'s files (its hooks, or nothing to commit)", t.name));
        }
        let sha = git(&["rev-parse", "--short", "HEAD"]).map(|(o, _)| o.trim().to_string()).unwrap_or_default();
        let n = c.files.len();
        self.set_teammate(lead, &t.call, |mate| {
            mate.state = "committed".into();
            mate.commit = sha.clone();
        });
        let on = if made.is_empty() { String::new() } else { format!(" on {made}") };
        Ok(format!("Committed {}'s {n} file{}{on} ({sha})", t.name, if n == 1 { "" } else { "s" }))
    }

    /// Make a branch and commit, for a folder on main, master or a detached HEAD (often the main checkout, where
    /// other sessions keep uncommitted work): team/<name> made at HEAD with `files` committed on it, through a
    /// temporary index. The folder's HEAD, index and files stay as they are; the branch appears only once its commit
    /// is made, so a failure leaves none.
    fn commit_to_new_branch(&self, lead: &str, origin: &Origin, t: &Teammate, files: &[String], on: &str) -> Result<String, String> {
        let dir = t.folder(&origin.cwd);
        let m = origin.machine.as_str();
        let filters = crate::safe_git::no_repo_filters(m, dir);
        let git = |args: &[&str]| crate::safe_git::run(m, dir, &filters, args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o.trim().to_string());
        let head = git(&["rev-parse", "-q", "--verify", "HEAD^{commit}"]).ok_or("Not committed: its folder has no commit yet to make a branch from")?;
        let git_dir = git(&["rev-parse", "--absolute-git-dir"]).ok_or("Not committed: git couldn't find the folder's repository")?;
        let made = free_branch(&t.name, |b| git(&["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")]).is_some());
        let index = format!("{}/cue-team-index-{}", git_dir.trim_end_matches('/'), now_ms());
        let with_index = |args: &[&str]| crate::safe_git::run_env(m, dir, &filters, &[("GIT_INDEX_FILE", index.as_str())], args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o.trim().to_string());
        let subject = if t.what.is_empty() { format!("{}'s work", t.name) } else { t.what.clone() };
        let message = if t.summary.is_empty() { subject } else { format!("{subject}\n\n{}", t.summary) };
        let add: Vec<&str> = ["add", "-A", "--"].into_iter().chain(files.iter().map(String::as_str)).collect();
        let made_it = with_index(&["read-tree", &head])
            .and_then(|_| with_index(&add))
            .and_then(|_| with_index(&["write-tree"]))
            .and_then(|tree| git(&["commit-tree", &tree, "-p", &head, "-m", &message]))
            .and_then(|commit| git(&["update-ref", "-m", "cue: make a branch and commit", &format!("refs/heads/{made}"), &commit, ""]).map(|_| commit));
        remove_file(m, &index);
        let commit = made_it.ok_or_else(|| format!("Not committed: git couldn't commit {}'s files on a new branch, so none was made", t.name))?;
        let sha: String = commit.chars().take(7).collect();
        self.set_teammate(lead, &t.call, |mate| {
            mate.state = "committed".into();
            mate.commit = sha.clone();
        });
        let n = files.len();
        let stays = if on.is_empty() { "its detached HEAD".to_string() } else { on.to_string() };
        Ok(format!("Committed {n} file{} to {made} ({sha}); your folder stays on {stays}.", if n == 1 { "" } else { "s" }))
    }
}

/// Remove `path` (a scratch file Cue made) here or on that machine; gone already is fine.
fn remove_file(machine: &str, path: &str) {
    if machine.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = crate::safe_git::on_machine(machine, &format!("rm -f {}", crate::safe_git::sh_quote(path)));
    }
}

/// A branch for teammate `name`'s commit that isn't taken (`taken`): team/<name>, else team/<name>-2, -3….
fn free_branch(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug = slug.trim_matches('-');
    let base = format!("team/{}", if slug.is_empty() { "teammate" } else { slug });
    (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|b| !taken(b)).unwrap_or(base)
}

/// Why Commit won't go on `branch` (what `git symbolic-ref` says HEAD is): main and master are the main line, and a
/// detached HEAD is no branch at all. None: it's a branch of its own.
fn not_on(branch: &str) -> Option<String> {
    match branch {
        "" => Some("Not committed: its folder isn't on a branch (a detached HEAD). Make a branch and commit puts it on a branch of its own".into()),
        "main" | "master" => Some(format!("Not committed: its folder is on {branch}. Cue commits a teammate's work only on a branch of its own: Make a branch and commit puts it on a new one")),
        _ => None,
    }
}

/// What a teammate changed, as git has it now: its files with changes (in a worktree, everything changed there), their
/// diff and its size.
struct Changes {
    files: Vec<String>,
    diff: String,
    added: u64,
    removed: u64,
}

impl Changes {
    /// "2 files +140 −0".
    fn stat(&self) -> String {
        let n = self.files.len();
        if n == 0 {
            return String::new();
        }
        format!("{n} file{} +{} −{}", if n == 1 { "" } else { "s" }, self.added, self.removed)
    }
}

/// A teammate in a worktree of its own whose path is known.
fn has_worktree(t: &Teammate) -> bool {
    !t.worktree.is_empty() && t.worktree != "?"
}

/// The files git says changed in `t`'s own worktree (None: it has none, or git couldn't say).
fn worktree_files(machine: &str, lead_cwd: &str, t: &Teammate) -> Option<Vec<String>> {
    if !has_worktree(t) {
        return None;
    }
    status(machine, t.folder(lead_cwd), &[]).ok().map(|(files, _)| files)
}

/// What a teammate that just finished keeps from git's look at its changes (`c`; None when git couldn't say): their
/// size; in its own worktree, every file changed there; in the lead's folder, its files read off its shell commands
/// that didn't change after all (a command that failed, a file put back) let go.
fn finished_files(m: &mut Teammate, c: Option<&Changes>) {
    let Some(c) = c else {
        m.stat.clear();
        return;
    };
    m.stat = c.stat();
    if has_worktree(m) {
        add_files(m, c.files.iter().cloned(), false);
    } else {
        // git's paths are from the repository's top, the row's from the lead's folder (which may be under it).
        let changed = |f: &String| c.files.iter().any(|g| g == f || g.ends_with(&format!("/{f}")));
        let unchanged: Vec<String> = m.via_shell.iter().filter(|f| !changed(f)).cloned().collect();
        m.files.retain(|f| !unchanged.contains(f));
        m.via_shell.retain(|f| !unchanged.contains(f));
    }
}

/// What `git status` says changed in `dir` (under `only`, when given): every path, and the untracked ones.
fn status(machine: &str, dir: &str, only: &[&str]) -> Result<(Vec<String>, Vec<String>), String> {
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    let mut args: Vec<&str> = vec!["status", "--porcelain=v1", "-z", "--untracked-files=all"];
    if !only.is_empty() {
        args.push("--");
        args.extend(only.iter().copied());
    }
    let (out, code) = crate::safe_git::run(machine, dir, &filters, &args)?;
    if code != 0 {
        return Err(format!("git couldn't read {dir}"));
    }
    let mut files: Vec<String> = vec![];
    let mut untracked: Vec<String> = vec![];
    let mut parts = out.split('\0').filter(|p| p.len() > 3);
    while let Some(p) = parts.next() {
        let (xy, path) = p.split_at(3);
        if xy.starts_with('R') || xy.starts_with('C') {
            parts.next(); // a rename's old path follows it
        }
        if xy.starts_with("??") {
            untracked.push(path.to_string());
        }
        files.push(path.to_string());
    }
    Ok((files, untracked))
}

/// `t`'s changes now. Its files as the stream says them (its own edits), or, in a worktree of its own, whatever changed
/// there (its shell commands' changes too). New files count as all added.
fn changes(machine: &str, lead_cwd: &str, t: &Teammate) -> Result<Changes, String> {
    let dir = t.folder(lead_cwd);
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    let git = |args: &[&str]| crate::safe_git::run(machine, dir, &filters, args);
    let mine: Vec<&str> = t.files.iter().map(String::as_str).collect();
    if t.worktree.is_empty() && mine.is_empty() {
        return Ok(Changes { files: vec![], diff: String::new(), added: 0, removed: 0 });
    }
    // What git says changed: under its files only, or anywhere in its own worktree.
    let (files, untracked) = status(machine, dir, if t.worktree.is_empty() { &mine } else { &[] })?;
    let tracked: Vec<&str> = files.iter().filter(|f| !untracked.contains(f)).map(String::as_str).collect();
    let mut diff = String::new();
    let (mut added, mut removed) = (0, 0);
    if !tracked.is_empty() {
        let q = crate::safe_git::QUIET_DIFF;
        let args: Vec<&str> = ["diff", "HEAD"].into_iter().chain(q).chain(["--"]).chain(tracked.iter().copied()).collect();
        diff.push_str(&git(&args)?.0);
        let args: Vec<&str> = ["diff", "HEAD", "--numstat", "--"].into_iter().chain(tracked.iter().copied()).collect();
        for l in git(&args)?.0.lines() {
            let mut w = l.split('\t');
            added += w.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
            removed += w.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        }
    }
    for f in &untracked {
        let (d, _) = git(&["diff", "--no-index", "--no-color", "--no-ext-diff", "--", "/dev/null", f])?;
        added += d.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count() as u64;
        diff.push_str(&d);
    }
    if diff.len() > DIFF_LIMIT {
        let mut cut = DIFF_LIMIT;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n… (cut here: the rest is in git)\n");
    }
    Ok(Changes { files, diff, added, removed })
}

fn first(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(200).collect()
}
