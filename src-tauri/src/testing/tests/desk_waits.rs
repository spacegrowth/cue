//! Waiting isn't stopping (#249): a holder's turn that ends saying it waits on a build, tests, a helper or another
//! commit shows its item as waiting on that, isn't asked "Done?", and doesn't send the Desk lead a stop notice. Nor does
//! one whose own shell commands still run in the background: "Running tests…" (#435). A stop that asks you nothing is
//! the lead's, every time: it never waits on you (#435).

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::{Transcript, World};

/// A holder at work on "Header" for the lead `l`, and the item's id; `turn` gives it a prompt and ends its turn.
fn routed(w: &World) -> (String, String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    ends(w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(w, &project), "Header")["id"].as_str().unwrap().to_string();
    (h, project, l, id)
}

fn turn(w: &World, h: &str, prompt: &str, reply: &str) {
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
    w.settle();
    w.advance(1000);
    ends(w, h, reply);
}

#[test]
fn a_turn_that_waits_on_a_build_tests_a_helper_or_a_commit_is_waiting_not_stopped() {
    for (reply, line) in [
        ("Started the build.\nThe build for 57d72d4e has started.", "The build for 57d72d4e has started."),
        ("Fixed the header.\n\nI'm waiting on the test run.", "I'm waiting on the test run."),
        ("Most of it is in.\n- still compiling", "still compiling"),
        ("Handed the footer over.\nWaiting for the helper to finish its part.", "Waiting for the helper to finish its part."),
        ("Ready.\n**Blocked on** cue-161 committing desk.rs.", "Blocked on cue-161 committing desk.rs."),
    ] {
        let w = World::new();
        let (h, project, l, _) = routed(&w);
        turn(&w, &h, "Header", reply);
        let it = item(&desk(&w, &project), "Header").clone();
        assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["waiting"].as_str(), it["latest"].as_str()), (Some("working"), None, Some(line), Some(line)), "{reply}: {it}");
        assert!(sent_to(&w, &h).iter().all(|t| !t.starts_with("Done? Write DONE")), "{reply}: not asked");
        assert!(sent_to(&w, &l).iter().all(|t| !t.contains(") stopped")), "{reply}: no notice to the lead: {:?}", sent_to(&w, &l));
        // Its time to settle passing changes nothing: it still waits, and nothing waits on you.
        w.advance(11 * 60_000);
        assert_eq!(item(&desk(&w, &project), "Header")["state"], "working", "{reply}");
    }
}

#[test]
fn a_plain_line_with_a_build_word_is_still_a_stop() {
    let w = World::new();
    let (h, project, _, id) = routed(&w);
    turn(&w, &h, "Header", "The build passed and the header is in.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["stopped"].as_bool(), it["waiting"].as_str()), (Some(true), None), "{it}");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| **t == format!("Done? Write DONE: #{id}, or say what's left")).count(), 1);
}

#[test]
fn a_turn_that_ends_with_its_tests_running_in_the_background_shows_running_tests() {
    let w = World::new();
    let (h, project, l, _) = routed(&w);
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    let log = w.hub().session_origin(&h).unwrap().transcript_path;
    Transcript::at(&log).prompt("Header").background_command("b1", "Run the desk tests", "cargo test --lib desk_").assistant("A re-run of the tests is going now.");
    w.advance(1000);
    ends(&w, &h, "A re-run of the tests is going now.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["waiting"].as_str()), (Some("working"), None, Some("Running tests…")), "{it}");
    assert!(sent_to(&w, &h).iter().all(|t| !t.starts_with("Done? Write DONE")), "not asked");
    assert!(sent_to(&w, &l).iter().all(|t| !t.contains(") stopped")), "no notice to the lead: {:?}", sent_to(&w, &l));
    w.advance(11 * 60_000);
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!(it["state"], "working", "{it}");
    assert!(desk(&w, &project)["items"].as_array().unwrap().iter().all(|i| i["state"] != "needs_you"), "nothing waits on you, so the header counts none");
}

#[test]
fn running_words_say_what_runs() {
    use crate::sessions::turn::running_words;
    let s = |v: &[&str]| running_words(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    assert_eq!(s(&["Run the desk tests"]), "Running tests…");
    assert_eq!(s(&["npx playwright test specs/desk.spec.mjs"]), "Running tests…");
    assert_eq!(s(&["Build Cue"]), "Building…");
    assert_eq!(s(&["npm run dev"]), "Running in the background: npm run dev");
    assert_eq!(s(&[]), "");
}

#[test]
fn every_stop_goes_to_the_lead_and_none_waits_on_you() {
    let w = World::new();
    let (h, project, l, id) = routed(&w);
    let told = || sent_to(&w, &l).iter().filter(|t| t.starts_with(&format!("#{id} (Header) stopped"))).count();
    let words = format!("Done? Write DONE: #{id}, or say what's left");
    turn(&w, &h, "Header", "Looked at it.");
    turn(&w, &h, &words, "The footer still overlaps it.");
    assert_eq!(told(), 1);
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool()), (Some("working"), Some(true)), "{it}");
    // The lead sends it back; it stops again: the lead hears of it again, and it still isn't yours.
    ends(&w, &l, &format!("ROUTE: #{id} -> {h} | why: fix the footer"));
    turn(&w, &h, "Fix the footer", "Moved things around.");
    turn(&w, &h, &words, "Still not right.");
    assert_eq!(told(), 2, "{:?}", sent_to(&w, &l));
    w.advance(11 * 60_000);
    let it = item(&desk(&w, &project), "Header").clone();
    assert_ne!(it["state"], "needs_you", "{it}");
    // Many turns in a row with nothing new from the lead: no flood of notices.
    for n in 0..4 {
        turn(&w, &h, "Go on", &format!("Try {n}."));
    }
    assert_eq!(told(), 2);
    assert_ne!(item(&desk(&w, &project), "Header")["state"], "needs_you");
}
