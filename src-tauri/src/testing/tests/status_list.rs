//! The status list and background agents (stability plan §4, Status list / background agents): a turn that
//! ends with its agents still at work, their reports coming in after it, the list's switches following a session
//! into the one that carries on from it, and what the prompt hook hands a session from a big or malformed reply.

use crate::testing::World;
use serde_json::{json, Value};

const LIST: &str = "**Where we are**\n✓ built it\n→ testing it\nWaiting on you: nothing";

#[test]
fn a_turn_that_ends_with_a_background_agent_at_work_waits_on_it_not_on_you() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("build and test");
    a.transcript().background_agent("ag1", "Run the tests");
    a.stop("The tests are running in the background.");
    let s = w.snapshot();
    assert!(s.cards().is_empty(), "nothing for you yet");
    assert_eq!((s.state("a").as_str(), s.session("a")["driven_by"].as_str()), ("agent", Some("1 background agent")));
}

#[test]
fn its_agent_reporting_after_the_turn_ended_brings_the_answer_to_you() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("build and test");
    a.transcript().background_agent("ag1", "Run the tests");
    a.stop("The tests are running in the background.");
    let notice = a.transcript().task_done("ag1");
    a.prompt(&notice);
    a.transcript().assistant("All 40 tests pass.");
    a.stop("All 40 tests pass.");
    let s = w.snapshot();
    assert_eq!(s.card("a")["message"], "All 40 tests pass.");
    assert_eq!(s.thread("a").iter().filter(|(role, _)| role == "you").count(), 1, "Claude Code's notice isn't a prompt of yours");
}

#[test]
fn one_of_two_agents_reporting_still_waits_on_the_other() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    a.transcript().background_agent("ag1", "Backend").background_agent("ag2", "Frontend");
    a.stop("Both started.");
    assert_eq!(w.snapshot().session("a")["driven_by"], "2 background agents");
    let notice = a.transcript().task_done("ag1");
    a.prompt(&notice);
    a.stop("Backend done; waiting on the frontend.");
    let s = w.snapshot();
    assert!(s.cards().is_empty());
    assert_eq!(s.session("a")["driven_by"], "1 background agent");
}

fn status_sessions(w: &World, section: &str) -> Vec<String> {
    w.snapshot().0["settings"][section]["sessions"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

#[test]
fn the_list_and_background_agents_follow_a_session_into_the_one_that_carries_on() {
    let w = World::new();
    w.hub().set_setting("status.sessions", json!(["old"])).unwrap();
    w.hub().set_setting("background.sessions", json!(["old"])).unwrap();
    assert!(crate::statuslist::carry_over("old", "new"));
    w.hub().redraw();
    w.settle();
    assert_eq!(status_sessions(&w, "status"), ["old", "new"]);
    assert_eq!(status_sessions(&w, "background"), ["old", "new"]);
    assert!(!crate::statuslist::carry_over("old", "new"), "once: on already");
}

#[test]
fn nothing_follows_a_session_into_itself_or_into_nothing() {
    let w = World::new();
    w.hub().set_setting("status.sessions", json!(["a"])).unwrap();
    assert!(!crate::statuslist::carry_over("a", "a") && !crate::statuslist::carry_over("a", "") && !crate::statuslist::carry_over("", "a"));
    assert_eq!(status_sessions(&w, "status"), ["a"]);
}

/// The prompt hook's added context for session `sid`, as the hook works it out from its transcript.
fn hook_context(w: &World, sid: &str) -> Option<String> {
    let p = json!({"session_id": sid, "transcript_path": w.home().join("claude").join(format!("{sid}.jsonl")).to_string_lossy()});
    crate::statuslist::on_prompt(&p, false)
}

#[test]
fn after_a_compaction_the_hook_hands_back_the_last_list_from_a_five_megabyte_conversation() {
    let w = World::new();
    let t = w.session("a").transcript();
    let filler = "x".repeat(1_000_000);
    for _ in 0..5 {
        t.prompt("more").assistant(&filler);
    }
    t.prompt("where are we?").assistant(&format!("Here.\n\n{LIST}")).compact_boundary();
    let c = hook_context(&w, "a").expect("the ask, at least");
    assert!(c.starts_with(&format!("Your status list from the end of your last reply:\n{LIST}\n\n")), "{}", &c[..c.len().min(300)]);
    assert!(c.len() < 2_000, "the list alone, not the conversation: {} bytes", c.len());
}

#[test]
fn a_reply_that_runs_on_after_its_list_isnt_handed_back_as_one() {
    let w = World::new();
    let t = w.session("a").transcript();
    let pasted_log: String = (0..100_000).map(|i| format!("line {i} of the build log\n")).collect();
    t.prompt("go").assistant(&format!("Done.\n\n{LIST}\n\nHere's the log:\n{pasted_log}")).compact_boundary();
    assert_eq!(hook_context(&w, "a").as_deref(), Some(crate::statuslist::SPEC), "just the ask: the window shows no list for it either");
}

#[test]
fn of_two_lists_in_one_reply_the_one_it_ends_with_is_handed_back() {
    let w = World::new();
    let t = w.session("a").transcript();
    let older = "**Where we are**\n✓ an old step\nWaiting on you: nothing";
    t.prompt("go").assistant(&format!("You said earlier:\n\n{older}\n\nNow:\n\n{LIST}")).compact_boundary();
    let c = hook_context(&w, "a").unwrap();
    assert!(c.contains("→ testing it") && !c.contains("an old step"), "{c}");
}

#[test]
fn a_heading_mentioned_mid_sentence_is_not_a_list() {
    let w = World::new();
    let t = w.session("a").transcript();
    t.prompt("go").assistant("Done. The **Where we are**\n✓ list goes at the end.").compact_boundary();
    assert_eq!(hook_context(&w, "a").as_deref(), Some(crate::statuslist::SPEC));
}

/// What the window shows for a list (ui/status.js is the window's reader; the hub only passes replies on): a
/// reply's text kept whole however big, for the window to read.
#[test]
fn a_five_megabyte_reply_with_its_list_reaches_the_window_whole() {
    let w = World::new();
    let reply = format!("{}\n\n{LIST}", "y".repeat(5_000_000));
    w.session("a").prompt("go").stop(&reply);
    let s = w.snapshot();
    let card: &Value = s.card("a");
    assert!(card["message"].as_str().unwrap().ends_with(LIST));
}
