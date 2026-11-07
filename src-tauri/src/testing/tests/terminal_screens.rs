//! What Cue reads off a session's screen before its agent says anything (stability plan §4, Sessions): Claude
//! Code's trust question and its "Press Enter to continue" page, read through the terminal port, so the
//! FakeTerminal's screen is what a tick sees.

use crate::model::Origin;
use crate::testing::World;

/// A Claude Code session Cue started in tmux pane %7, no hook from it yet.
fn started(w: &World, sid: &str) {
    let o = Origin { session_id: sid.into(), harness: "claude".into(), cwd: format!("/work/{sid}"), tmux_pane: "%7".into(), term_program: "tmux".into(), ..Default::default() };
    w.hub().started(o, "", &[], false);
    w.settle();
}

fn setup_ms(w: &World, sid: &str) -> (u64, u64) {
    let s = w.snapshot();
    let s = s.session(sid);
    (s["trust_ms"].as_u64().unwrap_or(0), s["enter_ms"].as_u64().unwrap_or(0))
}

const TRUST: &str = "Do you trust the files in this folder?\n\n❯ 1. Yes, I trust this folder\n  2. No, exit\n\nEnter to confirm · Esc to cancel";

#[test]
fn a_trust_question_on_a_new_sessions_screen_shows_until_it_is_answered() {
    let w = World::new();
    started(&w, "a");
    w.terminal.pane("a", |p| p.screen = Some(TRUST.into()));
    w.tick(false);
    let (trust, enter) = setup_ms(&w, "a");
    assert!(trust > 0 && enter == 0, "asking about the folder: {:?}", (trust, enter));
    w.advance(5_000);
    w.tick(false);
    assert_eq!(setup_ms(&w, "a").0, trust, "since when it asked stays as it was");
    w.terminal.pane("a", |p| p.screen = Some("╭───╮\n│ > │\n╰───╯".into()));
    w.tick(false);
    assert_eq!(setup_ms(&w, "a"), (0, 0), "answered in its terminal: no longer asking");
}

#[test]
fn a_press_enter_page_shows_as_waiting_for_enter_and_an_unreadable_screen_changes_nothing() {
    let w = World::new();
    started(&w, "a");
    w.terminal.pane("a", |p| p.screen = Some("Security notes:\n  1. Claude can make mistakes\n\nPress Enter to continue…".into()));
    w.tick(false);
    let (trust, enter) = setup_ms(&w, "a");
    assert!(trust == 0 && enter > 0, "{:?}", (trust, enter));
    w.terminal.pane("a", |p| p.screen = None);
    w.tick(false);
    assert_eq!(setup_ms(&w, "a").1, enter, "a pane Cue can't read leaves what it last saw");
}
