//! How a hosted session's agent is started: headless, JSON lines both ways, every permission and question
//! sent to Cue rather than to a terminal. Claude Code: stream-json, your messages echoed back as it takes them.
//! Pi: its RPC mode, Cue's Pi extension still the gate (it asks Cue, never a terminal). Through your login
//! shell, as Cue starts agents everywhere else, so the program, your PATH and your MCP servers' programs are
//! found as in your terminal. Values reach the shell as arguments and variables, never inside its script, so a
//! path or a model name can't run as more shell.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What a session is started with.
#[derive(Clone, Debug)]
pub struct Spec {
    pub agent: Agent,
    /// The folder it works in.
    pub cwd: PathBuf,
    pub start: Start,
    /// How much it may do without asking, as + New session's Permissions names it ("ask", "auto", "edits",
    /// "plan", "skip"; empty is "ask").
    pub permissions: String,
    /// None: the agent's own default.
    pub model: Option<String>,
    /// More arguments of Cue's own (an MCP selection, say), each passed whole.
    pub args: Vec<String>,
    pub program: Program,
    /// The machine it runs on (+ New → Machine), by name; empty: this Mac.
    pub machine: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// A new conversation with this id (Cue picks it, so it knows the session before Claude says anything).
    New(String),
    /// An earlier conversation, picked up where it stopped.
    Resume(String),
}

impl Start {
    /// A new conversation with an id of its own.
    pub fn fresh() -> Self {
        Start::New(crate::focus::new_session_id())
    }

    pub fn session_id(&self) -> &str {
        match self {
            Start::New(id) | Start::Resume(id) => id,
        }
    }
}

/// Which agent a session runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Pi,
    Codex,
}

impl Agent {
    /// By Cue's name for it (an origin's `harness`).
    pub fn named(harness: &str) -> Option<Agent> {
        match harness {
            "claude" => Some(Agent::Claude),
            "pi" => Some(Agent::Pi),
            "codex" => Some(Agent::Codex),
            _ => None,
        }
    }

    /// Its program, and Cue's name for it.
    pub fn program(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Pi => "pi",
            Agent::Codex => "codex",
        }
    }

    /// Its name, for saying so.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Pi => "Pi",
            Agent::Codex => "Codex",
        }
    }

    pub(super) fn protocol(self, spec: &Spec) -> Box<dyn super::protocol::Protocol> {
        match self {
            Agent::Claude => Box::<super::protocol::Claude>::default(),
            Agent::Pi => Box::<super::pi::Pi>::default(),
            Agent::Codex => Box::new(super::codex::Codex::new(spec)),
        }
    }
}

/// Which program runs as the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// The agent's own (`claude`, `pi`), through your login shell.
    Agent,
    /// This program, run directly with the same arguments (the tests' stand-in for the agent).
    #[cfg(test)]
    At(PathBuf),
}

/// The flags every hosted Claude runs with: JSON lines in and out, Cue answering every permission and
/// question (`stdio`: on those same lines), your messages echoed back as Claude takes them.
const HEADLESS: [&str; 11] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--replay-user-messages",
    "--permission-prompts",
    "host",
    "--permission-prompt-tool",
    "stdio",
];

/// The command for `spec`, its input and output piped, in a process group of its own (so closing it reaches
/// everything it started), with the environment a hosted session needs.
pub(super) fn command(spec: &Spec) -> Result<Command, String> {
    let args = arguments(spec)?;
    let mut c = match &spec.program {
        Program::Agent => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            let mut c = Command::new(shell);
            // -l: your profile (PATH); -i: what your shell sets up for you. `$0` names it in `ps`.
            // Your PATH first, then where Cue also finds agents (Codex inside the ChatGPT app).
            let script = format!("cd -- \"$CUE_CWD\" && PATH=\"$PATH:$CUE_AGENT_PATH\" exec {} \"$@\"", spec.agent.program());
            c.args(["-lic", &script, "cue-hosted"]).args(&args);
            c
        }
        #[cfg(test)]
        Program::At(path) => {
            let mut c = Command::new(path);
            c.args(&args);
            c
        }
    };
    environment(&mut c, spec);
    use std::os::unix::process::CommandExt;
    c.current_dir(&spec.cwd).process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok(c)
}

/// A command as cue-host runs it: the program and its arguments, the whole environment it gets, and its folder.
pub(super) struct HostCommand {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
}

/// `spec`'s command for cue-host: its environment is Cue's own with what `command` sets and removes, spelled out,
/// so the host runs exactly that, whatever its own environment.
pub(super) fn for_host(spec: &Spec) -> Result<HostCommand, String> {
    let c = command(spec)?;
    let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
    let argv: Vec<String> = std::iter::once(c.get_program()).chain(c.get_args()).map(text).collect();
    let mut env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    for (k, v) in c.get_envs() {
        match v {
            Some(v) => env.insert(text(k), text(v)),
            None => env.remove(&text(k)),
        };
    }
    let cwd = c.get_current_dir().map_or_else(|| spec.cwd.to_string_lossy().into_owned(), |d| d.to_string_lossy().into_owned());
    Ok(HostCommand { argv, env: env.into_iter().collect(), cwd })
}

/// What /bin/sh runs on a machine to start an agent (`for_machine`): your environment there, the session's folder
/// (`~` your home there), Cue's Pi extension for a Pi that doesn't have it installed (its gate), then the agent.
const MACHINE_SCRIPT: &str = r#"[ -r "$HOME/.cue/env.sh" ] && . "$HOME/.cue/env.sh"
case $CUE_CWD in "~"|"~/"*) CUE_CWD="$HOME${CUE_CWD#\~}" ;; esac
cd -- "$CUE_CWD" || { echo "$CUE_CWD isn't a folder there" >&2; exit 1; }
if [ -n "${CUE_PI_EXTENSION:-}" ] && [ ! -e "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/extensions/cue.ts" ] && [ -r "$HOME/$CUE_PI_EXTENSION" ]; then
  set -- -e "$HOME/$CUE_PI_EXTENSION" "$@"
fi
exec "$CUE_AGENT" "$@""#;

/// `spec`'s command for cue-host on its machine, where Cue knows neither the environment nor where the agent is: run
/// by /bin/sh, which takes your environment there as Cue keeps it (`~/.cue/env.sh`, your login shell's: PATH, nvm…;
/// machines/mod.rs), goes to its folder (`~` there is your home there) and runs the agent found on that PATH. The host
/// adds its own environment underneath (`inherit`).
pub(super) fn for_machine(spec: &Spec) -> Result<HostCommand, String> {
    let args = arguments(spec)?;
    let program = match &spec.program {
        Program::Agent => spec.agent.program().to_string(),
        #[cfg(test)]
        Program::At(path) => path.to_string_lossy().into_owned(),
    };
    let script = MACHINE_SCRIPT;
    let argv: Vec<String> = ["/bin/sh", "-c", script, "cue-hosted"].into_iter().map(String::from).chain(args).collect();
    let mut env = vec![(super::HOSTED_ENV.to_string(), "1".to_string()), ("CUE_CWD".into(), spec.cwd.to_string_lossy().into_owned()), ("CUE_AGENT".into(), program), ("SHELL_SESSIONS_DISABLE".into(), "1".into())];
    if spec.agent == Agent::Pi {
        env.push(("CUE_PI_EXTENSION".into(), super::machine_host::PI_EXTENSION.into()));
    }
    if let Ok((Some((k, v)), _)) = crate::focus::permission_flags(spec.agent.program(), &spec.permissions) {
        env.push((k.to_string(), v.to_string()));
    }
    Ok(HostCommand { argv, env, cwd: "/".into() })
}

/// The agent's arguments for `spec`.
pub(super) fn arguments(spec: &Spec) -> Result<Vec<String>, String> {
    let id = spec.start.session_id();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("“{id}” isn't a session id"));
    }
    match spec.agent {
        Agent::Pi => return pi_arguments(spec, id),
        // Its conversation, model and permissions go in what Cue sends it (codex.rs); on the line only its own
        // settings (`-c key=value`) from + New's flags.
        Agent::Codex => return Ok(std::iter::once("app-server".to_string()).chain(spec.args.iter().cloned()).collect()),
        Agent::Claude => {}
    }
    let (_, permission) = crate::focus::permission_flags("claude", &spec.permissions)?;
    let mut a: Vec<String> = HEADLESS.iter().chain(permission).map(|s| s.to_string()).collect();
    a.extend(match &spec.start {
        Start::New(_) => ["--session-id".to_string(), id.to_string()],
        Start::Resume(_) => ["--resume".to_string(), id.to_string()],
    });
    if let Some(m) = spec.model.as_deref().filter(|m| !m.is_empty()) {
        a.extend(["--model".to_string(), m.to_string()]);
    }
    a.extend(spec.args.iter().cloned());
    Ok(a)
}

/// Pi's: its RPC mode on the session `id` (Pi starts it if it's new and picks it up if not, so a resume needs
/// nothing more). Its permissions are its gate's (Cue's Pi extension), set by a variable (`environment`).
fn pi_arguments(spec: &Spec, id: &str) -> Result<Vec<String>, String> {
    crate::focus::permission_flags("pi", &spec.permissions)?;
    let mut a = vec!["--mode".to_string(), "rpc".to_string(), "--session-id".to_string(), id.to_string()];
    if let Some(m) = spec.model.as_deref().filter(|m| !m.is_empty()) {
        a.extend(["--model".to_string(), m.to_string()]);
    }
    a.extend(spec.args.iter().cloned());
    Ok(a)
}

/// Marked as hosted (Cue's hook stays out of it), and none of the variables that would make Claude think it
/// runs inside another session or a tmux pane of Cue's own process (when Cue was started from one).
fn environment(c: &mut Command, spec: &Spec) {
    for k in ["TMUX", "TMUX_PANE", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"] {
        c.env_remove(k);
    }
    c.env(super::HOSTED_ENV, "1").env("CUE_CWD", &spec.cwd).env("CUE_AGENT_PATH", crate::focus::AGENT_PATH).env("SHELL_SESSIONS_DISABLE", "1");
    if let Ok((Some((k, v)), _)) = crate::focus::permission_flags(spec.agent.program(), &spec.permissions) {
        c.env(k, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(start: Start) -> Spec {
        Spec { agent: Agent::Claude, cwd: "/tmp".into(), start, permissions: String::new(), model: None, args: vec![], program: Program::Agent, machine: String::new() }
    }

    #[test]
    fn a_new_session_runs_headless_with_the_id_cue_picked() {
        let a = arguments(&spec(Start::New("0f0e-11".into()))).unwrap();
        for flag in ["-p", "--replay-user-messages", "--permission-prompt-tool"] {
            assert!(a.contains(&flag.to_string()), "{flag} in {a:?}");
        }
        assert_eq!(a.windows(2).find(|w| w[0] == "--permission-prompts").map(|w| w[1].as_str()), Some("host"));
        assert_eq!(a.windows(2).find(|w| w[0] == "--session-id").map(|w| w[1].as_str()), Some("0f0e-11"));
        assert!(!a.contains(&"--resume".to_string()));
    }

    #[test]
    fn a_resumed_session_takes_its_permissions_model_and_cues_own_arguments() {
        let s = Spec { permissions: "plan".into(), model: Some("haiku".into()), args: vec!["--strict-mcp-config".into()], ..spec(Start::Resume("abc".into())) };
        let a = arguments(&s).unwrap();
        let after = |f: &str| a.windows(2).find(|w| w[0] == f).map(|w| w[1].clone());
        assert_eq!((after("--resume"), after("--permission-mode"), after("--model")), (Some("abc".into()), Some("plan".into()), Some("haiku".into())));
        assert_eq!(a.last().map(String::as_str), Some("--strict-mcp-config"));
    }

    #[test]
    fn an_id_that_isnt_one_or_a_mode_claude_lacks_is_refused() {
        assert!(arguments(&spec(Start::New("a; rm -rf ~".into()))).is_err());
        assert!(arguments(&spec(Start::New(String::new()))).is_err());
        assert!(arguments(&Spec { permissions: "sometimes".into(), ..spec(Start::fresh()) }).is_err());
    }

    #[test]
    fn pi_runs_its_rpc_mode_on_the_session_id_with_skip_turning_its_gate_off() {
        let s = Spec { agent: Agent::Pi, permissions: "skip".into(), model: Some("deepseek/deepseek-flash".into()), ..spec(Start::Resume("P1".into())) };
        assert_eq!(arguments(&s).unwrap(), ["--mode", "rpc", "--session-id", "P1", "--model", "deepseek/deepseek-flash"]);
        let c = command(&s).unwrap();
        let gate = c.get_envs().find(|(k, _)| *k == "CUE_PI_GATE").and_then(|(_, v)| v.map(|v| v.to_string_lossy().to_string()));
        assert_eq!(gate.as_deref(), Some("off"));
        assert!(arguments(&Spec { permissions: "plan".into(), ..s }).is_err(), "Pi has no plan mode");
    }

    #[test]
    fn for_the_host_it_is_the_same_command_with_its_whole_environment_spelled_out() {
        let s = spec(Start::New("0f0e-11".into()));
        let HostCommand { argv, env, cwd } = for_host(&s).unwrap();
        let c = command(&s).unwrap();
        assert_eq!(argv[0], c.get_program().to_string_lossy());
        assert_eq!(&argv[1..], c.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().as_slice());
        assert_eq!(cwd, "/tmp");
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get(crate::hosted::HOSTED_ENV), Some("1"), "marked hosted");
        assert!(get("TMUX").is_none() && get("CLAUDECODE").is_none(), "nothing that would make it think it's inside another session");
        assert_eq!(get("HOME").map(str::to_string), std::env::var("HOME").ok(), "and the rest of Cue's own");
    }

    /// The command `for_machine` gives the host, run here with HOME at a scratch folder standing in for the machine
    /// (its `.cue/env.sh` putting a stand-in agent on PATH): what the agent was started with, and where.
    fn run_on_machine(home: &std::path::Path, s: &Spec) -> String {
        let HostCommand { argv, env, cwd } = for_machine(s).unwrap();
        assert_eq!(cwd, "/");
        let out = Command::new(&argv[0]).args(&argv[1..]).env_clear().env("HOME", home).env("PATH", "/usr/bin:/bin").envs(env).output().unwrap();
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)
    }

    #[test]
    fn on_a_machine_the_agent_runs_with_your_environment_there_in_its_folder_found_on_your_path() {
        let home = std::env::temp_dir().join(format!("cue-machine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/pi")).unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::create_dir_all(home.join("my work")).unwrap();
        std::fs::write(home.join(".cue/env.sh"), format!("export PATH='{}/bin:/usr/bin:/bin'\nexport FROM_ENV=yes\n", home.display())).unwrap();
        for agent in ["claude", "pi"] {
            let fake = home.join("bin").join(agent);
            std::fs::write(&fake, "#!/bin/sh\necho \"$(basename \"$0\") in $PWD env=$FROM_ENV hosted=$CUE_HOSTED args=$*\"\n").unwrap();
            std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        }
        let s = Spec { cwd: "~/my work".into(), ..spec(Start::New("0f0e-11".into())) };
        let said = run_on_machine(&home, &s);
        assert!(said.starts_with(&format!("claude in {}/my work env=yes hosted=1 args=-p ", home.display())), "{said}");
        assert!(said.contains("--session-id 0f0e-11"), "{said}");
        // Pi, without Cue's extension installed there: loaded from where Cue put it (its gate).
        std::fs::write(home.join(".cue/pi/cue.ts"), "// gate").unwrap();
        let pi = Spec { agent: Agent::Pi, ..s.clone() };
        assert!(run_on_machine(&home, &pi).contains(&format!("args=-e {}/.cue/pi/cue.ts --mode rpc", home.display())));
        // With it installed there, Pi loads that one itself: not twice.
        std::fs::create_dir_all(home.join(".pi/agent/extensions")).unwrap();
        std::fs::write(home.join(".pi/agent/extensions/cue.ts"), "// gate").unwrap();
        assert!(run_on_machine(&home, &pi).contains("args=--mode rpc"));
        let gone = Spec { cwd: "~/nowhere".into(), ..s };
        assert!(run_on_machine(&home, &gone).contains("isn't a folder there"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_fresh_start_has_an_id_of_its_own() {
        let (a, b) = (Start::fresh(), Start::fresh());
        assert_ne!(a.session_id(), b.session_id());
        assert_eq!(a.session_id().len(), 36);
    }
}
