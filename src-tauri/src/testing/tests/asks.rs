//! Permission asks and questions (stability plan §4, Asks): answered in Cue, answered in the terminal,
//! given up on, and asks that race the session's other events.

use crate::testing::{Ask, World};
use serde_json::json;

fn rm(w: &World, sid: &str) -> Ask {
    let a = w.session(sid);
    a.prompt("clean up");
    a.ask_permission("Bash", json!({"command": "rm -rf build", "description": "clean"}))
}

#[test]
fn an_ask_is_a_card_and_the_session_is_deciding() {
    let w = World::new();
    let ask = rm(&w, "a");
    let s = w.snapshot();
    let card = s.card("a");
    assert_eq!((card["id"].as_str(), card["kind"].as_str(), card["tool_name"].as_str()), (Some(ask.id.as_str()), Some("permission"), Some("Bash")));
    assert_eq!(s.state("a"), "deciding");
    assert!(ask.told().is_none(), "the hook still waits");
}

#[test]
fn allow_reaches_the_hook_and_the_card_goes_to_history() {
    let w = World::new();
    let ask = rm(&w, "a");
    assert_eq!(w.answer(&ask.id, json!({"behavior": "allow"})), json!({"ok": true}));
    assert_eq!(ask.told(), Some(json!({"behavior": "allow"})));
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    assert_eq!((s.history(&ask.id)["status"].as_str(), s.history(&ask.id)["outcome"].as_str()), (Some("answered"), Some("allowed")));
    assert_eq!(s.state("a"), "working", "allowed: back at work");
}

#[test]
fn deny_with_a_reason_reaches_the_hook_with_it() {
    let w = World::new();
    let ask = rm(&w, "a");
    w.answer(&ask.id, json!({"behavior": "deny", "message": "use cargo clean"}));
    assert_eq!(ask.told(), Some(json!({"behavior": "deny", "message": "use cargo clean"})));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "denied: “use cargo clean”");
}

#[test]
fn allow_always_carries_the_suggestion_chosen() {
    let w = World::new();
    let ask = rm(&w, "a");
    let rule = json!({"type": "addRules", "rules": [{"toolName": "Bash"}]});
    w.answer(&ask.id, json!({"behavior": "allow_always", "permission": rule}));
    assert_eq!(ask.told(), Some(json!({"behavior": "allow_always", "permission": rule})));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "allowed (always)");
}

#[test]
fn a_question_answered_in_cue_gives_the_hook_its_answers() {
    let w = World::new();
    let a = w.session("q");
    a.prompt("plan it");
    let ask = a.ask_question("Which database?", &["Postgres", "SQLite"]);
    assert_eq!(w.snapshot().card("q")["kind"], "question");
    w.answer(&ask.id, json!({"behavior": "allow", "answers": {"Which database?": "SQLite"}}));
    assert_eq!(ask.told(), Some(json!({"behavior": "allow", "answers": {"Which database?": "SQLite"}})));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "answered: SQLite");
}

#[test]
fn the_decision_is_noted_in_the_chat() {
    let w = World::new();
    let ask = rm(&w, "a");
    w.answer(&ask.id, json!({"behavior": "allow"}));
    let thread = w.snapshot().thread("a");
    assert_eq!(thread[1].0, "agent", "{thread:?}");
    assert!(thread[1].1.starts_with("Bash: "), "the ask itself, in the chat: {thread:?}");
    assert_eq!(thread.last().unwrap().1, "allowed");
}

#[test]
fn answered_in_its_terminal_clears_the_card() {
    let w = World::new();
    let ask = rm(&w, "a");
    ask.resolved_elsewhere(&w, "terminal", "allow");
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    assert_eq!((s.history(&ask.id)["status"].as_str(), s.history(&ask.id)["outcome"].as_str()), (Some("answered_elsewhere"), Some("allow in the terminal")));
    assert_eq!(w.answer(&ask.id, json!({"behavior": "deny"})), json!({"ok": false}), "too late to answer it in Cue");
}

#[test]
fn an_ask_whose_hook_hung_up_is_gone() {
    let w = World::new();
    let ask = rm(&w, "a");
    ask.hang_up(&w);
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    assert_eq!((s.history(&ask.id)["status"].as_str(), s.history(&ask.id)["outcome"].as_str()), (Some("gone"), Some("agent stopped waiting")));
    assert_ne!(s.state("a"), "working", "nobody answered it: not back at work on Cue's word");
}

#[test]
fn answering_twice_answers_once() {
    let w = World::new();
    let ask = rm(&w, "a");
    assert_eq!(w.answer(&ask.id, json!({"behavior": "allow"})), json!({"ok": true}));
    assert_eq!(w.answer(&ask.id, json!({"behavior": "deny"})), json!({"ok": false}));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "allowed");
}

#[test]
fn an_answer_for_no_such_card_is_refused() {
    let w = World::new();
    assert_eq!(w.answer("n0", json!({"behavior": "allow"})), json!({"ok": false}));
}

#[test]
fn the_same_call_asked_twice_is_two_cards_each_answered_on_its_own() {
    let w = World::new();
    let first = rm(&w, "a");
    let second = w.session("a").ask_permission("Bash", json!({"command": "rm -rf build", "description": "clean"}));
    assert_ne!(first.id, second.id);
    assert_eq!(w.snapshot().cards_of("a").len(), 2);
    w.answer(&second.id, json!({"behavior": "deny"}));
    assert!(first.told().is_none(), "the other one still waits");
    assert_eq!(second.told().unwrap()["behavior"], "deny");
    assert_eq!(w.snapshot().cards_of("a").len(), 1);
}

#[test]
fn a_session_that_ends_with_an_ask_open_tells_its_hook_to_stand_down() {
    let w = World::new();
    let ask = rm(&w, "a");
    w.session("a").ended();
    assert_eq!(ask.told(), Some(json!({"cancel": "session ended"})));
    let s = w.snapshot();
    assert_eq!(s.history(&ask.id)["status"], "gone");
    assert!(s.find("a").is_none());
}

#[test]
fn an_ask_after_its_session_ended_is_still_answered() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").ended();
    let ask = a.ask_permission("Bash", json!({"command": "ls"}));
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str(), "a hook still waiting gets its card");
    w.answer(&ask.id, json!({"behavior": "allow"}));
    assert_eq!(ask.told().unwrap()["behavior"], "allow");
}

#[test]
fn an_ask_takes_the_place_of_its_sessions_waiting_card() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go").stop("Done, shall I deploy?");
    assert_eq!(w.snapshot().card("a")["kind"], "waiting");
    let ask = a.ask_permission("Bash", json!({"command": "deploy"}));
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str());
}

#[test]
fn a_turn_that_ends_with_an_ask_open_adds_no_waiting_card() {
    let w = World::new();
    let ask = rm(&w, "a");
    // A background agent of it asks; its own turn ends meanwhile.
    w.session("a").stop("Waiting on the cleanup.");
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str(), "only the ask, not a second card");
}

#[test]
fn a_prompt_in_its_terminal_cue_cant_answer_is_a_card_until_it_moves_on() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("fetch it");
    a.terminal_ask("Claude needs your permission to use network access");
    assert_eq!(w.snapshot().card("a")["kind"], "terminal");
    a.stop("Fetched.");
    assert_eq!(w.snapshot().card("a")["kind"], "waiting", "it moved on: the terminal card went");
}

#[test]
fn a_decision_for_one_session_leaves_anothers_ask_waiting() {
    let w = World::new();
    let (one, two) = (rm(&w, "a"), rm(&w, "b"));
    w.answer(&one.id, json!({"behavior": "allow"}));
    assert!(two.told().is_none());
    let s = w.snapshot();
    assert_eq!(s.state("b"), "deciding");
    assert_eq!(s.cards().len(), 1);
}
