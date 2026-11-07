//! relay's and pi-lead's own commands, here or on a machine, and what each button in the window runs.

use super::files::{home, pilead_dir};
use super::remote::{pilead_bin_on, relay_bin_on};
use super::Member;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A relay / pi-lead command, on this Mac or on the machine the session runs on.
#[derive(Debug, PartialEq)]
pub struct Cmd {
    pub bin: String,
    pub args: Vec<String>,
    /// The machine to run it on ("" for this Mac).
    pub machine: String,
}

impl Cmd {
    fn here(bin: PathBuf, args: Vec<String>) -> Cmd {
        Cmd { bin: bin.to_string_lossy().into_owned(), args, machine: String::new() }
    }

    /// Run it: Ok(what it printed) or Err(its last line of complaint). Blocking.
    pub fn run(&self) -> Result<String, String> {
        let last = |t: &str, or: &str| t.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(or).trim().to_string();
        if self.machine.is_empty() {
            let out = std::process::Command::new(&self.bin).args(&self.args).output().map_err(|e| format!("couldn't run {}: {e}", self.bin))?;
            return if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err(last(&String::from_utf8_lossy(&out.stderr), "it failed")) };
        }
        let m = crate::machines::get(&self.machine).ok_or(format!("{} isn't one of your machines any more", self.machine))?;
        let line = std::iter::once(&self.bin).chain(&self.args).map(|a| crate::terminal::focus::shq(a)).collect::<Vec<_>>().join(" ");
        // Its own words either way (Cue's SSH hides a script's stderr), and its exit code last.
        let out = crate::machines::run(&m.host, &format!("{line} 2>&1; printf '\\n__CUE_EXIT__:%d\\n' \"$?\""))?;
        match out.rsplit_once("\n__CUE_EXIT__:") {
            Some((said, code)) if code.trim() == "0" => Ok(said.to_string()),
            Some((said, _)) => Err(last(said, "it failed")),
            None => Err("it didn't run there".into()),
        }
    }
}

/// relay's command where this member runs: its machine's (as last listed there), or this Mac's.
pub(super) fn relay_for(m: &Member) -> Result<Cmd, String> {
    if m.machine.is_empty() {
        return relay_bin().map(|b| Cmd::here(b, vec![])).ok_or_else(|| "can't find relay's command (is the claude-relay plugin installed?)".to_string());
    }
    relay_bin_on(&m.machine).map(|bin| Cmd { bin, args: vec![], machine: m.machine.clone() }).ok_or_else(|| format!("can't find relay's command on {}", m.machine))
}

/// pilead's command where this executor runs: where it was launched (this Mac), or its machine's
/// (as last listed there).
pub(super) fn pilead_for(m: &Member) -> Result<Cmd, String> {
    if m.machine.is_empty() {
        return pilead_bin(&m.id).map(|b| Cmd::here(b, vec![])).ok_or_else(|| "can't find pilead's command".to_string());
    }
    pilead_bin_on(&m.machine).map(|bin| Cmd { bin, args: vec![], machine: m.machine.clone() }).ok_or_else(|| format!("can't find pilead's command on {}", m.machine))
}

/// What a relay / pi-lead button in the window does.
#[derive(Debug, PartialEq)]
pub enum Act {
    /// Type this into a session (Cue's session id): the lead runs it as a command.
    Send { to: String, text: String },
    /// Run relay's own CLI; on success, say this.
    Run(Cmd, String),
    /// relay's diff page for an executor on a machine: written there (its path is what relay
    /// prints), brought here, then opened.
    RemoteDiff(Cmd),
    /// Open a page that's already written (pi-lead's executor writes its diff page itself).
    Open(PathBuf),
}

/// relay's own CLI, from the installed plugin (`~/.claude/plugins/installed_plugins.json`), else
/// `~/.local/bin/relay`. A window app's PATH has neither.
pub(super) fn relay_bin() -> Option<PathBuf> {
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"));
    let installed = std::fs::read_to_string(claude.join("plugins/installed_plugins.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let from_plugin = installed.as_ref().and_then(|v| v.get("plugins")).and_then(Value::as_object).and_then(|plugins| {
        plugins.iter().filter(|(k, _)| k.starts_with("relay@")).find_map(|(_, v)| v.as_array()?.first()?.get("installPath")?.as_str().map(|p| PathBuf::from(p).join("bin/relay")))
    });
    from_plugin.into_iter().chain([home().join(".local/bin/relay")]).find(|p| p.is_file())
}

/// The newest diff page an executor wrote for itself (`sessions/<sid>/diff-NNNN.html`).
fn pilead_diff_page(root: &Path, id: &str) -> Option<PathBuf> {
    let dir = root.join("sessions").join(id);
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("diff-") && n.ends_with(".html"))).max()
}

/// pi-lead's own CLI, from where its executor was launched (`sessions/<id>/bootstrap.sh` runs
/// `pi … -e <pi-lead>/extensions/pi-lead.ts`), else ~/.local/bin/pilead.
fn pilead_bin(id: &str) -> Option<PathBuf> {
    let boot = std::fs::read_to_string(pilead_dir().join("sessions").join(id).join("bootstrap.sh")).unwrap_or_default();
    let from_boot = boot.split_whitespace().find(|w| w.ends_with("/extensions/pi-lead.ts")).and_then(|ext| Path::new(ext).parent()?.parent().map(|root| root.join("bin/pilead")));
    from_boot.into_iter().chain([home().join(".local/bin/pilead")]).find(|p| p.is_file())
}

/// `act` for a session Cue knows: `m` is its record.
pub(super) fn act_on(m: &Member, session_id: &str, action: &str) -> Result<Act, String> {
    let relay = |args: &[&str]| relay_for(m).map(|c| Cmd { args: args.iter().map(|a| a.to_string()).collect(), ..c });
    match (m.plugin.as_str(), m.role.as_str(), action) {
        (plugin, "executor", "review") => {
            if m.lead_armed != Some(true) {
                return Err(format!("its lead{} isn't listening", if m.lead_name.is_empty() { String::new() } else { format!(" ({})", m.lead_name) }));
            }
            let cmd = if plugin == "pilead" { "/pilead:review" } else { "/relay:review" };
            Ok(Act::Send { to: m.lead.clone(), text: format!("{cmd} {}", m.id) })
        }
        ("relay", "executor", "diff") if !m.machine.is_empty() => Ok(Act::RemoteDiff(relay(&["diff", &m.id])?)),
        ("relay", "executor", "diff") => Ok(Act::Run(relay(&["diff", &m.id, "--open"])?, "Opened in your browser".into())),
        // A lead's auto mode on or off (it goes ahead on routine steps without asking you).
        ("relay", "lead", "auto-on" | "auto-off") => {
            let on = action == "auto-on";
            Ok(Act::Run(relay(&["auto", if on { "on" } else { "off" }, "--session", session_id])?, format!("Auto mode {}", if on { "on" } else { "off" })))
        }
        // An executor whose agent exited before it reported: relay closes its record where it runs.
        ("relay", "executor", "close") if matches!(m.status.as_str(), "stalled" | "dead") => Ok(Act::Run(relay(&["close", &m.id])?, "Closed".into())),
        ("relay", "executor", "close") => Err("only an executor whose agent exited can be closed from here".into()),
        // pilead writes its diff page on the machine it runs on and Cue has no way to bring it here yet.
        ("pilead", "executor", "diff") if !m.machine.is_empty() => Err(format!("pilead's diff page is written on {}; Cue can't bring it here yet", m.machine)),
        ("pilead", "executor", "close") if matches!(m.status.as_str(), "stalled" | "dead") => Ok(Act::Run(pilead_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })?, "Closed".into())),
        ("pilead", "executor", "diff") => pilead_diff_page(&pilead_dir(), &m.id).map(Act::Open).ok_or_else(|| "it hasn't written a diff page yet".into()),
        _ => Err(format!("no {action} for this session")),
    }
}
