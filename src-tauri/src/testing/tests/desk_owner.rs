//! Your name instead of "the owner" (owner.rs, #425): the Desk the screen gets says it, and the lead is told it.
//! The name here is made up; tests never read the Mac account's.

use super::desk::{desk, ends, item, new_lead, holder};
use crate::owner::test_name;
use crate::testing::World;

#[test]
fn the_desk_says_your_name_and_the_lead_is_told_it() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Ask the owner about the header");

    // No name on the account: the screen says "you", and the lead isn't told one.
    test_name::set(None);
    let view = desk(&w, &project);
    assert!(item(&view, "Ask you about the header")["id"].is_string(), "{view}");
    assert!(!w.hub().desk_summary(&project, &d, false).0.contains("The user is"));

    // A name: the screen says it (after a change, as the snapshot is built again then), and the lead is told it.
    test_name::set(Some("Robin"));
    ends(&w, &d, "ITEM: Show the owner the footer");
    let view = desk(&w, &project);
    assert!(item(&view, "Ask Robin about the header")["id"].is_string(), "{view}");
    assert!(item(&view, "Show Robin the footer")["id"].is_string(), "{view}");
    let summary = w.hub().desk_summary(&project, &d, false).0;
    assert!(summary.contains("The user is Robin.") && summary.contains("call them Robin"), "{summary}");
    // The name line says what to avoid as it is; every other word of Cue's to the lead uses the name.
    let (rest, name_line) = summary.split_once("The user is Robin.").unwrap();
    assert!(!rest.contains("the owner"), "Cue's own words to the lead use the name: {summary}");
    assert!(name_line.contains("never \"the owner\" or \"the user\"."), "{summary}");
    test_name::set(None);
}
