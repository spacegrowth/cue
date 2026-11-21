//! The user asked when Desks on the same code can't settle it (#540, docs/desk-coordination-plan.md option B, step
//! 5): a lead's DISAGREE, a band unanswered 15 minutes, a hold an hour past its till. One question on every Desk it's
//! about; answering it on one closes it on the others, tells each lead, and notes it on each Desk.

use super::desk::{desk, ends, item, sent_to};
use crate::testing::World;
use serde_json::{json, Value};

fn start(w: &World, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &w.home().to_string_lossy(), true, Some(name)).unwrap();
    (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into())
}

fn id_of(w: &World, d: &str, title: &str) -> String {
    item(&desk(w, d), title)["id"].as_str().unwrap().to_string()
}

/// Lead `lead`'s cards that are Cue's question between Desks.
fn questions(w: &World, lead: &str) -> Vec<Value> {
    w.snapshot().cards_of(lead).into_iter().filter(|c| c["tool_input"]["desk_to_user"].is_object()).cloned().collect()
}

fn text(card: &Value) -> String {
    card["tool_input"]["questions"][0]["question"].as_str().unwrap().to_string()
}

fn choices(card: &Value) -> Vec<String> {
    card["tool_input"]["questions"][0]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap().to_string()).collect()
}

fn answer(w: &World, card: &Value, a: &str) {
    assert!(w.hub().respond(card["id"].as_str().unwrap(), crate::model::Decision { behavior: "allow".into(), answers: Some(json!({ text(card): a })), ..Default::default() }));
    w.settle();
}

/// Cue's last message to lead `lead` that isn't a band.
fn last_to(w: &World, lead: &str) -> String {
    sent_to(w, lead).into_iter().rfind(|t| !t.contains("[From another Desk: ")).unwrap_or_default()
}

fn decisions(w: &World, d: &str) -> Vec<String> {
    desk(w, d)["decisions"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap_or("").to_string()).collect()
}

/// The Desk's tick, `ms` from now.
fn tick_in(w: &World, ms: u64) {
    w.advance(ms);
    w.hub().desk_tick_now();
    w.settle();
}

#[test]
fn a_disagreement_is_one_question_on_both_desks_and_its_answer_reaches_both_leads() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    ends(&w, &lead_a, "ITEM: task | Fix the scraper");
    ends(&w, &lead_b, "ITEM: task | Move the Desk files");
    let (x, y) = (id_of(&w, &a, "Fix the scraper"), id_of(&w, &b, "Move the Desk files"));
    ends(&w, &lead_a, &format!("TELL DESK: Feature B | #{x} is 20 min from landing. Land it first?"));
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty(), "a band alone asks the user nothing");
    ends(&w, &lead_b, &format!("DISAGREE: Feature A | #{y} moves those files now; land after it."));
    assert!(!last_to(&w, &lead_b).contains("Asked:"), "the card is in the lead's name: nothing sent back, as for an ASK");
    let (qa, qb) = (questions(&w, &lead_a), questions(&w, &lead_b));
    assert!(qa.len() == 1 && qb.len() == 1, "{qa:?} {qb:?}");
    let want = format!("The leads of Feature A and Feature B can't agree on shared code. Feature A's lead: \"#{x} is 20 min from landing. Land it first?\" Feature B's lead: \"#{y} moves those files now; land after it.\" Which way?");
    assert_eq!((text(&qa[0]), text(&qb[0])), (want.clone(), want));
    let orders = vec![format!("#{x} goes first, #{y} waits"), format!("#{y} goes first, #{x} waits")];
    assert_eq!((choices(&qa[0]), choices(&qb[0])), (orders.clone(), orders.clone()));
    // A's lead is told B's side as a band (it waits while A's lead has the question open), noted on A's Desk.
    let side = format!("I don't agree: #{y} moves those files now; land after it. Cue asks the user to choose.");
    assert!(w.hub().read(|st| st.desk.bands.iter().any(|x| x.from == b && x.to == a && x.text == side && !x.ask)));
    assert!(decisions(&w, &a).contains(&format!("Feature B's lead told this Desk: {side}")), "{:?}", decisions(&w, &a));
    // Asked once: said again, it's refused; and the band it answers is never asked about as unanswered.
    ends(&w, &lead_b, &format!("DISAGREE: Feature A | #{y} first."));
    // With its card open the lead is asking the user: the refusal comes in its next Desk block.
    let held = w.hub().read(|st| st.desk.run.routes_held.get(&b).cloned().unwrap_or_default());
    assert!(held.contains("Refused: DISAGREE Feature A: the user is asked already. Their answer comes to you and Feature A's lead."), "{held}");
    tick_in(&w, 16 * 60_000);
    assert_eq!((questions(&w, &lead_a).len(), questions(&w, &lead_b).len()), (1, 1));
    answer(&w, &qa[0], &orders[1]);
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty(), "answered on one Desk, closed on both");
    for lead in [&lead_a, &lead_b] {
        assert!(sent_to(&w, lead).iter().any(|t| t.contains(&format!("The user's answer: {}. Go by it.", orders[1]))), "{:?}", sent_to(&w, lead));
    }
    for d in [&a, &b] {
        assert!(decisions(&w, d).contains(&format!("You answered: {}", orders[1])), "{:?}", decisions(&w, d));
    }
}

#[test]
fn disagree_needs_the_other_side_first() {
    let w = World::new();
    let (_, lead_a) = start(&w, "Feature A");
    let (_, lead_b) = start(&w, "Feature B");
    ends(&w, &lead_a, "DISAGREE: Feature B | mine first");
    assert_eq!(last_to(&w, &lead_a), "Refused: DISAGREE Feature B: Feature B's lead hasn't told this Desk anything. You can say your side first with TELL DESK: Feature B | <your words>; ask the user only when you can't agree.");
    ends(&w, &lead_a, "DISAGREE: Nowhere | mine first");
    assert!(last_to(&w, &lead_a).starts_with("Refused: DISAGREE Nowhere: no such Desk on this repository."));
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty());
}

#[test]
fn a_band_unanswered_15_minutes_is_asked_about_once_and_an_answered_one_never() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (_, lead_b) = start(&w, "Feature B");
    let (_, lead_d) = start(&w, "Docs");
    ends(&w, &lead_a, "TELL DESK: Feature B | Land yours first?");
    ends(&w, &lead_a, "TELL DESK: Docs | And yours?");
    tick_in(&w, 60_000);
    ends(&w, &lead_d, "TELL DESK: Feature A | Yes, go.");
    tick_in(&w, 13 * 60_000);
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty(), "not before 15 minutes");
    tick_in(&w, 2 * 60_000);
    let (qa, qb) = (questions(&w, &lead_a), questions(&w, &lead_b));
    assert!(qa.len() == 1 && qb.len() == 1, "{qa:?} {qb:?}");
    assert_eq!(text(&qa[0]), "Feature A's lead asked Feature B's lead: \"Land yours first?\" No answer from Feature B's lead in 15 min.");
    assert_eq!(choices(&qb[0]), ["Tell Feature B's lead to answer", "Go ahead without them"]);
    assert!(questions(&w, &lead_d).is_empty(), "Docs answered: nothing to ask");
    tick_in(&w, 30 * 60_000);
    assert_eq!((questions(&w, &lead_a).len(), questions(&w, &lead_b).len()), (1, 1), "asked once");
    answer(&w, &qb[0], "Tell Feature B's lead to answer");
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty());
    assert!(last_to(&w, &lead_b).contains("\"Land yours first?\"") && last_to(&w, &lead_b).contains("The user's answer: Tell Feature B's lead to answer."), "{}", last_to(&w, &lead_b));
    assert!(decisions(&w, &a).contains(&"You answered: Tell Feature B's lead to answer".to_string()));
}

#[test]
fn a_hold_an_hour_past_its_till_is_asked_about_once_and_lift_the_hold_lifts_it() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (_, lead_b) = start(&w, "Feature B");
    ends(&w, &lead_a, "ITEM: task | Move the Desk files\nHOLD: #new | area: ui/desk_*.js | till: 2 hours | why: the move");
    ends(&w, &lead_a, "ITEM: task | Rename the hub\nHOLD: #new | area: src/hub/*.rs | till: it lands | why: the rename");
    ends(&w, &lead_a, "ITEM: task | Split the CSS\nHOLD: #new | area: ui/*.css | till: in 30 min | why: the split");
    let (slow, never, css) = (id_of(&w, &a, "Move the Desk files"), id_of(&w, &a, "Rename the hub"), id_of(&w, &a, "Split the CSS"));
    tick_in(&w, 89 * 60_000);
    assert!(questions(&w, &lead_a).is_empty(), "not before its till and an hour");
    tick_in(&w, 2 * 60_000);
    let (qa, qb) = (questions(&w, &lead_a), questions(&w, &lead_b));
    assert!(qa.len() == 1 && qb.len() == 1, "{qa:?} {qb:?}");
    assert_eq!(text(&qa[0]), format!("Feature A #{css} still holds ui/*.css, an hour past its till of 30 min."));
    assert_eq!(choices(&qa[0]), ["Lift the hold", "Keep it"]);
    answer(&w, &qb[0], "Lift the hold");
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty());
    let holds = || w.hub().read(|st| st.desk.holds.iter().map(|h| h.item.clone()).collect::<Vec<_>>());
    assert!(!holds().contains(&css) && holds().contains(&slow), "{:?}", holds());
    assert!(last_to(&w, &lead_a).contains("The user's answer: Lift the hold. The hold is lifted."), "{}", last_to(&w, &lead_a));
    tick_in(&w, 90 * 60_000);
    let qa = questions(&w, &lead_a);
    assert!(qa.len() == 1 && text(&qa[0]) == format!("Feature A #{slow} still holds ui/desk_*.js, an hour past its till of 2 hours."), "{qa:?}");
    answer(&w, &qa[0], "Keep it");
    assert!(holds().contains(&slow), "kept");
    tick_in(&w, 5 * 60 * 60_000);
    assert!(questions(&w, &lead_a).is_empty() && questions(&w, &lead_b).is_empty(), "asked once; \"it lands\" never overstays");
    assert!(holds().contains(&never));
}
