//! A pick closes its item (#281, #543): you pick an option on any item, and Cue marks it picked and done, then files
//! and routes what its holder said follows that option (IF PICKED: lines), with no lead step. An item with a canvas
//! that named nothing gets one "Build: <topic> (option X)" item for the Desk lead to route; any other just closes.
//! A pick again makes none.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use super::team::lead;
use crate::hub::{NewAsk, Reply};
use crate::model::Decision;
use crate::testing::World;
use serde_json::{json, Value};
use tokio::sync::oneshot;

const ASKED: &str = "Pick one for working items: A, B or C?";

/// `h`'s question, which option to build: its id and where its answer arrives.
fn asks_which(w: &World, h: &str) -> (String, oneshot::Receiver<Reply>) {
    asks(w, h, json!({ "questions": [{ "question": ASKED, "header": "Pick", "options": [{ "label": "A" }, { "label": "B: compact list" }, { "label": "C" }], "multiSelect": false }] }))
}

fn asks(w: &World, h: &str, input: Value) -> (String, oneshot::Receiver<Reply>) {
    w.hub().add_ask(w.hub().session_origin(h).unwrap(), NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() })
}

/// The answer `rx` got to the question.
fn answer(rx: &mut oneshot::Receiver<Reply>) -> String {
    match rx.try_recv() {
        Ok(Reply::Decision(d)) => d.answers.unwrap()[ASKED].as_str().unwrap().to_string(),
        _ => panic!("no answer"),
    }
}

fn answered(text: &str) -> Decision {
    Decision { behavior: "allow".into(), message: None, answers: Some(json!({ ASKED: text })), permission: None }
}

/// A mocks item routed to `h`, its canvas up: its id.
fn mocks(w: &World, h: &str, project: &str) -> String {
    let l = new_lead(w, project);
    ends(w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(w, project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(w, h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas"));
    m
}

fn builds(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["items"].as_array().unwrap().iter().filter(|i| i["builds"].is_string()).cloned().collect()
}

#[test]
fn a_pick_on_mocks_makes_one_build_item_linked_both_ways_for_the_lead_to_route() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas\nPick one for working items: A, B or C."));
    assert_eq!(w.hub().desk_item_act(&m, "pick", "  ").unwrap_err(), "Say which option you picked");
    let told_lead = sent_to(&w, &l).len();

    let picked = w.hub().desk_item_act(&m, "pick", "option b").unwrap();
    assert_eq!((picked.state.as_str(), picked.picked.as_str()), ("done", "B"));
    let d = desk(&w, &project);
    let b = item(&d, "Build: working items (option B)").clone();
    let n = b["id"].as_str().unwrap().to_string();
    assert_eq!(picked.built_as, n, "the mocks item names its build item");
    assert_eq!(b["builds"], m.as_str(), "and the build item its mocks item");
    assert_eq!((b["state"].as_str(), b["kind"].as_str(), b["from"].as_str()), (Some("new"), Some("feature"), Some("you")));
    assert!(b["holder"].is_null());
    assert_eq!(b["why"], format!("You picked option B on #{m}: https://example.com/canvas"), "{b}");
    // Its steps say so, on both.
    let steps = d["decisions"].as_array().unwrap();
    assert!(steps.iter().any(|s| s["item_id"] == m.as_str() && s["text"] == "Picked option B" && s["why"] == format!("#{n} builds it")), "{d}");
    assert!(steps.iter().any(|s| s["item_id"] == n.as_str() && s["text"] == format!("Follows option B, picked on #{m}")), "{d}");
    // Its holder, still at work on the mocks, is told they're done.
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("option B was picked, so it's done") && t.contains(&format!("#{n} goes to the Desk lead to route")) && t.contains("Mark \"Picked: B\" where it shows its options (https://example.com/canvas)")));
    // The lead is told a moment later, to route it now, with the mocks' link.
    w.advance(1000);
    let sent = sent_to(&w, &l);
    assert_eq!(sent.len(), told_lead + 1, "{sent:?}");
    let told = sent.last().unwrap();
    assert!(told.contains(&format!("New item #{n} (feature): Build: working items (option B)")) && told.contains("https://example.com/canvas") && told.contains("Route it now"), "{told}");

    // Picked again, the same option or another: no second build item, and which it's being built as.
    let count = desk(&w, &project)["items"].as_array().unwrap().len();
    for again in ["B", "C"] {
        let e = w.hub().desk_item_act(&m, "pick", again).unwrap_err();
        assert_eq!(e, format!("Option B is picked already: it's being built as #{n}"));
    }
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), count);
    w.advance(2000);
    assert_eq!(sent_to(&w, &l).len(), told_lead + 1, "the lead isn't told twice");

    // Handed to a session, the build item says what was picked and where the mocks are.
    ends(&w, &l, &format!("ROUTE: #{n} -> cue-teams | why: it made the mocks"));
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{n} (feature): Build: working items (option B)")) && t.contains(&format!("You picked option B on #{m}: https://example.com/canvas"))));
}

#[test]
fn a_mocks_title_gives_the_build_its_topic() {
    let w = World::new();
    let (h, project) = holder(&w);
    for (title, built) in [("Mocks: the item page", "Build: the item page (option A)"), ("Search results (mocks)", "Build: Search results (option A)"), ("Header", "Build: Header (option A)")] {
        let it = w.hub().desk_add(&project, "", title, "", None).unwrap();
        ends(&w, &h, &format!("DESIGNS: #{} https://example.com/c", it.id));
        let done = w.hub().desk_item_act(&it.id, "pick", "a").unwrap();
        let b = item(&desk(&w, &project), built).clone();
        assert_eq!((done.built_as.as_str(), b["builds"].as_str(), b["kind"].as_str()), (b["id"].as_str().unwrap(), Some(it.id.as_str()), Some("feature")));
    }
}

#[test]
fn a_picked_mocks_item_isnt_undone() {
    let w = World::new();
    let (h, project) = holder(&w);
    let it = w.hub().desk_add(&project, "task", "Mocks: header", "", None).unwrap();
    ends(&w, &h, &format!("DESIGNS: #{} https://example.com/c", it.id));
    let done = w.hub().desk_item_act(&it.id, "pick", "C").unwrap();
    assert_eq!(w.hub().desk_item_act(&it.id, "reopen", "").unwrap_err(), format!("Option C is picked: it's being built as #{}", done.built_as));
    assert_eq!(item(&desk(&w, &project), "Mocks: header")["state"], "done");
}

#[test]
fn answering_the_holders_question_with_an_option_is_the_pick() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    // An option its question doesn't name: only the answer.
    let (q, mut rx) = asks_which(&w, &h);
    assert!(w.hub().respond(&q, answered("D")));
    assert_eq!(answer(&mut rx), "D");
    assert!(builds(&w, &project).is_empty());
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["picked"], Value::Null);

    let (q, mut rx) = asks_which(&w, &h);
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["state"], "needs_you");
    assert!(w.hub().respond(&q, answered("B: compact list")));
    assert_eq!(answer(&mut rx), "B: compact list", "the holder gets your answer as given");
    let d = desk(&w, &project);
    let it = item(&d, "Mocks: working items");
    let b = builds(&w, &project);
    assert_eq!(b.len(), 1);
    assert_eq!((it["state"].as_str(), it["picked"].as_str(), it["built_as"].as_str()), (Some("done"), Some("B"), b[0]["id"].as_str()));
    assert_eq!((b[0]["title"].as_str(), b[0]["builds"].as_str()), (Some("Build: working items (option B)"), Some(m.as_str())));
    // Then told the mocks are done and what builds B, so it doesn't build it there: kept for it till its turn ends.
    w.settle();
    ends(&w, &h, "Noted.");
    w.settle();
    let n = b[0]["id"].as_str().unwrap();
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("option B was picked, so it's done. #{n} builds it. What follows: #{n} goes to the Desk lead to route.")));
}

#[test]
fn an_answer_to_a_question_that_isnt_the_pick_picks_nothing() {
    let w = World::new();
    let (h, project) = holder(&w);
    mocks(&w, &h, &project);
    ends(&w, &h, "Pick one: A, B or C.");
    let q = "Keep the old tab? A) yes B) no";
    let (id, mut rx) = asks(&w, &h, json!({ "questions": [{ "question": q, "header": "Tab", "options": [{ "label": "A) yes" }, { "label": "B) no" }], "multiSelect": false }] }));
    assert!(w.hub().respond(&id, Decision { behavior: "allow".into(), message: None, answers: Some(json!({ q: "A) yes" })), permission: None }));
    assert!(matches!(rx.try_recv(), Ok(Reply::Decision(_))));
    assert!(builds(&w, &project).is_empty());
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["picked"], Value::Null);
}

#[test]
fn a_pick_doesnt_answer_an_ask_of_several_questions() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let two = json!([{ "label": "A" }, { "label": "B" }]);
    let (ask, mut rx) = asks(&w, &h, json!({ "questions": [{ "question": ASKED, "header": "Pick", "options": two, "multiSelect": false }, { "question": "Sticky header?", "header": "Header", "options": two, "multiSelect": false }] }));
    let done = w.hub().desk_item_act(&m, "pick", "B").unwrap();
    assert!(rx.try_recv().is_err(), "left for you to answer");
    // You answer it; once its turn ends it's told the mocks are done.
    assert!(w.hub().respond(&ask, Decision { behavior: "allow".into(), message: None, answers: Some(json!({ ASKED: "B", "Sticky header?": "A" })), permission: None }));
    assert_eq!(builds(&w, &project).len(), 1, "picked once only");
    w.settle();
    ends(&w, &h, "Noted.");
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("option B was picked, so it's done. #{0} builds it. What follows: #{0} goes to the Desk lead to route.", done.built_as))));
}

#[test]
fn a_pick_while_the_holder_waits_on_its_question_answers_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let (_, mut rx) = asks_which(&w, &h);
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["state"], "needs_you");
    let told = sent_to(&w, &h).len();
    let done = w.hub().desk_item_act(&m, "pick", "B").unwrap();
    assert_eq!(answer(&mut rx), "B: compact list", "answered with the option's own choice");
    assert_eq!((done.state.as_str(), done.picked.as_str()), ("done", "B"));
    assert_eq!(builds(&w, &project).len(), 1, "one build item, not one more for the answer");
    w.settle();
    ends(&w, &h, "Noted.");
    w.settle();
    let sent = sent_to(&w, &h);
    assert_eq!(sent.len(), told + 1, "{sent:?}");
    assert!(sent.last().unwrap().contains(&format!("option B was picked, so it's done. #{0} builds it. What follows: #{0} goes to the Desk lead to route.", done.built_as)), "then told it's done: {sent:?}");
}

#[test]
fn a_pick_tells_a_holder_that_isnt_at_work() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    ends(&w, &h, &format!("DONE: #{m} | three on the canvas"));
    w.hub().desk_item_act(&m, "pick", "A").unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("option A was picked")));
}

#[test]
fn dropping_the_build_item_lets_the_mocks_be_picked_again() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let first = w.hub().desk_item_act(&m, "pick", "B").unwrap();
    w.hub().desk_item_act(&first.built_as, "drop", "").unwrap();
    let d = desk(&w, &project);
    let it = item(&d, "Mocks: working items");
    assert_eq!((it["state"].as_str(), it["picked"].as_str(), it["built_as"].as_str()), (Some("done"), None, None), "{it}");
    assert!(d["decisions"].as_array().unwrap().iter().any(|s| s["item_id"] == m.as_str() && s["text"] == format!("Dropped #{}, the build of option B", first.built_as)), "{d}");
    // Picked again: a new build item.
    let again = w.hub().desk_item_act(&m, "pick", "C").unwrap();
    assert_eq!(again.picked, "C");
    let b = builds(&w, &project);
    assert_eq!((b.len(), b[0]["id"].as_str(), b[0]["title"].as_str()), (1, Some(again.built_as.as_str()), Some("Build: working items (option C)")));
    // Or undone, once its build is gone.
    w.hub().desk_item_act(&again.built_as, "drop", "").unwrap();
    assert_eq!(w.hub().desk_item_act(&m, "reopen", "").unwrap().state, "to_check");
}

#[test]
fn a_pick_files_and_routes_what_its_holder_said_follows_with_no_lead_step() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: task | Which search box to keep | done when: you picked one\nROUTE: #new -> cue-teams | why: it knows search");
    let id = item(&desk(&w, &project), "Which search box to keep")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Two ways, no canvas: A keeps the top box, B the side one. Pick one: A or B.\n\
        IF PICKED: #{id} | B | feature | Move search to the side | you'll see: one search box, at the side | done when: the top box is gone | by: me\n\
        IF PICKED: #{id} | any | task | Tell the docs which box stays | done when: the docs name it | by: new\n\
        IF PICKED: #{id} | A | bug | Fix the top box's focus | by: me"));
    let it = item(&desk(&w, &project), "Which search box to keep").clone();
    assert_eq!(it["next"].as_array().map(Vec::len), Some(3), "{it}");
    let told_lead = sent_to(&w, &l).len();

    let done = w.hub().desk_item_act(&id, "pick", "B").unwrap();
    assert_eq!((done.state.as_str(), done.picked.as_str()), ("done", "B"));
    w.settle();
    let d = desk(&w, &project);
    let side = item(&d, "Move search to the side").clone();
    let docs = item(&d, "Tell the docs which box stays").clone();
    assert!(d["items"].as_array().unwrap().iter().all(|i| i["title"] != "Fix the top box's focus"), "option A's isn't filed: {d}");
    assert_eq!(done.built_as, side["id"].as_str().unwrap());
    for b in [&side, &docs] {
        assert_eq!((b["builds"].as_str(), b["from"].as_str()), (Some(id.as_str()), Some("you")), "{b}");
        assert_ne!(b["state"], "new", "routed at once: {b}");
    }
    assert_eq!((side["kind"].as_str(), side["see"].as_str(), side["done_when"].as_str()), (Some("feature"), Some("one search box, at the side"), Some("the top box is gone")));
    // "me": its holder, the one that asked; "new": a new session.
    assert_eq!(side["holder"]["name"], "cue-teams", "{side}");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{} (feature): Move search to the side", side["id"].as_str().unwrap()))));
    assert!(docs["holder"]["name"].as_str().is_some_and(|n| n != "cue-teams"), "{docs}");
    // Its holder is told what follows; the lead has nothing to route.
    let n = side["id"].as_str().unwrap();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("option B was picked, so it's done. #") && t.contains(&format!("#{n} to cue-teams")) && !t.contains("Mark \"Picked")), "{:?}", sent_to(&w, &h));
    w.advance(2000);
    assert_eq!(sent_to(&w, &l).len(), told_lead, "no lead step: {:?}", &sent_to(&w, &l)[told_lead..]);
}

#[test]
fn a_pick_on_an_item_that_named_nothing_to_follow_just_closes_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let it = w.hub().desk_add(&project, "task", "Keep the old tab or not", "", None).unwrap();
    let count = desk(&w, &project)["items"].as_array().unwrap().len();
    let done = w.hub().desk_item_act(&it.id, "pick", "option a").unwrap();
    assert_eq!((done.state.as_str(), done.picked.as_str(), done.built_as.as_str()), ("done", "A", ""));
    let d = desk(&w, &project);
    assert_eq!(d["items"].as_array().unwrap().len(), count, "nothing filed: {d}");
    assert!(d["decisions"].as_array().unwrap().iter().any(|s| s["item_id"] == it.id.as_str() && s["text"] == "Picked option A" && s["why"] == "nothing follows it"), "{d}");
    assert_eq!(w.hub().desk_item_act(&it.id, "pick", "B").unwrap_err(), "Option A is picked already");
    let _ = h;
}

#[test]
fn if_picked_lines_are_read() {
    use crate::desk::parse::{lines, Line};
    let got = lines("IF PICKED: #12 | option b | feature | Move search | you'll see: one box | done when: top box gone | by: cue-teams\nIF PICKED: #12 | any | task | Note it");
    let next: Vec<_> = got.into_iter().filter_map(|l| if let Line::IfPicked { id, next } = l { Some((id, next)) } else { None }).collect();
    assert_eq!(next.len(), 2, "{next:?}");
    assert_eq!((next[0].0.as_str(), next[0].1.option.as_str(), next[0].1.kind.as_str(), next[0].1.title.as_str()), ("12", "B", "feature", "Move search"));
    assert_eq!((next[0].1.see.as_str(), next[0].1.done_when.as_str(), next[0].1.by.as_str()), ("one box", "top box gone", "cue-teams"));
    assert_eq!((next[1].1.option.as_str(), next[1].1.title.as_str(), next[1].1.by.as_str()), ("any", "Note it", "me"));
}

/// #619: options can be numbers (mocks 1 to 10). Before, a numbered IF PICKED: line was dropped, so a redone mocks item
/// kept the first round's A, B, C and their plans.
#[test]
fn numbered_if_picked_lines_are_read() {
    use crate::desk::parse::{lines, Line};
    let got = lines("IF PICKED: #607 | 9 | feature | A calm list | you'll see: dots | by: new\nIF PICKED: #607 | option 10 | feature | Item on the left\nIF PICKED: #607 | 100 | task | Not an option");
    let opts: Vec<String> = got.into_iter().filter_map(|l| if let Line::IfPicked { next, .. } = l { Some(next.option) } else { None }).collect();
    assert_eq!(opts, ["9", "10"]);
}

#[test]
fn an_answer_or_a_pick_line_can_name_numbered_options() {
    use crate::desk::hub::turn::act::{letter_of, pick_letters};
    for (a, want) in [("9", Some("9")), ("Option 10", Some("10")), ("4: a calm list", Some("4")), ("10 things", None), ("100", None), ("B", Some("B")), ("A new tab", None)] {
        assert_eq!(letter_of(a).as_deref(), want, "{a}");
    }
    assert_eq!(pick_letters("Pick one: 4, 6, 8 or 9."), ["4", "6", "8", "9"]);
    assert_eq!(pick_letters("Which option, 1 to 10, should we pick?"), (1..=10).map(|n| n.to_string()).collect::<Vec<_>>());
    assert_eq!(pick_letters("Pick one of A\u{2013}D."), ["A", "B", "C", "D"]);
    assert_eq!(pick_letters("Pick one for working items: A, B or C."), ["A", "B", "C"]);
}

#[test]
fn a_redone_mocks_item_picks_from_its_new_numbered_options_not_the_old_letters() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    ends(&w, &h, &format!("Pick one: A, B or C.\n\
        IF PICKED: #{m} | A | feature | Old band (option A)\n\
        IF PICKED: #{m} | B | feature | Old lanes (option B)\n\
        IF PICKED: #{m} | C | feature | Old track (option C)"));
    let letters = |w: &World| item(&desk(w, &project), "Mocks: working items")["next"].as_array().map(|n| n.iter().map(|x| x["option"].as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    assert_eq!(letters(&w), ["A", "B", "C"]);
    // Sent back: the rejected round's plans go, and its holder is told to give them again.
    w.hub().desk_item_act(&m, "send_back", "Not these: start again").unwrap();
    assert!(letters(&w).is_empty());
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("isn't done") && t.contains("give them again")), "{:?}", sent_to(&w, &h));
    ends(&w, &h, &format!("Four new mocks. Pick one: 4, 6, 8 or 9.\n\
        IF PICKED: #{m} | 9 | feature | A calm list with progress under each row | you'll see: dots on every row | by: new\n\
        IF PICKED: #{m} | 6 | feature | Progress is the page | by: new\n\
        IF PICKED: #{m} | 8 | feature | A List / Progress switch | by: new\n\
        IF PICKED: #{m} | 4 | feature | The picked row opens its journey | by: new"));
    assert_eq!(letters(&w), ["9", "6", "8", "4"]);
    let done = w.hub().desk_item_act(&m, "pick", "8").unwrap();
    assert_eq!(done.picked, "8");
    w.settle();
    let b = builds(&w, &project);
    assert_eq!(b.iter().map(|x| x["title"].as_str().unwrap()).collect::<Vec<_>>(), ["A List / Progress switch"], "{b:?}");
}

#[test]
fn a_build_item_the_lead_files_for_your_pick_links_both_ways() {
    // #606: you picked in the lead's chat; the lead closes the mocks item and files what builds it itself, naming
    // where it came from. Both items link to each other, as a Pick does.
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let l = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &l, &format!("CLOSE: #{m} | why: B was picked in the chat\nITEM: feature | Build: working items (option B) | you'll see: the compact list | done when: it's in the app | from: #{m} | picked: option B"));
    let b = item(&desk(&w, &project), "Build: working items (option B)").clone();
    let n = b["id"].as_str().unwrap().to_string();
    assert_eq!((b["builds"].as_str(), b["from_pick"].as_str()), (Some(m.as_str()), Some("B")), "{b}");
    assert_eq!(b["done_when"], "it's in the app", "the from: and picked: parts aren't its title or check: {b}");
    let mk = item(&desk(&w, &project), "Mocks: working items").clone();
    assert_eq!((mk["picked"].as_str(), mk["built_as"].as_str()), (Some("B"), Some(n.as_str())), "{mk}");
    // #917: its brief says where it came from, though the lead gave no why, so its chat opens with that line.
    w.advance(1000);
    ends(&w, &l, &format!("ROUTE: #{n} -> {h}"));
    let brief = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Desk item #{n} "))).expect("its brief");
    assert!(brief.contains(&format!("\nYou picked option B on #{m}.\n")), "{brief}");

    // A follow-on that isn't a pick: linked, no option. One naming an item that isn't there is refused, in words.
    w.advance(1000);
    ends(&w, &l, &format!("ITEM: task | Write the release note | done when: it's in the notes | from: #{n}\nITEM: task | Something else | from: #zz9"));
    let f = item(&desk(&w, &project), "Write the release note").clone();
    assert_eq!((f["builds"].as_str(), f["from_pick"].as_str()), (Some(n.as_str()), None), "{f}");
    let b = item(&desk(&w, &project), "Build: working items (option B)").clone();
    assert_eq!((b["built_as"].as_str(), b["picked"].as_str()), (f["id"].as_str(), None), "{b}");
    assert!(sent_to(&w, &l).iter().any(|t| t.contains("from: #zz9") && t.contains("there's no #zz9")), "{:?}", sent_to(&w, &l));
}

/// #667: Its mocks session `h` at work on mocks item `m` again (a turn of yours taken, not ended).
fn at_work(w: &World, h: &str) {
    w.hosted_says(h, crate::hosted::HostedEvent::Taken { text: "Tidy the canvas".into() });
    w.settle();
}

#[test]
fn a_pick_while_its_holder_is_still_at_work_closes_it_and_its_done_later_doesnt_reopen_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas\nPick one for working items: B1, B2 or B3."));
    ends(&w, &l, "Waiting on the pick.");
    at_work(&w, &h);
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["state"], "working", "its holder is still at it");

    let picked = w.hub().desk_item_act(&m, "pick", "B2").unwrap();
    assert_eq!((picked.state.as_str(), picked.picked.as_str()), ("done", "B2"));
    let n = picked.built_as.clone();
    assert!(!n.is_empty());
    // The lead hears it with the line its chat shows: "You picked B2 on #m; #n builds it".
    w.advance(1000);
    let told = sent_to(&w, &l).into_iter().filter(|t| t.contains("(Desk changes ")).collect::<Vec<_>>();
    assert!(told.iter().any(|t| t.contains(&format!("picked option B2 on #{m} (Mocks: working items)")) && t.contains(&format!("#{m} is done; #{n} builds it."))), "{told:?}");

    // Its holder finishes the turn it was on, with DONE: the item stays done, never back at your check.
    ends(&w, &h, &format!("Canvas tidied.\nDONE: #{m}"));
    w.advance(5000);
    w.tick(false);
    // Its holder is told (once its turn ends), in the words its chat shows as one line.
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("option B2 was picked, so it's done. #{n} builds it."))), "{:?}", sent_to(&w, &h));
    let it = crate::db::desk_get_item(&m).unwrap();
    assert_eq!((it.state.as_str(), it.picked.as_str(), it.built_as.as_str()), ("done", "B2", n.as_str()));

    // The lead's DONE or CLOSE on it now: nothing refused, nothing to say.
    let before = sent_to(&w, &l).len();
    ends(&w, &l, &format!("DONE: #{m}\nCLOSE: #{m} | why: picked"));
    w.advance(1000);
    let after = sent_to(&w, &l);
    assert!(after[before..].iter().all(|t| !t.contains("not closed")), "{:?}", &after[before..]);
}

#[test]
fn a_pick_in_the_leads_chat_closes_the_item_while_its_holder_still_has_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas\nPick one for working items: B1, B2 or B3."));
    at_work(&w, &h);
    let before = sent_to(&w, &l).len();

    // You tell the lead "B2"; it files what builds it and closes the mocks, as #652's lead did.
    ends(&w, &l, &format!("ITEM: feature | Build: working items B2 | you'll see: the B2 list | done when: B2 is in the app | from: #{m} | picked: B2\nDONE: #{m}\nCLOSE: #{m} | why: B2 was picked"));
    let d = desk(&w, &project);
    let n = item(&d, "Build: working items B2")["id"].as_str().unwrap().to_string();
    let it = item(&d, "Mocks: working items");
    assert_eq!((it["state"].as_str(), it["picked"].as_str(), it["built_as"].as_str()), (Some("done"), Some("B2"), Some(n.as_str())), "{it}");
    w.advance(1000);
    let after = sent_to(&w, &l);
    assert!(after[before..].iter().all(|t| !t.contains("not closed")), "never refused: {:?}", &after[before..]);

    // Its holder's DONE, said after, doesn't take it to your check.
    ends(&w, &h, &format!("Canvas tidied.\nDONE: #{m}"));
    w.advance(5000);
    w.tick(false);
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("option B2 was picked, so it's done. #{n} builds it."))), "its holder is told");
    assert_eq!(crate::db::desk_get_item(&m).unwrap().state, "done");
}

#[test]
fn the_leads_done_on_mocks_its_holder_still_has_closes_it_once_its_back() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Mocks: working items | done when: three ways on the canvas; nothing in the repo changes\nROUTE: #new -> cue-teams | why: it knows the list");
    let m = item(&desk(&w, &project), "Mocks: working items")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{m} https://example.com/canvas"));
    at_work(&w, &h);
    let before = sent_to(&w, &l).len();
    ends(&w, &l, &format!("DONE: #{m}"));
    w.advance(1000);
    let after = sent_to(&w, &l);
    assert!(after[before..].iter().all(|t| !t.contains("not closed")), "not refused: {:?}", &after[before..]);
    assert!(crate::db::desk_get_item(&m).unwrap().close_on_check);
    ends(&w, &h, &format!("Canvas tidied.\nDONE: #{m}"));
    for _ in 0..5 {
        w.advance(2000);
        w.tick(false);
    }
    assert_eq!(crate::db::desk_get_item(&m).unwrap().state, "done", "closed once its DONE brought it to your check");
}

#[test]
fn picked_lines_are_read() {
    use crate::desk::parse::{lines, Line};
    let got = lines("PICKED: #12 | option b\nPICKED: #12 | C | add: a search box | at the top\n**PICKED:** #ab3 | M2 | with: no border\nPICKED: #12 | maybe B or C\nPICKED: <#id> | B");
    let picks: Vec<_> = got.into_iter().filter_map(|l| if let Line::Picked { id, option, extras } = l { Some((id, option, extras)) } else { None }).collect();
    let want = [("12", "B", ""), ("12", "C", "a search box | at the top"), ("ab3", "M2", "no border")];
    assert_eq!(picks.iter().map(|(i, o, e)| (i.as_str(), o.as_str(), e.as_str())).collect::<Vec<_>>(), want, "an unclear option or an example isn't a pick");
}

#[test]
fn a_pick_typed_in_the_holders_chat_does_what_the_pick_button_does() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: task | Which search box to keep | done when: you picked one\nROUTE: #new -> cue-teams | why: it knows search");
    let id = item(&desk(&w, &project), "Which search box to keep")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("A keeps the top box, B the side one. Pick one: A or B.\n\
        IF PICKED: #{id} | B | feature | Move search to the side | you'll see: one search box, at the side | done when: the top box is gone | by: me\n\
        IF PICKED: #{id} | A | bug | Fix the top box's focus | by: me"));
    let told_lead = sent_to(&w, &l).len();

    // You answer in its chat, "B might be best, but keep a shortcut"; it reports your pick.
    ends(&w, &h, &format!("Going with B, with the shortcut.\nPICKED: #{id} | B | add: keep a keyboard shortcut to it"));
    w.settle();
    let d = desk(&w, &project);
    let it = item(&d, "Which search box to keep").clone();
    let side = item(&d, "Move search to the side").clone();
    assert_eq!((it["state"].as_str(), it["picked"].as_str(), it["built_as"].as_str()), (Some("done"), Some("B"), side["id"].as_str()), "{it}");
    assert!(d["items"].as_array().unwrap().iter().all(|i| i["title"] != "Fix the top box's focus"), "option A's isn't filed: {d}");
    assert_eq!(side["builds"].as_str(), Some(id.as_str()));
    assert_eq!(side["see"].as_str(), Some("one search box, at the side Also, as you asked: keep a keyboard shortcut to it"), "{side}");
    assert_eq!(side["holder"]["name"], "cue-teams", "routed at once, as the Pick does: {side}");
    let n = side["id"].as_str().unwrap();
    // Its brief carries what you asked for besides.
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{n} (feature): Move search to the side")) && t.contains("You also asked: keep a keyboard shortcut to it")), "{:?}", sent_to(&w, &h));
    w.advance(2000);
    assert_eq!(sent_to(&w, &l).len(), told_lead, "no lead step: {:?}", &sent_to(&w, &l)[told_lead..]);
}

#[test]
fn a_typed_pick_on_mocks_files_the_build_and_answers_its_question() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let (_ask, mut rx) = asks_which(&w, &h);
    ends(&w, &h, &format!("You said C.\nPICKED: #{m} | C"));
    w.settle();
    let d = desk(&w, &project);
    let it = item(&d, "Mocks: working items").clone();
    assert_eq!((it["state"].as_str(), it["picked"].as_str()), (Some("done"), Some("C")), "{it}");
    let b = builds(&w, &project);
    assert_eq!(b.len(), 1, "{b:?}");
    assert_eq!(b[0]["title"], "Build: working items (option C)");
    assert_eq!(answer(&mut rx), "C");
}

#[test]
fn a_typed_pick_on_an_item_it_doesnt_hold_or_picked_already_is_refused_and_it_is_told() {
    let w = World::new();
    let (h, project) = holder(&w);
    let m = mocks(&w, &h, &project);
    let other = lead(&w, "hi");
    ends(&w, &other, &format!("PICKED: #{m} | B"));
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["picked"].as_str().unwrap_or(""), "", "not its item");
    assert!(sent_to(&w, &other).iter().any(|t| t.contains(&format!("PICKED: not done: #{m} isn't yours"))), "{:?}", sent_to(&w, &other));

    ends(&w, &h, &format!("PICKED: #{m} | B"));
    ends(&w, &h, &format!("PICKED: #{m} | C"));
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Mocks: working items")["picked"], "B");
    assert_eq!(builds(&w, &project).len(), 1);
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("PICKED: not done on #{m}: Option B is picked already"))), "{:?}", sent_to(&w, &h));
}
