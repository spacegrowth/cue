//! The agents on this Mac and your machines as processes: which ones can be started here, and ending one
//! so its terminal can close.

use super::tmux::machine_of;
use crate::model::Origin;
use std::process::Command;

/// Which agents this Mac can start (a login shell's PATH: the one your terminal has), asked once.
pub fn agents_installed() -> Vec<String> {
    static FOUND: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            ["claude", "pi", "codex"]
                .into_iter()
                .filter(|a| Command::new(&shell).args(["-lic", &format!("command -v {a}")]).output().map(|o| o.status.success() && !o.stdout.is_empty()).unwrap_or(false))
                .map(String::from)
                .collect()
        })
        .clone()
}

/// End an agent's process so its tab can close without a "processes are running" prompt: SIGTERM
/// (Claude Code, Pi and Codex save as they go and exit on it), then wait up to 3s.
pub fn end_agent(pid: i32) -> Result<(), String> {
    if pid <= 0 {
        return Err("Cue doesn't know this session's process".into());
    }
    let alive = || unsafe { libc::kill(pid, 0) == 0 };
    if !alive() {
        return Ok(());
    }
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !alive() {
            return Ok(());
        }
    }
    Err("it didn't quit within 3 seconds (its tab is left open)".into())
}

/// End a session's agent on its machine, where it runs in tmux and Cue knows no pid. One SIGTERM to
/// every process in its pane, not one process group: the agent can be in a group of its own (Claude
/// Code makes one), and its shell ignores SIGTERM while it waits on its child. The shell then stays,
/// for close_tab's kill-pane to sweep up, as it does for a pane here. One script on the machine does
/// the walk, the signal and the wait: nothing to poll over SSH.
pub fn end_machine_agent(o: &Origin) -> Result<(), String> {
    let m = machine_of(o)?;
    let pane_pid = match crate::machines::tmux(&m.host, &["display-message", "-p", "-t", &o.tmux_pane, "#{pane_pid}"]) {
        Ok(p) => p.trim().to_string(),
        Err(_) => return Ok(()), // its pane is gone already: nothing to end
    };
    // Its pane's own pid only (list-panes would name every pane in its window).
    if pane_pid.is_empty() || pane_pid == "0" || !pane_pid.chars().all(|c| c.is_ascii_digit()) {
        return Ok(());
    }
    let script = r#"
p='PANE_PID'
# The pane's processes, the first one and everything below it (its shell is the first).
next=" $p"; all=""
while [ -n "$next" ]; do
  prev="$next"; next=""
  for q in $prev; do
    all="$all $q"
    for r in $(ps -eo pid=,ppid= | awk -v q="$q" '$2==q {print $1}'); do next="$next $r"; done
  done
done
# All of them at once, so nothing is reparented before it gets the signal.
kill -TERM $all 2>/dev/null
# Wait for everything but the shell to go (its children are gone when the agent is), up to 3 s.
i=0
while [ $i -lt 15 ]; do
  n=0
  for q in $all; do
    for r in $(ps -eo pid=,ppid= | awk -v q="$q" '$2==q {print $1}'); do n=$((n+1)); done
  done
  [ "$n" -eq 0 ] && { echo GONE; exit 0; }
  sleep 0.2 2>/dev/null || sleep 1
  i=$((i+1))
done
echo STILL
"#.replace("PANE_PID", &pane_pid);
    match crate::machines::run(&m.host, &script)?.trim() {
        "GONE" => Ok(()),
        _ => Err("it didn't quit within 3 seconds (its pane is left open)".into()),
    }
}

