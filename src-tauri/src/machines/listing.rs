//! What Cue reads from each of your machines in one SSH call: the Claude sessions open in a live process
//! there (so it can tell which of its sessions there ended while it wasn't looking), and the tmux panes
//! on a screen before Claude Code starts (it runs no hook until it's past them, so nothing else would tell).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Per machine: its tmux panes on a screen before Claude Code starts (pane id -> which), as last listed.
static SCREENS: Mutex<Option<HashMap<String, HashMap<String, crate::terminal::dialog::Setup>>>> = Mutex::new(None);

/// What runs on a machine: the Claude sessions open in a live process there, then the bottom of every tmux pane.
pub(crate) const LIST: &str = r#"c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
echo "__CUE_LIVE__"
for f in "$c"/sessions/*.json; do
  # Claude Code writes these without a final newline, and sed then prints none: one id per line regardless.
  [ -f "$f" ] && kill -0 "$(basename "$f" .json)" 2>/dev/null && { sed -n 's/.*"sessionId": *"\([^"]*\)".*/\1/p' "$f"; echo; }
done
echo "__CUE_SCREENS__"
t=$(command -v tmux 2>/dev/null)
for x in /opt/homebrew/bin/tmux /usr/local/bin/tmux /usr/bin/tmux; do [ -n "$t" ] || { [ -x "$x" ] && t="$x"; }; done
[ -n "$t" ] && "$t" list-panes -a -F '#{pane_id} #{session_name}' 2>/dev/null | while read -r p n; do
  echo "@@PANE $p $n"; "$t" capture-pane -p -t "$p" 2>/dev/null | grep -v '^[[:space:]]*$' | tail -n 6
done
exit 0"#;

/// Re-read one machine (blocking: an SSH round trip): whether its screens changed, and the Claude sessions
/// running there (None: it couldn't be reached; it keeps what was last read from it).
pub fn refresh(machine: &str, host: &str) -> (bool, Option<HashSet<String>>) {
    let Ok(out) = crate::machines::run(host, LIST) else { return (false, None) };
    let screens = screens_of(&out);
    let mut all = crate::locking::lock(&SCREENS, "SCREENS");
    let old = all.get_or_insert_with(HashMap::new).insert(machine.to_string(), screens.clone());
    drop(all);
    (old.as_ref() != Some(&screens), Some(live_of(&out)))
}

/// What a machine's tmux pane (`pane`, its id there) showed at the last listing, if it's a screen before
/// Claude Code starts. None as well when that machine hasn't been listed yet.
pub fn setup(machine: &str, pane: &str) -> Option<crate::terminal::dialog::Setup> {
    crate::locking::lock(&SCREENS, "SCREENS").as_ref()?.get(machine)?.get(pane).copied()
}

/// Whether this machine has been listed since Cue started (or since it was last removed).
pub fn known(machine: &str) -> bool {
    crate::locking::lock(&SCREENS, "SCREENS").as_ref().is_some_and(|r| r.contains_key(machine))
}

/// Forget machines that aren't yours any more (removed in + New → Machine): whether any was dropped.
pub fn retain(machines: &[String]) -> bool {
    crate::locking::lock(&SCREENS, "SCREENS").as_mut().is_some_and(|r| {
        let n = r.len();
        r.retain(|m, _| machines.contains(m));
        r.len() != n
    })
}

/// LIST's panes on a screen before Claude Code starts: pane id -> which screen.
pub(crate) fn screens_of(out: &str) -> HashMap<String, crate::terminal::dialog::Setup> {
    let Some((_, part)) = out.split_once("__CUE_SCREENS__") else { return HashMap::new() };
    part.split("@@PANE ")
        .skip(1)
        .filter_map(|chunk| {
            let (head, screen) = chunk.split_once('\n').unwrap_or((chunk, ""));
            let (pane, _session) = head.trim().split_once(' ')?;
            Some((pane.to_string(), crate::terminal::dialog::setup_on(screen)?))
        })
        .collect()
}

/// The Claude sessions LIST found running.
pub(crate) fn live_of(out: &str) -> HashSet<String> {
    let out = out.split_once("__CUE_SCREENS__").map_or(out, |(o, _)| o);
    out.split_once("__CUE_LIVE__").map(|(_, l)| l.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_script_gives_one_live_session_per_line() {
        // Two live sessions whose files end without a newline (as Claude Code writes them), and one dead.
        let home = std::env::temp_dir().join(format!("cue-machine-list-{}", std::process::id()));
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        let me = std::process::id();
        std::fs::write(home.join(format!("sessions/{me}.json")), r#"{"pid":1,"sessionId":"L1","cwd":"/tmp"}"#).unwrap();
        std::fs::write(home.join(format!("sessions/{}.json", std::os::unix::process::parent_id())), r#"{"pid":2,"sessionId":"C1","cwd":"/tmp"}"#).unwrap();
        std::fs::write(home.join("sessions/999999.json"), r#"{"pid":3,"sessionId":"DEAD"}"#).unwrap();
        let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("HOME", &home).env("TMUX_TMPDIR", &home).env_remove("TMUX").args(["-c", LIST]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(live_of(&out), HashSet::from(["L1".to_string(), "C1".to_string()]), "one id per line, the dead one left out: {out:?}");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_machines_panes_on_a_screen_before_claude_code_starts_are_read_from_its_listing() {
        let out = "__CUE_LIVE__\nC1\n__CUE_SCREENS__\n\
@@PANE %4 hello-md_2\nSecurity notes:\n1. Claude can make mistakes.\nPress Enter to continue…\n\
@@PANE %5 cue-app\n❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel\n\
@@PANE %6 work\n✻ Done\n❯ \n";
        let screens = screens_of(out);
        assert_eq!(screens.get("%4"), Some(&crate::terminal::dialog::Setup::Enter));
        assert_eq!(screens.get("%5"), Some(&crate::terminal::dialog::Setup::Trust));
        assert!(!screens.contains_key("%6"), "at its prompt: nothing to say");
        assert_eq!(live_of(out), HashSet::from(["C1".to_string()]), "the panes never read as live sessions");
    }
}
