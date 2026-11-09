//! Sessions Cue hosts (docs/dev/cue-hosted-sessions.md §4.3, §4.4): started by + New with Settings →
//! Cue-hosted sessions on, what each reports entering as a hook's message would, what's sent to it going
//! straight to it (FakeHosted records it), and its life: paused when its Claude ends, started again by your
//! next message, closed, parked, moved in from a terminal and out to one.

use crate::hosted::{Answer, Ask, AskKind, HostedEvent, InputAction, Start};
use crate::ipc::{end_session, move_hosted_to_terminal, move_session_to_cue, park, resume_parked_session, start_session_with, StartOpts};
use crate::testing::{until, AgentProcess, Transcript, World};
use serde_json::{json, Value};

fn hosted_on(on: bool) {
    crate::config::set("hosted.on", json!(on)).unwrap();
}

/// A hosted session started by + New in the World's folder, with `message`: its id.
fn started(w: &World, message: &str) -> String {
    hosted_on(true);
    let opts = StartOpts { hosted: true, perm: "auto".into(), extra: "--model opus".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), message, "trial", &opts).unwrap();
    r["session_id"].as_str().unwrap().to_string()
}

/// Claude takes `text` (the message Cue handed it), and has written its conversation, as it does from its first
/// message on: a resume finds it.
fn takes(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::Taken { text: text.into() });
    let log = w.hub().session_origin(sid).unwrap().transcript_path;
    if !std::path::Path::new(&log).is_file() {
        Transcript::at(&log).prompt(text);
    }
}

/// A hosted session started with `message`, which Claude has taken (its turn under way): its id.
fn running(w: &World, message: &str) -> String {
    let sid = started(w, message);
    takes(w, &sid, message);
    sid
}

/// `sid` at the end of a turn that said `reply`.
fn turn_ends(w: &World, sid: &str, reply: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: reply.into(), failed: false, cost_usd: 0.25, unprompted: false });
    w.settle();
}

fn ask(kind: AskKind, request_id: &str) -> HostedEvent {
    let (tool_name, input) = match kind {
        AskKind::Input => (String::new(), Value::Null),
        AskKind::Question => ("AskUserQuestion".into(), json!({ "questions": [{ "question": "Which?", "options": [{ "label": "A" }, { "label": "B" }] }] })),
        _ => ("Bash".into(), json!({ "command": "rm -rf build" })),
    };
    HostedEvent::Ask(Ask { request_id: request_id.into(), kind, tool_name, input, suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: "forms".into(), message: "Your name?".into(), form: json!({ "type": "object", "properties": { "name": { "type": "string" } } }) })
}

#[test]
fn with_the_setting_off_nothing_starts_hosted() {
    let w = World::new();
    let opts = StartOpts { hosted: true, ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", "", &opts);
    assert!(r.is_err(), "{r:?}");
    assert!(w.hosted.launched().is_empty() && w.launcher.launched().is_empty());
}

#[test]
fn a_new_hosted_session_starts_claude_with_no_terminal_and_hands_it_its_first_message() {
    let w = World::new();
    let sid = started(&w, "fix the login");
    let l = w.hosted.launched();
    assert_eq!(l.len(), 1);
    assert_eq!((&l[0].start, l[0].permissions.as_str(), l[0].args.clone()), (&Start::New(sid.clone()), "auto", vec!["--model".to_string(), "opus".to_string()]));
    assert!(w.launcher.launched().is_empty(), "no terminal");
    assert_eq!(w.hosted.sent(), vec![(sid.clone(), "fix the login".to_string())]);
    let o = w.hub().session_origin(&sid).unwrap();
    assert!(o.hosted && o.agent_pid.is_some() && o.tmux_pane.is_empty());
    assert!(o.transcript_path.ends_with(&format!("{sid}.jsonl")), "{}", o.transcript_path);
    let s = w.snapshot();
    assert_eq!(s.session(&sid)["name"], "trial");
    assert_eq!(s.last_yours(&sid)["pending"], true, "handed over, not yet taken");
    takes(&w, &sid, "fix the login");
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "working");
    assert!(s.last_yours(&sid)["pending"].is_null(), "taken");
}

#[test]
fn a_hosted_session_that_wont_start_says_why_and_adds_nothing() {
    let w = World::new();
    hosted_on(true);
    w.hosted.refuse("couldn't start Claude Code: not found");
    let opts = StartOpts { hosted: true, ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", "", &opts);
    assert_eq!(r, Err("couldn't start Claude Code: not found".to_string()));
    assert!(w.snapshot().session_ids().is_empty());
}

#[test]
fn a_message_sent_to_an_idle_one_goes_to_it_directly_and_starts_a_turn_when_taken() {
    let w = World::new();
    let sid = started(&w, "");
    w.send_to(&sid, "add a test");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "add a test".to_string())));
    assert!(w.terminal.typed(&sid).is_empty(), "nothing typed anywhere");
    w.hosted_says(&sid, HostedEvent::Taken { text: "add a test".into() });
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "working");
    let yours: Vec<_> = s.thread(&sid).into_iter().filter(|(r, t)| r == "you" && t == "add a test").collect();
    assert_eq!(yours.len(), 1, "one bubble for it: {:?}", s.thread(&sid));
    let last = s.last_yours(&sid);
    assert!(last["pending"].is_null() && last["unsent"].is_null(), "taken, nothing waiting on it (both are left out when false): {last}");
}

/// Claude says it started a turn the moment it reads your message, and repeats the message back only with its
/// first words, a second or more later: the message is taken at the first.
#[test]
fn a_message_to_an_idle_one_is_taken_when_its_turn_starts() {
    let w = World::new();
    let sid = started(&w, "");
    w.send_to(&sid, "add a test");
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "working");
    let last = s.last_yours(&sid);
    assert!(last["pending"].is_null() && last["unsent"].is_null(), "taken (both are left out when false): {last}");
    // A second message, sent before it repeats the first: kept in Cue (it's mid-turn), and the repeat of the
    // first takes nothing else.
    w.send_to(&sid, "and the docs");
    w.hosted_says(&sid, HostedEvent::Taken { text: "add a test".into() });
    let s = w.snapshot();
    assert_eq!(s.session(&sid)["held"]["text"], "and the docs", "{}", s.session(&sid));
    assert!(s.session(&sid)["queued"].is_null());
    let yours = s.thread(&sid).into_iter().filter(|(r, _)| r == "you").count();
    assert_eq!(yours, 1, "{:?}", s.thread(&sid));
}

/// A background task that finished starts a turn of its own: nothing of yours is on its way, nothing changes.
#[test]
fn a_turn_nothing_of_yours_started_takes_nothing() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Started the build in the background.");
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    assert_eq!(w.snapshot().state(&sid), "waiting", "at work only once it does something");
    w.hosted_says(&sid, HostedEvent::Said { text: "The build passed.".into(), agent: None });
    assert_eq!(w.snapshot().state(&sid), "working");
}

/// Once handed to Claude a message can't be taken back: mid-turn Cue keeps it (Esc or Edit brings it back to
/// the box), and it goes when the turn ends.
#[test]
fn a_message_sent_mid_turn_is_kept_in_cue_and_goes_when_the_turn_ends() {
    let w = World::new();
    let sid = running(&w, "long job");
    let before = w.hosted.sent().len();
    w.send_to(&sid, "also check the docs");
    assert_eq!(w.hosted.sent().len(), before, "not handed to Claude yet");
    assert_eq!(w.snapshot().session(&sid)["held"]["text"], "also check the docs");
    turn_ends(&w, &sid, "Done.");
    w.settle();
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "also check the docs".to_string())), "sent as the turn ended");
    assert!(w.snapshot().session(&sid)["held"].is_null());
}

#[test]
fn a_message_kept_mid_turn_can_be_taken_back() {
    let w = World::new();
    let sid = running(&w, "long job");
    let before = w.hosted.sent().len();
    w.send_to(&sid, "never mind");
    assert_eq!(w.hub().unhold(&sid).map(|e| e.text).as_deref(), Some("never mind"));
    turn_ends(&w, &sid, "Done.");
    w.settle();
    assert_eq!(w.hosted.sent().len(), before, "nothing went");
}

/// Send now (⌘↵) on a kept message: it's handed over and the turn stopped, so it reads it straight away.
#[test]
fn send_now_on_a_kept_message_hands_it_over_and_stops_the_turn() {
    let w = World::new();
    let sid = running(&w, "long job");
    w.send_to(&sid, "do this instead");
    w.hub().send_held(&sid, true).unwrap();
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "do this instead".to_string())));
    assert_eq!(w.hosted.interrupted(), [sid.clone()], "the turn it was on is stopped");
    let s = w.snapshot();
    assert!(s.session(&sid)["held"].is_null() && s.session(&sid)["queued"].is_null(), "it's reading it now: {}", s.session(&sid));
    // The stopped turn ends as Claude ends one it was told to stop, and the next starts with the message: taken
    // there, not seconds later when Claude repeats it, and the turn is about it.
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "[ede_diagnostic] stopped".into(), failed: true, cost_usd: 0.0, unprompted: false });
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    let s = w.snapshot();
    let last = s.last_yours(&sid);
    assert!(last["pending"].is_null(), "taken as its turn started: {last}");
    assert_eq!(s.session(&sid)["prompt"], "do this instead");
    assert_eq!(s.state(&sid), "working");
}

#[test]
fn a_turn_that_ends_is_a_finished_turn_card_with_its_cost() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "All done.");
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "waiting");
    assert_eq!(s.card(&sid)["kind"], "waiting");
    assert_eq!(w.hub().session_cost(&sid).unwrap_or(0.25), 0.25);
}

/// A Stop hook of yours sent it back to work (a checklist): the card shows the answer it gave before that,
/// and the hook's follow-up folds away under it, as for a terminal session.
#[test]
fn a_stop_hooks_follow_up_folds_under_the_answer() {
    let w = World::new();
    let sid = running(&w, "fix the token expiry");
    let log = w.hub().session_origin(&sid).unwrap().transcript_path;
    Transcript::at(&log)
        .assistant("Done: expiry fixed.")
        .entry(json!({ "type": "system", "subtype": "stop_hook_summary", "hookCount": 1 }))
        .entry(json!({ "type": "user", "isMeta": true, "message": { "role": "user", "content": "Stop hook feedback:\n[bash rules-check-stop.sh]: Unaudited edits this turn." } }))
        .assistant("1. SHAPE: fine.");
    turn_ends(&w, &sid, "1. SHAPE: fine.");
    let s = w.snapshot();
    let card = s.card(&sid);
    assert_eq!(card["message"], "Done: expiry fixed.");
    assert_eq!(card["followup"], "1. SHAPE: fine.");
}

#[test]
fn a_permission_is_a_card_whose_answer_goes_back_to_the_session() {
    let w = World::new();
    let sid = running(&w, "clean up");
    w.hosted_says(&sid, ask(AskKind::Permission, "r1"));
    let s = w.snapshot();
    let card = s.card(&sid);
    assert_eq!((card["kind"].as_str(), card["tool_name"].as_str(), s.state(&sid).as_str()), (Some("permission"), Some("Bash"), "deciding"));
    let id = card["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({ "behavior": "deny", "message": "use cargo clean" }));
    until("its answer", || !w.hosted.answers().is_empty());
    let (to, request, given) = w.hosted.answers().remove(0);
    assert_eq!((to.as_str(), request.as_str()), (sid.as_str(), "r1"));
    let Answer::Decision(d) = given else { panic!("{given:?}") };
    assert_eq!((d.behavior.as_str(), d.message.as_deref()), ("deny", Some("use cargo clean")));
}

#[test]
fn an_mcp_servers_form_is_an_input_card_answered_with_what_you_filled_in() {
    let w = World::new();
    let sid = running(&w, "sign me up");
    w.hosted_says(&sid, ask(AskKind::Input, "e1"));
    let card = w.snapshot().card(&sid).clone();
    assert_eq!((card["kind"].as_str(), card["tool_name"].as_str()), (Some("input"), Some("forms")));
    assert_eq!((card["tool_input"]["message"].as_str(), card["tool_input"]["form"]["type"].as_str()), (Some("Your name?"), Some("object")));
    w.answer(card["id"].as_str().unwrap(), json!({ "behavior": "allow", "answers": { "name": "Ada" } }));
    until("its answer", || !w.hosted.answers().is_empty());
    let Answer::Input { action, content } = w.hosted.answers().remove(0).2 else { panic!("not a form's answer") };
    assert_eq!((action, content), (InputAction::Accept, json!({ "name": "Ada" })));
}

#[test]
fn a_card_that_goes_unanswered_is_a_denial_so_claude_never_waits_on_it() {
    let w = World::new();
    let sid = running(&w, "go");
    w.hosted_says(&sid, ask(AskKind::Question, "q1"));
    let id = w.snapshot().card(&sid)["id"].as_str().unwrap().to_string();
    w.hub().finish(&id, "gone", "dismissed");
    until("its answer", || !w.hosted.answers().is_empty());
    let Answer::Decision(d) = w.hosted.answers().remove(0).2 else { panic!("not a decision") };
    assert_eq!(d.behavior, "deny");
}

#[test]
fn stop_interrupts_it_directly_and_presses_nothing() {
    let w = World::new();
    let sid = running(&w, "go");
    w.ask(json!({ "type": "interrupt", "session_id": sid }));
    assert_eq!(w.hosted.interrupted(), vec![sid.clone()]);
    assert!(w.terminal.keys(&sid).is_empty());
}

/// Claude ends a turn you stopped as an error ("[ede_diagnostic] …"): that's the stop, not a failure. No "Stopped
/// on an error" card, and it stays stopped though the step it was on reported back in between. A turn that
/// fails on its own still says so.
#[test]
fn a_turn_you_stopped_ends_stopped_not_on_an_error() {
    let w = World::new();
    let sid = running(&w, "go");
    w.ask(json!({ "type": "interrupt", "session_id": sid }));
    w.hosted_says(&sid, HostedEvent::ToolResult { id: "t1".into(), output: json!("[Request interrupted by user for tool use]"), failed: true, agent: None });
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=tool_use".into(), failed: true, cost_usd: 0.1, unprompted: false });
    w.settle();
    assert!(w.snapshot().cards_of(&sid).is_empty(), "no card: {:?}", w.snapshot().cards_of(&sid));
    assert_eq!(w.snapshot().state(&sid), "stopped");

    takes(&w, &sid, "again");
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "Overloaded".into(), failed: true, cost_usd: 0.0, unprompted: false });
    w.settle();
    assert_eq!(w.snapshot().card(&sid)["message"], "Stopped on an error: Overloaded", "nobody stopped this one");
}

/// Cue stopped mid-turn (a reinstall): the session comes back paused at its prompt, not still working, so your
/// next message starts it with a turn of its own instead of sitting as queued behind one that's gone.
#[test]
fn restarted_mid_turn_a_message_starts_it_rather_than_queueing() {
    let mut w = World::new();
    let sid = running(&w, "go");
    assert_eq!(w.snapshot().state(&sid), "working");
    w.relaunch();
    assert_ne!(w.snapshot().state(&sid), "working", "its turn ended with the Cue that ran it");
    assert_eq!(w.snapshot().session(&sid)["cut_by_restart"], true, "the chat says Cue ended it, not you");
    w.ask(json!({ "type": "send_to", "session_id": sid, "text": "what are you doing" }));
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume(sid.clone())));
    assert!(w.snapshot().session(&sid)["queued"].is_null(), "not queued: nothing is working on a turn");
    takes(&w, &sid, "what are you doing");
    assert!(w.snapshot().session(&sid)["cut_by_restart"].is_null(), "at work again: the note goes");
}

/// Stop, or Send now, while Claude is starting up with your message next in its input: nothing is interrupted
/// (that would end the turn your message starts before it begins).
#[test]
fn stop_while_it_starts_up_doesnt_end_the_turn_your_message_starts() {
    let mut w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    w.relaunch();
    w.ask(json!({ "type": "send_to", "session_id": sid, "text": "next" }));
    w.ask(json!({ "type": "interrupt", "session_id": sid }));
    assert!(w.hosted.interrupted().is_empty(), "Claude hasn't started a turn yet: {:?}", w.hosted.interrupted());
    // Once it has, Stop stops it.
    takes(&w, &sid, "next");
    w.ask(json!({ "type": "interrupt", "session_id": sid }));
    assert_eq!(w.hosted.interrupted(), vec![sid.clone()]);
}

#[test]
fn claude_exiting_mid_turn_leaves_it_paused_with_a_card_and_the_next_message_starts_it_again() {
    let w = World::new();
    let sid = running(&w, "go");
    w.hosted_says(&sid, ask(AskKind::Permission, "r1"));
    w.hosted.exits(&sid);
    w.hosted_says(&sid, HostedEvent::Exited { code: Some(3), during_turn: true, last_words: "fatal: out of memory\n".into() });
    w.settle();
    let s = w.snapshot();
    let card = s.card(&sid);
    assert_eq!(card["kind"], "waiting", "its ask went; the turn's end says why");
    assert!(card["message"].as_str().unwrap_or("").contains("out of memory"), "{card}");
    assert_eq!(w.hub().session_origin(&sid).unwrap().agent_pid, None, "paused");
    w.send_to(&sid, "try again");
    let l = w.hosted.launched();
    assert_eq!(l.last().map(|l| &l.start), Some(&Start::Resume(sid.clone())));
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "try again".to_string())));
    assert!(w.hub().session_origin(&sid).unwrap().agent_pid.is_some());
}

#[test]
fn after_a_relaunch_it_is_paused_kept_and_resumed_by_your_next_message() {
    let mut w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    w.relaunch();
    let o = w.hub().session_origin(&sid).expect("still there after the relaunch");
    assert!(o.hosted && o.agent_pid.is_none(), "paused: its Claude ended with the old Cue");
    // Claude Code's own list of running sessions never has a hosted one: that's no reason to end it.
    w.hub().sweep("", Some(&Default::default()));
    w.advance(60 * 60 * 1000);
    w.hub().sweep("", Some(&Default::default()));
    assert!(w.hub().session_origin(&sid).is_some());
    w.send_to(&sid, "next");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume(sid.clone())));
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "next".to_string())));
}

#[test]
fn close_ends_its_claude_and_the_session() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    end_session(w.hub(), &sid).unwrap();
    assert_eq!(w.hosted.closed(), vec![sid.clone()]);
    assert!(w.hub().session_origin(&sid).is_none());
}

#[test]
fn a_parked_hosted_session_resumes_hosted() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    park(w.hub(), &sid).unwrap();
    assert!(w.hub().parked(&sid).is_some());
    hosted_on(false);
    resume_parked_session(w.hub(), &sid, "pick it up").unwrap();
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume(sid.clone())), "hosted it was, hosted it stays");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "pick it up".to_string())));
    assert!(w.launcher.launched().is_empty());
    assert!(w.hub().session_origin(&sid).is_some_and(|o| o.hosted));
}

#[test]
fn move_to_cue_with_the_setting_on_resumes_a_terminal_session_hosted() {
    let w = World::new();
    hosted_on(true);
    let agent = AgentProcess::start();
    let a = w.session("t").in_folder(w.home()).running_as(agent.pid);
    a.transcript().prompt("go").assistant("Done.").turn_end();
    a.prompt("go").stop("Done.");
    w.hub().name_new("t", "mine");
    let said = move_session_to_cue(w.hub(), "t").unwrap();
    assert_eq!(said, "Moved to Cue: hosted here now");
    assert!(!agent.running(), "the terminal one ended first");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::Resume("t".into())));
    assert!(w.launcher.launched().is_empty(), "no tmux window");
    let o = w.hub().session_origin("t").unwrap();
    assert!(o.hosted && o.tty.is_empty());
    assert_eq!(w.snapshot().session("t")["name"], "mine");
}

#[test]
fn move_to_terminal_closes_it_and_resumes_it_in_a_tab_as_a_terminal_session() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    move_hosted_to_terminal(w.hub(), &sid).unwrap();
    assert_eq!(w.hosted.closed(), vec![sid.clone()]);
    let l = w.launcher.last();
    assert!(l.line.contains(&format!("claude --resume '{sid}'")), "{}", l.line);
    let o = w.hub().session_origin(&sid).unwrap();
    assert!(!o.hosted && o.agent_pid.is_none());
    assert_eq!(w.snapshot().session(&sid)["name"], "trial");
}

#[test]
fn its_card_isnt_taken_for_answered_in_a_terminal_it_doesnt_have() {
    let w = World::new();
    let sid = running(&w, "clean up");
    let log = Transcript::at(w.hub().session_origin(&sid).unwrap().transcript_path);
    log.tool_use("toolu_1", "Bash", json!({ "command": "rm -rf build" }));
    w.hosted_says(&sid, ask(AskKind::Permission, "r1"));
    // Cue's answer reached it a moment ago, and its transcript already says so.
    log.tool_result("toolu_1");
    w.tick(false);
    assert_eq!(w.snapshot().card(&sid)["kind"], "permission", "still the card you answer: only Cue answers it");
}

#[test]
fn the_watcher_leaves_a_hosted_session_to_its_own_exit_report() {
    let w = World::new();
    let sid = running(&w, "go");
    // The stand-in's process ids are no real process's: to the watcher, its Claude looks dead.
    w.tick(false);
    assert!(w.hub().session_origin(&sid).is_some(), "not dropped: an exit is the session's to report, and it pauses it");
}

#[test]
fn renaming_it_names_it_in_cue_and_types_nothing() {
    let w = World::new();
    let sid = started(&w, "");
    assert_eq!(w.hub().rename(&sid, "cue-hosted-test"), Ok("Renamed".to_string()));
    let row = w.snapshot().session(&sid).clone();
    assert_eq!(row["name"], "cue-hosted-test");
    assert!(row["rename_error"].is_null(), "no notice that it couldn't be told: {row}");
    assert!(w.terminal.typed(&sid).is_empty());
}

#[test]
fn one_paused_before_claude_wrote_anything_starts_new_under_its_id_not_a_resume_that_fails() {
    let mut w = World::new();
    let sid = started(&w, "");
    w.relaunch();
    w.send_to(&sid, "hello");
    assert_eq!(w.hosted.launched().last().map(|l| l.start.clone()), Some(Start::New(sid.clone())), "no conversation to resume yet");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "hello".to_string())));
}

#[test]
fn a_message_claude_exits_before_taking_is_marked_not_sent() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    w.send_to(&sid, "and the docs?");
    assert_eq!(w.snapshot().last_yours(&sid)["pending"], true);
    w.hosted.exits(&sid);
    w.hosted_says(&sid, HostedEvent::Exited { code: Some(1), during_turn: false, last_words: String::new() });
    let last = w.snapshot().last_yours(&sid).clone();
    assert_eq!((last["text"].as_str(), last["unsent"].as_bool()), (Some("and the docs?"), Some(true)), "Send again, not a message that silently went nowhere: {last}");
}

#[test]
fn a_reply_to_its_card_goes_straight_to_it_and_settles_when_taken() {
    let w = World::new();
    let sid = running(&w, "go");
    turn_ends(&w, &sid, "Done.");
    let id = w.snapshot().card(&sid)["id"].as_str().unwrap().to_string();
    let r = w.ask(json!({ "type": "reply", "id": id, "text": "next step", "images": [] }));
    assert_eq!(r["detail"], "sent", "{r}");
    assert!(w.snapshot().cards_of(&sid).is_empty(), "the card is answered");
    assert_eq!(w.snapshot().last_yours(&sid)["pending"], true, "on its way until Claude takes it");
    w.hosted_says(&sid, HostedEvent::Taken { text: "next step".into() });
    assert!(w.snapshot().last_yours(&sid)["pending"].is_null());
    assert_eq!(w.snapshot().state(&sid), "working");
}

/// A Pi session started hosted by + New with `message`: its id.
fn pi_started(w: &World, message: &str, perm: &str) -> String {
    hosted_on(true);
    let opts = StartOpts { hosted: true, perm: perm.into(), ..Default::default() };
    let r = start_session_with(w.hub(), "pi", &w.home().to_string_lossy(), message, "", &opts).unwrap();
    r["session_id"].as_str().unwrap().to_string()
}

#[test]
fn pi_starts_hosted_with_its_gate_as_chosen_and_takes_its_first_message() {
    let w = World::new();
    let sid = pi_started(&w, "add a test", "skip");
    let l = w.hosted.launched().pop().unwrap();
    assert_eq!((l.agent, l.permissions.as_str()), (crate::hosted::Agent::Pi, "skip"));
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "add a test".to_string())));
    let s = w.snapshot();
    assert_eq!(s.session(&sid)["harness"], "pi");
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    assert_eq!(w.snapshot().state(&sid), "working");
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "Added it.".into(), failed: false, cost_usd: 0.002, unprompted: false });
    w.settle();
    let s = w.snapshot();
    assert_eq!(s.state(&sid), "waiting");
    assert_eq!(s.card(&sid)["message"], "Added it.");
}

/// Its gate (Cue's Pi extension) asks over Cue's socket, as a terminal Pi's does: a card, answered there,
/// never closed as "answered in its terminal" (it has none).
#[test]
fn a_hosted_pis_gate_asks_on_a_card_and_gets_the_answer() {
    let w = World::new();
    let sid = pi_started(&w, "clean up", "ask");
    let pid = w.hub().session_origin(&sid).unwrap().agent_pid.unwrap();
    let ask = w.session(&sid).pi().hosted().running_as(pid).in_folder(w.home()).ask_bash("rm -rf build");
    w.advance(60_000);
    w.settle();
    assert!(ask.told().is_none(), "still waiting for you");
    assert!(w.snapshot().session(&sid)["hosted"].as_bool().unwrap_or(false), "still the hosted session");
    w.answer(&ask.id, json!({ "behavior": "allow" }));
    assert_eq!(ask.told().unwrap()["behavior"], "allow");
}

#[test]
fn a_paused_pi_picks_its_conversation_up_by_id_on_your_next_message() {
    let mut w = World::new();
    let sid = pi_started(&w, "", "");
    w.relaunch();
    w.send_to(&sid, "carry on");
    let l = w.hosted.launched().pop().unwrap();
    assert_eq!((l.agent, l.start), (crate::hosted::Agent::Pi, Start::Resume(sid.clone())), "Pi resumes by id even before it wrote anything");
    assert_eq!(w.hosted.sent().last(), Some(&(sid, "carry on".to_string())));
}

#[test]
fn a_message_the_agent_turns_down_offers_send_again_with_why() {
    let w = World::new();
    let sid = pi_started(&w, "", "");
    w.send_to(&sid, "carry on");
    w.hosted_says(&sid, HostedEvent::NotTaken { text: "carry on".into(), why: "No API key found for deepseek.".into() });
    let last = w.snapshot().last_yours(&sid);
    assert_eq!(last["unsent"], true, "{last}");
    assert!(last["delivery"].to_string().contains("No API key found"), "{last}");
}

/// Cue killed before it could close its hosted agents: the next Cue ends the processes they ran as, if they still
/// run on their own, before any is started again.
#[test]
fn the_next_cue_ends_what_its_hosted_sessions_ran_as() {
    let mut w = World::new();
    let a = pi_started(&w, "go", "");
    let b = running(&w, "go");
    let pids: Vec<i32> = [&a, &b].iter().map(|s| w.hub().session_origin(s).unwrap().agent_pid.unwrap()).collect();
    w.relaunch();
    let mut ended = w.hosted.ended_left();
    ended.sort();
    assert_eq!(ended, { let mut p = pids; p.sort(); p });
}

/// Codex picks a new conversation's id: the session is known by it from the start, with its rollout file, and its
/// model goes in what Cue sends it rather than on its command line.
#[test]
fn codex_starts_hosted_under_the_id_it_picks_and_resumes_by_it() {
    let mut w = World::new();
    hosted_on(true);
    let opts = StartOpts { hosted: true, perm: "edits".into(), extra: "--model gpt-6 -c model_reasoning_effort=high".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "codex", &w.home().to_string_lossy(), "add a test", "", &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    assert!(sid.starts_with("codex-"), "Codex's own id: {sid}");
    let l = w.hosted.launched().pop().unwrap();
    assert_eq!((l.agent, l.permissions.as_str()), (crate::hosted::Agent::Codex, "edits"));
    assert_eq!(l.args, ["-c", "model_reasoning_effort=high"], "the model isn't an argument");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "add a test".to_string())));
    let o = w.hub().session_origin(&sid).unwrap();
    assert_eq!((o.harness.as_str(), o.transcript_path.as_str()), ("codex", format!("/codex/rollout-{sid}.jsonl").as_str()));
    w.hosted_says(&sid, HostedEvent::TurnStarted);
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "Added it.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    assert_eq!(w.snapshot().card(&sid)["message"], "Added it.");
    w.relaunch();
    w.send_to(&sid, "and the docs");
    assert_eq!(w.hosted.launched().pop().unwrap().start, Start::Resume(sid));
}

/// Quitting asks first only when it would end something: sessions Cue runs itself, mid-turn first by their names.
/// Those cue-host runs go on without Cue, so they aren't named.
#[test]
fn quitting_names_the_sessions_cue_runs_itself_mid_turn_first_and_not_those_the_host_runs() {
    let w = World::new();
    assert!(w.hub().ends_with_cue().is_empty(), "nothing hosted: nothing to ask");
    let idle = started(&w, "");
    let busy = running(&w, "go");
    let hosted = running(&w, "and you");
    w.hosted.under_host(&hosted);
    let ends = w.hub().ends_with_cue();
    let seen: Vec<(&str, bool)> = ends.iter().map(|e| (e.session_id.as_str(), e.mid_turn)).collect();
    assert_eq!(seen, [(busy.as_str(), true), (idle.as_str(), false)], "{ends:?}");
    assert!(ends.iter().all(|e| !e.name.is_empty()), "each has a name to show: {ends:?}");
    turn_ends(&w, &busy, "Done.");
    assert!(w.hub().ends_with_cue().iter().all(|e| !e.mid_turn), "its turn over, nothing is cut off");
}

#[test]
fn one_of_claude_codes_own_screens_opens_in_a_terminal_tab_and_the_session_never_gets_it() {
    let w = World::new();
    let sid = started(&w, "");
    w.hosted_says(&sid, HostedEvent::Started { session_id: sid.clone(), model: "opus".into(), slash_commands: vec!["compact".into()], terminal_only: vec!["statusline".into()] });
    let before = w.hosted.sent().len();
    for (typed, name) in [("/config", "config"), ("/login", "login"), ("/statusline", "statusline")] {
        w.send_to(&sid, typed);
        let tab = w.launcher.last();
        assert_eq!((tab.line.as_str(), tab.name.as_str()), (format!("claude '{typed}'").as_str(), format!("/{name}").as_str()), "a terminal tab of its own, in the session's folder");
        assert_eq!(tab.dir, w.home().to_string_lossy());
    }
    assert_eq!(w.hosted.sent().len(), before, "none of them went to the session");
    // An ordinary command still goes to it.
    w.send_to(&sid, "/compact");
    assert_eq!(w.hosted.sent().last(), Some(&(sid.clone(), "/compact".to_string())));
}

#[test]
fn claude_codes_own_screens_are_in_the_slash_menu_marked_as_opening_a_terminal() {
    let w = World::new();
    let sid = started(&w, "");
    w.hosted_says(&sid, HostedEvent::Started { session_id: sid.clone(), model: "opus".into(), slash_commands: vec!["compact".into()], terminal_only: vec!["statusline".into()] });
    // What Claude Code's headless mode lists for the folder: none of its own screens.
    let compact = crate::commands::Cmd { name: "compact".into(), kind: "command".into(), desc: "Compact".into(), hint: String::new(), args: vec![] };
    crate::db::set_commands(&w.home().to_string_lossy(), crate::model::now_ms(), &[compact]);
    let menu = crate::ipc::commands_of(w.hub(), &sid, false);
    let kind = |n: &str| menu["list"].as_array().unwrap().iter().find(|c| c["name"] == n).map(|c| c["kind"].as_str().unwrap_or_default().to_string());
    assert_eq!(kind("login").as_deref(), Some("terminal"), "{menu}");
    assert_eq!(kind("config").as_deref(), Some("terminal"));
    assert_eq!(kind("statusline").as_deref(), Some("terminal"), "what Claude reported at its start");
    assert_eq!(kind("compact").as_deref(), Some("command"), "it runs headless");
}

/// A hosted `agent` session started by + New in the World's folder: its id.
fn started_as(w: &World, agent: &str) -> String {
    hosted_on(true);
    let opts = StartOpts { hosted: true, ..Default::default() };
    let r = start_session_with(w.hub(), agent, &w.home().to_string_lossy(), "", "trial", &opts).unwrap();
    r["session_id"].as_str().unwrap().to_string()
}

#[test]
fn pis_and_codexs_own_commands_never_reach_the_model_sign_in_opens_a_terminal_and_the_rest_is_refused() {
    let w = World::new();
    let pi = started_as(&w, "pi");
    let before = w.hosted.sent().len();
    let said = w.send_to(&pi, "/login anthropic");
    let tab = w.launcher.last();
    assert_eq!((tab.line.as_str(), tab.dir.as_str()), ("pi --no-session", w.home().to_str().unwrap()), "Pi itself, in a terminal tab, to type it in");
    assert!(said.to_string().contains("type /login anthropic there"), "{said}");
    let refused = w.send_to(&pi, "/compact");
    assert!(refused.to_string().contains("a hosted session doesn't have"), "{refused}");
    assert_eq!(w.hosted.sent().len(), before, "neither went to Pi as a message");
    w.send_to(&pi, "/pilead:mode");
    assert_eq!(w.hosted.sent().last(), Some(&(pi.clone(), "/pilead:mode".to_string())), "an extension's command does");
    let codex = started_as(&w, "codex");
    w.send_to(&codex, "/login");
    assert_eq!(w.launcher.last().line, "codex login");
    assert!(w.send_to(&codex, "/model").to_string().contains("a hosted session doesn't have"));
}
