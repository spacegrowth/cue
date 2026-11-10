//! Sessions as their hooks report them (stability plan §4, Sessions): turns, ends, who sent a prompt,
//! sessions driven by another agent, the sweep of sessions Claude Code no longer runs, and Closed.

use crate::testing::World;
use serde_json::json;
use std::collections::HashSet;

#[test]
fn a_prompt_then_a_stop_is_a_working_session_then_a_card() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("fix the login");
    assert_eq!(w.snapshot().state("a"), "working");
    a.stop("Fixed.");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.card("a")["kind"], "waiting");
    assert_eq!(s.card("a")["message"], "Fixed.");
    assert_eq!(s.thread("a"), vec![("you".to_string(), "fix the login".to_string()), ("agent".into(), "Fixed.".into())]);
}

#[test]
fn a_new_prompt_takes_its_waiting_card_down() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("one").stop("Done.");
    a.prompt("two");
    assert!(w.snapshot().cards_of("a").is_empty());
}

#[test]
fn a_session_started_by_cue_waits_for_you_with_no_card() {
    let w = World::new();
    w.session("a").started();
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert!(s.cards().is_empty());
}

#[test]
fn an_ended_session_leaves_the_list_for_closed_and_keeps_its_folder() {
    let w = World::new();
    let a = w.session("a").in_project("web");
    a.prompt("go").stop("Done.").ended();
    let s = w.snapshot();
    assert!(s.find("a").is_none());
    assert!(s.cards().is_empty(), "its card went with it");
    assert_eq!(s.ended(), ["a"]);
    assert_eq!(s.0["ended"][0]["cwd"], "/work/web");
}

#[test]
fn closed_lists_the_latest_ended_first() {
    let w = World::new();
    for sid in ["a", "b", "c"] {
        w.session(sid).prompt("go").ended();
        w.advance(1000);
    }
    assert_eq!(w.snapshot().ended(), ["c", "b", "a"]);
}

#[test]
fn a_session_resumed_after_it_ended_is_live_again_and_not_under_closed() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").ended();
    a.prompt("back again");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working");
    assert!(s.ended().is_empty());
}

#[test]
fn a_turn_driven_by_another_agent_is_its_and_not_your_card() {
    let w = World::new();
    let a = w.session("x");
    a.prompt("task 1");
    a.stop_driven("Report written.", "its lead (lead-1)");
    let s = w.snapshot();
    assert!(s.cards_of("x").is_empty());
    assert_eq!(s.state("x"), "agent");
    assert_eq!(s.session("x")["driven_by"], "its lead (lead-1)");
}

#[test]
fn a_prompt_the_transcript_says_you_sent_is_yours() {
    let w = World::new();
    w.session("a").prompt_logged("add tests", "p1", json!({"kind": "human"}));
    assert_eq!(w.snapshot().thread("a"), [("you".to_string(), "add tests".to_string())]);
}

#[test]
fn a_prompt_another_session_sent_shows_as_from_it() {
    let w = World::new();
    w.session("a").prompt_logged("please review", "p1", json!({"kind": "peer", "name": "cue-3e", "body": "please review"}));
    let s = w.snapshot();
    let thread = s.session("a")["thread"].clone();
    assert_eq!(thread[0]["role"], "peer", "{thread}");
    assert!(s.thread("a").iter().all(|(role, _)| role != "you"));
}

#[test]
fn claude_codes_own_notice_is_not_a_prompt_of_yours() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Started the build in the background.");
    a.prompt_logged("<task-notification><task-id>b1</task-id><status>completed</status></task-notification>", "p2", json!({"kind": "task-notification"}));
    assert_eq!(w.snapshot().thread("a").iter().filter(|(role, _)| role == "you").count(), 1, "only the first prompt is yours");
}

#[test]
fn a_usage_limit_pauses_the_session_without_a_card() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all");
    a.failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "limited");
    assert!(s.cards().is_empty());
    assert_eq!(s.session("a")["limit"]["scope"], "all");
}

#[test]
fn any_other_error_ends_the_turn_with_a_card_saying_so() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it");
    a.failed("server_error", "Overloaded");
    assert_eq!(w.snapshot().card("a")["message"], "Stopped on an error: Overloaded");
}

#[test]
fn compacting_shows_on_its_row() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    assert!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn what_a_session_is_doing_shows_on_its_row() {
    let w = World::new();
    let p = w.session("p").pi();
    p.prompt("build it");
    p.doing("Running: cargo test");
    assert_eq!(w.snapshot().session("p")["doing"], "Running: cargo test");
}

#[test]
fn a_codex_session_works_the_same_way() {
    let w = World::new();
    w.session("c").codex().prompt("port it").stop("Ported.");
    let s = w.snapshot();
    assert_eq!(s.card("c")["harness"], "codex");
    assert_eq!(s.state("c"), "waiting");
}

#[test]
fn a_pi_session_is_listed_from_the_moment_it_subscribes() {
    let w = World::new();
    let _rx = w.session("p").pi().subscribe();
    let s = w.snapshot();
    assert!(s.0["live"].as_array().unwrap().iter().any(|q| q["session_id"] == "p"), "{}", s.0["live"]);
}

#[test]
fn a_session_gone_from_claude_codes_list_is_ended_only_after_a_while() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.session("b").prompt("go").stop("Done.");
    let running: HashSet<String> = ["b".to_string()].into();
    w.hub().sweep("", Some(&running));
    w.advance(30_000);
    w.hub().sweep("", Some(&running));
    assert!(w.snapshot().find("a").is_some(), "missing a moment: still there");
    w.advance(20_000);
    w.hub().sweep("", Some(&running));
    let s = w.snapshot();
    assert!(s.find("a").is_none(), "missing past the wait: ended");
    assert!(s.cards_of("a").is_empty());
    assert!(s.find("b").is_some());
}

#[test]
fn a_session_writing_its_transcript_is_not_swept_whatever_the_list_says() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let none = HashSet::new();
    w.hub().sweep("", Some(&none));
    w.advance(50_000);
    a.transcript().assistant("still at it");
    w.hub().sweep("", Some(&none));
    assert!(w.snapshot().find("a").is_some(), "its conversation moved just now");
}

#[test]
fn back_in_the_list_resets_its_wait() {
    let w = World::new();
    w.session("a").prompt("go");
    let (none, back): (HashSet<String>, HashSet<String>) = (HashSet::new(), ["a".to_string()].into());
    w.hub().sweep("", Some(&none));
    w.advance(40_000);
    w.hub().sweep("", Some(&back));
    w.advance(10_000);
    w.hub().sweep("", Some(&none));
    assert!(w.snapshot().find("a").is_some(), "missing again only just now");
}

#[test]
fn an_unreachable_machine_marks_its_sessions_instead_of_ending_them() {
    let w = World::new();
    w.session("a").prompt("go");
    w.hub().sweep("box", None);
    w.advance(60_000);
    w.hub().sweep("box", None);
    let s = w.snapshot();
    assert!(s.find("a").is_some());
    assert_eq!(s.0["unreachable"], json!(["box"]));
}

#[test]
fn many_sessions_each_keep_their_own_state() {
    let w = World::new();
    for i in 0..50 {
        let a = w.session(&format!("s{i}"));
        a.prompt(&format!("task {i}"));
        if i % 2 == 0 {
            a.stop(&format!("done {i}"));
        }
    }
    let s = w.snapshot();
    assert_eq!(s.session_ids().len(), 50);
    assert_eq!(s.cards().len(), 25);
    assert_eq!(s.state("s7"), "working");
    assert_eq!(s.card("s8")["message"], "done 8");
}
