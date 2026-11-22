//! A lead moving an item to another Desk (#639, desk/move_desk.rs): `MOVE: #<id> -> <Desk> | why: …`. The item
//! leaves its Desk's list for the other's with its notes, copy and branch; the session at work on it stops and is
//! kept as the one it moved off, so the session the other Desk starts for it gets its hand-off; that Desk's lead is
//! told it came.

use super::desk::{desk, ends, sent_to};
use super::team::main_repo;
use crate::desk::parse::{lines, Line};
use crate::testing::World;
use serde_json::Value;

/// A new Desk named `name` on folder `folder`, with its lead: (Desk, lead).
fn start_in(w: &World, folder: &std::path::Path, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some(name)).unwrap();
    let (d, sid): (String, String) = (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into());
    // It takes what it was started with, as a real one does.
    let first = sent_to(w, &sid).pop().unwrap_or_default();
    w.hosted_says(&sid, crate::hosted::HostedEvent::Taken { text: first });
    ends(w, &sid, "I run the Desk now.");
    (d, sid)
}

fn find(w: &World, d: &str, title: &str) -> Option<Value> {
    desk(w, d)["items"].as_array().and_then(|a| a.iter().find(|i| i["title"] == title).cloned())
}

/// What Cue told lead `lead` after its line `line`, its bands left out.
fn answer(w: &World, lead: &str, line: &str) -> String {
    let before = sent_to(w, lead).len();
    ends(w, lead, line);
    w.advance(1000);
    w.settle();
    let sent: Vec<String> = sent_to(w, lead).into_iter().skip(before).filter(|t| !t.contains("[From another Desk: ")).collect();
    sent.join("\n")
}

#[test]
fn move_lines_are_read() {
    assert_eq!(lines("MOVE: #12 -> Loops | why: the user asked"), [Line::Move { id: "12".into(), to: "Loops".into(), why: "the user asked".into() }]);
    assert_eq!(lines("- **MOVE: #A3 → `Feature B`**"), [Line::Move { id: "a3".into(), to: "Feature B".into(), why: String::new() }]);
    assert!(lines("`MOVE: #<id> -> <Desk's name> | why: …`").is_empty() && lines("MOVE: #12 -> <Desk>").is_empty() && lines("MOVE: 12 -> Loops").is_empty() && lines("MOVE: #12 ->").is_empty());
}

#[test]
fn an_item_at_work_moves_to_another_desk_with_its_branch_and_its_sessions_hand_off() {
    let w = World::new();
    let dir = main_repo(&w);
    let (a, lead_a) = start_in(&w, &dir, "Feature A");
    let (b, lead_b) = start_in(&w, &dir, "Feature B");
    ends(&w, &lead_a, "ITEM: feature | Login screen | done when: it shows\nROUTE: #new -> new | why: nobody has it");
    w.advance(1000);
    w.settle();
    let it = find(&w, &a, "Login screen").expect("on Feature A");
    let id = it["id"].as_str().unwrap().to_string();
    let by = it["holder"]["sid"].as_str().unwrap_or_else(|| panic!("nobody took it: {it}")).to_string();
    let branch = it["tree"]["branch"].as_str().unwrap_or_else(|| panic!("no copy: {it}")).to_string();
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    ends(&w, &by, &format!("Half of it is built in app/login.rs. #{id}"));

    let told = answer(&w, &lead_a, &format!("MOVE: #{id} -> feature b | why: the user wants Feature B to own logins"));
    assert!(told.contains(&format!("Moved: #{id} to Feature B, with its notes and its branch {branch}. Its session was told to stop and write its hand-off. It's off your Desk.")), "{told}");
    assert!(find(&w, &a, "Login screen").is_none(), "gone from Feature A");
    let it = find(&w, &b, "Login screen").unwrap_or_else(|| panic!("on Feature B: {}", desk(&w, &b)));
    assert_eq!((it["id"].as_str(), it["state"].as_str(), it["holder"].is_null()), (Some(id.as_str()), Some("new"), true), "{it}");
    assert_eq!((it["tree"]["branch"].as_str(), it["tree"]["path"].as_str()), (Some(branch.as_str()), Some(path.as_str())), "its copy and branch go with it");
    assert_eq!(it["moved_from"]["sid"], by.as_str(), "kept for its hand-off: {it}");
    assert!(it["said"].as_array().is_some_and(|s| s.iter().any(|x| x["text"].as_str().unwrap_or("").contains("Half of it is built"))), "its notes go with it: {it}");
    let stop = sent_to(&w, &by);
    assert!(stop.iter().any(|t| t.contains(&format!("Desk item #{id} (Login screen) has moved to the Feature B Desk: stop work on it. Write your hand-off note"))), "{stop:?}");
    let band: Vec<String> = sent_to(&w, &lead_b).into_iter().filter(|t| t.contains("[From another Desk: Feature A")).collect();
    assert!(band.len() == 1 && band[0].contains(&format!("#{id} (Login screen) moved to your Desk from Feature A, with its notes and its branch {branch}.")) && band[0].contains("Why: the user wants Feature B to own logins"), "{band:?}");
    let decisions = |d: &str| desk(&w, d)["decisions"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>();
    assert!(decisions(&a).contains(&format!("Moved #{id} to Feature B")) && decisions(&b).contains(&format!("#{id} came from Feature A")));

    // Feature B's lead starts a session for it: that session is sent the hand-off of the one it moved off.
    ends(&w, &lead_b, &format!("ROUTE: #{id} -> new | why: carry on from the hand-off"));
    w.advance(1000);
    w.settle();
    let it = find(&w, &b, "Login screen").unwrap();
    let to = it["holder"]["sid"].as_str().unwrap_or_else(|| panic!("nobody took it: {it}")).to_string();
    assert_ne!(to, by);
    assert_eq!(it["tree"]["branch"].as_str(), Some(branch.as_str()), "the same branch: {it}");
    assert!(sent_to(&w, &to).iter().any(|t| t.contains(&format!("Hand-off: #{id} was with "))), "{:?}", sent_to(&w, &to));
}

#[test]
fn moves_cue_cant_make_are_refused_with_what_to_do() {
    let w = World::new();
    let dir = main_repo(&w);
    let (a, lead_a) = start_in(&w, &dir, "Feature A");
    let (b, lead_b) = start_in(&w, &dir, "Feature B");
    // A repository of its own (the World's home is the first one, so a folder in it would be on that one).
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(&other).status().unwrap().success());
    let (_, _) = start_in(&w, &other, "Elsewhere");
    ends(&w, &lead_a, "ITEM: task | Tidy the docs\nITEM: feature | Login screen\nROUTE: #new -> new | why: nobody has it");
    w.advance(1000);
    w.settle();
    let docs = find(&w, &a, "Tidy the docs").unwrap()["id"].as_str().unwrap().to_string();
    let login = find(&w, &a, "Login screen").unwrap()["id"].as_str().unwrap().to_string();
    assert!(answer(&w, &lead_a, &format!("MOVE: #{docs} -> Feature A")).contains(&format!("Refused: MOVE #{docs}: it's on your Desk already.")));
    assert!(answer(&w, &lead_a, &format!("MOVE: #{docs} -> Nowhere")).contains(&format!("Refused: MOVE #{docs}: no Desk by that name. You can move it to one of these: Elsewhere, Feature B.")));
    assert!(answer(&w, &lead_a, "MOVE: #999 -> Feature B").contains("Refused: MOVE #999: there's no item #999."));
    let told = answer(&w, &lead_a, &format!("MOVE: #{login} -> Elsewhere"));
    assert!(told.contains(&format!("Refused: MOVE #{login}: #{login} has a copy of this Desk's repository, and Elsewhere works on another one.")), "{told}");
    assert!(find(&w, &a, "Login screen").is_some() && find(&w, &a, "Tidy the docs").is_some(), "nothing refused moved");
    // An item with no copy can go to a Desk on any repository; it isn't the other Desk's lead's to move back.
    assert!(answer(&w, &lead_a, &format!("MOVE: #{docs} -> Feature B")).contains(&format!("Moved: #{docs} to Feature B, with its notes. It's off your Desk.")));
    assert!(find(&w, &b, "Tidy the docs").is_some_and(|i| i["state"] == "new"));
    assert!(answer(&w, &lead_a, &format!("MOVE: #{docs} -> Feature A")).contains(&format!("Refused: MOVE #{docs}: #{docs} is another Desk's.")));
    let _ = lead_b;
}
