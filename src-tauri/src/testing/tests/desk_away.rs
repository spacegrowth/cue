//! Away mode on a Desk (hub/desk_away.rs): turned on, its rules in the lead's summary and the lead told; ended by its
//! end time (with no turn ending) or at a Desk session's turn end; turned off. Ask me before still waits for the owner
//! while it's on; a session or lead a usage limit stopped is skipped and carried on after the reset.

use super::desk::{desk, ends, holder, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;

fn summary(w: &World, project: &str) -> String {
    w.hub().desk_summary(project, "", false).0
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
    let may = s.split("Everything else waits").next().unwrap();
    assert!(!may.contains("'How' questions") && !may.contains("'What' questions"), "switched off: not decided alone: {s}");
    assert!(s.contains("- Picking a mock") && s.contains("- 'What' questions") && s.contains("don't ASK about it"), "{s}");
    assert!(s.contains("stopped on a permission it can't grant itself waits for the owner") && s.contains("Never grant it yourself") && s.contains("Move the other items along meanwhile"), "{s}");
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
    assert_eq!(sent_to(&w, &d).iter().filter(|t| t.contains("Away is paused")).count(), 2, "told at the end, and with the rule change's block only: {:?}", sent_to(&w, &d));
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
