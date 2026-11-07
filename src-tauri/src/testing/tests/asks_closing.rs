//! Two asks open at once in one session (two tool calls asked together): the session is back at work once the
//! last of them closes, however it closes, when one was answered meanwhile.

use crate::testing::World;
use serde_json::json;

/// One ask answered while the other still waits, then the other gone (its agent stopped waiting): back at work.
#[test]
fn an_answer_given_while_another_ask_waited_counts_once_that_one_is_gone() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("tidy up");
    let first = a.ask_permission("Bash", json!({"command": "rm -rf build"}));
    let second = a.ask_permission("Bash", json!({"command": "rm -rf dist"}));
    w.answer(&first.id, json!({"behavior": "allow"}));
    let meanwhile = w.snapshot().state("a");
    second.hang_up(&w);
    assert_ne!(meanwhile, "working", "still asking meanwhile");
    assert_eq!(w.snapshot().state("a"), "working", "the answer given counts once no ask is left");
}

/// Both gone, nothing answered: not called working on the strength of no answer at all.
#[test]
fn asks_that_all_went_unanswered_dont_mark_it_working() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("tidy up");
    let first = a.ask_permission("Bash", json!({"command": "rm -rf build"}));
    let second = a.ask_permission("Bash", json!({"command": "rm -rf dist"}));
    let before = w.snapshot().state("a");
    first.hang_up(&w);
    second.hang_up(&w);
    assert_eq!(w.snapshot().state("a"), before);
}
