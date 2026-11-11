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

/// How often finished crew members are looked at for work committed (or thrown away) outside Cue.
const TIDY_MS: u64 = 60_000;

/// Where a finished crew member's work ended up, when it's settled.
enum Settled {
    /// Every one of its files is committed, and at least one in a commit since it started (that commit, short).
    Committed(String),
    /// Every file is clean, but nothing was committed since it started: its changes were thrown away.
    Gone,
}

/// Its files, asked of the git repo each is in (a crew member can work outside its lead's folder): None while any
/// has uncommitted changes, or git can't say (not in a repo, or ignored there; then it stays for you to see to).
fn settled(lead_cwd: &str, t: &Teammate) -> Option<Settled> {
    let since = t.started_ms / 1000;
    let mut newest: Option<(u64, String)> = None;
    // Whether any of its files is one git keeps: a member whose files are all scratch (in no repository, or ignored)
    // can't be told committed from thrown away, and stays for you.
    let mut kept = false;
    for mut r in repos("", lead_cwd, t, false) {
        // Files in no repository (scratch, screenshots in /tmp): nothing of git's to settle; the rest decide.
        if r.top.is_empty() {
            continue;
        }
        let dir = r.top.clone();
        let filters = crate::safe_git::no_repo_filters("", &dir);
        let git = |args: &[&str]| crate::safe_git::run("", &dir, &filters, args).ok();
        // Ignored files are always "clean" to git (build output, screenshots): left out, the rest decide.
        let ignored: Vec<String> = {
            let args: Vec<String> = ["check-ignore", "--"].iter().map(|s| s.to_string()).chain(r.paths.iter().cloned()).collect();
            git(&args.iter().map(String::as_str).collect::<Vec<_>>()).map(|(out, _)| out.lines().map(str::to_string).collect()).unwrap_or_default()
        };
        r.paths.retain(|p| !ignored.iter().any(|i| i == p || p.ends_with(&format!("/{i}"))));
        if r.paths.is_empty() {
            continue;
        }
        kept = true;
        let with = |head: &[&'static str]| -> Vec<String> { head.iter().map(|s| s.to_string()).chain(["--".to_string()]).chain(r.paths.iter().cloned()).collect() };
        let run = |head: &[&'static str]| {
            let args = with(head);
            git(&args.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let (out, code) = run(&["status", "--porcelain=v1", "--untracked-files=all"])?;
        if code != 0 || !out.trim().is_empty() {
            return None;
        }
        // The newest commit with any of its files in this repo.
        let (log, code) = run(&["log", "-1", "--format=%ct %h"])?;
        if code != 0 {
            return None;
        }
        if let Some((at, sha)) = log.trim().split_once(' ').and_then(|(a, s)| Some((a.parse::<u64>().ok()?, s.to_string()))) {
            if at >= since && newest.as_ref().is_none_or(|(n, _)| at > *n) {
                newest = Some((at, sha));
            }
        }
    }
    if !kept {
        return None;
    }
    Some(newest.map_or(Settled::Gone, |(_, sha)| Settled::Committed(sha)))
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
        let origin = origin.clone();
        self.later(0, move |h| {
            for t in &ended {
                let c = changes(&machine, &cwd, t).ok();
                h.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| finished_files(m, c.as_ref())));
            }
            // Stopped having written nothing: nothing to review or commit, so it leaves the crew (set aside).
            for t in &ended {
                h.write(|st| st.sessions.with_teammate(&sid, &t.call, |m| {
                    if m.state == "stopped" && m.files.is_empty() {
                        m.state = "aside".into();
                        m.step = "Stopped having written nothing".into();
                    }
                }));
            }
            h.changed();
            // Its review runs by itself: there's nothing for you to decide before it (docs/dev/cue-teams.md §5).
            for t in ended {
                h.review_on_finish(&sid, &origin, &t.call);
            }
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
    /// SHORTCUT: the next prompted reply with a Recommendation line is taken as the review (one without is some other
    /// reply: a hook's re-prompt, a "started it" line); a review that never says Recommendation leaves its teammate under
    /// review. Upgrade: match the fork's own Agent call (its task_notification) to the review sent.
    pub(super) fn team_reply(&self, lead: &str, text: &str, unprompted: bool) {
        if unprompted {
            return;
        }
        let filed = self.write(|st| {
            let t = st.sessions.team(lead).into_iter().filter(|t| t.state == "reviewing").min_by_key(|t| t.review_ms)?;
            let (findings, recommendation) = read_review(text);
            // Not a review: the lead's other replies while one is under review (one prompted by a hook's feedback,
            // say) have no Recommendation line; the teammate stays under review for the one that does.
            if recommendation.is_empty() {
                return None;
            }
            let next = auto_next(&st.sessions.crew_commit(lead), &recommendation, findings.len(), t.fix_rounds);
            st.sessions.with_teammate(lead, &t.call, |m| {
                m.state = "reviewed".into();
                m.findings = findings;
                m.recommendation = recommendation;
                m.review = text.to_string();
            });
            Some((t.call, next))
        });
        let Some((call, next)) = filed else { return };
        self.changed();
        let lead = lead.to_string();
        match next {
            AutoNext::Commit => self.later(0, move |h| h.commit_when_clean(&lead, &call)),
            AutoNext::Fixes => self.later(0, move |h| {
                if let Err(e) = h.team_action(&lead, &call, "fixes") {
                    h.set_teammate(&lead, &call, |m| m.step = format!("Fixes not sent by themselves: {e}"));
                }
            }),
            AutoNext::Ask => {}
        }
    }

    /// A crew member that just finished, reviewed by itself through its lead's review queue: only one that finished its
    /// work (not stopped), and not a loop's (a loop reviews its own crew).
    fn review_on_finish(&self, lead: &str, origin: &Origin, call: &str) {
        #[cfg(feature = "ext")]
        if self.ext_leads_loop(lead) {
            return;
        }
        let Some(t) = self.read(|st| st.sessions.team(lead)).into_iter().find(|t| t.call == call) else { return };
        if t.state != "done" || t.agent_id.is_empty() || !t.from.is_empty() {
            return;
        }
        if let Err(e) = self.review_teammate(lead, origin, &t) {
            eprintln!("cue: couldn't queue {}'s review: {e}", t.name);
        }
    }

    /// The lead's "commit when clean": a crew member whose review recommends commit goes on a branch of its own (never
    /// main); held or refused, it stays reviewed with Commit for you, and why is on its row.
    fn commit_when_clean(&self, lead: &str, call: &str) {
        let (origin, t) = self.read(|st| (st.sessions.origin(lead), st.sessions.team(lead).into_iter().find(|t| t.call == call)));
        let (Some(origin), Some(t)) = (origin, t) else { return };
        if t.state != "reviewed" {
            return;
        }
        if let Err(e) = self.commit_teammate(lead, &origin, &t, true) {
            self.set_teammate(lead, call, |m| m.step = format!("Not committed by itself: {e}"));
        }
    }

    /// Send fixes to a teammate carried over a handoff: its process ended with its old lead, so the lead starts a new one
    /// under its name with the findings.
    fn fixes_carried_over(&self, lead: &str, t: &Teammate) -> Result<String, String> {
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
            m.fix_rounds += 1;
            m.step = "Fixes sent: its lead starts it again".into();
            m.step_ms = now_ms();
        });
        Ok(format!("Asked its lead to start {} again with the fixes", t.name))
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
            "fixes" if !t.from.is_empty() => self.fixes_carried_over(lead, &t),
            "fixes" => {
                let text = format!(
                    "Send your teammate {name} its review's findings with SendMessage (to: \"{name}\") and let it fix them; don't fix them yourself:\n{}",
                    if t.findings.is_empty() { "(the review listed none: ask it to check its work against what it was asked)".to_string() } else { t.findings.join("\n") },
                    name = t.name
                );
                self.send_to_session(lead, &text, &[], false)?;
                self.set_teammate(lead, &t.call, |m| {
                    crate::sessions::team::back_at_work(m, now_ms());
                    m.fix_rounds += 1;
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
        self.team_tidy_tick(now);
    }

    /// At most every TIDY_MS, a tidy pass off the tick's thread (`team_tidy`).
    fn team_tidy_tick(&self, now: u64) {
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let last = LAST.load(std::sync::atomic::Ordering::Relaxed);
        if now >= last && now - last < TIDY_MS {
            return;
        }
        LAST.store(now, std::sync::atomic::Ordering::Relaxed);
        self.later(0, |h| {
            if h.team_tidy() {
                h.changed();
            }
        });
    }

    /// The crew tidied: a member that stopped having written nothing is set aside; a finished one whose files are all
    /// committed since it started (by Cue's Commit, or by you elsewhere) leaves as committed; one whose files are clean
    /// with nothing committed since it started (its changes thrown away) is set aside. Never a loop's crew (a loop
    /// commits its own) and never one on another machine (SHORTCUT: git there is a remote call per file each pass;
    /// upgrade: one `git status` per folder through cue-host). Runs git: call it off the hub's threads. Whether it
    /// changed anything.
    pub(crate) fn team_tidy(&self) -> bool {
        let leads: Vec<(Origin, Vec<Teammate>)> = self.read(|st| {
            st.sessions.all().into_iter().filter(|s| s.team.iter().any(|t| matches!(t.state.as_str(), "done" | "reviewed" | "stopped"))).map(|s| (s.origin.clone(), s.team.clone())).collect()
        });
        let mut changed = false;
        for (origin, team) in leads {
            #[cfg(feature = "ext")]
            if self.ext_leads_loop(&origin.session_id) {
                continue;
            }
            for t in team.iter().filter(|t| matches!(t.state.as_str(), "done" | "reviewed" | "stopped")) {
                let left = if t.files.is_empty() {
                    (t.state == "stopped").then(|| ("aside", String::new(), "Stopped having written nothing".to_string()))
                } else if origin.machine.is_empty() {
                    match settled(&origin.cwd, t) {
                        Some(Settled::Committed(sha)) => Some(("committed", sha, "Committed outside Cue".to_string())),
                        Some(Settled::Gone) => Some(("aside", String::new(), "Its changes are gone: nothing left to commit".to_string())),
                        None => None,
                    }
                } else {
                    None
                };
                let Some((state, sha, why)) = left else { continue };
                self.write(|st| st.sessions.with_teammate(&origin.session_id, &t.call, |m| {
                    m.state = state.into();
                    m.commit = sha;
                    m.step = why;
                }));
                changed = true;
            }
        }
        changed
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
        let files = changes(&origin.machine, &origin.cwd, t).map(|c| c.files()).unwrap_or_else(|_| t.files.clone());
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
    /// teammate's goes in, staged or not. Files in more than one repository: each repo's are committed in that repo,
    /// with the same message, and what happened is said per repo.
    /// `branch`: on main, master or a detached HEAD, the commit goes on a new branch crew/<name> made at HEAD
    /// (`commit_to_new_branch`); the folder stays where it is.
    fn commit_teammate(&self, lead: &str, origin: &Origin, t: &Teammate, branch: bool) -> Result<String, String> {
        if t.working() || t.agent_id.is_empty() {
            return Err(format!("{} is still working", t.name));
        }
        if let Some(other) = t.also.first() {
            return Err(format!("Held: {other} has uncommitted changes in one of {}'s files", t.name));
        }
        let c = changes(&origin.machine, &origin.cwd, t)?;
        let todo: Vec<&RepoChanges> = c.repos.iter().filter(|r| !r.files.is_empty()).collect();
        // Its files git couldn't read, or that are in no repository: never committed, and said so.
        let unread = c.unread.iter().map(|f| format!("{f}: git couldn't read its repository, not committed"));
        let left: Vec<String> = unread.chain(c.outside.iter().map(|f| format!("{f}: not in a git repository, not committed"))).collect();
        if todo.is_empty() {
            // Its work committed already (by you, elsewhere; this Mac's git only): it leaves as committed, as the tidy has it.
            if let (true, Some(Settled::Committed(sha))) = (origin.machine.is_empty(), settled(&origin.cwd, t)) {
                self.set_teammate(lead, &t.call, |m| {
                    m.state = "committed".into();
                    m.commit = sha.clone();
                    m.step = "Committed outside Cue".into();
                });
                return Ok(format!("{}'s work is already committed ({sha})", t.name));
            }
            let why = if left.is_empty() { String::new() } else { format!("\n{}", left.join("\n")) };
            return Err(format!("{} has nothing to commit in git{why}", t.name));
        }
        let m = origin.machine.as_str();
        let single = todo.len() == 1;
        let mut said: Vec<String> = vec![];
        let mut shas: Vec<String> = vec![];
        let mut failed = false;
        for r in todo {
            let place = if single { String::new() } else { format!("In {}: ", r.name()) };
            match commit_in(m, &r.top, t, &r.files, branch) {
                Ok((line, sha)) => {
                    said.push(format!("{place}{line}"));
                    shas.push(sha);
                }
                Err(e) => {
                    failed = true;
                    said.push(format!("{place}{e}"));
                }
            }
        }
        said.extend(left);
        // One repo's commit failed, or one couldn't be read: said as such, beside what the others committed, and it
        // stays uncommitted (Commit again does the rest).
        if failed || !c.unread.is_empty() {
            return Err(said.join("\n"));
        }
        let sha = shas.join(" ");
        self.set_teammate(lead, &t.call, |mate| {
            mate.state = "committed".into();
            mate.commit = sha;
        });
        Ok(said.join("\n"))
    }
}

/// Teammate `t`'s `files` (paths from the top of the repo at `dir`) committed there: on the branch it's on, or, on
/// main, master or a detached HEAD, refused (`branch` false) or on a new branch (`commit_to_new_branch`). What to
/// say, and the commit (short).
fn commit_in(m: &str, dir: &str, t: &Teammate, files: &[String], branch: bool) -> Result<(String, String), String> {
    let filters = crate::safe_git::no_repo_filters(m, dir);
    let git = |args: &[&str]| crate::safe_git::run(m, dir, &filters, args);
    // Never on the main line: a teammate's work goes on a branch of its own (its worktree's, or one you made).
    let on = git(&["symbolic-ref", "--short", "-q", "HEAD"]).map(|(o, _)| o.trim().to_string()).unwrap_or_default();
    if let Some(why) = not_on(&on) {
        if !branch {
            return Err(why);
        }
        return commit_to_new_branch(m, dir, t, files, &on);
    }
    let made = if branch { on } else { String::new() };
    let paths: Vec<&str> = files.iter().map(String::as_str).collect();
    let add: Vec<&str> = ["add", "-A", "--"].into_iter().chain(paths.iter().copied()).collect();
    let (_, code) = git(&add)?;
    if code != 0 {
        return Err(format!("git couldn't add {}'s files", t.name));
    }
    let message = commit_message(t);
    let commit: Vec<&str> = ["commit", "--only", "-q", "-m", message.as_str(), "--"].into_iter().chain(paths.iter().copied()).collect();
    let (_, code) = git(&commit)?;
    if code != 0 {
        return Err(format!("git didn't commit {}'s files (its hooks, or nothing to commit)", t.name));
    }
    let sha = git(&["rev-parse", "--short", "HEAD"]).map(|(o, _)| o.trim().to_string()).unwrap_or_default();
    let n = files.len();
    let on = if made.is_empty() { String::new() } else { format!(" on {made}") };
    Ok((format!("Committed {}'s {n} file{}{on} ({sha})", t.name, if n == 1 { "" } else { "s" }), sha))
}

/// The message a teammate's commit gets: what it was asked (short), and its last words.
fn commit_message(t: &Teammate) -> String {
    let subject = if t.what.is_empty() { format!("{}'s work", t.name) } else { t.what.clone() };
    if t.summary.is_empty() {
        subject
    } else {
        format!("{subject}\n\n{}", t.summary)
    }
}

/// Make a branch and commit, for a repo on main, master or a detached HEAD (often the main checkout, where other
/// sessions keep uncommitted work): crew/<name> made at HEAD with `files` committed on it, through a temporary
/// index. The folder's HEAD, index and files stay as they are; the branch appears only once its commit is made, so a
/// failure leaves none. What to say, and the commit (short).
fn commit_to_new_branch(m: &str, dir: &str, t: &Teammate, files: &[String], on: &str) -> Result<(String, String), String> {
    let filters = crate::safe_git::no_repo_filters(m, dir);
    let git = |args: &[&str]| crate::safe_git::run(m, dir, &filters, args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o.trim().to_string());
    let head = git(&["rev-parse", "-q", "--verify", "HEAD^{commit}"]).ok_or("Not committed: its folder has no commit yet to make a branch from")?;
    let git_dir = git(&["rev-parse", "--absolute-git-dir"]).ok_or("Not committed: git couldn't find the folder's repository")?;
    let made = free_branch(&t.name, |b| git(&["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")]).is_some());
    let index = format!("{}/cue-team-index-{}", git_dir.trim_end_matches('/'), now_ms());
    let with_index = |args: &[&str]| crate::safe_git::run_env(m, dir, &filters, &[("GIT_INDEX_FILE", index.as_str())], args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o.trim().to_string());
    let message = commit_message(t);
    let add: Vec<&str> = ["add", "-A", "--"].into_iter().chain(files.iter().map(String::as_str)).collect();
    let made_it = with_index(&["read-tree", &head])
        .and_then(|_| with_index(&add))
        .and_then(|_| with_index(&["write-tree"]))
        .and_then(|tree| git(&["commit-tree", &tree, "-p", &head, "-m", &message]))
        .and_then(|commit| git(&["update-ref", "-m", "cue: make a branch and commit", &format!("refs/heads/{made}"), &commit, ""]).map(|_| commit));
    remove_file(m, &index);
    let commit = made_it.ok_or_else(|| format!("Not committed: git couldn't commit {}'s files on a new branch, so none was made", t.name))?;
    let sha: String = commit.chars().take(7).collect();
    let n = files.len();
    let stays = if on.is_empty() { "its detached HEAD".to_string() } else { on.to_string() };
    Ok((format!("Committed {n} file{} to {made} ({sha}); your folder stays on {stays}.", if n == 1 { "" } else { "s" }), sha))
}

/// Remove `path` (a scratch file Cue made) here or on that machine; gone already is fine.
fn remove_file(machine: &str, path: &str) {
    if machine.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = crate::safe_git::on_machine(machine, &format!("rm -f {}", crate::safe_git::sh_quote(path)));
    }
}

/// A branch for teammate `name`'s commit that isn't taken (`taken`): crew/<name>, else crew/<name>-2, -3….
fn free_branch(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug = slug.trim_matches('-');
    let base = format!("crew/{}", if slug.is_empty() { "member" } else { slug });
    (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|b| !taken(b)).unwrap_or(base)
}

/// Why Commit won't go on `branch` (what `git symbolic-ref` says HEAD is): main and master are the main line, and a
/// detached HEAD is no branch at all. None: it's a branch of its own.
fn not_on(branch: &str) -> Option<String> {
    match branch {
        "" => Some("Not committed: its folder isn't on a branch (a detached HEAD). Make a branch and commit puts it on a branch of its own".into()),
        "main" | "master" => Some(format!("Not committed: its folder is on {branch}. Cue commits crew work only on a branch of its own: Make a branch and commit puts it on a new one")),
        _ => None,
    }
}

/// What a teammate changed, as git has it now, per repository its files are in: their diff and its size.
#[derive(Default)]
struct Changes {
    repos: Vec<RepoChanges>,
    diff: String,
    added: u64,
    removed: u64,
    /// Its files in a repository git failed to read, as the stream gave them.
    unread: Vec<String>,
    /// Its files in no git repository, as the stream gave them.
    outside: Vec<String>,
}

/// One repository's part of a teammate's changes.
struct RepoChanges {
    /// The repo's top folder.
    top: String,
    /// What its paths are shown under: "" in the teammate's own folder's repo, else the repo's folder name.
    label: String,
    /// Its files with changes, from the repo's top (in a worktree of its own, everything changed there).
    files: Vec<String>,
    /// Those of the teammate's files (as the stream gave them) git says changed.
    changed: Vec<String>,
    /// Its own worktree, read whole.
    whole: bool,
}

impl RepoChanges {
    /// What the repo is called when said: its label, else its folder name.
    fn name(&self) -> &str {
        if self.label.is_empty() {
            folder_name(&self.top)
        } else {
            &self.label
        }
    }
}

impl Changes {
    /// Every file with changes, as shown: from its repo's top in the teammate's own repo, else with the repo's
    /// folder name first ("other-repo/mac/ui/addon.js").
    fn files(&self) -> Vec<String> {
        self.repos.iter().flat_map(|r| r.files.iter().map(move |f| if r.label.is_empty() { f.clone() } else { format!("{}/{f}", r.label) })).collect()
    }

    /// "2 files +140 −0".
    fn stat(&self) -> String {
        let n: usize = self.repos.iter().map(|r| r.files.len()).sum();
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
/// size; in its own worktree, every file changed there; elsewhere, its files read off its shell commands that didn't
/// change after all (a command that failed, a file put back) let go.
fn finished_files(m: &mut Teammate, c: Option<&Changes>) {
    let Some(c) = c else {
        m.stat.clear();
        return;
    };
    m.stat = c.stat();
    if has_worktree(m) {
        add_files(m, c.repos.iter().filter(|r| r.whole).flat_map(|r| r.files.iter().cloned()), false);
    } else {
        // A file git couldn't read, or that's in no repository, stays: there's no telling.
        let changed = |f: &String| c.unread.contains(f) || c.outside.contains(f) || c.repos.iter().any(|r| r.changed.contains(f));
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

/// The top folder of the git repository folder `dir` is in, None when it's in none. Here, the nearest folder of it
/// that exists is asked (a file's folder may be gone); on a machine, `dir` as it is (nothing there is looked up from
/// this Mac).
fn repo_top(machine: &str, dir: &str) -> Option<String> {
    let dir = if machine.is_empty() {
        let mut d = std::path::PathBuf::from(dir);
        while !d.is_dir() {
            d = d.parent()?.to_path_buf();
        }
        d.to_string_lossy().to_string()
    } else {
        dir.to_string()
    };
    let (out, code) = crate::safe_git::run(machine, &dir, &[], &["rev-parse", "--show-toplevel"]).ok()?;
    let top = out.trim();
    (code == 0 && !top.is_empty()).then(|| top.to_string())
}

/// One git repository some of a teammate's files are in (or none).
struct Repo {
    /// Its top folder; "" for files in no git repository.
    top: String,
    /// What its paths are shown under: "" in the teammate's own folder's repo, else the repo's folder name.
    label: String,
    /// The teammate's files in it, as the stream gave them (absolute, or relative to its folder).
    given: Vec<String>,
    /// The same, absolute: what git is handed, run in `top`.
    paths: Vec<String>,
    /// Its own worktree, read whole: everything changed there is its (its files there aren't listed).
    whole: bool,
}

/// `t`'s files grouped by the git repository each is in: its own folder's repo first, then the others, then those in
/// none. `whole`: a teammate in a worktree of its own has that worktree read whole.
fn repos(machine: &str, lead_cwd: &str, t: &Teammate, whole: bool) -> Vec<Repo> {
    let folder = t.folder(lead_cwd).trim_end_matches('/');
    let home = repo_top(machine, folder);
    let whole = whole && !t.worktree.is_empty();
    let mut out: Vec<Repo> = vec![];
    if let Some(top) = home.as_ref().filter(|_| whole) {
        out.push(Repo { top: top.clone(), label: String::new(), given: vec![], paths: vec![], whole: true });
    }
    // Each folder asked once.
    let mut asked: Vec<(String, Option<String>)> = vec![];
    for f in &t.files {
        let inside = !f.starts_with('/');
        let abs = if inside { format!("{folder}/{f}") } else { f.clone() };
        let top = if inside {
            home.clone()
        } else {
            let parent = match abs.rsplit_once('/') {
                Some(("", _)) | None => "/".to_string(),
                Some((d, _)) => d.to_string(),
            };
            match asked.iter().find(|(d, _)| *d == parent) {
                Some((_, top)) => top.clone(),
                None => {
                    let top = repo_top(machine, &parent);
                    asked.push((parent, top.clone()));
                    top
                }
            }
        };
        if whole && top.is_some() && top == home {
            continue;
        }
        let top = top.unwrap_or_default();
        let i = match out.iter().position(|r| r.top == top && !r.whole) {
            Some(i) => i,
            None => {
                let mine = top.is_empty() || Some(&top) == home.as_ref();
                let label = if mine { String::new() } else { folder_name(&top).to_string() };
                out.push(Repo { top, label, given: vec![], paths: vec![], whole: false });
                out.len() - 1
            }
        };
        out[i].given.push(f.clone());
        out[i].paths.push(abs);
    }
    tell_apart(&mut out, home.as_deref());
    out.sort_by_key(|r| (r.top.is_empty(), !r.label.is_empty()));
    out
}

/// The last folder of `path` ("app" for /work/app).
fn folder_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// Two repos with the same folder name (two checkouts called "app"): each labelled one that shares its name with
/// another repo (its own folder's repo, `home`, too) gets its parent folder first ("work/app").
fn tell_apart(repos: &mut [Repo], home: Option<&str>) {
    let tops: Vec<String> = repos.iter().map(|r| r.top.clone()).filter(|t| !t.is_empty()).chain(home.map(str::to_string)).collect();
    for r in repos.iter_mut().filter(|r| !r.label.is_empty()) {
        if tops.iter().any(|t| *t != r.top && folder_name(t) == r.label) {
            let parent = r.top.trim_end_matches('/').rsplit_once('/').map_or("", |(p, _)| folder_name(p));
            r.label = format!("{parent}/{}", r.label);
        }
    }
}

/// `path` (absolute) from the top of the repo at `top`, when it's in it. A Mac's /tmp and /var are /private/tmp and
/// /private/var to git (the folder resolved), while a path may come as it was given.
fn from_top(top: &str, path: &str) -> Option<String> {
    let bare = |p: &str| p.strip_prefix("/private").filter(|r| r.starts_with('/')).unwrap_or(p).to_string();
    let (top, path) = (bare(top), bare(path));
    path.strip_prefix(top.trim_end_matches('/')).and_then(|r| r.strip_prefix('/')).map(str::to_string)
}

/// `t`'s changes now, per repository its files are in. Its files as the stream says them (its own edits), or, in a
/// worktree of its own, whatever changed there (its shell commands' changes too). New files count as all added. A
/// file in no repository is said so in the diff; an error only when git failed and nothing else could be said.
fn changes(machine: &str, lead_cwd: &str, t: &Teammate) -> Result<Changes, String> {
    let mut c = Changes::default();
    if t.worktree.is_empty() && t.files.is_empty() {
        return Ok(c);
    }
    let mut failed: Option<String> = None;
    for r in repos(machine, lead_cwd, t, true) {
        if r.top.is_empty() {
            for f in &r.given {
                c.diff.push_str(&format!("{f}: not in a git repository\n"));
            }
            c.outside.extend(r.given);
            continue;
        }
        match repo_changes(machine, &r) {
            Ok((part, diff, added, removed)) => {
                c.diff.push_str(&diff);
                c.added += added;
                c.removed += removed;
                c.repos.push(part);
            }
            Err(e) => {
                c.diff.push_str(&format!("{}: {e}\n", if r.label.is_empty() { r.top.as_str() } else { r.label.as_str() }));
                c.unread.extend(r.given);
                failed = Some(e);
            }
        }
    }
    if let Some(e) = failed {
        if c.repos.is_empty() && c.outside.is_empty() {
            return Err(e);
        }
    }
    if c.diff.len() > DIFF_LIMIT {
        let mut cut = DIFF_LIMIT;
        while !c.diff.is_char_boundary(cut) {
            cut -= 1;
        }
        c.diff.truncate(cut);
        c.diff.push_str("\n… (cut here: the rest is in git)\n");
    }
    Ok(c)
}

/// One repo's part of a teammate's changes: what changed, its diff (paths under the repo's label when it has one),
/// lines added and removed.
fn repo_changes(machine: &str, r: &Repo) -> Result<(RepoChanges, String, u64, u64), String> {
    let dir = r.top.as_str();
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    let git = |args: &[&str]| crate::safe_git::run(machine, dir, &filters, args);
    let only: Vec<&str> = r.paths.iter().map(String::as_str).collect();
    // What git says changed: under its files only, or anywhere in its own worktree.
    let (files, untracked) = status(machine, dir, if r.whole { &[] } else { &only })?;
    let tracked: Vec<&str> = files.iter().filter(|f| !untracked.contains(f)).map(String::as_str).collect();
    let prefixes = if r.label.is_empty() { vec![] } else { vec![format!("--src-prefix=a/{}/", r.label), format!("--dst-prefix=b/{}/", r.label)] };
    let prefixes: Vec<&str> = prefixes.iter().map(String::as_str).collect();
    let mut diff = String::new();
    let (mut added, mut removed) = (0, 0);
    if !tracked.is_empty() {
        let q = crate::safe_git::QUIET_DIFF;
        let args: Vec<&str> = ["diff", "HEAD"].into_iter().chain(q).chain(prefixes.iter().copied()).chain(["--"]).chain(tracked.iter().copied()).collect();
        diff.push_str(&git(&args)?.0);
        let args: Vec<&str> = ["diff", "HEAD", "--numstat", "--"].into_iter().chain(tracked.iter().copied()).collect();
        for l in git(&args)?.0.lines() {
            let mut w = l.split('\t');
            added += w.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
            removed += w.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        }
    }
    for f in &untracked {
        let args: Vec<&str> = ["diff", "--no-index", "--no-color", "--no-ext-diff"].into_iter().chain(prefixes.iter().copied()).chain(["--", "/dev/null", f.as_str()]).collect();
        let (d, _) = git(&args)?;
        added += d.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count() as u64;
        diff.push_str(&d);
    }
    // Which of its files (as given) changed: by path from the top, else by the end of the path.
    let changed = r
        .given
        .iter()
        .zip(&r.paths)
        .filter(|(_, abs)| match from_top(dir, abs) {
            Some(rel) => files.contains(&rel),
            None => files.iter().any(|g| abs.ends_with(&format!("/{g}"))),
        })
        .map(|(g, _)| g.clone())
        .collect();
    let part = RepoChanges { top: r.top.clone(), label: r.label.clone(), files, changed, whole: r.whole };
    Ok((part, diff, added, removed))
}

/// How many times Cue sends a teammate its findings by itself before the next review waits for you.
pub(crate) const AUTO_FIX_ROUNDS: u32 = 2;

/// What Cue does by itself once a teammate's review is in.
#[derive(Debug, PartialEq)]
enum AutoNext {
    Commit,
    Fixes,
    Ask,
}

/// With the lead handling reviews (`crew_commit` "", the default; "wait": ask you every time): a review that asks for
/// fixes, or says commit but lists findings (notes too: nothing is dropped), goes back to the teammate, up to
/// `AUTO_FIX_ROUNDS` times; a clean commit is committed to its branch. Anything else waits for you.
fn auto_next(crew_commit: &str, recommendation: &str, findings: usize, fix_rounds: u32) -> AutoNext {
    if crew_commit == "wait" {
        return AutoNext::Ask;
    }
    let commit = recommendation.starts_with("commit");
    if recommendation.contains("fix") || (commit && findings > 0) {
        return if fix_rounds < AUTO_FIX_ROUNDS { AutoNext::Fixes } else { AutoNext::Ask };
    }
    if commit { AutoNext::Commit } else { AutoNext::Ask }
}

fn first(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(200).collect()
}
