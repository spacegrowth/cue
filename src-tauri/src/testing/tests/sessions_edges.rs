//! Sessions at their edges (stability plan §4, Sessions list): events out of their usual order or twice,
//! sessions driven by another agent, the sweep across agents and machines, Closed, an empty folder, and
//! hundreds of sessions at once.

use crate::testing::World;
use serde_json::json;
use std::collections::HashSet;

/// Claude Code's running list on this Mac, as `sweep` takes it.
fn running(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

/// Session `sid` as Claude Code's registry lists it: running here, `status` since `since_ms`.
fn listed(sid: &str, status: &str, since_ms: u64) -> crate::live::Quiet {
    let (session_id, harness, name, cwd, pid) = (sid.to_string(), "claude".to_string(), String::new(), format!("/work/{sid}"), std::process::id() as i32);
    crate::live::Quiet { session_id, harness, name, generated: false, cwd, pid, status: status.into(), waiting_for: "approve Bash".into(), since_ms }
}

/// Sweep this Mac with `live` as Claude Code's list, twice, past the wait for a missing session.
fn sweep_past_the_wait(w: &World, live: &HashSet<String>) {
    w.hub().sweep("", Some(live));
    w.advance(46_000);
    w.hub().sweep("", Some(live));
}

#[test]
fn a_stop_from_a_session_cue_never_saw_start_is_a_card() {
    let w = World::new();
    w.session("a").stop("Done while Cue was closed.");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.card("a")["message"], "Done while Cue was closed.");
}

#[test]
fn the_same_stop_twice_is_one_card() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.stop("Done.");
    assert_eq!(w.snapshot().cards_of("a").len(), 1);
}

#[test]
fn a_late_stop_after_its_end_does_not_bring_the_session_back() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    a.ended();
    a.stop("Done.");
    let s = w.snapshot();
    assert!(s.find("a").is_none(), "its Stop hook was slower than its SessionEnd: it's still ended");
    assert!(s.cards().is_empty());
    assert_eq!(s.ended(), ["a"]);
}

#[test]
fn ended_twice_is_under_closed_once() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").ended();
    a.ended();
    assert_eq!(w.snapshot().ended(), ["a"]);
}

#[test]
fn ended_resumed_and_ended_again_is_under_closed_once() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").ended();
    w.advance(1000);
    a.prompt("back").ended();
    assert_eq!(w.snapshot().ended(), ["a"]);
}

#[test]
fn closed_keeps_the_latest_thirty() {
    let w = World::new();
    for i in 0..35 {
        w.session(&format!("s{i:02}")).prompt("go").ended();
        w.advance(10);
    }
    let ended = w.snapshot().ended();
    assert_eq!(ended.len(), crate::ipc::CLOSED_MAX);
    assert_eq!((ended[0].as_str(), ended[29].as_str()), ("s34", "s05"), "newest first, the oldest dropped");
}

#[test]
fn a_prompt_while_compacting_ends_the_compacting_mark() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    a.prompt("next");
    let s = w.snapshot();
    assert_eq!(s.session("a")["compacting_ms"].as_u64().unwrap_or(0), 0, "{}", s.session("a"));
    assert_eq!(s.state("a"), "working");
}

#[test]
fn compacting_twice_keeps_when_it_began() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    let first = w.snapshot().session("a")["compacting_ms"].as_u64().unwrap();
    w.advance(5000);
    a.compacting();
    assert_eq!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap(), first);
}

#[test]
fn a_usage_limit_ends_with_your_next_prompt() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all").failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    a.prompt("carry on");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working");
    assert!(s.session("a")["limit"].is_null(), "{}", s.session("a")["limit"]);
}

#[test]
fn a_driven_session_asking_permission_is_still_your_card() {
    let w = World::new();
    let a = w.session("x");
    a.prompt("task 1").stop_driven("Report written.", "its lead (L)");
    let ask = a.ask_bash("git push");
    assert_eq!(w.snapshot().card("x")["id"], ask.id.as_str(), "only you can answer a permission");
}

#[test]
fn a_turn_of_yours_after_a_driven_one_is_your_card_again() {
    let w = World::new();
    let a = w.session("x");
    a.prompt("task 1").stop_driven("Report written.", "its lead (L)");
    a.prompt("what did you change?").stop("Two files.");
    let s = w.snapshot();
    assert_eq!(s.card("x")["message"], "Two files.");
    assert_eq!(s.state("x"), "waiting");
}

#[test]
fn the_sweep_leaves_codex_and_pi_sessions_to_their_own_ends() {
    let w = World::new();
    w.session("c").codex().prompt("go").stop("Done.");
    w.session("p").pi().prompt("go").stop("Done.");
    sweep_past_the_wait(&w, &running(&[]));
    let s = w.snapshot();
    assert!(s.find("c").is_some() && s.find("p").is_some(), "Claude Code's list only speaks for Claude sessions");
}

#[test]
fn this_macs_list_does_not_sweep_a_machines_sessions() {
    let w = World::new();
    w.drive(json!({"type": "event", "event": "stopped", "session_id": "r", "harness": "claude", "cwd": "/home/me/x", "machine": "box", "message": "Done."}));
    sweep_past_the_wait(&w, &running(&[]));
    assert!(w.snapshot().find("r").is_some());
    w.hub().sweep("box", Some(&running(&[])));
    w.advance(46_000);
    w.hub().sweep("box", Some(&running(&[])));
    assert!(w.snapshot().find("r").is_none(), "its own machine's list does");
}

#[test]
fn a_swept_sessions_open_ask_is_told_to_stand_down_and_it_goes_under_closed() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    sweep_past_the_wait(&w, &running(&[]));
    assert_eq!(ask.told(), Some(json!({"cancel": "session ended"})));
    let s = w.snapshot();
    assert_eq!(s.ended(), ["a"]);
    assert_eq!(s.history(&ask.id)["status"], "gone");
}

#[test]
fn a_machine_reached_again_is_no_longer_marked_unreachable() {
    let w = World::new();
    w.hub().sweep("box", None);
    assert_eq!(w.snapshot().0["unreachable"], json!(["box"]));
    w.hub().sweep("box", Some(&running(&[])));
    w.settle();
    assert_eq!(w.snapshot().0["unreachable"], json!([]));
}

#[test]
fn a_session_with_no_folder_is_listed_and_answered_like_any() {
    let w = World::new();
    let a = w.session("a").in_folder(std::path::Path::new(""));
    a.prompt("go").stop("Done.");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.session("a")["cwd"], "");
    let card = s.card("a")["id"].as_str().unwrap().to_string();
    w.reply(&card, "next");
    assert_eq!(w.terminal.typed("a"), ["next"]);
}

#[test]
fn a_session_with_no_folder_ends_into_closed() {
    let w = World::new();
    w.session("a").in_folder(std::path::Path::new("")).prompt("go").ended();
    let s = w.snapshot();
    assert_eq!(s.ended(), ["a"]);
    assert_eq!(s.0["ended"][0]["cwd"], "");
}

/// Session `sid`'s hook event, as the socket hands it over, with the draw that follows left for one `settle`
/// at the end (five hundred sessions each drawn on its own would save the whole list five hundred times).
fn hook(w: &World, sid: &str, event: &str, message: &str) {
    let cwd = format!("/work/p{}", sid.len() % 13);
    w.client().drive(json!({"type": "event", "event": event, "session_id": sid, "harness": "claude", "cwd": cwd, "message": message}));
}

#[test]
fn five_hundred_sessions_each_keep_their_own_state_and_card() {
    let w = World::new();
    // Every tenth at work (a turn's start reads its folder's git state: kept to a few), the rest finished.
    for i in 0..500 {
        let sid = format!("s{i:03}");
        let (event, message) = if i % 10 == 9 { ("active", format!("task {i}")) } else { ("stopped", format!("done {i}")) };
        hook(&w, &sid, event, &message);
    }
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.session_ids().len(), 500);
    assert_eq!(s.cards().len(), 450);
    assert_eq!((s.state("s499").as_str(), s.state("s498").as_str()), ("working", "waiting"));
    assert_eq!(s.card("s250")["message"], "done 250");
}

#[test]
fn five_hundred_sessions_gone_from_claude_codes_list_are_swept_together() {
    let w = World::new();
    for i in 0..500 {
        hook(&w, &format!("s{i:03}"), "stopped", "Done.");
    }
    w.settle();
    let keep: Vec<String> = (0..500).step_by(100).map(|i| format!("s{i:03}")).collect();
    sweep_past_the_wait(&w, &keep.iter().cloned().collect());
    let s = w.snapshot();
    assert_eq!(s.session_ids().into_iter().collect::<HashSet<_>>(), keep.into_iter().collect::<HashSet<_>>());
    assert_eq!(s.cards().len(), 5);
    assert_eq!(s.ended().len(), crate::ipc::CLOSED_MAX, "Closed keeps its latest, not all 495");
}

#[test]
fn a_session_claude_code_runs_that_cue_never_heard_from_is_taken_in() {
    let w = World::new();
    crate::live::set_claude(vec![listed("q", "busy", crate::clock::now_ms())]);
    w.hub().sync_live();
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.state("q"), "working");
    assert!(s.cards().is_empty());
}

#[test]
fn a_prompt_open_in_a_quiet_sessions_terminal_is_a_card_until_it_moves_on() {
    let w = World::new();
    w.session("q").prompt("go");
    crate::live::set_claude(vec![listed("q", "waiting", crate::clock::now_ms())]);
    w.hub().sync_live();
    assert!(w.snapshot().cards().is_empty(), "a few seconds first: Cue's own ask may be on its way");
    w.advance(5000);
    w.hub().sync_live();
    w.settle();
    assert_eq!(w.snapshot().card("q")["kind"], "terminal");
    w.advance(1000);
    crate::live::set_claude(vec![listed("q", "busy", crate::clock::now_ms())]);
    w.hub().sync_live();
    w.settle();
    assert!(w.snapshot().cards().is_empty(), "answered there");
}
