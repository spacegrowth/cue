//! Away mode on a Desk (desk/hub/away/away.rs): turned on, its rules in the lead's summary and the lead told; ended by its
//! end time (with no turn ending) or at a Desk session's turn end; turned off. Ask me before still waits for the owner
//! while it's on; a session or lead a usage limit stopped is skipped and carried on after the reset.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;

fn summary(w: &World, project: &str) -> String {
    w.hub().desk_summary(project, "", false).0
}

/// Long enough, with Cue's ticks, for a row on the Desk's change feed that waits for a wake to go on its own.
fn feed_goes(w: &World) {
    for _ in 0..6 {
        w.advance(60_000);
        w.tick(true);
    }
}

#[test]
fn away_mode_turns_on_tells_the_lead_ends_at_its_time_and_turns_off() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    // Never set: off, the default rules.
    let away = desk(&w, &project)["away"].clone();
    assert_eq!((away["on"].clone(), away["active"].clone(), away["ended"].clone(), away["rules"]["route"].clone(), away["rules"]["what"].clone()), (json!(false), json!(false), json!(""), json!(true), json!(false)), "{away}");
    assert!(!summary(&w, &project).contains("Away mode"));

    let until = crate::clock::now_ms() + 60 * 60 * 1000;
    let v = w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": 20, "rules": { "how": false } })).unwrap();
    w.settle();
    assert_eq!((v["on"].clone(), v["active"].clone(), v["until"].clone(), v["budget"].clone(), v["rules"]["how"].clone(), v["rules"]["route"].clone()), (json!(true), json!(true), json!(until), json!(20.0), json!(false), json!(true)), "{v}");
    assert!(v["since"].as_u64().unwrap() > 0, "{v}");
    assert_eq!(desk(&w, &project)["away"], v);
    let s = summary(&w, &project);
    assert!(s.contains("Away mode:") && s.contains("with a $20.00 budget"), "{s}");
    assert!(s.contains("- Route, split and merge items; wake or start sessions (up to 3 new)") && s.contains("DECIDED: <what you chose> — <why>"), "{s}");
    let may = s.split("Don't write ASK").next().unwrap();
    assert!(!may.contains("'How' questions") && !may.contains("'What' questions"), "switched off: not decided alone: {s}");
    assert!(s.contains("- Picking a mock") && s.contains("- 'What' questions") && s.contains("don't ASK about it"), "{s}");
    assert!(s.contains("stopped on a permission it can't grant itself waits for the owner") && s.contains("Never grant it yourself") && s.contains("Move the other items along meanwhile"), "{s}");
    // Told on the Desk's change feed: with its next wake, or alone after a while.
    feed_goes(&w);
    assert!(sent_to(&w, &d).iter().any(|t| t.starts_with("Away mode:")), "the lead is told: {:?}", sent_to(&w, &d));

    // Ask me before still waits for the owner while Away is on, even with every rule switched on.
    let all = json!({ "route": true, "how": true, "stopped": true, "order": true, "what": true, "outside": true });
    let v = w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": 20, "rules": all })).unwrap();
    w.settle();
    assert_eq!((v["active"].clone(), v["rules"].clone()), (json!(true), all), "{v}");
    assert!(w.session(&h).in_folder(w.home()).guard_bash("git push origin main", "main").is_some(), "a push to main still waits");
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": 20, "rules": { "how": false, "what": false, "outside": false } })).unwrap();
    w.settle();

    // Its end time passes on an idle Desk (no turn ends): noticed then, the lead told once, though Away was set three
    // times (three timers) and a turn ends after.
    w.advance(2 * 60 * 60 * 1000);
    feed_goes(&w);
    assert_eq!(sent_to(&w, &d).iter().filter(|t| t.contains("Away is paused")).count(), 1, "{:?}", sent_to(&w, &d));
    let away = desk(&w, &project)["away"].clone();
    assert_eq!((away["on"].clone(), away["active"].clone(), away["ended"].clone()), (json!(true), json!(false), json!("until")), "{away}");
    let s = summary(&w, &project);
    assert!(s.contains("Away is paused: its end time has passed") && !s.contains("You may decide"), "{s}");
    ends(&w, &d, "Still routing.");
    assert_eq!(sent_to(&w, &d).iter().filter(|t| t.contains("Away is paused")).count(), 1, "{:?}", sent_to(&w, &d));

    // Off: the block is gone, the lead told.
    let v = w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    assert_eq!((v["on"].clone(), v["active"].clone(), v["ended"].clone(), v["rules"]["how"].clone()), (json!(false), json!(false), json!(""), json!(false)), "rules kept: {v}");
    assert!(!summary(&w, &project).contains("Away mode"));
    feed_goes(&w);
    assert!(sent_to(&w, &d).iter().any(|t| t.starts_with("Away mode is off")), "{:?}", sent_to(&w, &d));
}

#[test]
fn while_away_a_session_and_lead_a_usage_limit_stopped_are_skipped_and_carried_on_after_the_reset() {
    use crate::sessions::send_at::CARRY_ON;
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    let limit = "You've hit your session limit · resets 4:10am (America/Los_Angeles)";
    for sid in [&h, &d] {
        w.hosted_says(sid, HostedEvent::ApiError { error: "rate_limit".into(), text: limit.into() });
        w.hosted_says(sid, HostedEvent::TurnEnded { text: String::new(), failed: true, cost_usd: 0.0, unprompted: false });
    }
    w.settle();
    for sid in [&h, &d] {
        let s = w.snapshot().session(sid).clone();
        assert_eq!((s["state"].as_str(), s["send_at"]["at_ms"].as_u64()), (Some("limited"), Some(0)), "{s}");
    }
    let (to_lead, to_holder) = (sent_to(&w, &d).len(), sent_to(&w, &h).len());

    // Pressing Away again and changing a rule: the limited lead isn't sent the sweep or Away's words.
    crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap();
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": null, "rules": { "how": false } })).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(false), "no sweep to wait for");
    assert_eq!((sent_to(&w, &d).len(), sent_to(&w, &h).len()), (to_lead, to_holder), "{:?}", sent_to(&w, &d));
    for sid in [&h, &d] {
        assert_eq!(w.snapshot().session(sid)["send_at"]["at_ms"].as_u64(), Some(0), "its carry-on is kept");
    }

    // After the reset both are carried on, and Away's words are in the lead's summary.
    w.advance(25 * 3600 * 1000);
    w.hub().usage_tick();
    w.settle();
    for sid in [&h, &d] {
        assert_eq!(sent_to(&w, sid).last().map(String::as_str), Some(CARRY_ON), "{sid}: {:?}", sent_to(&w, sid));
    }
    assert!(summary(&w, &project).contains("A session stopped by a usage limit is carried on by Cue after the reset"));
}

#[test]
fn pressing_away_asks_the_lead_to_sort_things_out_until_its_turn_ends() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(false));
    crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({ "how": false })).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(true));
    let asked = sent_to(&w, &d).into_iter().find(|t| t.contains("The owner is about to go away")).unwrap_or_else(|| panic!("{:?}", sent_to(&w, &d)));
    assert!(asked.contains("- Route, split and merge items") && !asked.contains("'How' questions") && asked.contains("at most 5 questions"), "{asked}");
    ends(&w, &d, "DECIDED: Header first — it blocks the rest\nASK: Which color for the badge?");
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(false), "cleared at the lead's turn end");
}

/// #612: the lead's pass before Away runs only when something changed on the Desk since its last one (an item added
/// or moved, a new question); turning Away off and on again with nothing new doesn't send it again.
#[test]
fn the_pass_before_away_runs_only_when_the_desk_changed_since_the_last_one() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let passes = |w: &World| sent_to(w, &d).iter().filter(|t| t.contains("The owner is about to go away")).count();
    let away = |on: bool| {
        w.hub().desk_away_set(&project, &json!({ "on": on, "until": null, "budget": null, "rules": {} })).unwrap();
        w.settle();
    };
    // The first time: it goes through the items.
    assert!(crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap());
    w.settle();
    // Pressed again while it's still going through them: not sent twice.
    assert!(!crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap());
    away(true);
    ends(&w, &d, "DECIDED: Nothing to settle — the Desk is quiet");
    assert_eq!(passes(&w), 1);

    // Off and on again, nothing changed: no second pass, and nothing waits on one.
    away(false);
    assert!(!crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap());
    away(true);
    assert_eq!((passes(&w), desk(&w, &project)["away"]["sweeping"].clone()), (1, json!(false)));

    // A new item: the next Away goes through the items again.
    away(false);
    w.hub().desk_add(&project, "bug", "Header overlaps", "", None).unwrap();
    w.settle();
    assert!(crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap());
    w.settle();
    ends(&w, &d, "Routed it.");
    assert_eq!(passes(&w), 2);
    assert!(!crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap(), "nothing new since that pass");

    // A new question from a session on the Desk: due again.
    let input = json!({ "questions": [{ "question": "Which color?", "header": "Color", "options": [{ "label": "Blue" }, { "label": "Red" }], "multiSelect": false }] });
    w.hub().add_ask(w.hub().session_origin(&h).unwrap(), crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: serde_json::Value::Null, context: vec![], agent_id: String::new() });
    w.settle();
    assert!(crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap(), "a new question is worth a pass");
}

#[test]
fn away_keeps_its_limits_honest_a_zero_budget_is_none_routing_off_starts_nothing_ended_stays_ended() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    // $0 is no budget: Away doesn't end before it begins.
    let v = w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": 0, "rules": { "route": false } })).unwrap();
    assert_eq!((v["budget"].clone(), v["active"].clone()), (json!(null), json!(true)), "{v}");
    // Routing off: no word about starting sessions.
    let s = summary(&w, &project);
    assert!(s.contains("with no budget set") && !s.contains("new sessions while the owner is away"), "{s}");

    // Ended by its time; a rule changed after (same limits): still ended, the lead not told "paused" again.
    let until = crate::clock::now_ms() + 60 * 1000;
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": null, "rules": { "route": false } })).unwrap();
    w.advance(2 * 60 * 1000);
    ends(&w, &d, "Still routing.");
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": null, "rules": { "route": true } })).unwrap();
    ends(&w, &d, "Still routing.");
    w.settle();
    assert_eq!(desk(&w, &project)["away"]["ended"], json!("until"));
    // Both on the Desk's change feed before it went: folded into the latest, the rule change's block.
    feed_goes(&w);
    assert_eq!(sent_to(&w, &d).iter().filter(|t| t.contains("Away is paused")).count(), 1, "told once, with the rule change's block: {:?}", sent_to(&w, &d));
    // A later end time: it holds again.
    let v = w.hub().desk_away_set(&project, &json!({ "on": true, "until": crate::clock::now_ms() + 60 * 60 * 1000, "budget": null, "rules": { "route": true } })).unwrap();
    assert_eq!((v["active"].clone(), v["ended"].clone()), (json!(true), json!("")), "{v}");
}

#[test]
fn a_sweep_whose_turn_fails_stops_the_window_waiting() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    crate::ipc::desk_away_sweep_with(w.hub(), &project, &json!({})).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(true));
    w.hosted_says(&d, HostedEvent::TurnEnded { text: "Something broke".into(), failed: true, cost_usd: 0.0, unprompted: false });
    w.settle();
    assert_eq!(desk(&w, &project)["away"]["sweeping"], json!(false));
}

/// #416: each Away stretch is kept as it goes off, newest first: when, how long, spend, the items that moved, the
/// lead's calls (with Undo while it can still be undone) and the steps held for you, with your answer once given.
#[test]
fn each_away_stretch_is_kept_with_what_moved_the_calls_and_what_was_held() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().ask_before_set(&project, Some(&["data".into()])).unwrap();
    w.hub().desk_add(&project, "bug", "Before", "", None).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["away_log"], json!([]), "none yet");

    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": 10, "rules": {} })).unwrap();
    w.settle();
    w.advance(60 * 1000);
    w.hub().desk_add(&project, "task", "While away", "", None).unwrap();
    ends(&w, &d, "DECIDED: Header first — it blocks the rest");
    let s = w.session(&h).in_folder(w.home());
    assert_eq!(s.guard_bash("rm -rf build", "main").unwrap().told().unwrap()["behavior"], "deny", "held");
    w.advance(60 * 1000);
    // Still on: nothing kept yet.
    assert_eq!(desk(&w, &project)["away_log"], json!([]));
    w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();

    let log = desk(&w, &project)["away_log"].clone();
    let r = &log[0];
    assert_eq!(log.as_array().unwrap().len(), 1, "{log}");
    assert!(r["end"].as_u64().unwrap() >= r["since"].as_u64().unwrap() + 2 * 60 * 1000, "{r}");
    assert_eq!((r["budget"].clone(), r["ended"].clone()), (json!(10.0), json!("")), "{r}");
    let items = r["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["title"] == "While away" && i["from"] == ""), "added while away: {r}");
    assert!(items.iter().all(|i| i["title"] != "Before" || i["from"] != i["to"]), "{r}");
    let call = r["calls"].as_array().unwrap().iter().find(|c| c["text"] == "Header first").cloned().expect("the call kept");
    assert_eq!((call["why"].clone(), call["undo"].clone()), (json!("it blocks the rest"), json!("can")), "{call}");
    let held = r["held"].as_array().unwrap();
    assert_eq!((held.len(), held[0]["what"].clone(), held[0]["command"].clone(), held[0]["answer"].clone()), (1, json!("delete files"), json!("rm -rf build"), json!("")), "{r}");

    // You answer it after you're back: the stretch says so, though the held list has let it go.
    let id = held[0]["id"].as_str().unwrap().to_string();
    w.hub().desk_held_answer(&id, false).unwrap();
    w.settle();
    assert!(desk(&w, &project)["held"].as_array().unwrap().is_empty());
    assert_eq!(desk(&w, &project)["away_log"][0]["held"][0]["answer"], "no");

    // Undone: no Undo any more.
    w.hub().desk_undo(call["id"].as_str().unwrap()).unwrap();
    w.settle();
    let undo = desk(&w, &project)["away_log"][0]["calls"].as_array().unwrap().iter().find(|c| c["id"] == call["id"]).unwrap()["undo"].clone();
    assert_ne!(undo, "can", "{undo}");

    // A second stretch, ended by its time: newest first, and it ends when its time came, not when you turned it off.
    w.advance(60 * 1000);
    let until = crate::clock::now_ms() + 10 * 60 * 1000;
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": null, "rules": {} })).unwrap();
    w.settle();
    w.advance(60 * 60 * 1000);
    w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    let log = desk(&w, &project)["away_log"].clone();
    assert_eq!(log.as_array().unwrap().len(), 2, "{log}");
    assert_eq!((log[0]["ended"].clone(), log[0]["held"].clone()), (json!("until"), json!([])), "{log}");
    assert!(log[0]["end"].as_u64().unwrap() < until + 60 * 1000 && log[0]["since"].as_u64() > log[1]["since"].as_u64(), "{log}");
}

/// #471: while Away is in force the lead asks you nothing. Its ASK (with or without an item) isn't a card waiting on
/// you: Cue sends it back to decide itself, and the call it writes next shows with the question. One still open as Away
/// starts goes back the same way. A held delete still waits for you; once Away is off, an ASK reaches you again.
#[test]
fn while_away_the_leads_ask_goes_back_to_it_to_decide() {
    let w = World::new();
    // The lead's questions waiting on you (not its turn's end).
    let asks = |sid: &str| w.snapshot().cards_of(sid).into_iter().filter(|c| c["kind"] == "question").count();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().ask_before_set(&project, Some(&["data".into()])).unwrap();
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    // Asked before Away: a card waiting on you, and the item waits on you with it.
    ends(&w, &d, "ASK: Which font for the footer? | choices: Serif / Sans");
    assert_eq!(asks(&d), 1);

    // Away comes on: that open ASK goes back to the lead; the lead's instructions say not to ask.
    let until = crate::clock::now_ms() + 60 * 60 * 1000;
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": until, "budget": null, "rules": {} })).unwrap();
    w.settle();
    assert_eq!(asks(&d), 0, "no card waits on you");
    let back = sent_to(&w, &d).into_iter().find(|t| t.contains("Cue didn't show them this ASK")).unwrap_or_else(|| panic!("{:?}", sent_to(&w, &d)));
    assert!(back.contains("Decide it yourself") && back.contains("DECIDED:") && back.contains("- Which font for the footer?"), "{back}");
    let s = summary(&w, &project);
    assert!(s.contains("Don't write ASK: lines while the owner is away") && s.contains("Leave #<id> until the owner is back") && !s.contains("Always ASK about"), "{s}");
    ends(&w, &d, "DECIDED: Sans for the footer — it matches the header");

    // Asked while away, about an item and not: no card, the item doesn't wait on you, both go back in one message.
    ends(&w, &d, &format!("ASK: #{id} | Keep the old header for a release? | choices: Yes / No\nASK: Ship the footer today?"));
    assert_eq!(asks(&d), 0, "no card waits on you");
    let it = item(&desk(&w, &project), "Header").clone();
    assert!(it["state"] != "needs_you" && it["ask"].is_null(), "{it}");
    let back = sent_to(&w, &d).into_iter().rev().find(|t| t.contains("Cue didn't show them these ASKs")).unwrap_or_else(|| panic!("{:?}", sent_to(&w, &d)));
    assert!(back.contains(&format!("- #{id}: Keep the old header for a release?")) && back.contains("- Ship the footer today?"), "{back}");
    // A held delete still waits for you in the Away list.
    assert_eq!(w.session(&h).in_folder(w.home()).guard_bash("rm -rf build", "main").unwrap().told().unwrap()["behavior"], "deny", "held");
    assert_eq!(desk(&w, &project)["held"].as_array().unwrap().len(), 1);

    // Its calls next show with the questions they answer: by item first, then in order.
    ends(&w, &d, &format!("DECIDED: Ship the footer tomorrow — the review isn't done\nDECIDED: Keep the old header on #{id} — it's in the release notes"));
    let calls = desk(&w, &project)["decisions"].as_array().unwrap().clone();
    let asked = |text: &str| calls.iter().find(|c| c["text"].as_str().is_some_and(|t| t.starts_with(text))).map(|c| c["asked"].clone()).unwrap_or_else(|| panic!("{text}: {calls:?}"));
    assert_eq!(asked("Keep the old header"), json!("Keep the old header for a release?"));
    assert_eq!(asked("Ship the footer tomorrow"), json!("Ship the footer today?"));
    assert_eq!(asked("Sans for the footer"), json!("Which font for the footer?"), "the open ASK sent back as Away came on");
    ends(&w, &d, "DECIDED: Blue badge — matches");
    assert_eq!(desk(&w, &project)["decisions"].as_array().unwrap().iter().find(|c| c["text"] == "Blue badge").unwrap()["asked"], json!(null), "nothing left to answer");

    // Kept in the stretch's record as Away goes off.
    w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    let log = desk(&w, &project)["away_log"][0]["calls"].clone();
    assert!(log.as_array().unwrap().iter().any(|c| c["asked"] == "Ship the footer today?"), "{log}");

    // Back: an ASK reaches you as a card again.
    ends(&w, &d, "ASK: Which color for the badge?");
    assert_eq!(asks(&d), 1);
}

#[test]
fn a_lead_ask_about_an_item_closes_by_itself_once_the_item_is_done() {
    // #613: a question about an item that has since finished no longer needs you; "Before you go" never shows it.
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let asks = |sid: &str| w.snapshot().cards_of(sid).into_iter().filter(|c| c["kind"] == "question").count();
    ends(&w, &d, "ITEM: Header\nITEM: Footer");
    let head = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let foot = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ASK: #{head} | Keep the old header? | choices: Yes / No\nASK: #{foot} | Blue footer? | choices: Yes / No\nASK: Ship today?"));
    assert_eq!(asks(&d), 3);
    // The tick leaves them while their items are open.
    w.hub().desk_tick_now();
    assert_eq!(asks(&d), 3);
    // The header item is dropped: its question closes; the footer's and the one about no item stay.
    w.hub().desk_item_act(&head, "drop", "").unwrap();
    w.settle();
    w.hub().desk_tick_now();
    let left: Vec<String> = w.snapshot().cards_of(&d).into_iter().map(|c| c["tool_input"]["questions"][0]["question"].as_str().unwrap_or("").to_string()).collect();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.iter().any(|q| q.contains("old header")), "{left:?}");
    // Nothing was sent to the lead about it: it finished the item itself.
    assert!(!sent_to(&w, &d).iter().any(|t| t.contains("Keep the old header")), "{:?}", sent_to(&w, &d));
}
