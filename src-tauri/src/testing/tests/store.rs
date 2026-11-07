//! The hub's store is never held across a database write (stability plan §2.3, §2.5): with the database held
//! as a slow write holds it, an answer, a star or a queued review changes the store and lets it go, and the
//! write lands once the database is free. Each of those paths, with the database free, saves what it did.

use crate::leads::{install_index, reported_exec, Member};
use crate::testing::World;
use serde_json::json;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The database held on a thread of its own until the returned sender hears (or goes).
fn hold_db() -> mpsc::Sender<()> {
    let (held_tx, held) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    std::thread::spawn(move || crate::db::hold(held_tx, release));
    held.recv().expect("the database held");
    release_tx
}

/// Polls `done` (each look takes the store) for up to a second: well inside the lock watchdog's two.
fn soon(done: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn an_answer_lets_the_store_go_before_it_is_saved() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("clean up");
    let ask = a.ask_permission("Bash", json!({"command": "rm -rf build"}));
    let release = hold_db();
    let id = ask.id.clone();
    let answering = w.client().spawn(move |c| c.ask(json!({"type": "respond", "id": id, "decision": {"behavior": "allow"}})));
    // The card moves to History in the store while its save waits on the database: the store is free to look at.
    let hub = w.hub().clone();
    let moved = soon(|| hub.get(&ask.id).is_some_and(|i| i.status == "answered"));
    let saving = !answering.is_finished();
    release.send(()).unwrap();
    assert!(moved, "the store was let go before the save");
    assert!(saving, "the answer was still waiting on the database meanwhile");
    assert_eq!(answering.join().unwrap(), json!({"ok": true}));
    assert!(crate::db::history(50).iter().any(|i| i.id == ask.id && i.status == "answered"), "saved once the database was free");
}

#[test]
fn a_reply_moves_its_card_to_history_and_saves_it() {
    let w = World::new();
    let a = w.session("r");
    a.prompt("fix it").stop("Fixed.");
    let id = w.snapshot().card("r")["id"].as_str().unwrap().to_string();
    w.reply(&id, "thanks");
    assert!(w.snapshot().cards_of("r").is_empty());
    let saved = crate::db::history(50).into_iter().find(|i| i.id == id).expect("saved");
    assert!(saved.outcome.starts_with("replied: “thanks”"), "{}", saved.outcome);
}

#[test]
fn starring_a_parked_session_lets_the_store_go_before_it_is_saved() {
    let w = World::new();
    w.session("p").prompt("drift").stop("Three resources drifted.");
    let hub = w.hub().clone();
    hub.add_parked(hub.session_record("p").unwrap());
    let release = hold_db();
    let starring = std::thread::spawn({
        let hub = hub.clone();
        move || hub.set_starred("p", true)
    });
    let starred = soon(|| hub.parked("p").is_some_and(|p| p.session.starred_ms > 0));
    let saving = !starring.is_finished();
    release.send(()).unwrap();
    assert!(starred && saving, "starred in the store while its save waited (starred {starred}, saving {saving})");
    starring.join().unwrap();
    assert!(crate::db::parked().iter().any(|p| p.session.origin.session_id == "p" && p.session.starred_ms > 0), "saved once the database was free");
    hub.set_starred("p", false);
    assert_eq!(crate::db::parked()[0].session.starred_ms, 0, "and unstarred, saved too");
}

#[test]
fn a_review_queued_behind_its_busy_lead_is_saved_and_taken_off_once_sent() {
    let w = World::new();
    let lead = Member { role: "lead".into(), plugin: "relay".into(), name: "lead".into(), ..Default::default() };
    install_index([("L".to_string(), lead), ("E1".to_string(), Member { name: "E1-topic".into(), id: "relay-E1".into(), ready_late: true, ..reported_exec() })].into());
    let l = w.session("L");
    l.prompt("lead");
    let release = hold_db();
    let queued = {
        let hub = w.hub().clone();
        std::thread::spawn(move || hub.review("L", "E1", "/relay:review E1"))
    };
    let hub = w.hub().clone();
    let in_store = soon(|| hub.queued_reviews() == ["E1"]);
    let saving = !queued.is_finished();
    release.send(()).unwrap();
    assert!(in_store && saving, "queued in the store while its save waited (queued {in_store}, saving {saving})");
    assert!(queued.join().unwrap().unwrap().starts_with("Queued"));
    assert_eq!(crate::db::review_queue().len(), 1, "saved, so a restart keeps it");
    l.stop("Done.");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["/relay:review E1"]);
    assert!(crate::db::review_queue().is_empty(), "sent: off the saved queue too");
}

#[test]
fn a_moot_queued_review_is_dropped_from_the_saved_queue() {
    let w = World::new();
    let lead = Member { role: "lead".into(), plugin: "relay".into(), name: "lead".into(), ..Default::default() };
    install_index([("L".to_string(), lead), ("E1".to_string(), reported_exec())].into());
    let l = w.session("L");
    l.prompt("lead");
    w.hub().review("L", "E1", "/relay:review E1").unwrap();
    install_index(Default::default());
    w.hub().drain_reviews();
    assert!(crate::db::review_queue().is_empty());
    assert!(w.terminal.typed("L").is_empty());
}
