//! Parking, resuming and restarting (stability plan §4, Parking / restarting): what Park stops and keeps,
//! what it refuses, a restart that can't stop its agent, a restart that fails after its park (its tmux
//! session vanished), and what a relaunch after a reboot brings back by itself.

use crate::ipc::{move_to_terminal_now, park, rebuildable, restart_session_now};
use crate::testing::{AgentProcess, World};
use serde_json::json;

/// Session `sid` at its finished turn, its agent running as `agent`.
fn finished(w: &World, sid: &str, agent: &AgentProcess) {
    w.session(sid).running_as(agent.pid).prompt("go").stop("Done.");
}

fn parked_ids(w: &World) -> Vec<String> {
    w.snapshot().0["parked"].as_array().unwrap().iter().map(|p| p["session_id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn park_stops_its_agent_and_keeps_it_under_parked() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent);
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    assert_eq!(park(w.hub(), "a").unwrap(), "Parked: its agent stopped and its terminal closed");
    w.settle();
    assert!(!agent.running());
    let s = w.snapshot();
    assert!(s.find("a").is_none() && s.cards().is_empty());
    assert_eq!(parked_ids(&w), ["a"]);
    assert_eq!((s.history(&card)["status"].as_str(), s.history(&card)["outcome"].as_str()), (Some("gone"), Some("parked")), "not \"session ended\"");
}

#[test]
fn a_park_whose_agent_cant_be_stopped_leaves_the_session_and_its_card_as_they_were() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done."); // Cue never learned its process
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    assert_eq!(park(w.hub(), "a"), Err("Cue doesn't know this session's process".to_string()));
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.card("a")["id"], card.as_str(), "its finished turn still waits on you");
    assert_eq!(s.state("a"), "waiting");
    assert!(s.0["history"].as_array().unwrap().is_empty(), "nothing filed as parked: {}", s.0["history"]);
    assert!(parked_ids(&w).is_empty());
}

#[test]
fn park_refuses_a_session_at_work_and_leaves_its_agent_running() {
    let w = World::new();
    let agent = AgentProcess::start();
    w.session("a").running_as(agent.pid).prompt("go");
    assert_eq!(park(w.hub(), "a"), Err("it's working: stop it first, or let it finish".to_string()));
    assert!(agent.running());
    assert_eq!(w.snapshot().state("a"), "working");
}

#[test]
fn park_refuses_a_session_asking_you_something_and_its_ask_still_waits() {
    let w = World::new();
    let agent = AgentProcess::start();
    let ask = w.session("a").running_as(agent.pid).prompt("go").ask_bash("ls");
    assert_eq!(park(w.hub(), "a"), Err("it's asking you something: answer that first".to_string()));
    assert!(ask.told().is_none() && agent.running());
    assert_eq!(w.snapshot().card("a")["id"], ask.id.as_str());
}

#[test]
fn park_refuses_what_it_couldnt_resume() {
    let w = World::new();
    w.session("a").in_folder(std::path::Path::new("")).prompt("go").stop("Done.");
    assert_eq!(park(w.hub(), "a"), Err("Cue doesn't know which folder it runs in, so it couldn't resume it".to_string()));
    assert_eq!(park(w.hub(), "nobody"), Err("Cue has no record of that session".to_string()));
    assert_eq!(w.snapshot().cards_of("a").len(), 1);
}

#[test]
fn resumed_from_parked_it_is_back_at_its_prompt_as_it_was() {
    let w = World::new();
    w.session("a").prompt("fix the login").stop("Fixed.");
    w.hub().rename("a", "login").unwrap();
    w.hub().add_parked(w.hub().session_record("a").unwrap());
    let p = w.hub().parked("a").unwrap();
    let back = crate::model::Origin { session_id: "a".into(), harness: "claude".into(), cwd: "/work/a".into(), tmux_pane: "%5".into(), ..Default::default() };
    w.hub().unparked(p, back, "");
    w.settle();
    let s = w.snapshot();
    assert_eq!((s.state("a").as_str(), s.session("a")["name"].as_str(), s.session("a")["tmux_pane"].as_str()), ("waiting", Some("login"), Some("%5")));
    assert!(s.thread("a").contains(&("agent".to_string(), "Fixed.".to_string())), "its chat as it was: {:?}", s.thread("a"));
    assert!(parked_ids(&w).is_empty());
}

#[test]
fn resumed_with_a_message_it_is_at_work_on_it() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().add_parked(w.hub().session_record("a").unwrap());
    let origin = crate::model::Origin { session_id: "a".into(), harness: "claude".into(), cwd: "/work/a".into(), ..Default::default() };
    w.hub().unparked(w.hub().parked("a").unwrap(), origin, "now the tests");
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.state("a"), "working");
    assert_eq!(s.last_yours("a")["text"], "now the tests");
}

#[test]
fn unparked_it_is_off_parked_for_good() {
    let mut w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().add_parked(w.hub().session_record("a").unwrap());
    assert!(w.hub().take_parked("a").is_some());
    w.relaunch();
    assert!(parked_ids(&w).is_empty());
    assert!(w.hub().take_parked("a").is_none(), "taken once");
}

#[test]
fn a_parked_session_you_resumed_in_a_terminal_yourself_leaves_parked() {
    let mut w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().add_parked(w.hub().session_record("a").unwrap());
    w.session("a").prompt("back by hand");
    assert!(parked_ids(&w).is_empty(), "it's live");
    w.relaunch();
    assert!(parked_ids(&w).is_empty(), "and saved so");
}

#[test]
fn restart_refuses_a_session_at_work_or_one_it_cant_resume() {
    let w = World::new();
    w.session("a").prompt("go");
    assert_eq!(restart_session_now(w.hub(), "a"), Err("it's mid-turn; try when it's idle".to_string()));
    w.session("b").in_folder(std::path::Path::new("")).prompt("go").stop("Done.");
    assert_eq!(restart_session_now(w.hub(), "b"), Err("Cue can't stop and resume this one".to_string()));
    assert_eq!(restart_session_now(w.hub(), "nobody"), Err("Cue has no record of that session".to_string()));
    assert_eq!(w.snapshot().0["restarting"], json!({}));
}

#[test]
fn a_restart_whose_agent_cant_be_stopped_uncovers_its_pane_and_says_why_in_its_chat() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    let card = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    assert!(restart_session_now(w.hub(), "a").is_err());
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.0["restarting"], json!({}), "its pane isn't left covered");
    assert_eq!(s.0["restarted"]["a"][0]["error"], "Cue doesn't know this session's process");
    assert_eq!(s.card("a")["id"], card.as_str());
}

#[test]
fn a_restart_that_fails_after_its_park_leaves_it_under_parked_saying_why() {
    let w = World::new();
    w.session("a").prompt("go").stop("Done.");
    w.hub().restart_begin("a", None, false);
    w.hub().add_parked(w.hub().session_record("a").unwrap());
    w.hub().restart_step("a", "start");
    let s = w.snapshot();
    assert_eq!(s.0["restarting"]["a"]["step"], "start");
    assert!(s.find("a").is_some(), "in its place while its agent is down, not on Parked");
    assert!(parked_ids(&w).is_empty());
    // Its tmux session vanished as it was being started again.
    w.hub().restart_end("a", Some("can't find session: a"));
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.0["restarting"], json!({}));
    assert_eq!(parked_ids(&w), ["a"], "where park left it: Resume brings it back");
    assert_eq!(s.0["restarted"]["a"][0]["error"], "can't find session: a");
}

#[test]
fn a_session_keeps_its_last_five_restarts() {
    let w = World::at(1_800_000_000_000);
    w.session("a").prompt("go").stop("Done.");
    for i in 0..7 {
        w.hub().restart_begin("a", None, false);
        w.advance(1000 * (i + 1));
        w.hub().restart_end("a", None);
    }
    w.settle();
    let lines = w.snapshot().0["restarted"]["a"].as_array().unwrap().clone();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines.iter().map(|l| l["secs"].as_u64().unwrap()).collect::<Vec<_>>(), [3, 4, 5, 6, 7], "the latest, each with how long it took");
}

#[test]
fn after_a_reboot_cue_brings_back_its_claude_sessions_in_tmux() {
    let mut w = World::new();
    for sid in ["mine", "E"] {
        w.session(sid).in_tmux("%1").prompt("go").stop("Done.");
    }
    w.session("plain").prompt("go").stop("Done.");
    w.session("cx").codex().in_tmux("%2").prompt("go").stop("Done.");
    w.relaunch();
    let mut back: Vec<String> = w.hub().saved_sessions().into_iter().filter(|s| rebuildable(&s.origin)).map(|s| s.origin.session_id).collect();
    back.sort();
    assert_eq!(back, ["E", "mine"], "not a session with no pane, nor one only Claude can --resume");
}

/// ⋯ → Move to terminal for a session in Cue's own terminal (ipc/move_out.rs): its agent ended as Close does, then
/// resumed in a tab of your terminal app, with its name and star.
#[test]
fn move_to_terminal_takes_a_session_out_of_cues_terminal_into_a_tab_of_yours() {
    let w = World::new();
    let agent = AgentProcess::start();
    w.session("a").running_as(agent.pid).in_tmux("%7").prompt("go").stop("Done.");
    // Named as + New names it (a rename is typed into its terminal, which this test's terminal echoes as a message).
    w.hub().name_new("a", "billing");
    w.hub().set_starred("a", true);
    let said = move_to_terminal_now(w.hub(), "a").unwrap();
    assert!(said.starts_with("Moved to"), "{said}");
    w.settle();
    assert!(!agent.running(), "its agent in Cue's terminal ended");
    let l = w.launcher.last();
    assert!(l.line.contains("claude --resume 'a'"), "{}", l.line);
    assert_eq!(l.place, Some(crate::terminal::launch::Place::Tab), "a plain tab of your terminal app, not one Cue's tmux keeps");
    let s = w.snapshot();
    assert_eq!(s.session("a")["name"], "billing");
    assert_ne!(w.hub().session_origin("a").unwrap().tmux_pane, "%7", "not in Cue's terminal any more");
    assert!(w.hub().session_star("a") > 0, "starred still");
}

#[test]
fn move_to_terminal_leaves_a_session_already_in_a_terminal_of_yours_where_it_is() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent);
    assert_eq!(move_to_terminal_now(w.hub(), "a"), Err("it already runs in a terminal of yours".to_string()));
    assert!(agent.running());
}
