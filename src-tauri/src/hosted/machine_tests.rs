//! Hosted sessions on a Linux machine, for real (scripts/test-machines.sh): a Docker container running sshd
//! (tests/machines/), reached with real ssh over the shared connection Cue uses everywhere, cue-host put there by
//! Cue itself, and the tests' fake Claude standing in for the agent there (it's a shell script: it runs on Linux as
//! it does here). Ignored in the ordinary run: they need Docker and the image, which the script builds.
//!
//! What they show: a session runs and asks there as on this Mac; a dropped connection mid-turn loses nothing (the
//! turn goes on there, what it said meanwhile comes in once Cue is back, a message sent meanwhile gets there once);
//! a Cue that quits and starts again picks the session up with its ask; a host that dies there pauses the session.

use super::hosts_tests::{allow, bash_ask, echo, init, report, result, said, until_event};
use super::running::Hosted;
use super::{Agent, AskKind, HostedEvent, Hosts, Program, Spec, Start};
use crate::machines;
use crate::testing::{until, FakeClaude, FakeSsh, World};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// The platforms the script built images for ("linux/arm64 linux/amd64").
fn platforms() -> Vec<String> {
    let set = std::env::var("CUE_TEST_MACHINES").expect("run these with scripts/test-machines.sh (it builds the image and cue-host for Linux)");
    set.split_whitespace().map(str::to_string).collect()
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().expect("docker");
    assert!(out.status.success(), "docker {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A key of the test process's own, for the containers' user (never yours).
fn key() -> &'static (PathBuf, String) {
    static K: OnceLock<(PathBuf, String)> = OnceLock::new();
    K.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("cue-mkey-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("id");
        let ok = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&path).status().unwrap();
        assert!(ok.success());
        let public = std::fs::read_to_string(path.with_extension("pub")).unwrap().trim().to_string();
        (path, public)
    })
}

/// A Linux machine in a container, added to Cue (in a World of the test's own) and reached through real ssh.
struct Box_ {
    id: String,
    alias: String,
    config: PathBuf,
    name: String,
    _ssh: FakeSsh,
    _w: World,
}

impl Box_ {
    fn start(platform: &str) -> Box_ {
        let w = World::new();
        let ssh = FakeSsh::new();
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (key, public) = key();
        let tag = format!("cue-test-machine:{}", platform.trim_start_matches("linux/"));
        let id = docker(&["run", "-d", "--rm", "--platform", platform, "-e", &format!("AUTHORIZED_KEY={public}"), &tag]);
        let alias = format!("cue-docker-{}-{n}", std::process::id());
        let config = ssh.dir().join("config");
        // Reached through `docker exec` (nc to the container's own sshd), not a published port: no network between
        // this Mac and the container is needed, and none is opened.
        let docker_bin = String::from_utf8_lossy(&Command::new("/bin/sh").args(["-c", "command -v docker"]).output().unwrap().stdout).trim().to_string();
        std::fs::write(&config, format!("Host {alias}\n  User dev\n  ProxyCommand {docker_bin} exec -i {id} nc 127.0.0.1 22\n  IdentityFile {}\n  IdentitiesOnly yes\n  StrictHostKeyChecking no\n  UserKnownHostsFile /dev/null\n  LogLevel ERROR\n", key.display())).unwrap();
        ssh.through(&format!("#!/bin/sh\nexec /usr/bin/ssh -F {} \"$@\"\n", machines::shq(&config.to_string_lossy())));
        let m = machines::add(&alias, "").unwrap();
        let b = Box_ { id, alias, config, name: m.name, _ssh: ssh, _w: w };
        let end = std::time::Instant::now() + Duration::from_secs(30);
        while machines::run(&b.alias, "true").is_err() {
            assert!(std::time::Instant::now() < end, "sshd in {} never answered", b.id);
            std::thread::sleep(Duration::from_millis(300));
        }
        b
    }

    fn run(&self, script: &str) -> String {
        machines::run(&self.alias, script).unwrap_or_else(|e| panic!("{script}: {e}"))
    }

    /// `fake` as `claude` on the machine's PATH (~/bin), keeping what it hears in a folder there.
    fn agent(&self, fake: &FakeClaude) -> Agent_<'_> {
        let dir = format!("/home/dev/fake-{}", fake.program().parent().unwrap().file_name().unwrap().to_string_lossy());
        let script = std::fs::read_to_string(fake.program()).unwrap();
        let script = script.lines().map(|l| if l.starts_with("DIR=") { format!("DIR={}", machines::shq(&dir)) } else { l.to_string() }).collect::<Vec<_>>().join("\n") + "\n";
        machines::run_with_input(&self.alias, &format!("mkdir -p \"$HOME/bin\" {d} && cat >\"$HOME/bin/claude\" && chmod 755 \"$HOME/bin/claude\"", d = machines::shq(&dir)), &script).unwrap();
        Agent_ { on: self, dir }
    }

    /// The shared SSH connection to it ended, every session over it with it (as a network drop would).
    fn drop_connection(&self) {
        let _ = Command::new("/usr/bin/ssh").arg("-F").arg(&self.config).args(["-o", "ControlPath=~/.ssh/cue-%C", "-O", "exit", &self.alias]).output();
    }

    /// Something run in the container as root (what the machine does on its own).
    fn exec(&self, cmd: &str) -> String {
        docker(&["exec", &self.id, "sh", "-c", cmd])
    }
}

impl Drop for Box_ {
    fn drop(&mut self) {
        self.drop_connection();
        let _ = Command::new("docker").args(["rm", "-f", &self.id]).output();
    }
}

/// The fake agent on the machine: what Cue wrote to it there.
struct Agent_<'a> {
    on: &'a Box_,
    dir: String,
}

impl Agent_<'_> {
    fn heard(&self) -> Vec<serde_json::Value> {
        self.on.run(&format!("cat {}/input 2>/dev/null; true", machines::shq(&self.dir))).lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn heard_times(&self, text: &str) -> usize {
        self.heard().iter().filter(|v| v["message"]["content"] == text).count()
    }
}

/// Cue's hosts, with what it has read kept in a folder of the test's own (and no host of its own on this Mac: these
/// sessions are the machine's).
struct Cue {
    root: PathBuf,
}

impl Cue {
    fn new() -> Cue {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = PathBuf::from(format!("/tmp/cm-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&root);
        Cue { root }
    }

    fn hosts(&self) -> Hosts {
        Hosts::new(self.root.clone(), self.root.join("b-none"), vec![], self.root.join("seen"))
    }
}

impl Drop for Cue {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn spec_on(b: &Box_, start: Start) -> Spec {
    Spec { agent: Agent::Claude, cwd: "~".into(), start, permissions: String::new(), model: None, args: vec![], program: Program::Agent, machine: b.name.clone() }
}

/// Each test on every platform the script built for.
fn on_each(test: impl Fn(&Box_)) {
    for p in platforms() {
        eprintln!("-- on {p}");
        let b = Box_::start(&p);
        test(&b);
    }
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_session_on_a_machine_runs_under_cue_host_there_asks_and_is_answered() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(bash_ask("r1")).wait_for("control_response").say(result("done"));
        let agent = b.agent(&fake);
        let hosts = cue.hosts();
        let (r, rx) = report();
        let started = hosts.start(&spec_on(b, start), r).unwrap();
        assert_eq!(started.session_id, sid);
        // Cue put cue-host there, this build, and its Pi extension; the host runs from there.
        let placed = b.run("ls \"$HOME/.cue/bin\" \"$HOME/.cue/pi\"; ps -eo args | grep '[c]ue-host'");
        assert!(placed.contains(&format!("b-{}", cue_host::proto::BUILD)) && placed.contains("cue.ts"), "{placed}");
        assert!(placed.contains("serve --home"), "the host runs there: {placed}");
        hosts.send(&sid, "go").unwrap();
        let seen = until_event(&rx, |e| matches!(e, HostedEvent::Ask(_)));
        assert!(seen.contains(&HostedEvent::Taken { text: "go".into() }), "{seen:?}");
        hosts.answer(&sid, "r1", &allow()).unwrap();
        let end = until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { .. })).pop().unwrap();
        assert!(matches!(end, HostedEvent::TurnEnded { ref text, .. } if text == "done"), "{end:?}");
        assert_eq!(agent.heard_times("go"), 1);
        // It ran there with the marks a hosted session has (the hook there leaves it alone).
        assert!(b.run(&format!("cat {}/env", agent.dir)).starts_with("1\n"), "CUE_HOSTED set");
        hosts.close(&sid);
        until("it ended there", || !hosts.running(&sid));
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_connection_dropped_mid_turn_loses_nothing_the_turn_goes_on_there_and_a_message_sent_meanwhile_gets_there_once() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        // Says it's working, then carries on for a while (the connection drops meanwhile), then finishes; later
        // takes a second message.
        let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(said("working")).pause_ms(4000).say(said("finished while Cue couldn't hear")).say(result("done")).wait_for("and then").say(echo("and then")).say(result("then done"));
        let agent = b.agent(&fake);
        let hosts = cue.hosts();
        let (r, rx) = report();
        hosts.start(&spec_on(b, start), r).unwrap();
        hosts.send(&sid, "go").unwrap();
        until_event(&rx, |e| matches!(e, HostedEvent::Said { text, .. } if text == "working"));
        b.drop_connection();
        // Sent while the line is down: held, then sent once it's back.
        hosts.send(&sid, "and then").expect("held, not refused");
        let seen = until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { text, .. } if text == "then done"));
        assert!(seen.contains(&HostedEvent::Said { text: "finished while Cue couldn't hear".into(), agent: None }), "what it said meanwhile came in: {seen:?}");
        assert_eq!(seen.iter().filter(|e| matches!(e, HostedEvent::TurnEnded { .. })).count(), 2, "each turn's end once: {seen:?}");
        assert_eq!((agent.heard_times("go"), agent.heard_times("and then")), (1, 1), "nothing written twice");
        hosts.close(&sid);
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_cue_that_quits_with_an_ask_open_and_starts_again_asks_again_and_the_turn_finishes_there() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(bash_ask("r1")).wait_for("control_response").say(said("listed")).say(result("all done"));
        let _agent = b.agent(&fake);
        let s = spec_on(b, start);
        let first = cue.hosts();
        let (r, rx) = report();
        first.start(&s, r).unwrap();
        first.send(&sid, "go").unwrap();
        until_event(&rx, |e| matches!(e, HostedEvent::Ask(_)));
        first.close_all();
        drop(first);
        let again = cue.hosts();
        let (r2, rx2) = report();
        let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s }, r2).expect("still running there");
        assert!(back.started.as_ref().is_some_and(|st| st.in_turn), "mid-turn");
        (back.go)();
        let Ok(HostedEvent::Ask(a)) = rx2.recv_timeout(Duration::from_secs(10)) else { panic!("the ask, again") };
        assert_eq!((a.request_id.as_str(), a.kind), ("r1", AskKind::Permission));
        again.answer(&sid, "r1", &allow()).unwrap();
        let rest = until_event(&rx2, |e| matches!(e, HostedEvent::TurnEnded { .. }));
        assert!(rest.contains(&HostedEvent::Said { text: "listed".into(), agent: None }), "{rest:?}");
        again.close(&sid);
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_host_that_dies_there_takes_its_agent_and_the_session_shows_paused() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).pause_ms(60_000);
        let _agent = b.agent(&fake);
        let hosts = cue.hosts();
        let (r, rx) = report();
        hosts.start(&spec_on(b, start), r).unwrap();
        hosts.send(&sid, "go").unwrap();
        until_event(&rx, |e| matches!(e, HostedEvent::Taken { .. }));
        b.exec("pkill -KILL -f '[c]ue-host serve'");
        let gone = until_event(&rx, |e| matches!(e, HostedEvent::Exited { .. }));
        assert!(matches!(gone.last(), Some(HostedEvent::Exited { .. })), "{gone:?}");
        until("paused", || !hosts.running(&sid));
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_folder_that_isnt_there_says_so_and_a_damaged_cue_host_there_is_put_back_whole() {
    on_each(|b| {
        let cue = Cue::new();
        let hosts = cue.hosts();
        let fake = FakeClaude::new();
        let _agent = b.agent(&fake);
        let (r, rx) = report();
        let why = match hosts.start(&Spec { cwd: "~/no-such-folder".into(), ..spec_on(b, Start::fresh()) }, r) {
            Err(e) => e,
            Ok(_) => match until_event(&rx, |e| matches!(e, HostedEvent::Exited { .. })).pop() {
                Some(HostedEvent::Exited { last_words, .. }) => last_words,
                other => panic!("{other:?}"),
            },
        };
        assert!(why.contains("isn't a folder there"), "{why}");
        // Its host stopped, its program there damaged: the next Cue to use the machine puts it back whole.
        hosts.close_all();
        drop(hosts);
        b.exec("pkill -KILL -f '[c]ue-host serve'; true");
        b.run(&format!("printf broken >\"$HOME/.cue/bin/b-{}/cue-host\"", cue_host::proto::BUILD));
        super::machine_host::forget_placed();
        let again = Cue::new();
        let hosts = again.hosts();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let (r, _rx) = report();
        hosts.start(&spec_on(b, start), r).expect("cue-host put back and started");
        hosts.close(&sid);
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_session_cue_lost_sight_of_is_picked_up_by_the_next_message_never_started_twice() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(result("first")).wait_for("again").say(echo("again")).say(result("second"));
        let agent = b.agent(&fake);
        let s = spec_on(b, start);
        let first = cue.hosts();
        let (r, rx) = report();
        first.start(&s, r).unwrap();
        first.send(&sid, "go").unwrap();
        until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { .. }));
        first.close_all();
        drop(first);
        // A Cue that started while the machine couldn't be reached never picked it up: it wakes it with a start.
        let again = cue.hosts();
        let (r2, rx2) = report();
        let started = again.start(&Spec { start: Start::Resume(sid.clone()), ..s }, r2).expect("picked up");
        assert_eq!(started.session_id, sid);
        again.send(&sid, "again").unwrap();
        let rest = until_event(&rx2, |e| matches!(e, HostedEvent::TurnEnded { .. }));
        assert!(matches!(rest.last(), Some(HostedEvent::TurnEnded { text, .. }) if text == "second"), "{rest:?}");
        assert_eq!(b.run("ps -eo args | grep -c '[b]in/claude -p' || true").trim(), "1", "one agent, not a second beside it");
        assert_eq!(agent.heard_times("again"), 1);
        again.close(&sid);
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn older_cue_host_builds_on_a_machine_are_cleared_once_they_run_nothing_and_leftovers_go() {
    on_each(|b| {
        let cue = Cue::new();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let fake = FakeClaude::new();
        let _agent = b.agent(&fake);
        let s = spec_on(b, start);
        let first = cue.hosts();
        let (r, _rx) = report();
        first.start(&s, r).unwrap();
        first.close_all();
        drop(first);
        // Left by earlier Cues: a program with no folder (aa), a folder whose host is gone (bb), and an older build's
        // host still running, with nothing to run (cc).
        let b_ = cue_host::proto::BUILD;
        b.run(&format!(
            "cd \"$HOME/.cue\" && for x in aa cc; do mkdir -p bin/b-00000000000000$x && cp bin/b-{b_}/cue-host bin/b-00000000000000$x/; done && mkdir -p host/b-00000000000000bb && (setsid bin/b-00000000000000cc/cue-host serve --home \"$HOME/.cue/host/b-00000000000000cc\" </dev/null >/dev/null 2>&1 &) && sleep 0.5"
        ));
        let older = "b-00000000000000cc/cue-host serve";
        assert!(b.run("ps -eo args").contains(older));
        let again = cue.hosts();
        let (r2, _rx2) = report();
        let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s }, r2).expect("its session, on this build's host");
        (back.go)();
        again.settled();
        until("the older host stopped", || !b.run("ps -eo args").contains(older));
        let left = b.run("cd \"$HOME/.cue\" && ls bin host");
        for gone in ["b-00000000000000aa", "b-00000000000000bb", "b-00000000000000cc"] {
            assert!(!left.contains(gone), "{gone} cleared: {left}");
        }
        assert_eq!(left.matches(&format!("b-{b_}")).count(), 2, "this build's program and folder kept: {left}");
        // Every host there shows as cue-host.
        assert!(b.run("ps -eo comm").lines().any(|c| c.trim() == "cue-host"), "named cue-host in ps");
        assert!(again.running(&sid), "its session untouched");
        again.close(&sid);
    });
}

#[test]
#[ignore = "needs Docker: scripts/test-machines.sh"]
fn a_transcript_there_is_copied_over_the_cue_host_connection_with_nothing_lost_or_doubled_across_a_drop() {
    on_each(|b| {
        let cue = Cue::new();
        let fake = FakeClaude::new();
        let _agent = b.agent(&fake);
        let hosts = cue.hosts();
        let start = Start::fresh();
        let sid = start.session_id().to_string();
        let (r, _rx) = report();
        hosts.start(&spec_on(b, start), r).unwrap();
        let m = machines::get(&b.name).unwrap();
        let remote = "/home/dev/transcript.jsonl";
        b.run(&format!("printf 'one\\ntwo\\n' >{remote}"));
        let copy = crate::machines::logs::local_copy(&m, remote);
        assert!(!copy.is_empty());
        let read = || std::fs::read_to_string(&copy).unwrap_or_default();
        until("the start of it here", || read() == "one\ntwo\n");
        b.run(&format!("printf 'three\\n' >>{remote}"));
        until("what's added", || read() == "one\ntwo\nthree\n");
        assert_eq!(b.run("ps -eo args | grep -c '[t]ail -c' || true").trim(), "0", "no tail -F session: over the one connection");
        // The connection drops while more is written: once it's back, the rest comes, once.
        b.drop_connection();
        b.run(&format!("printf 'four\\nfive\\n' >>{remote}"));
        until("the rest after the drop", || read() == "one\ntwo\nthree\nfour\nfive\n");
        crate::machines::logs::stop(&m, remote);
        hosts.close(&sid);
    });
}

/// A real machine (`CUE_TEST_REAL_MACHINE`: an ssh host you can reach, with Claude Code signed in), reached with your
/// own ssh and its config: the agent is the real Claude Code there, on Haiku, in a scratch folder removed after (with
/// the test's conversation). It puts cue-host in ~/.cue there, as Cue does.
fn real_machine() -> Option<(World, FakeSsh, String)> {
    let host = std::env::var("CUE_TEST_REAL_MACHINE").ok()?;
    let w = World::new();
    let ssh = FakeSsh::new();
    ssh.through("#!/bin/sh\nexec /usr/bin/ssh \"$@\"\n");
    let m = machines::add(&host, "").unwrap();
    Some((w, ssh, m.name))
}

#[test]
#[ignore = "needs a real machine: CUE_TEST_REAL_MACHINE=<ssh host>"]
fn live_on_a_real_machine_claude_code_asks_runs_a_command_past_a_dropped_connection_and_is_picked_up_after_a_restart() {
    let Some((_w, _ssh, name)) = real_machine() else { panic!("set CUE_TEST_REAL_MACHINE") };
    let m = machines::get(&name).unwrap();
    let run = |s: &str| machines::run(&m.host, s).unwrap_or_else(|e| panic!("{s}: {e}"));
    let dir = format!("/tmp/cue-live-{}", std::process::id());
    run(&format!("mkdir -p {dir}"));
    let cue = Cue::new();
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let _scratch = Scratch { host: m.host.clone(), dir: dir.clone(), sid: sid.clone() };
    let spec = Spec { agent: Agent::Claude, cwd: dir.clone().into(), start, permissions: String::new(), model: Some("haiku".into()), args: vec![], program: Program::Agent, machine: name.clone() };
    let hosts = cue.hosts();
    let (r, rx) = report();
    let started = hosts.start(&spec, r).expect("started there");
    eprintln!("-- started {} (pid {} there)", started.session_id, started.pid);
    let long = Duration::from_secs(120);
    let until_long = |rx: &std::sync::mpsc::Receiver<HostedEvent>, stop: &dyn Fn(&HostedEvent) -> bool| -> Vec<HostedEvent> {
        let end = std::time::Instant::now() + long;
        let mut seen = vec![];
        loop {
            let e = rx.recv_timeout(end.saturating_duration_since(std::time::Instant::now())).unwrap_or_else(|_| panic!("not in {long:?}; saw {seen:?}"));
            let done = stop(&e);
            seen.push(e);
            if done {
                return seen;
            }
        }
    };
    let allow_each = |hosts: &Hosts, rx: &std::sync::mpsc::Receiver<HostedEvent>, drop_after_first: bool| -> Vec<HostedEvent> {
        let mut all = vec![];
        let mut dropped = false;
        loop {
            let seen = until_long(rx, &|e| matches!(e, HostedEvent::Ask(_) | HostedEvent::TurnEnded { .. }));
            let last = seen.last().cloned().unwrap();
            all.extend(seen);
            match last {
                HostedEvent::Ask(a) => {
                    eprintln!("-- asked: {} {}", a.tool_name, a.input);
                    hosts.answer(&sid, &a.request_id, &super::hosts_tests::allow()).unwrap();
                    if drop_after_first && !dropped {
                        dropped = true;
                        let out = Command::new("/usr/bin/ssh").args(["-o", "ControlPath=~/.ssh/cue-%C", "-O", "exit", &m.host]).output().unwrap();
                        eprintln!("-- dropped the connection: {}", String::from_utf8_lossy(&out.stderr).trim());
                    }
                }
                _ => return all,
            }
        }
    };
    // A command, asked about, answered from here.
    // Commands that write (touch): Claude Code asks about those whatever it's set to allow (it runs echo and printf
    // without asking), in the test's scratch folder.
    hosts.send(&sid, "Run exactly this shell command and nothing else: touch one.txt && printf 'cue-live-one\\n'. Then reply with its output only.").unwrap();
    let first = allow_each(&hosts, &rx, false);
    if let Some(HostedEvent::TurnEnded { text, .. }) = first.last().filter(|e| matches!(e, HostedEvent::TurnEnded { text, .. } if text.contains("authenticate"))) {
        panic!("Claude Code on {} isn't signed in ({text}): sign in there (claude, then /login) and run this again", m.host);
    }
    assert!(first.iter().any(|e| matches!(e, HostedEvent::Ask(a) if a.tool_name == "Bash")), "it asked: {first:?}");
    assert!(matches!(first.last(), Some(HostedEvent::TurnEnded { text, failed: false, .. }) if text.contains("cue-live-one")), "{:?}", first.last());
    // A slow command, the connection dropped as it runs: the turn finishes there, and comes in once Cue is back.
    hosts.send(&sid, "Run exactly this shell command and nothing else: touch two.txt && sleep 6 && printf 'cue-live-two\\n'. Then reply with its output only.").unwrap();
    let second = allow_each(&hosts, &rx, true);
    assert!(matches!(second.last(), Some(HostedEvent::TurnEnded { text, failed: false, .. }) if text.contains("cue-live-two")), "{:?}", second.last());
    // Cue quits and starts again: the session is picked up, its agent still running there.
    hosts.close_all();
    drop(hosts);
    let again = cue.hosts();
    let (r2, rx2) = report();
    let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..spec }, r2).expect("picked up");
    assert_eq!(back.started.as_ref().map(|s| s.pid), Some(started.pid), "the same agent, not a new one");
    (back.go)();
    again.send(&sid, "Reply with the word picked-up and nothing else.").unwrap();
    let third = until_long(&rx2, &|e| matches!(e, HostedEvent::TurnEnded { .. }));
    assert!(matches!(third.last(), Some(HostedEvent::TurnEnded { text, .. }) if text.to_lowercase().contains("picked")), "{:?}", third.last());
    // Every process of Cue's there says so.
    let ps = run("ps -u \"$(id -u)\" -o comm=,args= | grep -E 'cue-host|cue-hosted' | grep -v grep; true");
    eprintln!("-- Cue's processes there:\n{ps}");
    assert!(ps.lines().any(|l| l.trim_start().starts_with("cue-host ") && l.contains(" serve ")), "the host shows as cue-host: {ps}");
    again.close(&sid);
}

/// What the live test leaves on the machine, removed however it ends: its agent (by its session id, exactly), its
/// scratch folder, and its conversation. cue-host stays (it leaves on its own when idle).
struct Scratch {
    host: String,
    dir: String,
    sid: String,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (dir, sid) = (&self.dir, &self.sid);
        let projects = format!("${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}/projects/{}", crate::logs::live::claude_project_folder(dir));
        // Its agent ended first, and waited for: Claude Code writes its conversation once more as it goes.
        let script = format!("ps=$(ps -u \"$(id -u)\" -o pid=,args= | grep -F -- '--session-id {sid}' | grep -v grep | awk '{{print $1}}'); [ -n \"$ps\" ] && kill $ps; i=0; while [ -n \"$ps\" ] && kill -0 $ps 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i+1)); done; rm -rf {dir} \"{projects}\"; true");
        if let Err(e) = machines::run(&self.host, &script) {
            eprintln!("-- couldn't clean up on {}: {e}", self.host);
        }
    }
}
