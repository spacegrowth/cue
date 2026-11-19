//! A Desk session's question about clearing the way for a landing goes to its Desk lead first (#349).

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use crate::desk::parse::{lines, Line};
use crate::hub::{NewAsk, Reply};
use crate::testing::World;
use serde_json::{json, Value};

fn ask(w: &World, sid: &str, question: &str, labels: &[&str]) -> (String, tokio::sync::oneshot::Receiver<Reply>) {
    let options: Vec<Value> = labels.iter().map(|l| json!({ "label": l, "description": "" })).collect();
    let input = json!({ "questions": [{ "question": question, "header": "Landing", "options": options, "multiSelect": false }] });
    let r = w.hub().add_ask(w.hub().session_origin(sid).unwrap(), NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    w.settle();
    r
}

fn told(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

#[test]
fn answer_and_pass_lines_are_read() {
    assert_eq!(
        lines("ANSWER: #349 | 2\n- **PASS:** #5 | why: only the user knows\nPASS: #6\nANSWER: #7\nPASS: 8 | no hash"),
        vec![
            Line::Answer { id: "349".into(), text: "2".into() },
            Line::Pass { id: "5".into(), why: "only the user knows".into() },
            Line::Pass { id: "6".into(), why: "".into() },
        ]
    );
}

#[test]
fn a_landing_clean_up_question_goes_to_the_lead_who_answers_it_and_you_see_it_only_if_it_cant() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let q = "Leftover edits in the main checkout block landing. What should I do with them?";
    let choices = ["Drop them", "Commit both", "Leave them"];

    // Asked: the lead has it, with its choices and how to answer; you see no card, and the item isn't waiting on you.
    let (_, mut rx) = ask(&w, &h, q, &choices);
    assert!(w.snapshot().cards_of(&h).is_empty(), "the lead has it");
    assert_ne!(item(&w, &project, "Header jumps")["state"], "needs_you");
    let to_lead = told(&w, &lead).into_iter().find(|t| t.contains(q)).expect("the lead is told");
    assert!(to_lead.contains("2. Commit both") && to_lead.contains(&format!("ANSWER: #{id} |")) && to_lead.contains(&format!("PASS: #{id} |")), "{to_lead}");

    // The lead answers with a choice's number: the session gets that choice, as your answer would be.
    ends(&w, &lead, &format!("It's the other session's work.\nANSWER: #{id} | 2"));
    match rx.try_recv() {
        Ok(Reply::Decision(d)) => assert_eq!(d.answers, Some(json!({ q: "Commit both" }))),
        other => panic!("no answer: {:?} {:?}", other.is_ok(), told(&w, &lead)),
    }
    assert!(w.snapshot().cards_of(&h).is_empty(), "answered");
    // What the lead did on its own (#360): a step on the item, shown in its chat and the Dispatcher's panel.
    let did = desk(&w, &project)["decisions"].as_array().unwrap().iter().find(|d| d["act"] == "answer").cloned().expect("the answer noted");
    assert_eq!((did["item_id"].as_str(), did["text"].as_str(), did["sid"].as_str()), (Some(id.as_str()), Some(format!("Answered #{id}'s session: 2").as_str()), Some(lead.as_str())));

    // Asked again, the lead passes it on: now it's yours, on the item too.
    let (card, _rx) = ask(&w, &h, "Stray copy still holds the branch, landing fails. Remove the copy?", &["Remove it", "Keep it"]);
    ends(&w, &lead, &format!("PASS: #{id} | why: removing a copy is the user's call"));
    assert_eq!(w.snapshot().cards_of(&h).len(), 1);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["ask"]["item_id"].as_str()), (Some("needs_you"), Some(card.as_str())), "{it}");
    w.answer(&card, json!({ "behavior": "allow", "answers": { "Stray copy still holds the branch, landing fails. Remove the copy?": "Keep it" } }));
    w.settle();

    // Asked, and the lead says nothing: yours once its time is up.
    let (_, _rx) = ask(&w, &h, "Untracked files left over from another session block the commit. Commit them too?", &["Yes", "No"]);
    assert!(w.snapshot().cards_of(&h).is_empty());
    w.advance(10 * 60 * 1000 + 1000);
    assert_eq!(w.snapshot().cards_of(&h).len(), 1, "time's up: yours");
}

#[test]
fn a_question_that_isnt_about_clearing_the_way_is_yours_at_once() {
    let w = World::new();
    let (_, project, _, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ask(&w, &h, "Keep the header sticky?", &["Yes", "No"]);
    assert_eq!(w.snapshot().cards_of(&h).len(), 1);
    assert!(!told(&w, &lead).iter().any(|t| t.contains("sticky")));
}
