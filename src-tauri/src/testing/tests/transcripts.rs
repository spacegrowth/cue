//! What Cue learns from a Claude Code session's transcript when no hook tells it (stability plan §4,
//! Sessions and Asks): a turn whose Stop never came, a compaction done, an ask answered in the terminal,
//! the status list a prompt asked for, and your latest request.

use crate::testing::{Snap, World};
use std::time::Duration;
use serde_json::json;

#[test]
fn a_turn_whose_stop_never_came_is_finished_from_its_transcript() {
    let w = World::new();
    let a = w.session("a").claude();
    a.prompt("tidy up");
    w.advance(15_000);
    a.transcript().prompt("tidy up").assistant("Tidied.").turn_end();
    w.advance(4_000);
    w.tick(true);
    assert_eq!(w.snapshot().state("a"), "working", "ended seconds ago: its hook may still come");
    w.advance(6_000);
    w.tick(true);
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.card("a")["message"], "Tidied.");
}

#[test]
fn a_turn_still_going_in_its_transcript_is_left_working() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("tidy up");
    a.transcript().prompt("tidy up").assistant("Working on it");
    w.advance(30_000);
    w.tick(true);
    assert_eq!(w.snapshot().state("a"), "working");
}

#[test]
fn a_compaction_ends_when_its_transcript_marks_it_done() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    w.tick(false);
    assert!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0) > 0, "not done yet");
    a.transcript().compact_boundary();
    w.tick(false);
    let row = w.snapshot().session("a").clone();
    assert_eq!(row["compacting_ms"].as_u64().unwrap_or(0), 0, "{row}");
}

#[test]
fn an_ask_its_transcript_shows_answered_is_cleared_by_the_watcher() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("clean up");
    let input = json!({"command": "rm -rf build"});
    a.transcript().tool_use("toolu_1", "Bash", input.clone());
    let ask = a.ask_permission("Bash", input);
    w.tick(false);
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str(), "no result yet: still open");
    a.transcript().tool_result("toolu_1");
    w.tick(false);
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    assert_eq!(s.history(&ask.id)["outcome"], "answered in the terminal");
    assert_eq!(ask.told(), Some(json!({"cancel": "answered in the terminal"})), "its hook stands down");
}

#[test]
fn the_status_list_a_prompt_asked_for_goes_on_its_bubble() {
    let w = World::new();
    let a = w.session("a");
    let ask = "End this reply with **Where we are**";
    a.prompt("ship it");
    a.transcript().prompt_with_status_ask("u1", "ship it", ask);
    a.stop("Shipped.");
    assert_eq!(w.snapshot().last_yours("a")["status_ask"], ask);
}

#[test]
fn a_sessions_latest_request_is_read_from_its_transcript() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    a.transcript().last_prompt("make the login page faster");
    assert_eq!(w.snapshot().0["about"]["a"]["prompt"], "make the login page faster");
}

#[test]
fn a_sessions_cost_from_its_status_line_is_kept() {
    let w = World::new();
    let a = w.session("a").in_tmux("%3");
    a.prompt("go").cost(1.25);
    assert_eq!(w.hub().session_cost("a"), Some(1.25));
    assert_eq!(w.snapshot().session("a")["tmux_pane"], "%3");
}

/// A session's log slow to read (a stalled disk) holds up only the read waiting on it: the chat's read of it
/// waits, while the watcher's tick, which asks the same session's feed for its agents, and the window's state
/// go on. (The feed's lock is never held while its log is read.)
#[test]
fn a_slow_read_of_a_sessions_log_holds_up_neither_the_tick_nor_the_window() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("tidy up");
    let log = w.home().join("claude").join("a.jsonl").to_string_lossy().into_owned();
    a.transcript().prompt("tidy up").assistant("Working on it");
    // The chat has it open (its feed is there), and the log has grown since.
    crate::steps::steps(&log, 0);
    a.transcript().assistant("Still at it");
    let (release, started) = crate::steps::slow_disk::hold_next(&log);
    let chat = { let log = log.clone(); std::thread::spawn(move || crate::steps::steps(&log, 0)) };
    started.recv_timeout(Duration::from_secs(5)).expect("the chat's read reached the disk");
    let hub = w.hub().clone();
    let (done_tx, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        crate::server::tick(&hub, false);
        let _ = done_tx.send(hub.snapshot());
    });
    let built = done.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    let snap = built.expect("the tick and the window's state waited on the chat's slow read");
    assert_eq!(Snap(snap.as_ref().clone()).state("a"), "working");
    let steps = chat.join().unwrap();
    let says: Vec<&str> = steps["turns"][0]["items"].as_array().unwrap().iter().filter_map(|i| i["text"].as_str()).collect();
    assert_eq!(says, ["Working on it", "Still at it"], "each line once, whichever read put it in");
}
