//! The Desk lead drafts a loop for you (#583): its LOOP: line names a project, this one or another of yours; Cue
//! finds that project's folder and keeps the draft as one of the lead's moves, which its chat shows as a loop card
//! with Start loop. Nothing starts until you press it.

use super::desk::{desk, ends, holder, new_lead, sent_to};
use crate::desk::parse::{lines, Line, LoopDraft};
use crate::testing::World;
use serde_json::{json, Value};

/// The loops drafted on Desk `project`: each move with act "loop", as the window gets it.
fn drafted(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["decisions"].as_array().unwrap().iter().filter(|x| x["act"] == "loop").cloned().collect()
}

#[test]
fn loop_lines_are_read_with_their_limits_and_a_check_that_holds_a_pipe() {
    let got = lines("LOOP: weather | name: Forecast fixes | goal: Make the forecast tests pass | done when: every forecast test passes | check: `npm test | tail -5` | rounds: 4 | time: 60 min | spend: $8.50 | decisions: every");
    assert_eq!(
        got,
        [Line::Loop(LoopDraft {
            project: "weather".into(),
            name: "Forecast fixes".into(),
            goal: "Make the forecast tests pass".into(),
            done_when: "every forecast test passes".into(),
            check: "npm test | tail -5".into(),
            rounds: Some(4),
            time_min: Some(60),
            spend_usd: Some(8.5),
            decisions: "every".into(),
        })]
    );
    // A check of none is no check (the loop's lead proposes one); limits and decisions left out stay unsaid.
    let Line::Loop(d) = &lines("- **LOOP: cue | goal: tidy the docs | done when: no broken links | check: none**")[0] else { panic!() };
    assert_eq!((d.check.as_str(), d.rounds, d.decisions.as_str()), ("", None, ""));
    assert!(lines("`LOOP: <project> | goal: …`").is_empty() && lines("LOOP: | goal: x").is_empty());
}

#[test]
fn the_lead_drafts_a_loop_for_another_project_in_its_folder_without_starting_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let lead = new_lead(&w, &project);
    // Another project of yours: a session in its folder is how Cue knows where it is.
    let other = w.home().join("weather");
    std::fs::create_dir_all(&other).unwrap();
    w.session("weather-1").claude().in_folder(&other).started();
    w.settle();
    let sessions = w.snapshot().0["sessions"].as_array().map(|a| a.len());
    let before = sent_to(&w, &lead).len();
    let line = "LOOP: weather | name: Forecast fixes | goal: Make the forecast tests pass | done when: every forecast test passes | check: npm test | rounds: 4 | time: 60 | spend: 8 | decisions: every";
    ends(&w, &lead, &format!("Here's a loop for it; press Start loop when you're ready.\n{line}"));
    let got = drafted(&w, &project);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["text"], "Drafted \"Forecast fixes\" for weather, for you to start");
    assert_eq!(got[0]["sid"], lead.as_str(), "the lead's move: its chat shows the card");
    assert_eq!(
        got[0]["draft"],
        json!({ "name": "Forecast fixes", "project": "weather", "folder": other.to_string_lossy(), "goal": "Make the forecast tests pass",
            "done_when": "every forecast test passes", "check": "npm test", "decisions": "every", "rounds": 4, "time_min": 60, "spend_usd": 8.0 })
    );
    // Nothing started: no new session, and the lead isn't told anything back.
    assert_eq!(w.snapshot().0["sessions"].as_array().map(|a| a.len()), sessions);
    assert_eq!(sent_to(&w, &lead).len(), before, "{:?}", &sent_to(&w, &lead)[before..]);
    // Said again in a later reply: still one card.
    ends(&w, &lead, line);
    assert_eq!(drafted(&w, &project).len(), 1);
}

#[test]
fn a_loop_for_a_project_cue_has_no_folder_for_or_with_no_goal_is_refused_with_why() {
    let w = World::new();
    let (_, project) = holder(&w);
    let lead = new_lead(&w, &project);
    ends(&w, &lead, "LOOP: nowhere | goal: g | done when: d");
    ends(&w, &lead, &format!("LOOP: {project} | name: No goal | done when: d"));
    assert!(drafted(&w, &project).is_empty());
    let told = sent_to(&w, &lead).join("\n");
    assert!(told.contains("Refused: LOOP: nowhere: Cue knows no folder of a project by that name on this Mac."), "{told}");
    assert!(told.contains(&format!("Refused: LOOP: {project}: a loop needs goal: and done when:.")), "{told}");
}
