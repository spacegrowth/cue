//! Names: given in Cue, and kept in step with Claude Code's list.

use super::*;

#[test]
fn a_rename_shows_at_once_and_is_settled_when_claude_codes_list_shows_it() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.sync_agent_names(&listed(&[("a", "old-name")]), 0);
    assert_eq!(get(&s, "a").name, "old-name", "first seen: Cue takes the name it has there");
    s.set_name("a", "fix the login page");
    s.expect_rename("a", "fix the login page", 15_000);
    assert_eq!(get(&s, "a").name, "fix the login page", "shown at once, before the agent has it");
    // Not there yet, still in time: nothing changes, no error, the old name there doesn't win.
    assert!(!s.sync_agent_names(&listed(&[("a", "old-name")]), 5_000));
    assert_eq!((get(&s, "a").name.as_str(), get(&s, "a").rename_error.as_str()), ("fix the login page", ""));
    // There now: settled.
    assert!(s.sync_agent_names(&listed(&[("a", "fix the login page")]), 6_000));
    let a = get(&s, "a");
    assert_eq!((a.name.as_str(), a.rename_error.as_str(), a.rename_check.is_none()), ("fix the login page", "", true));
    assert_eq!(a.claude_name.as_deref(), Some("fix the login page"));
    // And it stays settled: the same name next time changes nothing.
    assert!(!s.sync_agent_names(&listed(&[("a", "fix the login page")]), 7_000));
}

#[test]
fn a_rename_claude_code_never_took_keeps_your_name_and_says_so() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.sync_agent_names(&listed(&[("a", "old-name")]), 0);
    s.set_name("a", "new-name");
    s.expect_rename("a", "new-name", 15_000);
    assert!(s.sync_agent_names(&listed(&[("a", "old-name")]), 15_000));
    let a = get(&s, "a");
    assert_eq!(a.name, "new-name", "your name stays");
    assert_eq!(a.rename_error, "Claude Code didn't take the new name: it still calls it “old-name”");
    // The old name there is now known: it doesn't come back as if renamed in the terminal.
    assert!(!s.sync_agent_names(&listed(&[("a", "old-name")]), 20_000));
    assert_eq!(get(&s, "a").name, "new-name");
    // Try again (a fresh rename) clears the notice; × does too.
    s.set_name("a", "new-name");
    assert_eq!(get(&s, "a").rename_error, "");
    s.rename_failed("a", "Pi isn't connected");
    assert!(s.dismiss_rename_error("a"));
    assert!(!s.dismiss_rename_error("a"), "nothing left to dismiss");
    // Never named there at all: says so without quoting an empty name.
    let mut s = Sessions::default();
    named(&mut s, "b", "claude", "");
    s.set_name("b", "x");
    s.expect_rename("b", "x", 10);
    s.sync_agent_names(&listed(&[("b", "")]), 10);
    assert_eq!(get(&s, "b").rename_error, "Claude Code didn't take the new name");
}

#[test]
fn a_rename_made_in_claude_codes_terminal_becomes_cues_name() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.set_name("a", "cue-name");
    s.sync_agent_names(&listed(&[("a", "cue-name")]), 0);
    // /rename typed in its terminal: the name there changes, Cue follows.
    assert!(s.sync_agent_names(&listed(&[("a", "typed-in-terminal")]), 1_000));
    assert_eq!(get(&s, "a").name, "typed-in-terminal");
    // A failed rename's notice goes once the name there changes to something you chose.
    s.rename_failed("a", "Claude Code didn't take the new name");
    s.sync_agent_names(&listed(&[("a", "another")]), 2_000);
    assert_eq!((get(&s, "a").name.as_str(), get(&s, "a").rename_error.as_str()), ("another", ""));
    // Its name there emptied: Cue keeps the one it has.
    s.sync_agent_names(&listed(&[("a", "")]), 3_000);
    assert_eq!(get(&s, "a").name, "another");
}

#[test]
fn first_seen_in_claude_codes_list_its_name_there_wins() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.set_name("a", "web-shop"); // e.g. a folder name an old resume stored as its name
    assert!(s.sync_agent_names(&listed(&[("a", "[Lead] checkout-flow")]), 0));
    assert_eq!(get(&s, "a").name, "[Lead] checkout-flow", "as the window showed it before");
    // No name there: Cue's stays.
    named(&mut s, "n", "claude", "");
    s.set_name("n", "mine");
    s.sync_agent_names(&listed(&[("n", "")]), 0);
    assert_eq!(get(&s, "n").name, "mine");
    // No name in Cue: it takes the one there (started with -n, or named in its terminal before Cue saw it).
    named(&mut s, "b", "claude", "");
    s.sync_agent_names(&listed(&[("b", "from-claude")]), 0);
    assert_eq!(get(&s, "b").name, "from-claude");
    // Not in the list (not running here): untouched.
    named(&mut s, "c", "claude", "");
    assert!(!s.sync_agent_names(&listed(&[]), 0));
    assert_eq!(get(&s, "c").claude_name, None);
}

#[test]
fn a_rename_waiting_for_the_turn_to_end_isnt_undone_by_the_old_name() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.sync_agent_names(&listed(&[("a", "old")]), 0);
    s.set_name("a", "new");
    s.defer_rename("a");
    for t in [1_000, 60_000, 600_000] {
        assert!(!s.sync_agent_names(&listed(&[("a", "old")]), t));
        assert_eq!((get(&s, "a").name.as_str(), get(&s, "a").rename_error.as_str()), ("new", ""));
    }
    // The turn ends: delivered, then watched like any other.
    assert_eq!(s.take_rename("a").map(|(n, _)| n), Some("new".into()));
    s.expect_rename("a", "new", 700_000);
    s.sync_agent_names(&listed(&[("a", "new")]), 650_000);
    assert_eq!((get(&s, "a").name.as_str(), get(&s, "a").rename_error.as_str(), get(&s, "a").rename_check.is_none()), ("new", "", true));
}

#[test]
fn only_claude_sessions_on_this_mac_are_matched_with_its_list() {
    let mut s = Sessions::default();
    named(&mut s, "x", "codex", "");
    named(&mut s, "p", "pi", "");
    named(&mut s, "m", "claude", "box");
    for sid in ["x", "p", "m"] {
        s.set_name(sid, "mine");
    }
    assert!(!s.sync_agent_names(&listed(&[("x", "other"), ("p", "other"), ("m", "other")]), 0));
    assert!(["x", "p", "m"].iter().all(|sid| get(&s, sid).name == "mine"));
}

#[test]
fn a_new_rename_drops_one_still_on_its_way() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.set_name("a", "first");
    s.defer_rename("a");
    s.expect_rename("a", "first", 100);
    s.rename_failed("a", "x");
    s.set_name("a", "second");
    let a = get(&s, "a");
    assert_eq!((a.rename_pending, a.rename_check.is_none(), a.rename_error.as_str()), (false, true, ""));
    // A parked session coming back has no rename in flight.
    s.expect_rename("a", "second", 100);
    let back = Parked { parked_ms: 0, session: get(&s, "a").clone() }.into_session(Origin::default());
    assert!(back.rename_check.is_none() && back.name == "second");
}

#[test]
fn the_agent_name_and_rename_notice_survive_a_restart_but_a_check_in_flight_does_not() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.sync_agent_names(&listed(&[("a", "there")]), 0);
    s.rename_failed("a", "nope");
    s.expect_rename("a", "z", 5);
    let back: Session = serde_json::from_value(serde_json::to_value(get(&s, "a")).unwrap()).unwrap();
    assert_eq!((back.claude_name.as_deref(), back.rename_error.as_str(), back.rename_check.is_none()), (Some("there"), "nope", true));
}
