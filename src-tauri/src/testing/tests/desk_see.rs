//! An item's two lines (#382): what you'll see when it's done, in plain words (shown to you), and the technical check
//! (told only to the session doing it). Read from ITEM lines and the lead's SEE: lines; the lead asked for missing ones.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

#[test]
fn item_and_see_lines_are_read() {
    assert_eq!(
        lines("ITEM: bug | Header jumps | you’ll see: the header stays put | done when: scroll test passes"),
        vec![Line::Item { kind: "bug".into(), title: "Header jumps".into(), see: "the header stays put".into(), done_when: "scroll test passes".into(), why: String::new(), follows: String::new(), picked: String::new() }]
    );
    // A title that starts "See" stays the title.
    assert_eq!(
        lines("ITEM: task | See the logs | see: one list of logs"),
        vec![Line::Item { kind: "task".into(), title: "See the logs".into(), see: "one list of logs".into(), done_when: String::new(), why: String::new(), follows: String::new(), picked: String::new() }]
    );
    assert_eq!(
        lines("SEE: #12 | the header stays put | done when: scroll test passes\nSEE: #13 | you'll see: a green dot\nSEE: 14 | no hash"),
        vec![
            Line::See { id: "12".into(), see: "the header stays put".into(), done_when: "scroll test passes".into() },
            Line::See { id: "13".into(), see: "a green dot".into(), done_when: String::new() },
        ]
    );
}

#[test]
fn the_lead_is_asked_for_both_lines_and_gives_them_with_see() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add_with(&project, "", "header jumps", "", "", None, &[]).unwrap();
    w.advance(1000);
    let told = sent_to(&w, &l).pop().unwrap();
    assert!(told.contains("Give one missing its two lines both first: SEE:"), "{told}");
    ends(&w, &l, &format!("SEE: #{} | the header stays put when you scroll | done when: desk.spec scroll test passes", it.id));
    let d = desk(&w, &project);
    let got = item(&d, "header jumps");
    assert_eq!((got["see"].as_str(), got["done_when"].as_str()), (Some("the header stays put when you scroll"), Some("desk.spec scroll test passes")));
}

#[test]
fn the_session_doing_it_is_told_both_lines() {
    let it = crate::desk::DeskItem { id: "7".into(), kind: "bug".into(), title: "Header jumps".into(), see: "it stays put".into(), done_when: "scroll test passes".into(), ..Default::default() };
    let told = crate::desk::handed_words(&it, "", "");
    assert!(told.starts_with("Desk item #7 (bug): Header jumps\nYou'll see: it stays put\nDone when: scroll test passes"), "{told}");
}

#[test]
fn every_brief_says_to_run_only_the_tests_a_change_touches() {
    let it = crate::desk::DeskItem { id: "7".into(), kind: "task".into(), title: "Faster".into(), ..Default::default() };
    let told = crate::desk::handed_words(&it, "", "");
    assert!(told.contains(crate::desk::TESTS_RULE), "{told}");
    assert!(told.contains("only the tests for the files you changed and for code that uses them"), "{told}");
    assert!(told.contains("never the full suite: Cue's own check runs it at landing"), "{told}");
    // The rule is written in the routing rules doc too.
    let doc = include_str!("../../../../docs/desk-routing-rules.md");
    assert!(doc.contains("## Tests while you work"), "the doc lost its tests rule");
}

#[test]
fn an_item_you_add_keeps_what_youll_see() {
    let w = World::new();
    let it = w.hub().desk_add_with("web", "", "Header jumps", "it stays put", "", None, &[]).unwrap();
    assert_eq!((it.see.as_str(), it.done_when.as_str()), ("it stays put", ""));
}
