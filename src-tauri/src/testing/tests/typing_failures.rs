//! Typing into a session when its terminal misbehaves (stability plan §4, Typing, F): a tmux trip that answers
//! late or not at all, sends piling up behind it on the session's typing gate, a paste whose Enter doesn't take,
//! a pane back at a shell, and an agent that never says it took the message.

use super::typing_support::{card_id, detail, waiting, Stuck};
use crate::testing::{Key, Takes, World};
use std::sync::{Arc, Barrier};

#[test]
fn a_typing_whose_tmux_trip_answers_after_30s_still_goes_and_is_taken() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Slow(30_000));
    let r = w.send_to("a", "slow but fine");
    assert_eq!(detail(&r), "sent via fake pane a");
    let s = w.snapshot();
    assert_ne!(s.last_yours("a")["pending"], true);
    assert_eq!(s.state("a"), "working");
}

#[test]
fn a_message_behind_a_slow_typing_waits_its_turn_and_then_is_kept() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Slow(10_000));
    let first = w.client().spawn(|c| c.send_to("a", "first", false));
    let second = w.client().spawn(|c| c.send_to("a", "second", false));
    let (first, second) = (first.join().unwrap(), second.join().unwrap());
    let said = [detail(&first), detail(&second)];
    assert!(said.contains(&"sent via fake pane a".to_string()) && said.contains(&"kept in Cue: it goes when this turn ends".to_string()), "{said:?}");
    assert_eq!(w.terminal.typed("a").len(), 1, "one paste; the other waits for the turn");
    assert_eq!(w.terminal.most_at_once(), 1);
}

#[test]
fn many_sends_at_once_to_one_session_never_type_two_at_a_time_and_lose_nothing() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Slow(200));
    let n = 8;
    let go = Arc::new(Barrier::new(n));
    let sends: Vec<_> = (0..n)
        .map(|i| {
            let go = go.clone();
            w.client().spawn(move |c| {
                go.wait();
                c.send_to("a", &format!("m{i}"), false)
            })
        })
        .collect();
    for s in sends {
        assert_eq!(s.join().unwrap()["ok"], true);
    }
    w.settle();
    assert_eq!(w.terminal.most_at_once(), 1, "never two pastes into one box");
    let typed = w.terminal.typed("a");
    assert_eq!(typed.len(), 1, "{typed:?}");
    let held = w.snapshot().session("a")["held"]["text"].as_str().unwrap_or("").to_string();
    let mut all: Vec<&str> = held.split("\n\n").chain(typed.iter().map(String::as_str)).collect();
    all.sort();
    let mut want: Vec<String> = (0..n).map(|i| format!("m{i}")).collect();
    want.sort();
    assert_eq!(all, want, "each message typed or kept, once");
}

#[test]
fn a_session_stuck_typing_holds_up_no_other_session() {
    let w = World::new();
    waiting(&w, "a");
    waiting(&w, "b");
    let stuck = Stuck::sending(&w, "a", "into the void");
    let r = w.send_to("b", "hello b");
    assert_eq!(detail(&r), "sent via fake pane b", "b has a gate of its own");
    assert_eq!(stuck.release()["ok"], true);
}

#[test]
fn a_reply_behind_a_stuck_typing_gives_up_after_the_gate_wait_and_its_card_stays() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    let stuck = Stuck::sending(&w, "a", "first");
    let r = w.reply(&id, "an answer");
    assert_eq!(r["ok"], false);
    assert!(detail(&r).starts_with("the message before this one is still being typed"), "{r}");
    assert_eq!(w.snapshot().card("a")["id"], id.as_str(), "the card is still there to answer");
    stuck.release();
}

#[test]
fn a_message_refused_at_the_gate_can_be_sent_again_once_the_stuck_one_lands() {
    let w = World::new();
    waiting(&w, "a");
    let stuck = Stuck::sending(&w, "a", "first");
    assert_eq!(w.send_to("a", "second")["ok"], false);
    stuck.release();
    let again = w.send_to("a", "second");
    assert_eq!(detail(&again), "kept in Cue: it goes when this turn ends", "the first started a turn");
    w.session("a").stop("did the first");
    assert_eq!(w.terminal.typed("a"), ["first", "second"]);
}

#[test]
fn a_paste_whose_enter_didnt_take_gets_one_more_enter_and_is_taken() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::AtEnter(1));
    let r = w.send_to("a", "hello");
    assert_eq!(detail(&r), "sent via fake pane a");
    assert_eq!(w.terminal.keys("a"), [Key::Typed("hello".into()), Key::Enter]);
    assert_ne!(w.snapshot().last_yours("a")["pending"], true);
}

#[test]
fn a_paste_that_takes_two_more_enters_is_taken_before_the_send_returns() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::AtEnter(2));
    let r = w.send_to("a", "hello");
    assert_eq!(detail(&r), "sent via fake pane a");
    assert_eq!(w.terminal.enters("a"), 2);
    assert_eq!(w.waiting_work(), 0, "nothing left to watch");
}

#[test]
fn a_paste_still_in_its_box_after_the_send_is_taken_at_a_later_look() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::AtEnter(3));
    let r = w.send_to("a", "hello");
    assert_eq!(detail(&r), "via fake pane a; Cue is watching for it to take it");
    w.advance(2_000);
    assert_eq!(w.terminal.enters("a"), 3, "the watch's first look pressed Enter once more");
    let mine = w.snapshot().last_yours("a");
    assert_ne!(mine["pending"], true, "{mine}");
    assert_ne!(mine["unsent"], true);
}

#[test]
fn a_paste_whose_enter_never_takes_gets_a_few_enters_at_most_then_a_verdict() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.in_box = Some(true);
    });
    w.send_to("a", "hello?");
    w.advance(11 * 60_000);
    // One right after the paste, one as the send settles, then two from the watch: never an Enter storm.
    assert_eq!(w.terminal.enters("a"), 4);
    assert_eq!(w.snapshot().last_yours("a")["unsent"], true);
    assert_eq!(w.waiting_work(), 0);
}

#[test]
fn text_left_in_its_box_under_a_dialog_never_gets_an_enter() {
    let w = World::new();
    waiting(&w, "a");
    let dialog = crate::terminal::dialog::Dialog { title: "Allow?".into(), options: vec!["Yes".into(), "No".into()], ..Default::default() };
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.in_box = Some(true);
        p.dialog = Some(dialog);
    });
    w.send_to("a", "hello?");
    w.advance(5 * 60_000);
    assert_eq!(w.terminal.enters("a"), 0, "a bare Enter would say Yes to it");
}

#[test]
fn a_pane_back_at_a_shell_is_not_called_before_the_wait_is_up() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.gone = Some(true);
    });
    w.send_to("a", "hello?");
    w.advance(20_000);
    let mine = w.snapshot().last_yours("a");
    assert_eq!((mine["pending"].as_bool(), mine["unsent"].as_bool()), (Some(true), None), "{mine}");
}

#[test]
fn a_held_message_whose_pane_went_back_to_a_shell_at_turn_end_stays_kept_saying_so() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    w.send_to("a", "next");
    let why = "its pane shows a shell prompt (zsh), not the agent: nothing typed";
    w.terminal.pane("a", |p| p.takes = Takes::Refuses(why.into()));
    a.stop("done");
    let held = w.snapshot().session("a")["held"].clone();
    assert_eq!(held["text"], "next");
    assert!(held["delivery"].to_string().contains(why), "{held}");
}

#[test]
fn a_late_prompt_hook_settles_a_message_cue_was_still_watching() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Nothing);
    w.send_to("a", "hello?");
    w.advance(20_000);
    w.session("a").prompt("hello?");
    let mine = w.snapshot().last_yours("a");
    assert_ne!(mine["pending"], true);
    assert!(mine["delivery"].to_string().contains("its prompt hook"), "{mine}");
    w.advance(60_000);
    assert_eq!(w.waiting_work(), 0, "its watch ended at the next look");
}

#[test]
fn a_reply_nobody_vouches_for_goes_to_history_as_typed_and_is_watched() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Nothing);
    let r = w.reply(&id, "go on");
    assert_eq!(detail(&r), "via fake pane a; Cue is watching for it to take it");
    assert_eq!(w.snapshot().history(&id)["outcome"], "replied: “go on” (typed; didn't see it submit)");
    assert_eq!(w.snapshot().last_yours("a")["pending"], true);
}

#[test]
fn a_reply_the_terminal_refuses_fails_and_its_card_stays() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Refuses("tmux pane %3 is gone".into()));
    let r = w.reply(&id, "go on");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "tmux pane %3 is gone".to_string()));
    assert_eq!(w.snapshot().card("a")["id"], id.as_str(), "nothing you wrote is lost: answer it again");
}

#[test]
fn stop_whose_esc_is_refused_fails_and_leaves_the_session_working() {
    let w = World::new();
    w.session("a").prompt("job");
    w.send_to("a", "kept");
    w.terminal.pane("a", |p| p.no_escape = Some("tmux pane is gone".into()));
    let r = w.interrupt("a");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "tmux pane is gone".to_string()));
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working");
    assert_eq!(s.session("a")["held"]["text"], "kept", "the kept message still waits for the turn");
    w.advance(2_000);
    assert!(w.terminal.typed("a").is_empty());
}

#[test]
fn stop_for_a_session_that_ended_is_refused() {
    let w = World::new();
    waiting(&w, "a");
    w.session("a").ended();
    let r = w.interrupt("a");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "that session is gone".to_string()));
    assert!(w.terminal.keys("a").is_empty());
}
