//! When each open Desk item reached each of its six steps (#409): picked up, building, tests, commit, landing, to
//! check. The Desk's Progress view (ui/desk_progress.js) shows them as a row of marks with how long each step took.
//! Stamped from what Cue already hears (its holder's latest step, its DONE's finish steps, its state) each time the
//! window's state is built after a change: no clock of its own.

use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// The steps, in order: their index is the place in `DeskItem::steps`.
pub(crate) const STEPS: usize = 6;
pub(crate) const BUILDING: usize = 1;
const TESTS: usize = 2;
const COMMIT: usize = 3;
const LANDING: usize = 4;
pub(crate) const TO_CHECK: usize = 5;

/// A holder's step that runs tests ("Running: cargo test", "Running: npx playwright test …").
fn runs_tests(doing: &str) -> bool {
    let Some(cmd) = doing.strip_prefix("Running: ") else { return false };
    let cmd = cmd.to_ascii_lowercase();
    ["test", "spec", "playwright", "pytest", "jest", "vitest"].iter().any(|w| cmd.contains(w))
}

/// What its holder is doing on it now, as its latest step says ("Editing desk.js", "Running: cargo test"), or "".
fn doing_on(st: &Store, it: &DeskItem) -> String {
    let Some(h) = it.holder.as_ref() else { return String::new() };
    if let Some((lead, agent)) = crate::desk::hub::core::desk::crew_parts(h) {
        return st.sessions.get(lead).and_then(|s| s.team.iter().find(|t| (t.agent_id == agent || t.call == agent) && t.working())).map(|t| t.step.clone()).unwrap_or_default();
    }
    let Some(s) = st.sessions.get(&h.sid).filter(|s| s.state == "working") else { return String::new() };
    // Its step is this item's only while it's on this one: it can hold several.
    if crate::desk::hub::stall::stall::now_on(st, &h.sid).as_deref() != Some(it.id.as_str()) {
        return String::new();
    }
    s.doing.clone()
}

/// The step item `it` is at as Cue last heard it, or None when it hasn't started (new, queued, parked, done).
fn step_now(st: &Store, it: &DeskItem) -> Option<usize> {
    // Its holder answering your question (#475): still where it was, not back at work (desk/hub/turn/asked.rs says otherwise).
    if it.asked.is_some() {
        return None;
    }
    match it.state.as_str() {
        "to_check" => Some(TO_CHECK),
        "finishing" => Some(match it.finish.as_ref().map(|f| f.step.as_str()).unwrap_or("commit") {
            "commit" | "commit_wait" | "" => COMMIT,
            _ => LANDING,
        }),
        "working" | "needs_resume" | "paused" => {
            let doing = doing_on(st, it);
            Some(if runs_tests(&doing) { TESTS } else if doing.starts_with("Editing ") { BUILDING } else { 0 })
        }
        _ => None,
    }
}

/// Stamps each open item's step as first reached (ms): picked up when its holder took it, the rest as Cue hears them.
/// A step only fills in: work back to Editing after its tests stays at tests. Sent back from its finish (your Send
/// back, a failed landing, its work taken out), it's at work again (#421): the round is kept in `backs` (the step it
/// was at and why), and it's building afresh from now: its later stamps go, to fill in again. True when anything changed.
pub(crate) fn stamp(st: &mut Store, now: u64) -> bool {
    let at: Vec<(String, usize)> = st.desk.items.iter().filter_map(|i| step_now(st, i).map(|s| (i.id.clone(), s))).collect();
    let mut changed = false;
    for (id, step) in at {
        let Some(it) = st.desk.item_mut(&id) else { continue };
        if it.steps.len() != STEPS {
            it.steps.resize(STEPS, 0);
        }
        let before = it.steps.clone();
        if step < COMMIT {
            if let Some(from) = it.steps[COMMIT..].iter().rposition(|s| *s != 0).map(|k| k + COMMIT) {
                let why = it.checks.last().map(|c| c.back.trim().to_string()).unwrap_or_default();
                it.backs.push(crate::desk::model::StepBack { from, why, at_ms: now, steps: before.clone() });
                it.steps[BUILDING..].iter_mut().for_each(|s| *s = 0);
                it.steps[BUILDING] = now;
            }
        }
        if it.steps[0] == 0 {
            it.steps[0] = it.held_from.as_ref().map_or(now, |(_, ms)| *ms).min(now);
        }
        if it.steps[step] == 0 {
            it.steps[step] = now;
        }
        changed |= it.steps != before;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_run_is_told_from_other_commands() {
        assert!(runs_tests("Running: cargo test --lib"));
        assert!(runs_tests("Running: npx playwright test tests/ui/specs/desk.spec.mjs"));
        assert!(!runs_tests("Running: git status"));
        assert!(!runs_tests("Editing test.rs"));
    }

    #[test]
    fn steps_fill_in_as_it_moves_and_a_send_back_clears_the_finish() {
        let mut st = Store::default();
        st.desk.items.push(DeskItem { id: "1".into(), state: "working".into(), held_from: Some(("s".into(), 1_000)), ..Default::default() });
        let at = |st: &Store| st.desk.items[0].steps.clone();
        assert!(stamp(&mut st, 5_000));
        assert_eq!(at(&st), vec![1_000, 0, 0, 0, 0, 0]);
        assert!(!stamp(&mut st, 6_000), "nothing new: nothing changes");
        st.desk.items[0].state = "finishing".into();
        stamp(&mut st, 7_000);
        st.desk.items[0].state = "to_check".into();
        stamp(&mut st, 9_000);
        assert_eq!(at(&st), vec![1_000, 0, 0, 7_000, 0, 9_000]);
        // Sent back: at work again, building afresh, the round kept with why.
        st.desk.items[0].checks.push(Default::default());
        st.desk.items[0].checks[0].back = "The arrow is missing".into();
        st.desk.items[0].state = "working".into();
        stamp(&mut st, 10_000);
        assert_eq!(at(&st), vec![1_000, 10_000, 0, 0, 0, 0]);
        let b = &st.desk.items[0].backs;
        assert_eq!((b.len(), b[0].from, b[0].why.as_str(), b[0].at_ms), (1, TO_CHECK, "The arrow is missing", 10_000));
        assert_eq!(b[0].steps, vec![1_000, 0, 0, 7_000, 0, 9_000]);
        assert!(!stamp(&mut st, 11_000), "went back once: no second round");
        // Queued items haven't started: no steps.
        st.desk.items.push(DeskItem { id: "2".into(), state: "queued".into(), ..Default::default() });
        stamp(&mut st, 12_000);
        assert!(st.desk.items[1].steps.is_empty());
    }
}
