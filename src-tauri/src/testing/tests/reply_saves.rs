//! A reply's database writes (stability plan §2.3): its answered card is saved with the draw that follows it,
//! in the same write as the live sessions, so the reply never waits on the database, and a database held by a
//! slow write costs one wait (the draw's), not two.

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

#[test]
fn a_reply_never_waits_on_the_database_and_is_saved_with_the_next_draw() {
    let w = World::new();
    w.session("a").prompt("fix it").stop("Fixed.");
    let id = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    let release = hold_db();
    let reply_id = id.clone();
    let replying = w.client().spawn(move |c| c.ask(json!({"type": "reply", "id": reply_id, "text": "thanks"})));
    let until = Instant::now() + Duration::from_secs(1);
    while !replying.is_finished() && Instant::now() < until {
        std::thread::yield_now();
    }
    let waited = !replying.is_finished();
    release.send(()).unwrap();
    assert!(!waited, "the reply waited on the database");
    assert_eq!(replying.join().unwrap()["ok"], true);
    assert!(!crate::db::history(50).iter().any(|i| i.id == id), "not saved before the draw");
    w.settle();
    assert!(crate::db::history(50).iter().any(|i| i.id == id && i.status == "answered"), "saved by the draw");
}

/// A session deleted after its card was answered, before the draw: its answer isn't saved.
#[test]
fn an_answer_of_a_session_deleted_before_the_draw_isnt_saved() {
    let w = World::new();
    w.session("a").prompt("fix it").stop("Fixed.");
    let id = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    w.client().ask(json!({"type": "reply", "id": id, "text": "thanks"}));
    w.hub().purge("a");
    w.settle();
    assert!(!crate::db::history(50).iter().any(|i| i.id == id));
}
