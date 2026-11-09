//! Whether a machine's agent can run a session Cue hosts, asked before one starts there: installed, and taking what
//! Cue starts it with (launch.rs). Otherwise the start would fail inside the start script with a shell's "not found",
//! or the agent's own complaint about an option it doesn't know, which say neither what's wrong nor what to do.
//!
//! What Cue relies on is checked, not a version number: an older agent that has every option still works, and the one
//! it lacks is named. Asked over the machine's shared SSH connection, in the environment the agent starts in (Cue's
//! copy of your login shell's, `~/.cue/env.sh`, as launch.rs's MACHINE_SCRIPT has it). A machine whose agent passes is
//! remembered while Cue runs; one that doesn't is asked again next time (you'll have installed or updated it).

use super::Agent;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// What the agent's `--help` must mention: the options Cue starts it with (launch.rs HEADLESS, `arguments`,
/// `pi_arguments`). Codex is started as `codex app-server`: that it has the command is what's asked.
fn needs(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &["--input-format", "--output-format", "--verbose", "--replay-user-messages", "--permission-prompts", "--permission-prompt-tool", "--session-id", "--resume"],
        Agent::Pi => &["--mode", "--session-id"],
        Agent::Codex => &[],
    }
}

/// The script asking about `agent` there: MISSING when it isn't on the PATH, else its version's first line, a line of
/// its own, then its help (Codex: APP-SERVER when it has that command).
fn script(agent: Agent) -> String {
    let p = agent.program();
    let has = match agent {
        Agent::Codex => format!("{p} app-server --help >/dev/null 2>&1 && echo APP-SERVER"),
        _ => format!("{p} --help 2>&1"),
    };
    format!(r#"[ -r "$HOME/.cue/env.sh" ] && . "$HOME/.cue/env.sh"; command -v {p} >/dev/null 2>&1 || {{ echo MISSING; exit 0; }}; {p} --version 2>&1 | head -1; echo ---; {has}; true"#)
}

/// What the machine said (`script`'s output), judged: Ok, or what's wrong and what to do about it there.
fn judge(agent: Agent, machine: &str, said: &str) -> Result<(), String> {
    let name = agent.name();
    if said.lines().next().is_some_and(|l| l.trim() == "MISSING") {
        return Err(format!("{name} isn't installed on {machine}: Cue looks for `{}` on the PATH your login shell sets there. Install it there, then start again", agent.program()));
    }
    // The version's line (none, if it printed nothing), then the line `---`, then the help.
    let (version, help) = match said.find("---\n") {
        Some(i) if i == 0 || said[..i].ends_with('\n') => (&said[..i], &said[i + 4..]),
        _ => (said, ""),
    };
    let version = version.trim();
    let lacks: Vec<&str> = match agent {
        Agent::Codex => if help.contains("APP-SERVER") { vec![] } else { vec!["app-server"] },
        _ => needs(agent).iter().copied().filter(|o| !help.contains(o)).collect(),
    };
    if lacks.is_empty() {
        return Ok(());
    }
    let which = if version.is_empty() { String::new() } else { format!(" ({version})") };
    // Each one's own updater, where it has one (Pi's help lists `pi update self`).
    let how = match agent {
        Agent::Claude => " (`claude update`)",
        Agent::Pi => " (`pi update self`)",
        Agent::Codex => "",
    };
    Err(format!("{name} on {machine}{which} is too old for a session Cue hosts: it has no {}. Update it there{how}, then start again", lacks.join(", ")))
}

/// Machines (host, agent) whose agent passed, while Cue runs.
fn passed() -> &'static Mutex<HashSet<(String, &'static str)>> {
    static P: OnceLock<Mutex<HashSet<(String, &'static str)>>> = OnceLock::new();
    P.get_or_init(Mutex::default)
}

/// Whether `agent` on machine `name` can run a session Cue hosts: why not, saying what to do there.
pub(super) fn on_machine(name: &str, agent: Agent) -> Result<(), String> {
    let m = crate::machines::get(name).ok_or_else(|| format!("{name} isn't one of your machines any more"))?;
    let key = (m.host.clone(), agent.program());
    if crate::locking::lock(passed(), "agent checks passed").contains(&key) {
        return Ok(());
    }
    let said = crate::machines::run(&m.host, &script(agent))?;
    judge(agent, &crate::machines::display_name(&m.name), &said)?;
    crate::locking::lock(passed(), "agent checks passed").insert(key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a machine says for `version` and `help` (as `script` prints them).
    fn said(version: &str, help: &str) -> String {
        format!("{version}\n---\n{help}\n")
    }

    const CLAUDE_HELP: &str = "  --input-format <format>\n  --output-format <format>\n  --verbose\n  --replay-user-messages\n  --permission-prompts <mode>\n  --permission-prompt-tool <tool>\n  --session-id <uuid>\n  -r, --resume [value]\n";

    #[test]
    fn an_agent_with_every_option_cue_starts_it_with_passes_whatever_its_version() {
        assert_eq!(judge(Agent::Claude, "box", &said("2.1.319 (Claude Code)", CLAUDE_HELP)), Ok(()));
        assert_eq!(judge(Agent::Claude, "box", &said("1.9.0 (Claude Code)", CLAUDE_HELP)), Ok(()), "an older one that has them all");
        assert_eq!(judge(Agent::Pi, "box", &said("0.87.1", "  --mode <mode>\n  --session-id <id>\n")), Ok(()));
        assert_eq!(judge(Agent::Codex, "box", &said("codex-cli 0.50.0", "APP-SERVER")), Ok(()));
        assert_eq!(judge(Agent::Pi, "box", &format!("---\n  --mode <mode>\n  --session-id <id>\n")), Ok(()), "a version that printed nothing");
    }

    #[test]
    fn one_that_isnt_installed_says_so_and_where_cue_looked() {
        let why = judge(Agent::Pi, "box", "MISSING\n").unwrap_err();
        assert!(why.starts_with("Pi isn't installed on box") && why.contains("`pi` on the PATH"), "{why}");
    }

    #[test]
    fn one_too_old_names_what_it_lacks_its_version_and_how_to_update() {
        let old = CLAUDE_HELP.replace("  --permission-prompts <mode>\n", "").replace("  --replay-user-messages\n", "");
        let why = judge(Agent::Claude, "box", &said("1.0.3 (Claude Code)", &old)).unwrap_err();
        assert!(why.contains("Claude Code on box (1.0.3 (Claude Code)) is too old"), "{why}");
        assert!(why.contains("--replay-user-messages, --permission-prompts") && why.contains("Update it there (`claude update`), then start again"), "{why}");
        let codex = judge(Agent::Codex, "box", &said("codex-cli 0.1.0", "")).unwrap_err();
        assert!(codex.contains("it has no app-server. Update it there, then start again"), "{codex}");
    }

    #[test]
    fn the_question_is_asked_in_the_environment_the_agent_starts_in() {
        let s = script(Agent::Claude);
        assert!(s.starts_with(r#"[ -r "$HOME/.cue/env.sh" ] && . "$HOME/.cue/env.sh";"#), "as MACHINE_SCRIPT: {s}");
        assert!(s.contains("command -v claude") && s.contains("claude --help"));
        assert!(script(Agent::Codex).contains("codex app-server --help"));
    }

    /// The real agents on this Mac, judged as a machine's would be (skipped where one isn't installed).
    #[test]
    fn the_agents_on_this_mac_pass() {
        for a in [Agent::Claude, Agent::Pi, Agent::Codex] {
            let out = std::process::Command::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())).args(["-lc", &script(a)]).output();
            let Ok(out) = out else { continue };
            let said = String::from_utf8_lossy(&out.stdout).to_string();
            if said.trim() == "MISSING" {
                continue;
            }
            assert_eq!(judge(a, "this Mac", &said), Ok(()), "{a:?}: {said}");
        }
    }
}

/// A real machine (CUE_TEST_REAL_MACHINE=<ssh host>, one of your machines in Cue): its agents asked as a start asks.
#[cfg(test)]
mod real {
    use super::*;

    #[test]
    #[ignore = "a real machine: CUE_TEST_REAL_MACHINE=<host>"]
    fn a_real_machines_agents_are_judged_as_a_start_would() {
        let Ok(host) = std::env::var("CUE_TEST_REAL_MACHINE") else { return };
        for a in [Agent::Claude, Agent::Pi, Agent::Codex] {
            let said = crate::machines::run(&host, &script(a)).expect("ssh");
            eprintln!("{a:?}: {:?}", judge(a, &host, &said));
        }
    }
}

/// `on_machine` over SSH (testing::FakeSsh: the machine runs what Cue sends it, in a home of its own), its agents on
/// the PATH its `~/.cue/env.sh` sets, as a real machine's are.
#[cfg(test)]
mod over_ssh {
    use super::*;
    use crate::testing::{FakeSsh, World, RUNS_IT};

    /// A program `name` in `home`'s `bin` that prints `out` for `--version` and `help` for anything else.
    fn agent(home: &std::path::Path, name: &str, version: &str, help: &str) {
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\ncase \"$1\" in --version) echo '{version}' ;; *) printf '%s\\n' '{help}' ;; esac\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn a_start_asks_once_refuses_whats_missing_or_old_and_remembers_what_passed() {
        let _w = World::new();
        let ssh = FakeSsh::new();
        ssh.host("agents-box", RUNS_IT);
        let m = crate::machines::add("agents-box", "").unwrap();
        let home = ssh.home("agents-box");
        // Cue writes the machine's env.sh the first time it reaches it (your login shell's environment there): then
        // its agents' folder goes first on that PATH, as an install there would put it.
        crate::machines::run(&m.host, "true").unwrap();
        let env = home.join(".cue/env.sh");
        let mut e = std::fs::read_to_string(&env).unwrap_or_default();
        e.push_str(&format!("PATH=\"{}/bin:/usr/bin:/bin\"; export PATH\n", home.display()));
        std::fs::write(&env, e).unwrap();
        let full = needs(Agent::Claude).join("\n");
        agent(&home, "claude", "2.1.319 (Claude Code)", &full);
        agent(&home, "pi", "0.74.2", "--mode <mode>\n--session <path|id>");

        assert_eq!(on_machine(&m.name, Agent::Claude), Ok(()));
        let asked = ssh.calls_with("agents-box", "command -v claude");
        assert_eq!(on_machine(&m.name, Agent::Claude), Ok(()));
        assert_eq!(ssh.calls_with("agents-box", "command -v claude"), asked, "passed once: not asked again");

        let pi = on_machine(&m.name, Agent::Pi).unwrap_err();
        assert!(pi.contains("Pi on agents-box (0.74.2) is too old") && pi.contains("it has no --session-id. Update it there (`pi update self`)"), "{pi}");
        let codex = on_machine(&m.name, Agent::Codex).unwrap_err();
        assert!(codex.starts_with("Codex isn't installed on agents-box"), "{codex}");

        // Updated there: the next start asks again, and goes ahead.
        agent(&home, "pi", "0.87.1", "--mode <mode>\n--session-id <id>");
        assert_eq!(on_machine(&m.name, Agent::Pi), Ok(()));
    }
}
