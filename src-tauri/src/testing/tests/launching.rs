//! Starting a session's agent (stability plan §4, Sessions / Hand-offs / Parking): + New, Continue in a new
//! session, Start fresh, Fork, Resume and Move to Cue, each hosted (FakeHosted records what it was asked to start),
//! and, at launch, sessions left in Cue's old terminal moved to hosted. A start that's refused leaves everything as it was.

use crate::hosted::Start;
use crate::ipc::{fork, handoff_continue_now, handoff_fresh_now, move_between_turns, launch_moves, move_session_to_cue, park, resume_parked_session, start_session_with, StartOpts};
use crate::testing::{AgentProcess, World};

/// Session `sid` at its finished turn in the World's own folder (a real one, as + New needs), its agent
/// running as `agent`, its transcript ending in `reply`.
fn finished(w: &World, sid: &str, agent: &AgentProcess, reply: &str) {
    let a = w.session(sid).in_folder(w.home()).running_as(agent.pid);
    a.transcript().prompt("go").assistant(reply).turn_end();
    a.prompt("go").stop(reply);
}

#[test]
fn a_new_session_starts_hosted_with_its_message_and_name() {
    let w = World::new();
    let opts = StartOpts { tmux: Some(true), ..Default::default() };
    let started = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "fix the login", "bugs", &opts).unwrap();
    let sid = started["session_id"].as_str().unwrap().to_string();
    let l = w.hosted.launched().last().cloned().expect("started hosted");
    assert_eq!((l.start, l.cwd.as_str()), (Start::New(sid.clone()), w.home().to_string_lossy().as_ref()));
    assert!(w.launcher.launched().is_empty(), "no terminal, though it asked for Cue's");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "fix the login".to_string())));
    w.settle();
    assert_eq!(w.snapshot().session(&sid)["name"], "bugs");
    assert!(w.hub().session_origin(&sid).is_some_and(|o| o.hosted));
}

#[test]
fn a_new_session_whose_agent_wont_start_says_why_and_adds_nothing() {
    let w = World::new();
    w.hosted.refuse("claude couldn't start");
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "fix it", "", &StartOpts::default());
    assert_eq!(r, Err("claude couldn't start".to_string()));
    assert!(w.snapshot().0["sessions"].as_array().is_none_or(|s| s.is_empty()));
}

#[test]
fn continue_in_a_new_session_starts_it_hosted_on_the_hand_off_and_closes_the_old_one() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Note in HANDOFF.md.");
    crate::db::handoff_sent("a", "handoff", true, 0);
    let r = handoff_continue_now(w.hub(), "a").unwrap();
    w.settle();
    let new_id = r["session_id"].as_str().unwrap().to_string();
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::New(new_id.clone())), "hosted, though the old one ran in a terminal");
    let first = w.hosted.sent().into_iter().filter(|(s, _)| *s == new_id).map(|(_, t)| t).next().unwrap_or_default();
    assert!(first.ends_with("/handoff resume"), "its first message is the skill's resume: {first}");
    assert_eq!(r["closed"], true, "{r}");
    assert!(!agent.running(), "the old one's agent ended");
    assert_eq!(crate::db::handoff("a").map(|k| k.next), Some(new_id.clone()), "where it continued is kept");
    assert!(w.launcher.launched().is_empty());
}

#[test]
fn start_fresh_starts_a_hosted_session_from_the_last_reply_and_a_refused_start_keeps_the_old_one() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "The login works now; the tests pass.");
    w.hosted.refuse("claude couldn't start");
    assert_eq!(handoff_fresh_now(w.hub(), "a"), Err("claude couldn't start".to_string()));
    assert!(agent.running() && w.snapshot().find("a").is_some(), "nothing ended when nothing started");
    w.hosted.accept();
    let r = handoff_fresh_now(w.hub(), "a").unwrap();
    w.settle();
    let new_id = r["session_id"].as_str().unwrap().to_string();
    let first = w.hosted.sent().into_iter().filter(|(s, _)| *s == new_id).map(|(_, t)| t).next().unwrap_or_default();
    assert!(first.contains("The login works now; the tests pass."), "started from what it last said: {first}");
    assert_eq!((r["closed"].as_bool(), agent.running()), (Some(true), false));
    assert!(w.hub().session_origin(&new_id).is_some_and(|o| o.hosted));
}

#[test]
fn a_fork_starts_hosted_as_a_named_copy_of_its_session() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    w.hub().name_new("a", "billing");
    let r = fork(w.hub(), "a", "try it the other way").unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    assert_eq!(r["name"], "billing-fork");
    let l = w.hosted.launched().last().cloned().expect("forked hosted");
    assert_eq!(l.start, Start::New(sid.clone()));
    let at = l.args.iter().position(|a| a == "--resume").expect("from its session");
    assert_eq!(&l.args[at..at + 5], ["--resume", "a", "--fork-session", "-n", "billing-fork"]);
    assert!(w.launcher.launched().is_empty() && agent.running(), "no terminal, and the session it forks goes on");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "try it the other way".to_string())));
}

#[test]
fn a_fork_closed_before_its_conversation_is_written_comes_back_as_the_fork_again() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    w.hub().name_new("a", "billing");
    let sid = fork(w.hub(), "a", "").unwrap()["session_id"].as_str().unwrap().to_string();
    let o = w.hub().session_origin(&sid).unwrap();
    assert!(!std::path::Path::new(&o.transcript_path).exists(), "Claude hasn't written the fork yet");
    w.hub().resume_hosted_as(&o).unwrap();
    let again = w.hosted.launched().last().cloned().unwrap();
    assert_eq!(again.start, Start::New(sid.clone()), "not resumed: there's nothing under its id yet");
    assert!(again.args.windows(5).any(|a| a == ["--resume", "a", "--fork-session", "-n", "billing-fork"]), "forked from its session again, named: {:?}", again.args);
}

#[test]
fn a_fork_once_written_resumes_as_itself_and_is_no_longer_kept_as_a_fork() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    let sid = fork(w.hub(), "a", "").unwrap()["session_id"].as_str().unwrap().to_string();
    assert!(crate::hub::hosted::fork_entry(&sid).is_some(), "kept until it's written");
    let o = w.hub().session_origin(&sid).unwrap();
    let log = std::path::Path::new(&o.transcript_path);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(log, "{}\n").unwrap();
    w.hub().resume_hosted_as(&o).unwrap();
    let again = w.hosted.launched().last().cloned().unwrap();
    assert_eq!(again.start, Start::Resume(sid.clone()));
    assert!(!again.args.iter().any(|a| a == "--fork-session"), "{:?}", again.args);
    assert!(crate::hub::hosted::fork_entry(&sid).is_none(), "its entry went");
}

#[test]
fn a_fork_of_a_fork_not_yet_written_copies_the_first_written_conversation() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    let first = fork(w.hub(), "a", "").unwrap()["session_id"].as_str().unwrap().to_string();
    let second = fork(w.hub(), &first, "").unwrap()["session_id"].as_str().unwrap().to_string();
    let l = w.hosted.launched().last().cloned().unwrap();
    assert_eq!(l.start, Start::New(second.clone()));
    assert!(l.args.windows(3).any(|a| a == ["--resume", "a", "--fork-session"]), "from a, the written one: {:?}", l.args);
}

#[test]
fn a_fork_that_wont_start_leaves_nothing_kept() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    w.hosted.refuse("claude couldn't start");
    assert_eq!(fork(w.hub(), "a", "").unwrap_err(), "claude couldn't start");
    let kept = crate::config::load().pointer("/hosted/forked_from").cloned().unwrap_or_default();
    assert!(kept.as_object().is_none_or(|m| m.is_empty()), "{kept}");
}

#[test]
fn resume_brings_back_a_closed_terminal_session_hosted() {
    let w = World::new();
    let agent = AgentProcess::start();
    finished(&w, "a", &agent, "Done.");
    park(w.hub(), "a").unwrap();
    resume_parked_session(w.hub(), "a", "pick it up").unwrap();
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume("a".into())));
    assert!(w.launcher.launched().is_empty(), "no terminal");
    assert!(w.hub().session_origin("a").is_some_and(|o| o.hosted && o.tmux_pane.is_empty()));
    assert_eq!(w.hosted.sent().last(), Some(&("a".to_string(), "pick it up".to_string())));
}

/// Launch (ipc/restore.rs): a session still running in Cue's old terminal moves to hosted with its conversation, once
/// it's between turns: one mid-turn, or with a prompt open in its terminal, is left until that's over. One in your own
/// tmux, or whose agent ended, isn't touched.
#[test]
fn at_launch_sessions_in_cues_terminal_move_to_hosted_each_once_its_turn_is_over() {
    let mut w = World::new();
    let (idle, busy) = (AgentProcess::start(), AgentProcess::start());
    let t = w.session("t").in_folder(w.home()).in_tmux("%990").running_as(idle.pid);
    t.transcript().prompt("go").assistant("Done.").turn_end();
    t.prompt("go").stop("Done.");
    let b = w.session("b").in_folder(w.home()).in_tmux("%991").running_as(busy.pid);
    b.transcript().prompt("go").assistant("On it.");
    b.prompt("go");
    w.session("yours").in_folder(w.home()).in_tmux("%992").prompt("go").stop("Done.");
    w.session("gone").in_folder(w.home()).in_tmux("%993").prompt("go").stop("Done.");
    w.relaunch();
    let running: std::collections::HashSet<String> = ["t", "b", "yours"].map(String::from).into();
    let mut sids = launch_moves(w.hub().saved_sessions(), &running, |pane, _| pane != "%992");
    sids.sort();
    assert_eq!(sids, ["b", "t"], "not one in your own tmux, nor one whose agent ended");
    let left = move_between_turns(w.hub(), sids);
    assert_eq!(left, ["b"], "mid-turn: left to finish");
    assert!(!idle.running() && busy.running());
    assert!(w.hub().session_origin("t").is_some_and(|o| o.hosted && o.tmux_pane.is_empty()), "moved, its pane gone with it");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume("t".into())), "resumed: the same conversation");
    let b = w.session("b").in_folder(w.home()).in_tmux("%991").running_as(busy.pid);
    b.terminal_ask("Allow Bash: rm -rf build?");
    let left = move_between_turns(w.hub(), left);
    assert_eq!(left, ["b"], "a prompt open in its terminal: left until it's answered");
    b.stop("Done.");
    assert!(move_between_turns(w.hub(), left).is_empty());
    assert!(!busy.running(), "its turn over, it's moved");
    assert!(w.hub().session_origin("b").is_some_and(|o| o.hosted));
    assert!(w.hub().session_origin("yours").is_some_and(|o| !o.hosted), "yours is left where it is");
}

#[test]
fn move_to_cue_takes_a_session_from_cues_own_terminal_into_hosting() {
    let w = World::new();
    let agent = AgentProcess::start();
    let a = w.session("t").in_folder(w.home()).in_tmux("%990").running_as(agent.pid);
    a.transcript().prompt("go").assistant("Done.").turn_end();
    a.prompt("go").stop("Done.");
    w.hub().name_new("t", "mine");
    assert_eq!(move_session_to_cue(w.hub(), "t").unwrap(), "Moved to Cue: hosted here now");
    assert!(!agent.running(), "the one in Cue's terminal ended first");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(crate::hosted::Start::Resume("t".into())));
    let o = w.hub().session_origin("t").unwrap();
    assert!(o.hosted && o.tmux_pane.is_empty(), "hosted, its pane gone with it");
    assert_eq!(w.snapshot().session("t")["name"], "mine");
    // Already hosted: nothing more to move.
    assert_eq!(move_session_to_cue(w.hub(), "t").unwrap_err(), "it's already hosted in Cue");
}

#[test]
fn move_to_cue_hosts_a_session_on_another_machine_on_that_machine() {
    let w = World::new();
    crate::machines::add("box", "").unwrap();
    w.session("m").in_folder(w.home()).in_tmux("%991").on_machine("box").prompt("go").stop("Done.");
    assert_eq!(move_session_to_cue(w.hub(), "m").unwrap(), "Moved: hosted on box now");
    let l = w.hosted.launched().last().cloned().expect("started hosted");
    assert_eq!((l.start, l.machine.as_str()), (crate::hosted::Start::Resume("m".into()), "box"), "resumed there, on its machine");
    let o = w.hub().session_origin("m").unwrap();
    assert!(o.hosted && o.tmux_pane.is_empty() && o.machine == "box", "{o:?}");
}
