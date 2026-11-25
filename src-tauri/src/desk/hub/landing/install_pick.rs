//! Which commit an install of this Cue builds (docs/faster-checks-plan.md "The app only updates to a version that
//! passed"): the newest commit of the main line a full run has passed (full_run.rs), not its tip. Items landed after
//! that commit stay queued and say they wait for the full run. So a full run that is broken or absent can never stop
//! installs for good, the tip is taken as before once main has moved and no run has finished on it for two hours, or
//! when nothing at all is kept of full runs and none is going; one line then tells the Desk's lead. A run that failed
//! holds the install only when it was traced to an item, and then for four hours at most: after that the tip is
//! taken and one line says the app was installed although the full run still fails. A run that failed on tests no
//! item of its batch caused holds nothing: its commit is built as one that passed. Install now is your word: it
//! takes the tip.

use crate::desk::feed::ChangeKind;
use crate::desk::hub::landing::finish::NOT_STARTED;
use crate::desk::hub::landing::full_run_kit::FullRunState;
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// What a queued item landed after the last commit that passed says it waits for.
pub(crate) const FULL_RUN_WAIT: &str = "waiting for the full run";

/// How long the main line may stand at a commit no full run has finished on before an install takes it anyway.
pub(crate) const UNRUN_AFTER_MS: u64 = 2 * 60 * 60_000;

/// How long a failed run traced to an item holds the install on the last commit that passed.
pub(crate) const HELD_FOR_MS: u64 = 4 * 60 * 60_000;

/// The commit an install builds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pick {
    /// The main line's tip: a full run passed it (or this Cue makes no full runs).
    Tip,
    /// The tip with no full run behind it: why.
    Unrun(String),
    /// The tip although the full run fails on it: the item it was traced to, not fixed in four hours.
    Failing(String),
    /// The last commit that passed, behind the tip.
    Passed(String),
    /// Nothing yet: no commit has passed, and a run may still.
    Wait,
}

/// The pick for a main line at `tip`: `state` is what's kept of its full runs (None: nothing), `going` whether a run
/// nothing is kept of is under way, `on_main` whether a commit is in the tip's history.
pub(crate) fn pick(state: Option<&FullRunState>, going: bool, tip: &str, on_main: impl Fn(&str) -> bool, now: u64) -> Pick {
    let Some(s) = state else {
        return if going { Pick::Wait } else { Pick::Unrun("none has been made of this project".into()) };
    };
    if s.good() == tip {
        return Pick::Tip;
    }
    if s.held_since > 0 && now.saturating_sub(s.held_since) >= HELD_FOR_MS {
        return Pick::Failing(s.held_item.clone());
    }
    if s.unrun_since > 0 && now.saturating_sub(s.unrun_since) >= UNRUN_AFTER_MS {
        return Pick::Unrun("none has finished in the 2 hours since main moved".into());
    }
    // A commit that passed and is no longer in the main line (its history was rewritten) is no commit to build:
    // nothing passed. The wait is never for good: the head has a run going or due (two hours at most), or one that
    // failed on an item (four), or one that failed on no item, which is the tip above.
    if s.good().is_empty() || !on_main(s.good()) {
        return Pick::Wait;
    }
    Pick::Passed(s.good().to_string())
}

impl Hub {
    /// The install of `repo` for the queued items `batch`, due now (`now_said`: you said Install now): started at the
    /// commit picked, with the items that commit has; the others say they wait for the full run. Nothing starts when
    /// the commit picked is the one running. Called with the hub's store let go: it reads files and asks git.
    pub(crate) fn install_start(&self, repo: &str, batch: &[&DeskItem], now: u64, now_said: bool) {
        // The main line's tip, not the folder's HEAD: an item may work on its own branch there (in_place.rs). Built from
        // a clean copy at that commit, so the folder's files don't matter.
        let Some(tip) = crate::desk::hub::landing::in_place::main_head(repo) else {
            // Said Install now: it says why nothing started, rather than nothing happening (#641).
            if now_said {
                for it in batch {
                    self.finish_failed(it, &format!("{NOT_STARTED}: Cue couldn't read the latest commit in {repo}"), "failed", "");
                }
            }
            return;
        };
        let inst = self.installer();
        let pick = if now_said { Pick::Tip } else { crate::desk::hub::landing::full_run::install_pick(repo, &tip, now) };
        let in_it = |i: &&DeskItem, c: &str| i.finish.as_ref().is_none_or(|f| f.commit.is_empty() || f.commit == c || inst.contains(repo, &f.commit, c));
        let (commit, go, later): (String, Vec<&DeskItem>, Vec<&DeskItem>) = match &pick {
            Pick::Tip | Pick::Unrun(_) | Pick::Failing(_) => (tip.clone(), batch.to_vec(), vec![]),
            Pick::Passed(p) => {
                let (go, later) = batch.iter().copied().partition(|i| in_it(i, p));
                (p.clone(), go, later)
            }
            Pick::Wait => (String::new(), vec![], batch.to_vec()),
        };
        self.install_waits(&later);
        // The commit that passed is the one running: nothing to install until a later one passes.
        if go.is_empty() || (matches!(pick, Pick::Passed(_)) && inst.has(repo, &commit)) {
            return;
        }
        let tag = format!("desk-{now}");
        let started = inst.start(repo, &commit, &tag);
        for it in &go {
            match &started {
                Ok(()) => {
                    let (tag, commit) = (tag.clone(), commit.clone());
                    // The commit built is the item's now: main as it is, a fix made since it last failed in it (#343).
                    self.finish_set(&it.id, |i| {
                        if let Some(f) = i.finish.as_mut() {
                            (f.step, f.at, f.tag, f.now, f.commit, f.wait) = ("building".into(), now, tag, false, commit, String::new());
                        }
                    });
                }
                Err(e) => self.finish_failed(it, &format!("{NOT_STARTED}: {e}"), "failed", e),
            }
        }
        if let (Ok(()), Some(it)) = (&started, go.first()) {
            self.install_said(&pick, &commit, &it.project);
        }
    }

    /// One line for Desk `project`'s lead when the install of `commit` has no passed full run behind it (`pick`).
    fn install_said(&self, pick: &Pick, commit: &str, project: &str) {
        let at = commit.get(..7).unwrap_or(commit);
        match pick {
            Pick::Unrun(why) => {
                let words = format!("Cue is being installed from main at {at} without a full run: {why}. Its tests were not all run on it.");
                self.desk_feed_line(project, ChangeKind::Installed, &words, &format!("installed without a full run: {commit}"));
            }
            Pick::Failing(item) => {
                let words = format!("Cue is being installed from main at {at} although the full run still fails: #{item} broke it and no fix has passed in 4 hours.");
                self.desk_feed_line(project, ChangeKind::Installed, &words, &format!("installed although the full run fails: {commit}"));
            }
            _ => {}
        }
    }

    /// The queued items `later`, landed after the last commit a full run passed: each says it waits for that run.
    fn install_waits(&self, later: &[&DeskItem]) {
        for it in later {
            if it.finish.as_ref().is_some_and(|f| f.wait == FULL_RUN_WAIT) {
                continue;
            }
            self.finish_set(&it.id, |i| {
                if let Some(f) = i.finish.as_mut().filter(|f| f.step == "queued") {
                    f.wait = FULL_RUN_WAIT.into();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(passed: &str, unrun_since: u64) -> FullRunState {
        FullRunState { passed: passed.into(), unrun_since, ..Default::default() }
    }

    #[test]
    fn the_pick_is_the_last_commit_that_passed_until_two_hours_with_no_run_finished() {
        let on = |_: &str| true;
        assert_eq!(pick(Some(&state("b2", 0)), false, "b2", on, 9), Pick::Tip, "the tip passed");
        assert_eq!(pick(Some(&state("a1", 1000)), false, "b2", on, 1000 + UNRUN_AFTER_MS - 1), Pick::Passed("a1".into()));
        assert!(matches!(pick(Some(&state("a1", 1000)), false, "b2", on, 1000 + UNRUN_AFTER_MS), Pick::Unrun(_)), "two hours on");
        assert_eq!(pick(Some(&state("", 1000)), false, "b2", on, 2000), Pick::Wait, "the first run hasn't ended");
        assert_eq!(pick(Some(&state("a1", 1000)), false, "b2", |_: &str| false, 2000), Pick::Wait, "what passed is off main");
    }

    #[test]
    fn a_failed_run_never_holds_the_install_for_good() {
        let on = |_: &str| true;
        // Traced to an item: on what passed, for four hours from that failure, then the tip.
        let held = FullRunState { passed: "a1".into(), held_since: 1000, held_item: "12".into(), ..Default::default() };
        assert_eq!(pick(Some(&held), false, "b2", on, 1000 + HELD_FOR_MS - 1), Pick::Passed("a1".into()));
        assert_eq!(pick(Some(&held), false, "b2", on, 1000 + HELD_FOR_MS), Pick::Failing("12".into()));
        // No item to blame: that commit is built as one that passed.
        let nobody = FullRunState { passed: "a1".into(), good: "b2".into(), failing: "b2".into(), ..Default::default() };
        assert_eq!(pick(Some(&nobody), false, "b2", on, 9), Pick::Tip);
        assert_eq!(pick(Some(&nobody), false, "c3", on, 9), Pick::Passed("b2".into()));
        // What passed is off main, which is nothing passed: the same four hours, the same two.
        let off = |_: &str| false;
        assert_eq!(pick(Some(&held), false, "b2", off, 1000 + HELD_FOR_MS - 1), Pick::Wait);
        assert_eq!(pick(Some(&held), false, "b2", off, 1000 + HELD_FOR_MS), Pick::Failing("12".into()));
        assert!(matches!(pick(Some(&state("a1", 1000)), false, "b2", off, 1000 + UNRUN_AFTER_MS), Pick::Unrun(_)));
    }

    #[test]
    fn with_nothing_kept_of_full_runs_the_tip_is_taken_unless_one_is_going() {
        assert!(matches!(pick(None, false, "b2", |_: &str| true, 9), Pick::Unrun(_)));
        assert_eq!(pick(None, true, "b2", |_: &str| true, 9), Pick::Wait);
    }
}
