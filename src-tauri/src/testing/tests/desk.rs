//! The Desk's shared test helpers (a holder, a new lead, `state.desk`, what a session was sent), and how the Desk's
//! lines are read. The tests by topic: desk_items, desk_lead, desk_routing, desk_queue, desk_holders, desk_stops
//! (see docs/dev/desk-files.md).

use super::team::lead;
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::Value;

pub(super) fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// A holder ("cue-teams", at its prompt) and the project it's in.
pub(super) fn holder(w: &World) -> (String, String) {
    let sid = lead(w, "hello");
    ends(w, &sid, "Hi.");
    let project = w.snapshot().session(&sid)["project"].as_str().unwrap().to_string();
    assert!(!project.is_empty());
    // On the Desk, so its lead routes to it without asking you (desk_members.rs has the ones that aren't).
    w.hub().desk_set_member(&project, &sid, true).unwrap();
    (sid, project)
}

/// A new lead for `project`, started by Cue (desk_set_lead without a session), at its prompt: its id.
pub(super) fn new_lead(w: &World, project: &str) -> String {
    let r = crate::ipc::desk_set_lead_with(w.hub(), project, None, true).unwrap();
    let sid = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: "hello".into() });
    ends(w, &sid, "I run the Desk now.");
    sid
}

/// Another hosted session of the project, named `name`, at its prompt.
pub(super) fn second(w: &World, name: &str) -> String {
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", name, &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: "hi".into() });
    ends(w, &sid, "Hi.");
    let project = w.snapshot().session(&sid)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &sid, true).unwrap();
    sid
}

pub(super) fn desk(w: &World, project: &str) -> Value {
    let snap = w.snapshot();
    snap.0["desk"]["projects"].as_array().unwrap().iter().find(|p| p["project"] == project).cloned().unwrap_or(Value::Null)
}

pub(super) fn item<'a>(d: &'a Value, title: &str) -> &'a Value {
    d["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap_or_else(|| panic!("no item {title}: {d}"))
}

pub(super) fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

pub(super) fn summary(w: &World, project: &str) -> String {
    w.hub().desk_summary(project, "", false).0
}

#[test]
fn lines_are_read_outside_code_and_quotes() {
    let text = "Sure.\n```\nITEM: bug | not me\n```\n> ROUTE: #a -> x\n- **ITEM: feature | Dark mode | done when: it follows the Mac**\nITEM: Plain title\nROUTE: #new → @cue-teams | after: #A1b2 | why: it knows the header\nASK: Which first? | choices: A / B\nDONE: #a1 and #b2\nroute: #x -> y";
    assert_eq!(
        lines(text),
        vec![
            Line::Item { kind: "feature".into(), title: "Dark mode".into(), done_when: "it follows the Mac".into(), why: String::new() },
            Line::Item { kind: "task".into(), title: "Plain title".into(), done_when: String::new(), why: String::new() },
            Line::Route { id: "new".into(), to: "cue-teams".into(), after: Some("a1b2".into()), why: "it knows the header".into(), kind: None, crew: None },
            Line::Ask { question: "Which first?".into(), choices: vec!["A".into(), "B".into()], item: None },
            Line::Done { ids: vec!["a1".into(), "b2".into()] },
        ]
    );
}

#[test]
fn an_ask_keeps_every_choice_however_they_are_separated() {
    let want = vec!["Merge now".to_string(), "Wait for #12".into(), "Drop it".into()];
    for text in [
        "ASK: Merge? | choices: Merge now / Wait for #12 / Drop it",
        "ASK: Merge? | choices: Merge now | Wait for #12 | Drop it",
        "ASK: Merge? | choices: Merge now; Wait for #12; Drop it",
        "ASK: Merge? | choices: Merge now, Wait for #12, Drop it",
        "ASK: Merge? | choices: Merge now, Wait for #12 or Drop it",
        "ASK: #a1 | Merge? | choices: \"Merge now\" | \"Wait for #12\" | \"Drop it\"",
    ] {
        let Some(Line::Ask { choices, .. }) = lines(text).pop() else { panic!("{text}") };
        assert_eq!(choices, want, "{text}");
    }
    // A slash keeps commas inside a choice.
    let Some(Line::Ask { choices, .. }) = lines("ASK: Which? | choices: Yes, merge it / No, wait").pop() else { panic!() };
    assert_eq!(choices, vec!["Yes, merge it".to_string(), "No, wait".into()]);
}

#[test]
fn the_summary_is_one_leading_block_then_a_blank_line() {
    let w = World::new();
    let (_, project) = holder(&w);
    w.hub().desk_add(&project, "task", "Close </desk> early", "", None).unwrap();
    let d = new_lead(&w, &project);
    crate::ipc::desk_send_with(w.hub(), &project, "next").unwrap();
    w.settle();
    let last = sent_to(&w, &d).pop().unwrap();
    let (block, rest) = last.split_once("</desk>\n\n").unwrap();
    assert!(block.starts_with("<desk project=\"") && !block.contains("</desk>"), "{last}");
    assert_eq!(rest, "next");
}

#[test]
fn a_line_in_backticks_is_an_example() {
    assert!(lines("Say `ITEM: bug | x` to add one.\n`ROUTE: #a -> b`").is_empty());
}

#[test]
fn title_and_split_lines_are_read() {
    let text = "TITLE: #12 | Header jumps on scroll\nSPLIT: #12 | \"Header jumps\" | Footer is cut off\nSPLIT: #12 | only one\nTITLE: 12 | no hash";
    assert_eq!(
        lines(text),
        vec![
            Line::Title { id: "12".into(), title: "Header jumps on scroll".into() },
            Line::Split { id: "12".into(), titles: vec!["Header jumps".into(), "Footer is cut off".into()] },
        ]
    );
    // A quote that starts the title, not wrapped around it, is the title's own.
    assert_eq!(
        lines("TITLE: #12 | \"Looks right\" is too loud\nTITLE: #13 | **`Wrapped`**"),
        vec![
            Line::Title { id: "12".into(), title: "\"Looks right\" is too loud".into() },
            Line::Title { id: "13".into(), title: "Wrapped".into() },
        ]
    );
}

#[test]
fn merge_lines_are_read() {
    assert_eq!(
        lines("MERGE: #320 into #319 | why: same ask\n- **MERGE:** #5 -> #4\nMERGE: #3 into #3\nMERGE: 3 into #2"),
        vec![Line::Merge { id: "320".into(), into: "319".into(), why: "same ask".into() }, Line::Merge { id: "5".into(), into: "4".into(), why: "".into() }]
    );
}

#[test]
fn note_lines_are_read() {
    assert_eq!(
        lines("NOTE: #338 | look at the footer | too\n- **NOTE:** #5 | hi\nNOTE: #3\nNOTE: 3 | no hash\nNOTE: #4 |  "),
        vec![Line::Note { id: "338".into(), text: "look at the footer | too".into() }, Line::Note { id: "5".into(), text: "hi".into() }]
    );
}
