//! Words to an item that's to check reach its session and leave it to check (#584): your comment on its page ("ok
//! testing now", #535) and the lead's note (#542, after it landed) aren't a Send back. Only Send back reopens it.

use super::desk::sent_to;
use super::desk_commit::{ends, item, ready};
use super::desk_finish::lead_of;
use crate::desk::hub::landing::unlanded::NOTHING_NEW;
use crate::hosted::HostedEvent;
use crate::testing::World;

/// Every copy of the repository in use, as on a busy day: an item reopened would have to wait in line for one.
fn copies_in_use(w: &World) {
    let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
    let top = out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string();
    w.hub().desk_hold_copies(&top, &[1, 2, 3]);
}

fn holder(w: &World) -> String {
    let snap = w.snapshot();
    snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap()
}

/// Still to check, as it was: not at work, not in line for a copy, no new round.
fn still_to_check(w: &World, project: &str) {
    let it = item(w, project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert!(it.get("tree_wait").is_none_or(|t| t.is_null() || t == false), "not in line for a copy: {it}");
    assert!(it["backs"].as_array().is_none_or(|b| b.is_empty()), "not sent back: {it}");
    assert!(it["checks"].as_array().and_then(|c| c.last()).is_none_or(|c| c["back"].as_str().unwrap_or("").is_empty()), "no send-back on its DONE: {it}");
}

/// Its holder takes `prompt` and answers `reply`.
fn answers(w: &World, h: &str, prompt: &str, reply: &str) {
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
    w.settle();
    w.advance(1000);
    ends(w, h, reply);
}

#[test]
fn your_comment_on_an_item_to_check_reaches_its_session_and_leaves_it_to_check() {
    // #535: "ok testing now" on an item to check was sent as a Send back ("isn't done: ok testing now"), so it
    // reopened and went in line for a copy, every copy being in use.
    let w = World::new();
    let (_, project, id) = ready(&w);
    copies_in_use(&w);
    let h = holder(&w);
    let before = sent_to(&w, &h).len();
    w.hub().desk_item_act(&id, "note", "ok testing now").unwrap();
    w.settle();
    let sent = sent_to(&w, &h)[before..].to_vec();
    assert_eq!(sent, vec![format!("About #{id} (Header jumps): ok testing now")], "your words, as a comment");
    still_to_check(&w, &project);
    answers(&w, &h, &sent[0], "Thanks, I'll wait for your check.");
    still_to_check(&w, &project);
    // Send back still reopens it.
    w.hub().desk_item_act(&id, "send_back", "The header still jumps").unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert!(matches!(it["state"].as_str(), Some("working" | "queued")), "sent back: {it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{id} (Header jumps) isn't done: The header still jumps"))));
}

#[test]
fn the_leads_note_on_an_item_to_check_reaches_its_session_and_leaves_it_to_check() {
    // #542: the lead's note on an item that had landed reopened it as its session took it; its DONE then got "Nothing
    // of it landed on main since it was reopened", round and round.
    let w = World::new();
    let (_, project, id) = ready(&w);
    copies_in_use(&w);
    let h = holder(&w);
    let lead = lead_of(&w, &project);
    w.advance(1000);
    ends(&w, &lead, &format!("Telling it.\nNOTE: #{id} | Good. The landing installs Cue afterwards; don't run install.sh yourself."));
    let note = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Note from the Desk lead on Desk item #{id}"))).expect("the note sent");
    still_to_check(&w, &project);
    answers(&w, &h, &note, &format!("Understood.\nDONE: #{id}"));
    still_to_check(&w, &project);
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(NOTHING_NEW)), "not told it isn't done");
}
