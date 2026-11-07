//! Starting a session's agent (stability plan §4, Sessions / Hand-offs / Parking): + New, Continue in a new
//! session, Start fresh, Move to Cue, and a pane rebuilt after a restart, each through the launcher port. FakeLauncher
//! records the command line Cue would have typed and gives the new session a pane, so the success paths can
//! be followed through to what the window shows; a launcher that refuses leaves everything as it was.

use crate::ipc::{handoff_continue_now, handoff_fresh_now, move_session_to_cue, rebuild, start_session_with, StartOpts};
use crate::terminal::launch::Place;
use crate::testing::{AgentProcess, World};

/// Session `sid` at its finished turn in the World's own folder (a real one, as + New needs), its agent
/// running as `agent`, its transcript ending in `reply`.
fn finished(w: &World, sid: &str, agent: &AgentProcess, reply: &str) {
    let a = w.session(sid).in_folder(w.home()).running_as(agent.pid);
    a.transcript().prompt("go").assistant(reply).turn_end();
    a.prompt("go").stop(reply);
}

fn pane_of(w: &World, sid: &str) -> String {
    w.hub().session_origin(sid).map(|o| o.tmux_pane).unwrap_or_default()
}

#[test]
fn a_new_session_starts_its_agent_with_its_message_and_shows_in_the_pane_it_was_given() {
    let w = World::new();
    let opts = StartOpts { tmux: Some(true), ..Default::default() };
    let started = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "fix the login", "bugs", &opts).unwrap();
    let sid = started["session_id"].as_str().unwrap().to_string();
    let l = w.launcher.last();
    let dir = w.home().to_string_lossy().into_owned();
    assert_eq!(l.line, format!("cd '{dir}' && claude --session-id '{sid}' -n 'bugs' 'fix the login'"));
    assert_eq!((l.dir.as_str(), l.name.as_str(), l.place), (dir.as_str(), "bugs", Some(Place::chosen(Some(true)))));
    w.settle();
    assert_eq!(w.snapshot().session(&sid)["name"], "bugs");
    assert_eq!(pane_of(&w, &sid), l.pane, "Cue types into the pane it started it in");
}

#[test]
fn a_new_session_whose_terminal_wont_open_says_why_and_adds_nothing() {
    let w = World::new();
    w.launcher.refuse("tmux couldn't open a window");
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "fix it", "", &StartOpts::default());
    assert_eq!(r, Err("tmux couldn't open a window".to_string()));
    assert!(w.snapshot().0["sessions"].as_array().is_none_or(|s| s.is_empty()));
}

#[test]
fn continue_in_a_new_session_starts_it_on_the_hand_off_and_closes_the_old_one() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Note in HANDOFF.md.");
    crate::db::handoff_sent("a", "handoff", true, 0);
    let r = handoff_continue_now(w.hub(), "a").unwrap();
    w.settle();
    let new_id = r["session_id"].as_str().unwrap().to_string();
    let l = w.launcher.last();
    assert!(l.line.contains(&format!("--session-id '{new_id}'")) && l.line.ends_with(" '/handoff resume'"), "its first message is the skill's resume: {}", l.line);
    assert_eq!(r["closed"], true, "{r}");
    assert!(!agent.running(), "the old one's agent ended");
    assert_eq!(crate::db::handoff("a").map(|k| k.next), Some(new_id.clone()), "where it continued is kept");
    assert_eq!(pane_of(&w, &new_id), l.pane);
}

#[test]
fn start_fresh_starts_a_new_session_from_the_last_reply_and_a_refused_start_keeps_the_old_one() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "The login works now; the tests pass.");
    w.launcher.refuse("tmux couldn't open a window");
    assert_eq!(handoff_fresh_now(w.hub(), "a"), Err("tmux couldn't open a window".to_string()));
    assert!(agent.running() && w.snapshot().find("a").is_some(), "nothing ended when nothing started");
    w.launcher.accept();
    let r = handoff_fresh_now(w.hub(), "a").unwrap();
    w.settle();
    let l = w.launcher.last();
    assert!(l.line.contains("The login works now; the tests pass."), "started from what it last said: {}", l.line);
    assert_eq!((r["closed"].as_bool(), agent.running()), (Some(true), false));
    assert_eq!(pane_of(&w, r["session_id"].as_str().unwrap()), l.pane);
}

#[test]
fn after_a_restart_a_session_whose_pane_died_is_resumed_in_a_new_one_with_its_card() {
    let mut w = World::new();
    w.session("a").in_tmux("%40").prompt("go").stop("Done.");
    w.relaunch();
    let saved = w.hub().saved_sessions().into_iter().find(|s| s.origin.session_id == "a").unwrap();
    rebuild(w.hub(), &saved.origin, "work-a");
    w.settle();
    let l = w.launcher.last();
    assert!(l.line.starts_with("cd '/work/a' && claude --resume 'a'"), "{}", l.line);
    assert_eq!((l.name.as_str(), l.place), ("work-a", Some(Place::Tmux)));
    assert_eq!(pane_of(&w, "a"), l.pane, "the new pane, not the one the reboot took");
    let s = w.snapshot();
    assert_eq!(s.state("a"), "waiting");
    assert_eq!(s.cards_of("a").len(), 1, "its finished turn is still the turn to answer");
}

#[test]
fn move_to_cue_opens_its_window_first_then_ends_the_old_agent_and_resumes_it_there() {
    if !crate::focus::tmux_installed() {
        return;
    }
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    // Claude Code up in the new pane: its prompt on the screen.
    w.terminal.pane("a", |p| p.screen = Some("❯ ".into()));
    let said = move_session_to_cue(w.hub(), "a").unwrap();
    assert_eq!(said, "Moved to Cue: resumed in its terminal here");
    let opened = w.launcher.last();
    assert_eq!((opened.line.as_str(), opened.place), ("", Some(Place::Tmux)), "its window opened empty, before anything ended");
    let typed = w.launcher.typed();
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].0, opened.pane);
    assert!(typed[0].1.starts_with(&format!("cd '{}' && claude --resume 'a'", w.home().display())), "{}", typed[0].1);
    assert!(!agent.running() && w.launcher.closed().is_empty());
    assert_eq!(pane_of(&w, "a"), opened.pane);
    assert_eq!(w.snapshot().state("a"), "waiting");
}

#[test]
fn move_to_cue_that_cant_open_its_window_leaves_the_session_running_where_it_was() {
    if !crate::focus::tmux_installed() {
        return;
    }
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    w.launcher.refuse("tmux couldn't open a window");
    assert_eq!(move_session_to_cue(w.hub(), "a"), Err("tmux couldn't open a window".to_string()));
    assert!(agent.running() && pane_of(&w, "a").is_empty());
}
