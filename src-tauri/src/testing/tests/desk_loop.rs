//! The Desk lead drafts a loop for you (#583): its LOOP: line names a project, this one or another of yours; Cue
//! finds that project's folder and keeps the draft as one of the lead's moves, which its chat shows as a loop card
//! with Start loop. Nothing starts until you press it.

use super::desk::{desk, ends, holder, new_lead, sent_to};
use super::team::git;
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
            folder: String::new(),
        })]
    );
    // A check of none is no check (the loop's lead proposes one); limits and decisions left out stay unsaid.
    let Line::Loop(d) = &lines("- **LOOP: cue | goal: tidy the docs | done when: no broken links | check: none**")[0] else { panic!() };
    assert_eq!((d.check.as_str(), d.rounds, d.decisions.as_str()), ("", None, ""));
    let Line::Loop(d) = &lines("LOOP: web | goal: g | done when: d | folder: `~/development/web`")[0] else { panic!() };
    assert_eq!(d.folder, "~/development/web");
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
    assert!(told.contains("Refused: LOOP: nowhere: Cue knows no folder of a project by that name on this Mac. You can add its folder to the line"), "{told}");
    assert!(told.contains(&format!("Refused: LOOP: {project}: a loop needs goal: and done when:.")), "{told}");
}

/// A repository you put into another project: its sessions show under that project and work in worktrees under
/// .claude/worktrees. A LOOP line naming the repository still finds its main checkout; one with folder: finds that
/// folder; a folder: that isn't there is refused with why.
#[test]
fn a_loop_finds_a_repository_filed_under_another_project_by_its_own_name_or_its_folder() {
    let w = World::new();
    let (_, project) = holder(&w);
    let lead = new_lead(&w, &project);
    let web = w.home().join("web");
    std::fs::create_dir_all(&web).unwrap();
    let web = std::fs::canonicalize(&web).unwrap();
    std::fs::write(web.join("a.txt"), "a\n").unwrap();
    git(&web, &["init", "-q", "-b", "main"]);
    git(&web, &["config", "user.email", "t@example.com"]);
    git(&web, &["config", "user.name", "T"]);
    git(&web, &["add", "."]);
    git(&web, &["commit", "-q", "-m", "start"]);
    git(&web, &["worktree", "add", "-q", "-b", "sector", ".claude/worktrees/sector"]);
    let wt = web.join(".claude/worktrees/sector");
    let web_s = web.to_string_lossy().to_string();
    crate::config::set("project.folders", json!({ web_s.clone(): project.clone() })).unwrap();
    assert_eq!(crate::model::project_at(&wt.to_string_lossy(), ""), project, "filed under the Desk's project");
    w.session("web-1").claude().in_folder(&wt).started();
    w.settle();
    ends(&w, &lead, "LOOP: web | name: Sector basket | goal: build the basket | done when: score.py passes | check: python score.py");
    let got = drafted(&w, &project);
    assert_eq!(got.len(), 1, "{:?}", sent_to(&w, &lead));
    assert_eq!((got[0]["draft"]["project"].as_str(), got[0]["draft"]["folder"].as_str()), (Some("web"), Some(web_s.as_str())));
    // Named by its folder, under any name.
    let other = std::fs::canonicalize({
        let o = w.home().join("charts");
        std::fs::create_dir_all(&o).unwrap();
        o
    })
    .unwrap();
    ends(&w, &lead, &format!("LOOP: charts | goal: g | done when: d | folder: {}", other.to_string_lossy()));
    let got = drafted(&w, &project);
    assert_eq!(got.len(), 2, "{:?}", sent_to(&w, &lead));
    let charts = got.iter().find(|x| x["draft"]["project"] == "charts").unwrap();
    assert_eq!(charts["draft"]["folder"].as_str(), Some(other.to_string_lossy().as_ref()));
    ends(&w, &lead, "LOOP: gone | goal: g | done when: d | folder: /no/such/folder");
    assert_eq!(drafted(&w, &project).len(), 2);
    assert!(sent_to(&w, &lead).join("\n").contains("Refused: LOOP: gone: /no/such/folder is no folder on this Mac."));
}
