//! Permission asks and questions at their edges (stability plan §4, Asks): every way a card is answered or
//! given up on, hooks that go away without a word, a dialog answered by its number, and bursts of asks.

use crate::terminal::dialog::Dialog;
use crate::testing::{Key, World};
use serde_json::json;

#[test]
fn deny_without_a_reason_is_a_plain_deny() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("rm -rf build");
    w.answer(&ask.id, json!({"behavior": "deny"}));
    assert_eq!(ask.told(), Some(json!({"behavior": "deny"})));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "denied");
}

#[test]
fn a_question_with_two_questions_hands_back_both_answers() {
    let w = World::new();
    let a = w.session("q");
    a.prompt("plan it");
    let ask = a.ask_question("Which database?", &["Postgres", "SQLite"]);
    let answers = json!({"Which database?": "SQLite", "Which port?": "5433"});
    w.answer(&ask.id, json!({"behavior": "allow", "answers": answers}));
    assert_eq!(ask.told().unwrap()["answers"], answers);
    let outcome = w.snapshot().history(&ask.id)["outcome"].as_str().unwrap().to_string();
    assert!(outcome.starts_with("answered: ") && outcome.contains("SQLite") && outcome.contains("5433"), "{outcome}");
}

#[test]
fn dismissing_an_ask_tells_its_hook_to_stand_down_so_the_terminal_asks_instead() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("make deploy");
    w.hub().dismiss(&ask.id);
    w.settle();
    let told = ask.told().expect("the hook was told");
    assert!(told["cancel"].as_str().unwrap().starts_with("dismissed in Cue"), "{told}");
    let s = w.snapshot();
    assert!(s.cards_of("a").is_empty());
    assert_eq!(s.history(&ask.id)["status"], "gone");
}

#[test]
fn only_a_finished_turn_goes_to_the_back_not_an_ask() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    assert_eq!(w.hub().send_to_back(&ask.id), Err("that card isn't waiting".to_string()));
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str(), "it stays where it was");
}

#[test]
fn an_answer_after_the_hook_hung_up_is_refused() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    ask.hang_up(&w);
    assert_eq!(w.answer(&ask.id, json!({"behavior": "allow"})), json!({"ok": false}));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "agent stopped waiting", "not rewritten as allowed");
}

#[test]
fn an_answer_to_a_hook_that_died_unnoticed_is_kept_as_gone_not_as_answered() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    let id = ask.id.clone();
    drop(ask); // its process died; the socket hasn't said so yet
    assert_eq!(w.answer(&id, json!({"behavior": "allow"})), json!({"ok": false}), "nobody got it");
    let s = w.snapshot();
    assert_eq!((s.history(&id)["status"].as_str(), s.history(&id)["outcome"].as_str()), (Some("gone"), Some("agent had already stopped waiting")));
    assert!(s.cards_of("a").is_empty());
    assert_ne!(s.state("a"), "working", "nobody took the answer: not back at work on Cue's word");
}

#[test]
fn answered_in_its_terminal_the_session_is_back_at_work_and_its_notification_goes() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    assert_eq!(w.notes.shown(), [ask.id.clone()], "an ask notifies");
    ask.resolved_elsewhere(&w, "", "deny");
    let s = w.snapshot();
    assert_eq!(s.history(&ask.id)["outcome"], "deny in the terminal", "an empty `by` is the terminal");
    assert_eq!(s.state("a"), "working");
    assert_eq!(w.notes.taken_back(), [ask.id.clone()]);
}

#[test]
fn a_session_with_a_second_ask_open_is_still_deciding_once_the_first_is_answered() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let (one, two) = (a.ask_bash("ls"), a.ask_bash("pwd"));
    w.answer(&one.id, json!({"behavior": "allow"}));
    let s = w.snapshot();
    assert_eq!(s.card("a")["id"], two.id.as_str());
    assert_eq!(s.state("a"), "deciding", "a card still asks you: it can't be at work");
}

#[test]
fn one_of_two_asks_hanging_up_leaves_the_other_waiting() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let (one, two) = (a.ask_bash("ls"), a.ask_bash("pwd"));
    one.hang_up(&w);
    assert_eq!(w.snapshot().card("a")["id"], two.id.as_str());
    w.answer(&two.id, json!({"behavior": "allow"}));
    assert_eq!(two.told().unwrap()["behavior"], "allow");
}

#[test]
fn a_session_that_ends_with_two_asks_open_tells_both_hooks() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let (one, two) = (a.ask_bash("ls"), a.ask_bash("pwd"));
    a.ended();
    for ask in [&one, &two] {
        assert_eq!(ask.told(), Some(json!({"cancel": "session ended"})));
    }
    assert!(w.snapshot().cards().is_empty());
}

#[test]
fn an_ask_from_a_session_cue_never_heard_of_lists_it_as_deciding() {
    let w = World::new();
    let ask = w.session("new").ask_bash("ls");
    let s = w.snapshot();
    assert_eq!(s.state("new"), "deciding");
    assert_eq!(s.card("new")["id"], ask.id.as_str());
}

#[test]
fn a_codex_ask_is_answered_the_same_way() {
    let w = World::new();
    let ask = w.session("c").codex().prompt("port it").ask_bash("cargo test");
    assert_eq!(w.snapshot().card("c")["harness"], "codex");
    w.answer(&ask.id, json!({"behavior": "allow"}));
    assert_eq!(ask.told(), Some(json!({"behavior": "allow"})));
}

#[test]
fn a_hundred_asks_in_one_second_are_a_hundred_cards_each_answered_on_its_own() {
    let w = World::at(1_800_000_000_000);
    let asks: Vec<_> = (0..100).map(|i| {
        let ask = w.session(&format!("s{i}")).ask_bash(&format!("echo {i}"));
        w.advance(10);
        ask
    }).collect();
    let ids: std::collections::HashSet<&str> = asks.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids.len(), 100, "every id its own, though many came in the same millisecond");
    assert_eq!(w.snapshot().cards().len(), 100);
    for (i, ask) in asks.iter().enumerate().rev() {
        let behavior = if i % 2 == 0 { "allow" } else { "deny" };
        assert_eq!(w.answer(&ask.id, json!({"behavior": behavior})), json!({"ok": true}));
    }
    for (i, ask) in asks.iter().enumerate() {
        assert_eq!(ask.told().unwrap()["behavior"], if i % 2 == 0 { "allow" } else { "deny" }, "s{i}'s hook got its own answer");
    }
    assert!(w.snapshot().cards().is_empty());
}

#[test]
fn a_hundred_asks_of_one_session_at_one_instant_get_ids_of_their_own() {
    let w = World::at(1_800_000_000_000);
    let a = w.session("a");
    let asks: Vec<_> = (0..100).map(|i| a.ask_bash(&format!("echo {i}"))).collect();
    let ids: std::collections::HashSet<&str> = asks.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids.len(), 100);
    w.answer(&asks[42].id, json!({"behavior": "allow"}));
    assert!(asks.iter().enumerate().all(|(i, a)| (i == 42) == a.told().is_some()), "only the one answered heard");
}

/// A dialog only its terminal can answer (a command's "/model …"), as Claude Code draws it.
fn model_dialog() -> Dialog {
    Dialog { title: "Switch model?".into(), detail: String::new(), options: vec!["Yes, switch".into(), "No, go back".into()], selected: 0 }
}

#[test]
fn a_dialogs_choice_is_pressed_by_its_number_in_the_pane() {
    let w = World::new();
    let a = w.session("a").in_tmux("%3");
    a.prompt("go");
    w.terminal.pane("a", |p| p.dialog = Some(model_dialog()));
    a.terminal_ask("Claude needs your attention");
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    w.terminal.pane("a", |p| p.dialog_takes_a_key = true);
    assert_eq!(w.hub().answer_terminal(&card, 2), Ok("chose No, go back".to_string()));
    assert_eq!(w.terminal.keys("a"), [Key::Keys(vec!["2".into()])], "no Enter: the dialog went with the key");
    assert_eq!(w.snapshot().history(&card)["outcome"], "chose 2. No, go back");
}

#[test]
fn a_dialog_still_up_after_its_number_gets_enter_too() {
    let w = World::new();
    let a = w.session("a").in_tmux("%3");
    a.prompt("go");
    w.terminal.pane("a", |p| p.dialog = Some(model_dialog()));
    a.terminal_ask("Claude needs your attention");
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    w.hub().answer_terminal(&card, 1).unwrap();
    assert_eq!(w.terminal.keys("a"), [Key::Keys(vec!["1".into()]), Key::Keys(vec!["Enter".into()])]);
}

#[test]
fn a_dialog_that_closed_meanwhile_presses_nothing() {
    let w = World::new();
    let a = w.session("a").in_tmux("%3");
    a.prompt("go");
    w.terminal.pane("a", |p| p.dialog = Some(model_dialog()));
    a.terminal_ask("Claude needs your attention");
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    w.terminal.pane("a", |p| p.dialog = None);
    assert_eq!(w.hub().answer_terminal(&card, 1), Err("that dialog isn't on its screen any more".to_string()));
    assert!(w.terminal.keys("a").is_empty(), "a key there now could answer something else");
    assert_eq!(w.hub().answer_terminal(&card, 3), Err("that isn't one of its choices".to_string()));
}

#[test]
fn a_permission_ask_while_the_terminal_card_is_up_does_not_make_two_cards_for_one_prompt() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    let ask = a.ask_bash("curl example.com");
    a.terminal_ask("Claude needs your permission to use Bash");
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str(), "Cue's own card already shows it");
}

#[test]
fn a_question_turned_down_reaches_the_hook_as_a_deny() {
    let w = World::new();
    let a = w.session("q");
    a.prompt("plan it");
    let ask = a.ask_question("Which database?", &["Postgres", "SQLite"]);
    w.answer(&ask.id, json!({"behavior": "deny", "message": "neither: use files"}));
    assert_eq!(ask.told(), Some(json!({"behavior": "deny", "message": "neither: use files"})));
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "denied: “neither: use files”");
}

#[test]
fn allow_always_with_no_rule_chosen_is_an_allow_always_with_none() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    w.answer(&ask.id, json!({"behavior": "allow_always"}));
    assert_eq!(ask.told(), Some(json!({"behavior": "allow_always"})));
}

#[test]
fn with_decision_notifications_off_an_ask_is_a_card_and_nothing_more() {
    let w = World::new();
    w.hub().set_setting("notify.decisions", json!(false)).unwrap();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str());
    assert!(w.notes.shown().is_empty());
}

#[test]
fn an_ask_whose_hook_hung_up_takes_its_notification_back() {
    let w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    ask.hang_up(&w);
    assert_eq!(w.notes.taken_back(), [ask.id.clone()]);
}
