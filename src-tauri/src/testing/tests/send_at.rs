//! A message Cue sends by itself later: at a time you picked, and Cue's carry-on after a usage limit resets.

use crate::sessions::send_at::{CARRY_ON, CARRY_ON_TRIES};
use crate::testing::World;

#[test]
fn a_message_set_for_a_time_goes_then_and_not_before() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("plan it").stop("Planned.");
    let at = crate::clock::now_ms() + 60_000;
    w.hub().send_at("a", "build step 1", at).unwrap();
    assert_eq!(w.snapshot().session("a")["send_at"]["text"], "build step 1");
    w.advance(30_000);
    w.hub().usage_tick();
    w.settle();
    assert!(w.terminal.typed("a").is_empty(), "not yet: {:?}", w.terminal.typed("a"));
    w.advance(40_000);
    w.hub().usage_tick();
    w.settle();
    assert_eq!(w.terminal.typed("a"), ["build step 1"]);
    assert!(w.snapshot().session("a")["send_at"].is_null(), "sent once: {}", w.snapshot().session("a"));
    w.hub().usage_tick();
    w.settle();
    assert_eq!(w.terminal.typed("a").len(), 1, "never twice");
}

#[test]
fn a_time_already_past_is_refused() {
    let w = World::new();
    w.session("a").prompt("plan it").stop("Planned.");
    assert!(w.hub().send_at("a", "too late", crate::clock::now_ms() - 1).is_err());
}

#[test]
fn a_turn_a_usage_limit_stopped_carries_on_when_it_resets() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all").failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    let snap = w.snapshot();
    let s = snap.session("a");
    assert_eq!((s["send_at"]["text"].as_str(), s["send_at"]["at_ms"].as_u64()), (Some(CARRY_ON), Some(0)), "{s}");
    w.hub().usage_tick();
    w.settle();
    assert!(w.terminal.typed("a").is_empty(), "not before the reset");
    w.advance(25 * 3600 * 1000);
    w.hub().usage_tick();
    w.settle();
    assert_eq!(w.terminal.typed("a"), [CARRY_ON]);
}

#[test]
fn a_limited_session_you_carried_on_yourself_gets_no_carry_on() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all").failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    a.prompt("go on");
    w.advance(25 * 3600 * 1000);
    w.hub().usage_tick();
    w.settle();
    assert!(w.terminal.typed("a").is_empty(), "{:?}", w.terminal.typed("a"));
    assert!(w.snapshot().session("a")["send_at"].is_null());
}

#[test]
fn a_limit_with_no_known_reset_is_checked_every_15_minutes_and_carries_on_once_claude_answers() {
    let w = World::new();
    let a = w.session("a");
    crate::hub::send_at_probe::set(false);
    a.prompt("do it all").failed("rate_limit", "You've hit your session limit");
    let snap = w.snapshot();
    assert!(snap.session("a")["limit"]["resets_ms"].is_null(), "no reset time known: {}", snap.session("a"));
    w.hub().usage_tick();
    w.settle();
    w.advance(16 * 60 * 1000);
    w.hub().usage_tick();
    w.settle();
    w.hub().usage_tick();
    w.settle();
    assert!(w.terminal.typed("a").is_empty(), "still out: nothing sent");
    crate::hub::send_at_probe::set(true);
    w.advance(16 * 60 * 1000);
    w.hub().usage_tick();
    w.settle();
    crate::hub::send_at_probe::set(false);
    w.advance(61_000);
    w.hub().usage_tick();
    w.settle();
    assert_eq!(w.terminal.typed("a"), [CARRY_ON]);
}

#[test]
fn the_carry_on_goes_a_minute_after_the_reset_and_a_bounce_is_tried_a_minute_later_three_times_then_waits_for_you() {
    let w = World::new();
    let a = w.session("a");
    let limit = "You've hit your session limit · resets 4:10am (America/Los_Angeles)";
    a.prompt("do it all").failed("rate_limit", limit);
    let reset = crate::usage::reset_time(limit, crate::clock::now_ms()).unwrap();
    w.advance(reset - crate::clock::now_ms());
    for n in 0..CARRY_ON_TRIES as usize {
        w.hub().usage_tick();
        w.settle();
        w.advance(59_000);
        w.hub().usage_tick();
        w.settle();
        assert_eq!(w.terminal.typed("a").len(), n, "not before the minute: try {}", n + 1);
        w.advance(1_000);
        w.hub().usage_tick();
        w.settle();
        assert_eq!(w.terminal.typed("a").len(), n + 1, "a minute on: try {}", n + 1);
        // It bounced off the limit again (whose message now names tomorrow's 4:10am).
        a.prompt(CARRY_ON).failed("rate_limit", limit);
    }
    w.advance(10 * 60_000);
    w.hub().usage_tick();
    w.settle();
    assert_eq!(w.terminal.typed("a").len(), CARRY_ON_TRIES as usize, "{:?}", w.terminal.typed("a"));
    let snap = w.snapshot();
    let s = snap.session("a");
    assert_eq!(s["send_at"]["text"], CARRY_ON, "kept for you: {s}");
    assert!(!s["send_at"]["failed"].as_str().unwrap_or("").is_empty(), "waits for you: {s}");
}
