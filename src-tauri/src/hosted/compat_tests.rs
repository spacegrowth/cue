//! Real Claude Code, through `Session`, for every case Cue-hosted sessions depend on
//! (docs/dev/cue-hosted-sessions.md §2): run on each Claude Code update, so a change to its headless protocol
//! shows up here rather than in your sessions. Opt-in, as they start real Claude on a small model (cents a run):
//!
//!   CUE_CLAUDE_COMPAT=1 cargo test --lib hosted::compat -- --ignored --test-threads=2
//!
//! Each runs in a folder of its own with only its own settings (no hooks, plugins or MCP servers of yours),
//! as Claude normally runs: with your real home folder, where its login is. That's the one thing taken from
//! outside the tests' sandbox, for the Claude it starts only; the session folder it writes there is removed.

use super::{Answer, Ask, AskKind, HostedEvent, InputAction, Program, Session, Spec, Start};
use crate::model::Decision;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const MODEL: &str = "haiku";
const PATIENCE: Duration = Duration::from_secs(150);

/// A real hosted session in a folder of its own.
struct Real {
    dir: PathBuf,
    session: Option<Session>,
    rx: Receiver<HostedEvent>,
}

impl Real {
    /// `None` unless CUE_CLAUDE_COMPAT is set. `settings`: the session's own settings (hooks, sandbox).
    fn start(name: &str, settings: Value, start: Start, args: &[&str]) -> Option<Real> {
        std::env::var_os("CUE_CLAUDE_COMPAT")?;
        let dir = std::env::temp_dir().join(format!("cue-compat-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        Some(Real::start_in(dir, settings, start, args))
    }

    fn start_in(dir: PathBuf, settings: Value, start: Start, args: &[&str]) -> Real {
        let mut a = vec!["--setting-sources".into(), "local".into(), "--strict-mcp-config".into(), "--settings".into(), settings.to_string()];
        a.extend(args.iter().map(|s| s.to_string()));
        let spec = Spec { cwd: dir.clone(), start, permissions: String::new(), model: Some(MODEL.into()), args: a, program: Program::At(claude_with_real_home(&dir)) };
        let (tx, rx) = channel();
        let session = Session::start(&spec, move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        Real { dir, session: Some(session), rx }
    }

    fn s(&self) -> &Session {
        self.session.as_ref().unwrap()
    }

    /// Send `text`, allow every ask with `answer`, and return everything up to the turn's end.
    fn turn(&self, text: &str, answer: impl Fn(&Ask) -> Answer) -> Vec<HostedEvent> {
        self.s().send(text).unwrap();
        self.until_turn_ends(answer)
    }

    fn until_turn_ends(&self, answer: impl Fn(&Ask) -> Answer) -> Vec<HostedEvent> {
        self.until(answer, |e| matches!(e, HostedEvent::TurnEnded { .. } | HostedEvent::Exited { .. }))
    }

    /// Everything up to and including the first event `stop` accepts, every ask on the way answered with `answer`.
    fn until(&self, answer: impl Fn(&Ask) -> Answer, stop: impl Fn(&HostedEvent) -> bool) -> Vec<HostedEvent> {
        let end = Instant::now() + PATIENCE;
        let mut seen = vec![];
        loop {
            let e = self.rx.recv_timeout(end.saturating_duration_since(Instant::now())).unwrap_or_else(|_| panic!("not seen in time; saw {seen:?}"));
            if let HostedEvent::Ask(a) = &e {
                self.s().answer(&a.request_id, &answer(a)).unwrap();
            }
            let done = stop(&e);
            seen.push(e);
            if done {
                return seen;
            }
        }
    }

    /// Close it, keeping its folder (a resume runs there).
    fn close(&mut self) {
        if let Some(s) = self.session.take() {
            s.close();
        }
    }

    /// Claude Code's transcripts for this folder, in your home's ~/.claude.
    fn transcripts(&self) -> PathBuf {
        let slug: String = self.dir.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        crate::testing::sandbox::real_home().join(".claude/projects").join(slug)
    }
}

impl Drop for Real {
    fn drop(&mut self) {
        self.close();
        let _ = std::fs::remove_dir_all(self.transcripts());
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `claude` through your login shell with your real home folder, written as a program in `dir`.
fn claude_with_real_home(dir: &Path) -> PathBuf {
    let path = dir.join(".claude-real");
    let home = crate::testing::sandbox::real_home().to_string_lossy().replace('\'', "'\\''");
    let script = format!("#!/bin/sh\nexport HOME='{home}'\nexec \"${{SHELL:-/bin/zsh}}\" -lc 'exec claude \"$@\"' claude \"$@\"\n");
    std::fs::write(&path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn allow(_: &Ask) -> Answer {
    Answer::Decision(Decision { behavior: "allow".into(), ..Default::default() })
}

fn quiet() -> Value {
    json!({ "disableAllHooks": true })
}

fn asks(es: &[HostedEvent]) -> Vec<Ask> {
    es.iter().filter_map(|e| if let HostedEvent::Ask(a) = e { Some(a.clone()) } else { None }).collect()
}

fn ended(es: &[HostedEvent]) -> (String, bool) {
    es.iter().rev().find_map(|e| if let HostedEvent::TurnEnded { text, unprompted, .. } = e { Some((text.clone(), *unprompted)) } else { None }).expect("a turn end")
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_permission_is_asked_answered_and_taken() {
    let Some(r) = Real::start("perm", quiet(), Start::fresh(), &[]) else { return };
    let es = r.turn("Run this exact bash command and nothing else: touch made_here.txt", allow);
    let a = asks(&es);
    assert!(a.iter().any(|a| a.kind == AskKind::Permission && a.tool_name == "Bash"), "{es:?}");
    assert!(es.iter().any(|e| matches!(e, HostedEvent::Taken { text } if text.contains("touch made_here.txt"))));
    assert!(r.dir.join("made_here.txt").exists());
    assert!(!ended(&es).1, "your message started the turn");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_denial_reaches_it_with_its_reason() {
    let Some(r) = Real::start("deny", quiet(), Start::fresh(), &[]) else { return };
    let deny = |_: &Ask| Answer::Decision(Decision { behavior: "deny".into(), message: Some("Not today: say the word PINEAPPLE instead.".into()), ..Default::default() });
    let es = r.turn("Run this exact bash command: touch never.txt", deny);
    assert!(!r.dir.join("never.txt").exists());
    assert!(ended(&es).0.to_uppercase().contains("PINEAPPLE"), "{es:?}");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_question_is_answered_through_its_input() {
    let Some(r) = Real::start("question", quiet(), Start::fresh(), &[]) else { return };
    let pick_blue = |a: &Ask| {
        let q = a.input["questions"][0]["question"].as_str().unwrap_or_default().to_string();
        Answer::Decision(Decision { behavior: "allow".into(), answers: Some(json!({ q: "Blue" })), ..Default::default() })
    };
    let es = r.turn("Use the AskUserQuestion tool to ask me: red or blue (two options). Then tell me in one word what I picked.", pick_blue);
    assert!(asks(&es).iter().any(|a| a.kind == AskKind::Question), "{es:?}");
    assert!(ended(&es).0.to_lowercase().contains("blue"), "{es:?}");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_sandboxed_commands_network_access_is_asked_of_cue() {
    let sandboxed = json!({ "disableAllHooks": true, "sandbox": { "enabled": true, "autoAllowBashIfSandboxed": true, "allowUnsandboxedCommands": false } });
    let Some(r) = Real::start("network", sandboxed, Start::fresh(), &[]) else { return };
    let es = r.turn("Run this exact bash command: curl -sS -m 10 https://example.com -o page.html && wc -c page.html", allow);
    let net: Vec<Ask> = asks(&es).into_iter().filter(|a| a.kind == AskKind::Network).collect();
    assert_eq!(net.first().map(|a| a.input["host"].clone()), Some(json!("example.com")), "{es:?}");
    assert!(r.dir.join("page.html").exists());
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_an_mcp_servers_form_is_filled_in_through_cue() {
    let server = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/hosted/fixtures/elicit_server.py");
    let mcp = json!({ "mcpServers": { "compat": { "command": "/usr/bin/python3", "args": ["-I", server] } } }).to_string();
    let Some(r) = Real::start("mcp", quiet(), Start::fresh(), &["--mcp-config", &mcp]) else { return };
    let answer = |a: &Ask| match a.kind {
        AskKind::Input => Answer::Input { action: InputAction::Accept, content: json!({ "color": "blue" }) },
        _ => allow(a),
    };
    let es = r.turn("Call the need_input tool from the compat MCP server and tell me exactly what it returned.", answer);
    let form = asks(&es).into_iter().find(|a| a.kind == AskKind::Input).unwrap_or_else(|| panic!("{es:?}"));
    assert_eq!((form.server.as_str(), form.message.as_str()), ("compat", "Pick a color"));
    let delivered = es.iter().any(|e| matches!(e, HostedEvent::ToolResult { output, .. } if output.to_string().contains(r#"\"color\": \"blue\""#)));
    assert!(delivered, "the server got the answer: {es:?}");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_message_sent_mid_turn_is_taken_in_that_turn_and_an_interrupt_stops_it() {
    let Some(r) = Real::start("queue", quiet(), Start::fresh(), &[]) else { return };
    r.s().send("Run this exact foreground bash command: python3 -c 'import time; time.sleep(6); print(1)'. Then reply in one line.").unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let es = r.turn("Also include the word BANANA in that reply.", allow);
    let taken = es.iter().filter(|e| matches!(e, HostedEvent::Taken { .. })).count();
    assert_eq!(taken, 2, "{es:?}");
    assert!(ended(&es).0.to_uppercase().contains("BANANA"), "{es:?}");
    r.s().send("Run this exact foreground bash command: python3 -c 'import time; time.sleep(60)'").unwrap();
    r.until(allow, |e| matches!(e, HostedEvent::ToolCall { name, .. } if name == "Bash"));
    std::thread::sleep(Duration::from_secs(3));
    let interrupted = Instant::now();
    r.s().interrupt().unwrap();
    r.until_turn_ends(allow);
    assert!(interrupted.elapsed() < Duration::from_secs(15), "the interrupt ended the turn, not the minute-long command");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_resume_keeps_the_conversation_in_the_same_transcript() {
    let start = Start::fresh();
    let id = start.session_id().to_string();
    let Some(mut r) = Real::start("resume", quiet(), start, &[]) else { return };
    r.turn("Remember the word MANGO. Reply with just OK.", allow);
    r.close();
    let file = r.transcripts().join(format!("{id}.jsonl"));
    let lines = |f: &Path| std::fs::read_to_string(f).map(|t| t.lines().count()).unwrap_or(0);
    let before = lines(&file);
    assert!(before > 0, "its transcript is at {}", file.display());
    let again = Real::start_in(r.dir.clone(), quiet(), Start::Resume(id.clone()), &[]);
    let es = again.turn("What word did I ask you to remember? One word.", allow);
    assert!(ended(&es).0.to_uppercase().contains("MANGO"), "{es:?}");
    assert!(lines(&file) > before, "the resume added to the same transcript");
    let files = std::fs::read_dir(r.transcripts()).unwrap().filter(|e| e.as_ref().is_ok_and(|e| e.path().extension().is_some_and(|x| x == "jsonl"))).count();
    assert_eq!(files, 1, "no second transcript");
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_its_hooks_run_and_see_that_cue_hosts_it() {
    let seen = std::env::temp_dir().join(format!("cue-compat-hook-{}", std::process::id()));
    let hook = format!("printenv CUE_HOSTED > '{}'", seen.display());
    let settings = json!({ "hooks": { "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": hook }] }] } });
    let Some(r) = Real::start("hooks", settings, Start::fresh(), &[]) else { return };
    r.turn("Reply with just OK.", allow);
    assert_eq!(std::fs::read_to_string(&seen).unwrap_or_default().trim(), "1", "a hook it runs sees CUE_HOSTED");
    let _ = std::fs::remove_file(seen);
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_always_allow_is_kept_and_asked_no_more() {
    let Some(r) = Real::start("always", quiet(), Start::fresh(), &[]) else { return };
    let always = |a: &Ask| {
        let rule = a.suggestions.as_array().and_then(|s| s.first()).cloned();
        Answer::Decision(Decision { behavior: "allow_always".into(), permission: rule, ..Default::default() })
    };
    let first = r.turn("Run this exact bash command: touch one.txt", always);
    let suggestion = asks(&first).first().map(|a| a.suggestions.clone()).unwrap_or_default();
    assert!(suggestion.as_array().is_some_and(|s| !s.is_empty()), "Claude offered an always-allow rule: {first:?}");
    let second = r.turn("Run this exact bash command: touch one.txt", allow);
    assert!(asks(&second).iter().all(|a| a.tool_name != "Bash"), "the rule held: {second:?}");
    let kept = std::fs::read_to_string(r.dir.join(".claude/settings.local.json")).unwrap_or_default();
    eprintln!("always-allow destination {:?}; settings.local.json: {kept}", suggestion[0]["destination"]);
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_a_named_agents_permission_comes_to_cue_naming_the_agent() {
    let Some(r) = Real::start("agent", quiet(), Start::fresh(), &[]) else { return };
    let ask = "Spawn one agent named probe whose only task is to run this bash command: touch by_probe.txt . Don't run it yourself. Wait for it to finish, then say done.";
    r.s().send(ask).unwrap();
    let es = r.until(allow, |e| matches!(e, HostedEvent::Ask(a) if a.tool_name == "Bash"));
    let HostedEvent::Ask(a) = es.last().unwrap() else { unreachable!() };
    assert!(!a.agent_id.is_empty(), "asked from inside the agent, and named so: {a:?}");
    until_file(&r.dir.join("by_probe.txt"));
}

fn until_file(path: &Path) {
    let end = Instant::now() + PATIENCE;
    while !path.exists() {
        assert!(Instant::now() < end, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(200));
    }
}
