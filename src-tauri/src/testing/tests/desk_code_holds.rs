//! Holds on a repository's code (#540, docs/desk-coordination-plan.md option B, step 1): a lead's HOLD keeps part of
//! the code for its item, saved against the repository so every Desk on it sees it, in its lead's Desk block and its
//! view; it ends when its item is done, dropped or parked, or on UNHOLD.

use super::desk::{desk, ends, item, sent_to};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

/// A new independent Desk named `name` on folder `folder`, with its lead: (Desk, lead).
fn start_in(w: &World, folder: &std::path::Path, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some(name)).unwrap();
    (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into())
}

fn start(w: &World, name: &str) -> (String, String) {
    start_in(w, w.home(), name)
}

/// What Desk `d`'s lead `lead` is told before each message.
fn told(w: &World, d: &str, lead: &str) -> String {
    w.hub().desk_summary(d, lead, false).0
}

/// The id of Desk `d`'s item titled `title`.
fn id_of(w: &World, d: &str, title: &str) -> String {
    item(&desk(w, d), title)["id"].as_str().unwrap().to_string()
}

const AREA: &str = "src-tauri/src/hub/desk_*.rs, ui/desk_*.js";

/// Lead `lead` of Desk `d` adds "Move the Desk files" and holds AREA for it: its id.
fn held(w: &World, d: &str, lead: &str) -> String {
    ends(w, lead, &format!("ITEM: task | Move the Desk files\nHOLD: #new | area: {AREA} | till: it lands | why: the refactor moves these files"));
    id_of(w, d, "Move the Desk files")
}

#[test]
fn hold_and_unhold_lines_read_and_a_hold_with_no_area_still_parks() {
    assert_eq!(
        lines("HOLD: #509 | area: src/hub/desk_*.rs, `ui/desk_*.js` | till: 5pm | why: the move"),
        [Line::Hold { id: "509".into(), area: vec!["src/hub/desk_*.rs".into(), "ui/desk_*.js".into()], till: "5pm".into(), why: "the move".into() }]
    );
    assert_eq!(lines("- **UNHOLD: #509**"), [Line::Unhold { id: "509".into() }]);
    assert_eq!(lines("HOLD: #12 | why: later"), [Line::Unstick { what: "hold".into(), id: "12".into(), commit: String::new(), why: "later".into() }]);
    assert!(lines("`HOLD: #<id> | area: <globs>`").is_empty() && lines("UNHOLD: soon").is_empty());
}

#[test]
fn a_hold_is_seen_by_every_desk_on_the_repository_and_survives_a_relaunch() {
    let mut w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let other = w.home().join("elsewhere");
    std::fs::create_dir(&other).unwrap();
    let (c, lead_c) = start_in(&w, &other, "Elsewhere");
    let id = held(&w, &a, &lead_a);
    let answer = sent_to(&w, &lead_a).last().cloned().unwrap_or_default();
    assert!(answer.starts_with(&format!("Held: #{id} holds {AREA} till it lands.")), "{answer}");
    let line = format!("Holds on this code: Feature A #{id} holds {AREA} till it lands (the refactor moves these files)");
    let said_b = told(&w, &b, &lead_b);
    assert!(said_b.contains(&line), "{said_b}");
    assert!(told(&w, &a, &lead_a).contains(&format!("{line} [this Desk's]")), "the holder's own block shows it too");
    assert!(!told(&w, &c, &lead_c).contains("Holds on this code"), "a Desk on another repository doesn't");
    let hb = &desk(&w, &b)["holds"];
    assert_eq!((hb[0]["item"].as_str(), hb[0]["desk_name"].as_str(), hb[0]["till"].as_str(), hb[0]["mine"].as_bool()), (Some(id.as_str()), Some("Feature A"), Some("it lands"), Some(false)), "{hb}");
    assert_eq!(hb[0]["area"], serde_json::json!(["src-tauri/src/hub/desk_*.rs", "ui/desk_*.js"]));
    assert_eq!(desk(&w, &c)["holds"], serde_json::json!([]));
    w.relaunch();
    assert!(told(&w, &b, &lead_b).contains(&line), "kept over a relaunch");
    assert_eq!(desk(&w, &b)["holds"][0]["item"], id.as_str());
}

#[test]
fn a_hold_ends_when_its_item_is_parked_or_dropped_and_unhold_lifts_it() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let id = held(&w, &a, &lead_a);
    ends(&w, &lead_a, &format!("PARK: #{id} | why: the user asked"));
    assert_eq!(item(&desk(&w, &a), "Move the Desk files")["state"], "held");
    assert!(!told(&w, &b, &lead_b).contains("Holds on this code"), "parked: the hold ended");
    assert_eq!(desk(&w, &b)["holds"], serde_json::json!([]));
    ends(&w, &lead_a, "ITEM: task | Rename the hub\nHOLD: #new | area: src-tauri/src/hub/*.rs | till: 6pm | why: rename");
    let two = id_of(&w, &a, "Rename the hub");
    assert!(told(&w, &b, &lead_b).contains(&format!("Feature A #{two} holds src-tauri/src/hub/*.rs till 6pm (rename)")));
    ends(&w, &lead_a, &format!("DROP: #{two} | why: not wanted"));
    assert!(!told(&w, &b, &lead_b).contains("Holds on this code"), "dropped: the hold ended");
    ends(&w, &lead_a, "ITEM: task | Split the view\nHOLD: #new | area: ui/desk/view.js | till: it lands | why: split");
    let three = id_of(&w, &a, "Split the view");
    assert!(told(&w, &b, &lead_b).contains("holds ui/desk/view.js"));
    ends(&w, &lead_a, &format!("UNHOLD: #{three}"));
    assert_eq!(sent_to(&w, &lead_a).last().unwrap(), &format!("Unheld: #{three} no longer holds ui/desk/view.js."));
    assert!(!told(&w, &b, &lead_b).contains("Holds on this code"));
    assert_eq!(item(&desk(&w, &a), "Split the view")["state"], "new", "the item itself goes on");
}

#[test]
fn a_hold_ends_when_its_item_is_done() {
    let w = World::new();
    let _ = super::team::main_repo(&w);
    let (h, project) = super::desk::holder(&w);
    let d = super::desk::new_lead(&w, &project);
    let (b, lead_b) = start(&w, "Feature B");
    ends(&w, &d, "ITEM: task | Mocks: header | done when: three ways on the canvas. No code changes\nROUTE: #new -> cue-teams | why: it knows the header\nHOLD: #new | area: ui/desk_*.js | till: it lands | why: mocks first");
    let id = id_of(&w, &project, "Mocks: header");
    assert!(told(&w, &b, &lead_b).contains(&format!("#{id} holds ui/desk_*.js till it lands (mocks first)")), "{}", told(&w, &b, &lead_b));
    w.advance(1000);
    ends(&w, &h, &format!("Three on the canvas.\nDONE: #{id}"));
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Mocks: header")["state"], "done");
    assert!(!told(&w, &b, &lead_b).contains("Holds on this code"), "done: the hold ended");
    ends(&w, &d, &format!("HOLD: #{id} | area: ui/desk_*.js | till: it lands | why: again"));
    assert_eq!(sent_to(&w, &d).last().unwrap(), &format!("Refused: HOLD #{id}: #{id} is done already. You can hold an open item."));
}

#[test]
fn holds_and_unholds_cue_cant_make_are_refused_with_what_to_do() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    ends(&w, &lead_b, "ITEM: task | Only B");
    let theirs = id_of(&w, &b, "Only B");
    let refused = |lead: &str, line: &str| {
        let before = sent_to(&w, lead).len();
        ends(&w, lead, line);
        let sent = sent_to(&w, lead);
        assert_eq!(sent.len(), before + 1, "one answer to {line}: {sent:?}");
        sent.last().unwrap().clone()
    };
    assert_eq!(refused(&lead_a, &format!("HOLD: #{theirs} | area: ui/*.js | till: it lands | why: x")), format!("Refused: HOLD #{theirs}: #{theirs} is on Feature B's Desk, not this one. You can ask its lead to hold it."));
    assert_eq!(refused(&lead_a, "HOLD: #999 | area: ui/*.js | why: x"), "Refused: HOLD #999: this Desk has no item #999. You can hold one of this Desk's own items.");
    ends(&w, &lead_a, "ITEM: task | Later\nPARK: #new | why: the user asked");
    let parked = id_of(&w, &a, "Later");
    assert_eq!(refused(&lead_a, &format!("HOLD: #{parked} | area: ui/*.js | why: x")), format!("Refused: HOLD #{parked}: #{parked} is parked. You can hold it once it's routed again."));
    let id = held(&w, &a, &lead_a);
    assert!(refused(&lead_a, &format!("HOLD: #{id} | area: | why: x")).starts_with(&format!("Refused: HOLD #{id}: it names no code. You can write the files it holds after area:")));
    assert_eq!(refused(&lead_b, &format!("UNHOLD: #{id}")), format!("Refused: UNHOLD #{id}: it's Feature A's hold. You can ask its lead to lift it."));
    assert_eq!(refused(&lead_a, &format!("UNHOLD: #{theirs}")), format!("Refused: UNHOLD #{theirs}: #{theirs} holds no code. You can see the holds that stand in your Desk block."));
    assert!(told(&w, &b, &lead_b).contains(&format!("Feature A #{id} holds {AREA}")), "the refused lines left the hold as it was");
}
