//! What a session's row says, case by case (stability plan §4, Sessions list): sessions first heard of mid-way,
//! names (yours, cleared, kept through a busy turn and a restart), stopping one, compacting until it's done,
//! and what Closed keeps across a restart.

use crate::testing::{Key, World};
use serde_json::json;

#[test]
fn a_prompt_from_a_session_cue_never_saw_lists_it_at_work() {
    let w = World::new();
    w.session("a").prompt("go");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working");
    assert!(s.cards().is_empty());
}

#[test]
fn a_turn_that_ends_with_nothing_said_still_waits_for_you() {
    let w = World::new();
    w.session("a").prompt("go").stop("");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.card("a")["kind"], "waiting");
}

#[test]
fn an_error_from_a_session_cue_never_saw_is_a_card_saying_so() {
    let w = World::new();
    w.session("a").failed("server_error", "Overloaded");
    assert_eq!(w.snapshot().card("a")["message"], "Stopped on an error: Overloaded");
}

#[test]
fn compacting_shows_until_its_transcript_says_the_compaction_is_done() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    w.advance(1000);
    a.stop("Done.");
    w.tick(false);
    assert!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0) > 0, "a turn's end isn't the compaction's");
    a.transcript().compact_boundary();
    w.tick(false);
    assert_eq!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0), 0);
}

#[test]
fn compacting_that_never_finishes_is_given_up_after_fifteen_minutes() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    w.advance(15 * 60_000);
    w.tick(false);
    assert!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0) > 0);
    w.advance(1000);
    w.tick(false);
    assert_eq!(w.snapshot().session("a")["compacting_ms"].as_u64().unwrap_or(0), 0);
}

#[test]
fn a_session_that_ends_while_compacting_goes_under_closed() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done.");
    a.compacting();
    a.ended();
    let s = w.snapshot();
    assert!(s.find("a").is_none());
    assert_eq!(s.ended(), ["a"]);
}

#[test]
fn stopping_a_working_session_presses_esc_in_its_terminal_and_says_so_in_its_chat() {
    let w = World::new();
    w.session("a").in_tmux("%4").prompt("go");
    let r = w.interrupt("a");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(w.terminal.keys("a"), [Key::Escape]);
    let s = w.snapshot();
    assert_eq!(s.state("a"), "stopped");
    assert_eq!(s.last_yours("a")["text"], "⏹ stopped it");
}

#[test]
fn stopping_a_session_cue_has_no_record_of_says_so() {
    let w = World::new();
    assert_eq!(w.interrupt("nobody"), json!({"ok": false, "detail": "that session is gone"}));
}

#[test]
fn a_name_given_while_claude_works_is_typed_once_its_turn_ends() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    assert_eq!(w.hub().rename("a", "  login   fix ").unwrap(), "Renamed · Claude Code gets it when this turn ends");
    assert_eq!(w.snapshot().session("a")["name"], "login fix", "shown at once, its spaces tidied");
    assert!(w.terminal.typed("a").is_empty(), "not typed into a turn at work");
    a.stop("Done.");
    w.advance(2000);
    assert_eq!(w.terminal.typed("a"), ["/rename login fix"]);
}

#[test]
fn a_name_cleared_is_cleared_in_cue_only() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().rename("a", "x").unwrap();
    assert_eq!(w.hub().rename("a", "   ").unwrap(), "Name cleared in Cue");
    assert_eq!(w.terminal.typed("a"), ["/rename x"], "nothing more typed");
    assert_eq!(w.hub().rename("nobody", "x"), Err("that session is gone".to_string()));
}

#[test]
fn a_name_its_terminal_couldnt_take_is_kept_with_why() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.terminal.pane("a", |p| p.takes = crate::testing::Takes::Refuses("its pane is gone".into()));
    let e = w.hub().rename("a", "login").unwrap_err();
    assert!(e.contains("its pane is gone"), "{e}");
    assert_eq!(w.snapshot().session("a")["name"], "login", "Cue keeps it");
}

#[test]
fn a_name_is_kept_through_a_restart() {
    let mut w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().rename("a", "login").unwrap();
    w.relaunch();
    assert_eq!(w.snapshot().session("a")["name"], "login");
}

#[test]
fn a_name_sixty_characters_long_at_most() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().rename("a", &"n".repeat(200)).unwrap();
    assert_eq!(w.snapshot().session("a")["name"].as_str().unwrap().chars().count(), 60);
}

#[test]
fn closed_still_lists_a_session_after_a_restart() {
    let mut w = World::new();
    w.session("a").in_project("web").prompt("go").stop("Done.");
    w.hub().rename("a", "web-fix").unwrap();
    w.settle();
    w.session("a").ended();
    w.relaunch();
    let closed = w.hub().closed(24 * 3600 * 1000, crate::ipc::CLOSED_MAX);
    assert_eq!((closed[0]["session_id"].as_str(), closed[0]["name"].as_str()), (Some("a"), Some("web-fix")));
    assert!(w.snapshot().ended().is_empty(), "the window's list is this run's; Closed asks the saved one");
}

#[test]
fn a_starred_session_stays_starred_after_a_restart() {
    let mut w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().set_starred("a", true);
    w.relaunch();
    assert!(w.hub().session_star("a") > 0);
}
