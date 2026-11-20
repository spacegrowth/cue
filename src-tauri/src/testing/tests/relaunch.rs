//! Cue quit and opened again (a restart, an update, the Mac rebooted): what comes back from what it saved,
//! what doesn't, and an ask whose hook asks again once Cue is back (hook.rs keeps trying for 15 minutes).

use crate::testing::World;
use serde_json::json;

#[test]
fn a_restart_cuts_off_an_open_ask_and_doesnt_bring_its_card_back() {
    let mut w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("make deploy");
    w.relaunch();
    assert!(ask.cut_off(), "its connection went with the old Cue: the hook asks again");
    assert!(w.snapshot().cards().is_empty(), "a decision isn't saved: its hook brings it back");
}

#[test]
fn the_hook_asking_again_after_a_restart_is_a_card_answered_as_before() {
    let mut w = World::new();
    let before = w.session("a").prompt("go").ask_bash("make deploy");
    w.relaunch();
    let again = w.session("a").ask_bash("make deploy");
    assert_ne!(again.id, before.id);
    let s = w.snapshot();
    assert_eq!(s.state("a"), "deciding");
    assert_eq!(s.card("a")["id"], again.id.as_str());
    assert_eq!(w.answer(&again.id, json!({"behavior": "allow"})), json!({"ok": true}));
    assert_eq!(again.told(), Some(json!({"behavior": "allow"})));
}

#[test]
fn an_answer_for_a_card_from_before_the_restart_is_refused() {
    let mut w = World::new();
    let before = w.session("a").prompt("go").ask_bash("ls");
    w.relaunch();
    assert_eq!(w.answer(&before.id, json!({"behavior": "allow"})), json!({"ok": false}));
}

#[test]
fn a_finished_turns_card_and_the_session_are_back_after_a_restart() {
    let mut w = World::new();
    w.session("a").in_project("web").prompt("fix the login").stop("Fixed.");
    w.relaunch();
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.card("a")["message"], "Fixed.");
    assert_eq!(s.thread("a"), [("you".to_string(), "fix the login".to_string()), ("agent".into(), "Fixed.".into())]);
    assert_eq!(s.session("a")["cwd"], "/work/web");
}

#[test]
fn a_session_that_ended_before_the_restart_stays_ended() {
    let mut w = World::new();
    w.session("a").prompt("go").ended();
    w.session("b").prompt("go").stop("Done.");
    w.relaunch();
    let s = w.snapshot();
    assert!(s.find("a").is_none());
    assert_eq!(s.session_ids(), ["b"]);
}

#[test]
fn history_answered_before_a_restart_is_still_there() {
    let mut w = World::new();
    let ask = w.session("a").prompt("go").ask_bash("ls");
    w.answer(&ask.id, json!({"behavior": "deny", "message": "not now"}));
    w.relaunch();
    assert_eq!(w.snapshot().history(&ask.id)["outcome"], "denied: “not now”");
}

#[test]
fn a_parked_session_is_still_parked_after_a_restart() {
    let mut w = World::new();
    w.session("old").prompt("go").stop("Done.");
    let record = w.hub().session_record("old").unwrap();
    w.hub().add_parked(record);
    w.relaunch();
    let s = w.snapshot();
    assert!(s.find("old").is_none());
    assert_eq!(s.0["parked"][0]["session_id"], "old", "{}", s.0["parked"]);
}

#[test]
fn a_review_queued_before_a_restart_goes_once_its_lead_is_free() {
    let mut w = World::new();
    w.session("L").prompt("lead");
    let key = w.hub().queued_teammate("L", "a1");
    w.hub().review("L", &key, "review a1").unwrap();
    w.relaunch();
    assert_eq!(w.hub().queued_reviews(), [key], "the queue is saved");
    w.session("L").stop("Done with that.");
    w.advance(1500);
    assert_eq!(w.terminal.typed("L"), ["review a1"]);
}

#[test]
fn a_message_typed_after_a_restart_goes_to_the_session_as_before() {
    let mut w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.relaunch();
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    w.reply(&card, "now the tests");
    assert_eq!(w.terminal.typed("a"), ["now the tests"]);
    assert_eq!(w.snapshot().state("a"), "working", "its prompt hook, through the new Cue");
}

#[test]
fn the_plans_usage_is_back_after_a_relaunch_and_a_window_that_reset_meanwhile_is_not() {
    let mut w = World::new();
    let soon = crate::model::now_ms() / 1000 + 3600;
    w.hub().set_rates(&serde_json::json!({
        "five_hour": { "used_percentage": 12.0, "resets_at": 1 },
        "seven_day": { "used_percentage": 91.0, "resets_at": soon },
    }));
    assert_eq!(w.snapshot().0["usage"]["seven_day"]["pct"], 91.0);
    w.relaunch();
    let usage = &w.snapshot().0["usage"];
    assert_eq!(usage["seven_day"]["pct"], 91.0, "kept across the restart: {usage}");
    assert!(usage["five_hour"].is_null(), "reset long ago: its number is over: {usage}");
}

/// Upgrading to Ultra: the reports now carry the weekly window only. The old 5-hour number goes, the usage says
/// there's no 5-hour limit, and that holds through a report with no numbers and a relaunch.
#[test]
fn after_an_upgrade_to_ultra_the_old_session_limit_is_gone_not_kept() {
    let mut w = World::new();
    let soon = crate::model::now_ms() / 1000 + 3600;
    w.hub().set_rates(&serde_json::json!({
        "five_hour": { "used_percentage": 97.0, "resets_at": soon },
        "seven_day": { "used_percentage": 92.0, "resets_at": soon },
    }));
    assert_eq!(w.snapshot().0["usage"]["five_hour"]["pct"], 97.0);
    w.hub().set_rates(&serde_json::json!({ "seven_day": { "used_percentage": 0.0, "resets_at": soon } }));
    let usage = w.snapshot().0["usage"].clone();
    assert!(usage["five_hour"].is_null(), "Ultra has no 5-hour limit: {usage}");
    assert_eq!((usage["seven_day"]["pct"].as_f64(), usage["no_limit"].clone()), (Some(0.0), serde_json::json!(["five_hour"])));
    w.hub().set_rates(&serde_json::json!({}));
    assert_eq!(w.snapshot().0["usage"]["no_limit"], serde_json::json!(["five_hour"]), "a report without numbers changes nothing");
    w.relaunch();
    let usage = &w.snapshot().0["usage"];
    assert_eq!((usage["five_hour"].is_null(), usage["no_limit"].clone()), (true, serde_json::json!(["five_hour"])), "{usage}");
}
