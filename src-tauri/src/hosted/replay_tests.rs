//! Cue read back the host's log after a restart: each agent's protocol, replayed from its lines and Cue's own
//! (`read` and `wrote`), ends as the one that lived through them did. Over lines each real agent wrote (fixtures/),
//! with Cue's writes as it made them: its opening, its answers, its replies to what it read.

use super::codex::Codex;
use super::pi::Pi;
use super::protocol::{Claude, Protocol};
use super::{Agent, Answer, HostedEvent, Program, Spec, Start};
use crate::model::Decision;

/// One record of the host's log: the agent's line, or Cue's.
#[derive(Clone)]
enum Rec {
    Out(String),
    In(String),
}

/// `p` living through `fixture` as a session does: its opening written, each of its lines read, its replies written,
/// each ask answered (allowed) at once unless `answer` says not to. The log the host would keep, and the asks.
fn live(p: &mut dyn Protocol, fixture: &str, answer: impl Fn(usize) -> bool) -> (Vec<Rec>, Vec<String>) {
    let mut log: Vec<Rec> = p.opening().into_iter().map(Rec::In).collect();
    let mut asks = vec![];
    for line in fixture.lines() {
        log.push(Rec::Out(line.to_string()));
        let (events, replies) = p.read(line);
        log.extend(replies.into_iter().map(Rec::In));
        for e in events {
            if let HostedEvent::Ask(a) = e {
                if answer(asks.len()) {
                    let allow = Answer::Decision(Decision { behavior: "allow".into(), message: None, answers: None, permission: None });
                    log.extend(p.answer(&a.request_id, &allow).unwrap().into_iter().map(Rec::In));
                }
                asks.push(a.request_id);
            }
        }
    }
    (log, asks)
}

/// A fresh protocol given `log` back: the agent's lines read, Cue's written, nothing sent anywhere.
fn replay(p: &mut dyn Protocol, log: &[Rec]) {
    for r in log {
        match r {
            Rec::Out(l) => {
                p.read(l);
            }
            Rec::In(l) => p.wrote(l),
        }
    }
}

/// What can be seen of a protocol from outside, and what it writes next.
fn seen(p: &mut dyn Protocol, asks: &[String]) -> (Option<String>, Option<String>, bool, Vec<bool>, Vec<String>, Vec<String>) {
    let waits = asks.iter().map(|a| p.waits_on(a)).collect();
    (p.session_id(), p.transcript(), p.in_turn(), waits, p.message("and one more thing"), p.interrupt())
}

fn same_after_replay(make: impl Fn() -> Box<dyn Protocol>, fixture: &str, answer: impl Fn(usize) -> bool) -> Vec<bool> {
    let mut lived = make();
    let (log, asks) = live(lived.as_mut(), fixture, answer);
    let mut replayed = make();
    replay(replayed.as_mut(), &log);
    let (a, b) = (seen(lived.as_mut(), &asks), seen(replayed.as_mut(), &asks));
    assert_eq!(a, b, "replayed differs from lived");
    a.3
}

fn codex(start: Start) -> Box<dyn Protocol> {
    Box::new(Codex::new(&Spec { agent: Agent::Codex, cwd: "/work/demo".into(), start, permissions: String::new(), model: None, args: vec![], program: Program::Agent }))
}

#[test]
fn claude_replayed_is_claude_as_it_was_answered_or_still_asking() {
    for fixture in [include_str!("fixtures/bash_permission.jsonl"), include_str!("fixtures/question.jsonl"), include_str!("fixtures/agent_permission.jsonl")] {
        let answered = same_after_replay(|| Box::<Claude>::default(), fixture, |_| true);
        assert!(answered.iter().all(|w| !w), "answered asks don't wait: {answered:?}");
        let open = same_after_replay(|| Box::<Claude>::default(), fixture, |_| false);
        assert!(!open.is_empty() && open.iter().all(|w| *w), "unanswered asks still wait: {open:?}");
    }
}

#[test]
fn pi_replayed_is_pi_as_it_was() {
    let fixture = include_str!("fixtures/pi_rpc.jsonl");
    same_after_replay(|| Box::<Pi>::default(), fixture, |_| true);
    let open = same_after_replay(|| Box::<Pi>::default(), fixture, |_| false);
    assert!(!open.is_empty() && open.iter().all(|w| *w), "{open:?}");
}

#[test]
fn codex_replayed_knows_its_conversation_its_turn_and_what_still_waits() {
    for (fixture, start) in [(include_str!("fixtures/codex_new.jsonl"), Start::New("ignored".into())), (include_str!("fixtures/codex_resume.jsonl"), Start::Resume("t1".into()))] {
        let answered = same_after_replay(|| codex(start.clone()), fixture, |_| true);
        assert!(answered.iter().all(|w| !w), "{answered:?}");
        let mut lived = codex(start.clone());
        let (log, _) = live(lived.as_mut(), fixture, |_| true);
        let mut replayed = codex(start.clone());
        replay(replayed.as_mut(), &log);
        assert!(replayed.ready() && replayed.session_id().is_some(), "the conversation is known again");
    }
    let open = same_after_replay(|| codex(Start::New("ignored".into())), include_str!("fixtures/codex_new.jsonl"), |_| false);
    assert!(!open.is_empty() && open.iter().all(|w| *w), "{open:?}");
}
