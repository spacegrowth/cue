//! What Cue reads from each of your machines in one SSH call: the Claude sessions open in a live process
//! there (so it can tell which of its sessions there ended while it wasn't looking).

use std::collections::HashSet;

/// What runs on a machine: the Claude sessions open in a live process there.
pub(crate) const LIST: &str = r#"c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
echo "__CUE_LIVE__"
for f in "$c"/sessions/*.json; do
  # Claude Code writes these without a final newline, and sed then prints none: one id per line regardless.
  [ -f "$f" ] && kill -0 "$(basename "$f" .json)" 2>/dev/null && { sed -n 's/.*"sessionId": *"\([^"]*\)".*/\1/p' "$f"; echo; }
done
exit 0"#;

/// Re-read one machine (blocking: an SSH round trip): the Claude sessions running there (None: it couldn't be
/// reached).
pub fn refresh(host: &str) -> Option<HashSet<String>> {
    crate::machines::run(host, LIST).ok().map(|out| live_of(&out))
}

/// The Claude sessions LIST found running.
pub(crate) fn live_of(out: &str) -> HashSet<String> {
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

}
