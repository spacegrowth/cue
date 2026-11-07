//! tmux as a test wants it, on the test's own thread (test_tmux::stand_in): a shell script that notes each
//! call and answers as told, so typing into a pane and opening its terminal can be driven through a tmux
//! that hangs, refuses a key, shows a shell prompt, or isn't installed, without a tmux server.

use super::test_tmux::{stand_in, StandIn};
use std::path::PathBuf;

/// A stand-in tmux for an agent's pane. Each call's arguments are noted (`calls`); `set-buffer` keeps the
/// pasted text and `capture-pane` shows it in the agent's box, `display-message` says the agent is running,
/// and anything else succeeds saying nothing. `answering` puts the test's own answers first.
pub(crate) struct FakeTmux {
    dir: PathBuf,
    stand_ins: Vec<StandIn>,
}

/// The answers an agent's pane gives, after the test's own (sh `case` arms on the tmux command).
const AGENT: &str = r#"
  display-message) printf 'claude\n> ' ;;
  set-buffer) printf '%s' "$5" > "$DIR/pasted" ;;
  capture-pane) printf '> '; cat "$DIR/pasted" 2>/dev/null ;;
"#;

impl FakeTmux {
    /// An agent's pane.
    pub(crate) fn agent() -> Self {
        Self::answering("")
    }

    /// An agent's pane, but `arms` (sh `case` arms on the tmux command, `$1` after any `-u`) answer first,
    /// e.g. `send-keys) exit 1 ;;`.
    pub(crate) fn answering(arms: &str) -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cue-fake-tmux-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!(
            "#!/bin/sh\nDIR='{}'\nprintf '%s\\n' \"$*\" >> \"$DIR/calls\"\n[ \"$1\" = -u ] && shift\ncase \"$1\" in\n{arms}\n{AGENT}\n  *) ;;\nesac\n",
            dir.display()
        );
        let on = stand_in("tmux", &executable(&dir, "tmux", &script));
        Self { dir, stand_ins: vec![on] }
    }

    /// No tmux on this Mac: a name the system can't find.
    pub(crate) fn missing() -> StandIn {
        stand_in("tmux", "cue-test-no-such-tmux")
    }

    /// Each call's arguments, joined by spaces, in order.
    pub(crate) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default().lines().map(String::from).collect()
    }

    /// The calls of tmux command `cmd` ("send-keys").
    pub(crate) fn calls_of(&self, cmd: &str) -> Vec<String> {
        self.calls().into_iter().filter(|c| c.trim_start_matches("-u ").starts_with(cmd)).collect()
    }
}

/// `script` as program `name` in `dir`, ready to run; its path.
fn executable(dir: &std::path::Path, name: &str, script: &str) -> String {
    let bin = dir.join(name);
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    bin.to_string_lossy().into_owned()
}

impl Drop for FakeTmux {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
