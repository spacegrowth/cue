//! A hosted lead's crew tidied (hub/team.rs has the rest): a finished member whose work was committed (by Cue, by its
//! lead, or by you elsewhere) or thrown away leaves the crew by itself, so nobody has to set it aside. A member whose
//! files Cue never saw is committed by its lead, asked by Cue; its reply says what it committed.

use super::team::{first, free_branch, repos};
use super::*;
use super::team_review::{didnt_come_back, LeadTurn};
use crate::sessions::team::{read_committed, Teammate};

/// How often finished crew members are looked at for work committed (or thrown away) outside Cue.
const TIDY_MS: u64 = 60_000;

/// Where a finished crew member's work ended up, when it's settled.
pub(super) enum Settled {
    /// Every one of its files is committed, and at least one in a commit since it started (that commit, short).
    Committed(String),
    /// Every file is clean, but nothing was committed since it started: its changes were thrown away.
    Gone,
}

/// Its files, asked of the git repo each is in (a crew member can work outside its lead's folder): None while any
/// has uncommitted changes, or git can't say (not in a repo, or ignored there; then it stays for you to see to).
pub(super) fn settled(lead_cwd: &str, t: &Teammate) -> Option<Settled> {
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

impl Hub {
    /// At most every TIDY_MS, a tidy pass off the tick's thread (`team_tidy`).
    pub(super) fn team_tidy_tick(&self, now: u64) {
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let last = LAST.load(std::sync::atomic::Ordering::Relaxed);
        if now >= last && now - last < TIDY_MS {
            return;
        }
        LAST.store(now, std::sync::atomic::Ordering::Relaxed);
        self.later(0, |h| {
            h.team_tidy();
        });
    }

    /// The crew tidied: a member that stopped having written nothing is set aside; a finished one whose files are all
    /// committed since it started (by Cue's Commit, or by you elsewhere) leaves as committed; one whose files are clean
    /// with nothing committed since it started (its changes thrown away) is set aside. Never a loop's crew (a loop
    /// commits its own) and never one on another machine (SHORTCUT: git there is a remote call per file each pass;
    /// upgrade: one `git status` per folder through cue-host). Members stuck waiting for a review are let go first
    /// (`unstick`). Runs git: call it off the hub's threads. Whether it changed anything (drawn when it did).
    pub(crate) fn team_tidy(&self) -> bool {
        let mut changed = self.write(|st| unstick(st, now_ms()));
        let leads: Vec<(Origin, Vec<Teammate>)> = self.read(|st| {
            st.sessions.all().into_iter().filter(|s| s.team.iter().any(|t| matches!(t.state.as_str(), "done" | "reviewing" | "reviewed" | "stopped"))).map(|s| (s.origin.clone(), s.team.clone())).collect()
        });
        for (origin, team) in leads {
            #[cfg(feature = "ext")]
            if self.ext_leads_loop(&origin.session_id) {
                continue;
            }
            // One whose lead was asked to commit it waits for the lead's reply (`commit_reply`).
            for t in team.iter().filter(|t| matches!(t.state.as_str(), "done" | "reviewing" | "reviewed" | "stopped") && t.commit_asked_ms == 0 && t.commit_wait_ms == 0) {
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
        if changed {
            self.changed();
        }
        changed
    }

    /// Commit on a member whose files Cue never saw (it wrote them with shell commands outside its lead's folder): its
    /// lead is asked to commit them, on a branch of their own, and to say what it committed; the tidy leaves it alone
    /// until the lead's reply (`commit_reply`). A busy lead isn't sent it then: the ask waits on the member's row
    /// (`commit_wait_ms`, never in the lead's chat) and goes once the lead is free (`drain_commit_asks`).
    pub(super) fn ask_lead_to_commit(&self, lead: &str, t: &Teammate) -> Result<String, String> {
        let now = now_ms();
        let go = self.write(|st| {
            let m = st.sessions.team(lead).into_iter().find(|m| m.call == t.call)?;
            if m.commit_asked_ms > 0 || m.commit_wait_ms > 0 {
                return Some(Err(format!("Its lead is already asked to commit {}'s work", t.name)));
            }
            let waits = Self::lead_waits(st, lead, now);
            st.sessions.with_teammate(lead, &t.call, |m| if waits { mark_waiting(m, now) } else { mark_asked(m, now) });
            if !waits {
                st.review_last.insert(lead.to_string(), now);
            }
            Some(Ok(!waits))
        });
        self.changed();
        match go.ok_or("Cue doesn't know that teammate")?? {
            false => Ok(format!("Cue asks its lead to commit {}'s work once it's free", t.name)),
            true => self.send_commit_asks(lead, std::slice::from_ref(t)).map(|_| format!("Asked its lead to commit {}'s work", t.name)),
        }
    }

    /// Lead `lead` can't be sent anything of Cue's now: busy (`lead_busy`: working, settling after Cue's last message,
    /// compacting, asking you something), or a message of yours kept for it (yours goes first).
    fn lead_waits(st: &Store, lead: &str, now: u64) -> bool {
        Self::lead_busy(st, lead, now).is_some() || st.sessions.held(lead).is_some()
    }

    /// Each free lead's members whose commit ask waits (`commit_wait_ms`), sent to it now, joined into one message.
    /// The lead then settles (`review_last`) before anything else of Cue's goes to it.
    pub(super) fn drain_commit_asks(&self) {
        let now = now_ms();
        let due: Vec<(String, Vec<Teammate>)> = self.write(|st| {
            let waiting = st.sessions.all().into_iter().filter(|s| s.team.iter().any(|t| t.commit_wait_ms > 0)).map(|s| s.origin.session_id.clone());
            let leads: Vec<String> = waiting.filter(|l| !Self::lead_waits(st, l, now)).collect();
            let mut due = vec![];
            for lead in leads {
                let mates: Vec<Teammate> = st.sessions.team(&lead).into_iter().filter(|t| t.commit_wait_ms > 0 && t.in_team() && !t.working()).collect();
                if mates.is_empty() {
                    continue;
                }
                for t in &mates {
                    st.sessions.with_teammate(&lead, &t.call, |m| mark_asked(m, now));
                }
                // Noted here, in the same step: a review taken off the queue meanwhile waits for this to settle.
                st.review_last.insert(lead.clone(), now);
                due.push((lead, mates));
            }
            due
        });
        for (lead, mates) in due {
            if let Err(e) = self.send_commit_asks(&lead, &mates) {
                eprintln!("cue: couldn't ask {lead} to commit its crew's work: {e}");
            }
        }
    }

    /// The commit asks of `mates` (marked asked already) sent to `lead` as one message; not sent, each row says so and
    /// waits on you.
    fn send_commit_asks(&self, lead: &str, mates: &[Teammate]) -> Result<String, String> {
        let text = mates.iter().map(|t| commit_words(&t.name)).collect::<Vec<_>>().join("\n\n");
        let sent = self.send_to_session(lead, &text, &[], false);
        self.write(|st| {
            st.review_last.insert(lead.to_string(), now_ms());
            if let Err(e) = &sent {
                for t in mates {
                    st.sessions.with_teammate(lead, &t.call, |m| {
                        m.commit_asked_ms = 0;
                        m.step = format!("Couldn't ask its lead to commit it: {e}");
                        m.needs = "Cue couldn't ask its lead to commit it".into();
                    });
                }
            }
        });
        self.changed();
        sent
    }
}

/// What a member's row says while Cue's ask to commit it waits for its lead to be free.
const COMMIT_WAITS: &str = "Cue asks its lead to commit it when it's free";

/// `m`'s commit ask waits for its lead to be free.
fn mark_waiting(m: &mut Teammate, now: u64) {
    m.commit_wait_ms = now;
    m.needs.clear();
    m.step = COMMIT_WAITS.into();
    m.step_ms = now;
}

/// `m`'s commit ask goes to its lead now.
fn mark_asked(m: &mut Teammate, now: u64) {
    m.commit_wait_ms = 0;
    m.commit_asked_ms = now;
    m.needs.clear();
    m.step = "Asked its lead to commit it".into();
    m.step_ms = now;
}

/// Members stuck in a state nothing will move them out of, put back to done (Review in their menu): one queued for a
/// review that isn't in Review's queue (nor on its way), and one under review whose lead has been free while
/// REVIEW_SHOWN_MS passed since its ask (a lead whose transcript Cue can't read, or an ask scrolled out of it: the turn
/// that answers it, team_review.rs `unanswered`, is the usual way). Whether any changed.
pub(super) fn unstick(st: &mut Store, now: u64) -> bool {
    let mut stuck: Vec<(String, String, bool)> = vec![];
    for s in st.sessions.all() {
        let lead = s.origin.session_id.clone();
        // Free as Review's queue judges it: not working, asking you something, compacting, or settling.
        let free = Hub::lead_busy(st, &lead, now).is_none();
        for t in &s.team {
            let key = format!("{}{lead}:{}", super::team::KEY, t.agent_id);
            let lost = t.state == "queued" && !st.review_queue.iter().any(|q| q.exec == key) && !st.review_sent.contains_key(&key);
            let long = t.state == "reviewing" && free && now.saturating_sub(t.review_ms) >= super::reviews::REVIEW_SHOWN_MS;
            if lost || long {
                stuck.push((lead.clone(), t.call.clone(), lost));
            }
        }
    }
    for (lead, call, lost) in &stuck {
        st.sessions.with_teammate(lead, call, |m| {
            didnt_come_back(m);
            if *lost {
                m.step = "Its review didn't go".into();
            }
        });
    }
    !stuck.is_empty()
}

/// What a lead is asked to commit its teammate `name`'s work with (files Cue never saw). Its answer names the
/// teammate, so asks joined into one message are answered each by its own line.
fn commit_words(name: &str) -> String {
    let branch = free_branch(name, |_| false);
    format!("Commit your teammate {name}'s work now: the files it changed, in the git repository where it changed them, on the branch that repo is on (not main/master: make a branch {branch} first if it is). Don't change anything else. End your reply with a line `Committed {name}: <short sha>` for each commit, or `Committed {name}: nothing` if there was nothing to commit.")
}

/// Lead `lead`'s turn as the answer to Cue's asks to commit members' work, and as its own word on what it committed.
/// A member asked whose ask has gone (not still kept for, or queued at, the lead): `Committed <name>: <sha>` commits it
/// (the first its commit), `Committed <name>: nothing` sets it aside, neither leaves it to you (`needs`), unless the
/// turn is a `review` (a review may answer an ask, but saying nothing of it is no answer) or unprompted (a teammate's
/// finish started it: only a commit it names counts). A finished member whose files Cue never saw, not asked (or its
/// ask still waiting): a `Committed <name>: <sha>` line is its lead committing it by itself, and it's committed (its
/// ask never goes). Whether it changed a member.
/// SHORTCUT: the lead's first prompted reply after an ask went is taken as its answer; a reply to something else of
/// yours typed meanwhile reads as "didn't say" and leaves it to you. Upgrade: match the reply to the turn the ask
/// started (`LeadTurn::said`, once Cue's asks are in the transcript every time).
pub(super) fn commit_reply(st: &mut Store, lead: &str, turn: &LeadTurn, review: bool) -> bool {
    let on_its_way = [st.sessions.held(lead).map(|h| h.text), st.sessions.queued_text(lead)].into_iter().flatten().collect::<Vec<_>>().join("\n");
    let team = st.sessions.team(lead);
    let asked = |t: &Teammate| t.commit_asked_ms > 0 && !on_its_way.contains(&commit_words(&t.name));
    let alone = team.iter().filter(|t| t.in_team() && asked(t)).count() == 1;
    let mut changed = false;
    for t in team.iter().filter(|t| t.in_team() && !t.working()) {
        let said = read_committed(&turn.all, &t.name, alone && asked(t));
        let answers = match &said {
            Some(shas) if !shas.is_empty() => asked(t) || (t.files.is_empty() && t.worktree.is_empty()),
            _ => asked(t) && !turn.unprompted && (said.is_some() || !review),
        };
        if answers {
            st.sessions.with_teammate(lead, &t.call, |m| settle_asked(m, said, turn.answer));
            changed = true;
        }
    }
    changed
}

/// Member `m` settled by what its lead `said` (`read_committed`) in reply `text`.
fn settle_asked(m: &mut Teammate, said: Option<Vec<String>>, text: &str) {
    m.commit_asked_ms = 0;
    m.commit_wait_ms = 0;
    m.step_ms = now_ms();
    match said {
        Some(shas) if !shas.is_empty() => {
            m.state = "committed".into();
            m.commit = shas[0].clone();
            m.step = "Committed by its lead".into();
        }
        Some(_) => {
            m.state = "aside".into();
            m.step = "Nothing to commit".into();
        }
        None => {
            m.step = format!("Its lead didn't say it committed: {}", first(text));
            m.needs = "Its lead didn't say it committed it".into();
        }
    }
}
