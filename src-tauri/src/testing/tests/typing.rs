//! Typing into a session (stability plan §4, Typing): send, reply, kept while it works, ⌘↵, Stop, the
//! typing gate, and what Cue concludes when the agent never says it took a message.

use crate::testing::{Key, Takes, World};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Session `sid` at its prompt: one turn done, its card up.
fn waiting(w: &World, sid: &str) {
    w.session(sid).prompt("start").stop("Done.");
}

fn detail(v: &Value) -> String {
    v["detail"].as_str().unwrap_or("").to_string()
}

fn card_id(w: &World, sid: &str) -> String {
    w.snapshot().card(sid)["id"].as_str().unwrap().to_string()
}

#[test]
fn a_message_to_a_session_at_its_prompt_is_typed_and_taken() {
    let w = World::new();
    waiting(&w, "a");
    let r = w.send_to("a", "run the tests");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(true), "sent via fake pane a".to_string()));
    assert_eq!(w.terminal.typed("a"), ["run the tests"]);
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working", "its prompt hook came");
    let mine = s.last_yours("a");
    assert_eq!(mine["text"], "run the tests");
    assert_ne!(mine["pending"], true);
    assert!(s.cards_of("a").is_empty(), "working: its card is gone");
}

#[test]
fn a_reply_to_a_finished_turn_is_typed_and_the_card_goes_to_history() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    let r = w.reply(&id, "yes, ship it");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(w.terminal.typed("a"), ["yes, ship it"]);
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    let h = s.history(&id);
    assert_eq!(h["status"], "answered");
    assert_eq!(h["outcome"], "replied: “yes, ship it”");
}

#[test]
fn a_reply_to_a_card_that_is_gone_is_refused() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    w.reply(&id, "first");
    let r = w.reply(&id, "second");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "that card is gone".to_string()));
    assert_eq!(w.terminal.typed("a"), ["first"]);
}

#[test]
fn a_message_sent_while_it_works_is_kept_and_typed_when_the_turn_ends() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("refactor the router");
    let r = w.send_to("a", "keep the old API");
    assert_eq!(detail(&r), "kept in Cue: it goes when this turn ends");
    assert!(w.terminal.typed("a").is_empty(), "nothing typed mid-turn");
    assert_eq!(w.snapshot().session("a")["held"]["text"], "keep the old API");
    a.stop("Router done.");
    assert_eq!(w.terminal.typed("a"), ["keep the old API"], "typed as the turn ended");
    let s = w.snapshot();
    assert!(s.session("a")["held"].is_null());
    assert_eq!(s.state("a"), "working", "and it took it");
}

#[test]
fn two_messages_kept_during_one_turn_go_as_one() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("refactor");
    w.send_to("a", "keep the old API");
    w.send_to("a", "and run the tests");
    assert_eq!(w.snapshot().session("a")["held"]["text"], "keep the old API\n\nand run the tests");
    a.stop("ok");
    assert_eq!(w.terminal.typed("a"), ["keep the old API\n\nand run the tests"]);
}

#[test]
fn a_reply_to_a_card_whose_session_is_compacting_is_kept_not_typed_mid_compaction() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    // /compact typed in its terminal: its card stays, but Claude Code holds what's typed until it's done.
    w.session("a").compacting();
    let r = w.reply(&id, "late answer");
    assert_eq!(detail(&r), "kept in Cue: it goes when this turn ends");
    assert!(w.terminal.typed("a").is_empty(), "typed now it would land mid-turn");
    assert!(w.snapshot().history(&id)["outcome"].as_str().unwrap().ends_with("(kept in Cue until its turn ends)"));
}

#[test]
fn send_now_stops_the_turn_then_types_apart_from_the_stopped_prompt() {
    let w = World::new();
    w.session("a").prompt("long job");
    let r = w.send_now("a", "stop, do this instead");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(w.terminal.keys("a"), [Key::Escape, Key::Typed("\n\nstop, do this instead".into())], "Esc, then a blank line so the two prompts don't run together");
    assert_eq!(w.snapshot().last_yours("a")["text"], "stop, do this instead");
}

#[test]
fn stop_presses_esc_and_says_so_in_the_chat() {
    let w = World::new();
    w.session("a").prompt("long job");
    let r = w.interrupt("a");
    assert_eq!(detail(&r), "stopped via fake pane a");
    assert_eq!(w.terminal.keys("a"), [Key::Escape]);
    let s = w.snapshot();
    assert_eq!(s.state("a"), "stopped");
    assert_eq!(s.thread("a").last().unwrap().1, "⏹ stopped it");
}

#[test]
fn a_message_kept_for_a_session_goes_a_moment_after_stop() {
    let w = World::new();
    w.session("a").prompt("long job");
    w.send_to("a", "then this");
    w.interrupt("a");
    assert!(w.terminal.typed("a").is_empty(), "not at once: the terminal settles first");
    w.advance(700);
    assert_eq!(w.terminal.typed("a"), ["\n\nthen this"], "after Stop, apart from the prompt Claude Code hands back");
}

#[test]
fn a_message_to_an_ended_session_is_refused() {
    let w = World::new();
    waiting(&w, "a");
    w.session("a").ended();
    let r = w.send_to("a", "are you there");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "that session is gone".to_string()));
    assert!(w.terminal.typed("a").is_empty());
}

#[test]
fn a_message_while_it_asks_you_something_is_refused() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let _ask = a.ask_permission("Bash", json!({"command": "rm -rf build"}));
    let r = w.send_to("a", "hello");
    assert_eq!(detail(&r), "it's asking you something first: answer that, then send");
    assert!(w.terminal.typed("a").is_empty(), "typing now would answer its prompt");
}

#[test]
fn an_empty_message_is_refused() {
    let w = World::new();
    waiting(&w, "a");
    assert_eq!(detail(&w.send_to("a", "  ")), "nothing to send");
}

#[test]
fn a_terminal_that_refuses_the_message_fails_the_send_and_leaves_the_chat_as_it_was() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Refuses("tmux pane is gone".into()));
    let r = w.send_to("a", "hello");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "tmux pane is gone".to_string()));
    assert!(w.snapshot().thread("a").iter().all(|(_, t)| t != "hello"), "not shown as sent");
}

#[test]
fn a_kept_message_that_cant_be_typed_when_the_turn_ends_is_kept_and_says_why() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    w.send_to("a", "next step");
    w.terminal.pane("a", |p| p.takes = Takes::Refuses("tmux pane is gone".into()));
    a.stop("done");
    let held = w.snapshot().session("a")["held"].clone();
    assert_eq!(held["text"], "next step", "nothing you wrote is lost");
    assert_eq!(held["unsent"], true);
    assert!(held["delivery"].to_string().contains("couldn't send: tmux pane is gone"), "{held}");
}

#[test]
fn a_message_nobody_vouches_for_is_watched_and_then_left_with_no_verdict() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Nothing);
    let r = w.send_to("a", "hello?");
    assert_eq!(detail(&r), "via fake pane a; Cue is watching for it to take it");
    assert_eq!(w.snapshot().last_yours("a")["pending"], true, "a ring on the bubble while Cue looks");
    assert!(w.waiting_work() > 0, "a watch is scheduled");
    w.advance(11 * 60_000);
    let mine = w.snapshot().last_yours("a");
    assert_ne!(mine["pending"], true);
    assert_ne!(mine["unsent"], true, "nothing seen either way: no verdict");
    assert!(mine["delivery"].to_string().contains("Cue stopped looking"), "{mine}");
    assert_eq!(w.waiting_work(), 0, "and the watch is over");
}

#[test]
fn a_message_seen_still_in_its_box_gets_enter_again_then_is_called_not_taken() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.in_box = Some(true);
    });
    w.send_to("a", "hello?");
    assert!(w.terminal.keys("a").contains(&Key::Enter), "Enter once more for text still in its box");
    w.advance(60_000);
    let mine = w.snapshot().last_yours("a");
    assert_eq!(mine["unsent"], true, "{mine}");
    assert!(mine["delivery"].to_string().contains("not taken: still in its box"), "{mine}");
}

#[test]
fn a_message_its_log_shows_taken_is_settled_by_the_log() {
    let w = World::new();
    let a = w.session("a");
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Nothing);
    w.send_to("a", "hello?");
    // No hook came (its hooks report to another Cue), but its log has the prompt.
    a.transcript().prompt("hello?");
    w.advance(2_000);
    let s = w.snapshot();
    let mine = s.last_yours("a");
    assert_ne!(mine["pending"], true);
    assert!(mine["delivery"].to_string().contains("its log has it as your turn"), "{mine}");
    assert_eq!(s.state("a"), "working", "Cue starts the turn its hook would have");
}

#[test]
fn a_message_whose_pane_went_back_to_a_shell_is_called_not_taken() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.gone = Some(true);
    });
    w.send_to("a", "hello?");
    w.advance(45_000);
    let mine = w.snapshot().last_yours("a");
    assert_eq!(mine["unsent"], true);
    assert!(mine["delivery"].to_string().contains("shell prompt"), "{mine}");
}

#[test]
fn a_second_message_waits_for_the_first_to_be_typed_then_finds_it_working() {
    let w = World::new();
    waiting(&w, "a");
    let release = Arc::new(AtomicBool::new(false));
    w.terminal.pane("a", |p| p.takes = Takes::Hangs(release.clone()));
    let first = w.client().spawn(|c| c.send_to("a", "first", false));
    w.terminal.wait_hung();
    let second = w.client().spawn(|c| c.send_to("a", "second", false));
    std::thread::sleep(std::time::Duration::from_millis(50));
    release.store(true, Ordering::SeqCst);
    let (first, second) = (first.join().unwrap(), second.join().unwrap());
    assert_eq!(first["detail"], "sent via fake pane a");
    assert_eq!(detail(&second), "kept in Cue: it goes when this turn ends", "it went after the first, which had started a turn");
    assert_eq!(w.terminal.typed("a"), ["first"], "never two pastes into one box");
}

#[test]
fn a_message_behind_one_stuck_typing_gives_up_after_the_gate_wait() {
    let w = World::new();
    waiting(&w, "a");
    let release = Arc::new(AtomicBool::new(false));
    w.terminal.pane("a", |p| p.takes = Takes::Hangs(release.clone()));
    let first = w.client().spawn(|c| c.send_to("a", "first", false));
    w.terminal.wait_hung();
    let r = w.send_to("a", "second");
    assert_eq!(r["ok"], false);
    assert!(detail(&r).starts_with("the message before this one is still being typed"), "{r}");
    release.store(true, Ordering::SeqCst);
    assert_eq!(first.join().unwrap()["ok"], true);
}

#[test]
fn a_pi_session_gets_your_message_down_its_own_connection() {
    let w = World::new();
    let p = w.session("p").pi();
    let mut rx = p.subscribe();
    p.prompt("hello").stop("hi");
    let r = w.send_to("p", "next");
    assert_eq!(detail(&r), "sent via the session directly");
    let got = rx.try_recv().unwrap();
    assert_eq!((got["type"].as_str(), got["text"].as_str()), (Some("reply"), Some("next")));
    assert!(w.terminal.typed("p").is_empty(), "nothing typed into a terminal");
}

#[test]
fn a_message_to_a_working_pi_session_is_queued_with_it_not_kept() {
    let w = World::new();
    let p = w.session("p").pi();
    let mut rx = p.subscribe();
    p.prompt("refactor");
    let r = w.send_to("p", "keep the old API");
    assert!(detail(&r).starts_with("queued via the session directly"), "{r}");
    assert_eq!(rx.try_recv().unwrap()["text"], "keep the old API");
    assert_eq!(w.snapshot().session("p")["queued"]["text"], "keep the old API");
    p.stop("done");
    assert!(w.snapshot().session("p")["queued"].is_null(), "read once the turn is over");
}
