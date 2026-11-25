//! Cue's tmux guard: a `tmux` in Cue's bin folder (`bin/guard/tmux`, tmux_guard.sh) that Claude Code sessions
//! get first on their PATH. An agent's tmux command that would end the tmux server your own sessions run on
//! (`kill-server`, `kill-session -a`) is refused with a note on what to do instead; every other tmux command
//! runs the real tmux unchanged.
//!
//! Agents run inside your tmux and inherit its $TMUX, and tmux picks its server from $TMUX before $TMUX_TMPDIR:
//! an agent trying tmux out on a "private" server can end yours, and every session in it with it. The server
//! the agent itself runs in (its $TMUX when it started) is guarded, and so is tmux's default one.
//!
//! On the PATH by the SessionStart hook (config::wanted): Claude Code runs the lines it leaves in the session's
//! CLAUDE_ENV_FILE before each command the agent runs, so scripts it starts get the guard as well.

use std::path::{Path, PathBuf};

const SCRIPT: &str = include_str!("tmux_guard.sh");

/// What marks the lines as the guard's in a session's env file, so a resumed session doesn't add them twice.
const MARK: &str = "# Cue's tmux guard";

/// The guard's folder (its `tmux` is the only thing in it, so nothing else is put ahead on the PATH).
fn dir() -> PathBuf {
    crate::server::cue_dir().join("bin/guard")
}

/// At launch, with Cue's other scripts (hook::write_shim).
pub(crate) fn write() {
    crate::hook::put_executable(&dir().join("tmux"), SCRIPT);
}

/// SessionStart (`cue hook start claude`): the guard first on the session's PATH, and the server it runs in
/// guarded, through Claude Code's CLAUDE_ENV_FILE. Nothing when there's no env file or no guard written.
pub(crate) fn on_session_start() {
    let Some(file) = std::env::var_os("CLAUDE_ENV_FILE").filter(|f| !f.is_empty()) else { return };
    if !dir().join("tmux").exists() {
        return;
    }
    let file = PathBuf::from(file);
    if std::fs::read_to_string(&file).is_ok_and(|t| t.contains(MARK)) {
        return;
    }
    use std::io::Write;
    let lines = env_lines(&dir(), std::env::var("TMUX").ok().as_deref());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(lines.as_bytes());
    }
}

/// The shell lines that put `guard` first on the PATH (once, however often they're run) and, for an agent
/// started inside tmux ($TMUX is "socket,pid,session"), name its server as one the guard keeps.
fn env_lines(guard: &Path, tmux: Option<&str>) -> String {
    let d = guard.display().to_string();
    let mut s = format!("{MARK}\ncase \":$PATH:\" in *\":{d}:\"*) ;; *) export PATH=\"{d}:$PATH\" ;; esac\n");
    if let Some(sock) = tmux.and_then(|t| t.split(',').next()).filter(|s| !s.is_empty()) {
        s += &format!("export CUE_TMUX_PROTECT='{}'\n", sock.replace('\'', "'\\''"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A folder with the guard, a stand-in `tmux` beside it that prints what it was given, and a socket folder
    /// standing in for the user's server. The real tmux is never run.
    struct Rig {
        root: PathBuf,
    }

    impl Rig {
        fn new(tag: &str) -> Rig {
            let root = std::env::temp_dir().join(format!("cue-tmux-guard-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for d in ["guard", "real", "yours", "private"] {
                std::fs::create_dir_all(root.join(d)).unwrap();
            }
            crate::hook::put_executable(&root.join("guard/tmux"), SCRIPT);
            crate::hook::put_executable(&root.join("real/tmux"), "#!/bin/sh\necho \"ran: $*\"\n");
            Rig { root }
        }

        /// The guard run with `args`, $TMUX naming the stand-in for your server unless `in_tmux` is false, and
        /// `env` on top: whether it ran the real tmux, and what it said.
        fn run(&self, args: &[&str], in_tmux: bool, env: &[(&str, &str)]) -> (bool, String) {
            let path = format!("{}:{}:/usr/bin:/bin", self.root.join("guard").display(), self.root.join("real").display());
            let mut c = Command::new("/bin/sh");
            c.arg(self.root.join("guard/tmux")).args(args).env_clear().env("PATH", path).env("TMUX_TMPDIR", self.root.join("tmpdir"));
            let yours = self.root.join("yours/default");
            c.env("CUE_TMUX_PROTECT", &yours);
            if in_tmux {
                c.env("TMUX", format!("{},123,0", yours.display()));
            }
            for (k, v) in env {
                c.env(k, v);
            }
            let out = c.output().unwrap();
            let said = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
            (said.starts_with("ran: "), said)
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn ending_the_server_an_agent_runs_in_is_refused() {
        let rig = Rig::new("refuse");
        let private = rig.root.join("private").to_string_lossy().to_string();
        for args in [&["kill-server"][..], &["kill-ser"], &["-u", "kill-server"], &["kill-session", "-a"], &["kill-session", "-at", "keep"], &["ls", ";", "kill-server"]] {
            let (ran, said) = rig.run(args, true, &[]);
            assert!(!ran && said.contains("refused") && said.contains("CUE_TMUX_GUARD=off"), "{args:?}: {said}");
        }
        // $TMUX_TMPDIR alone doesn't move it off yours: tmux goes by $TMUX first (how the user's sessions were lost).
        let (ran, _) = rig.run(&["kill-server"], true, &[("TMUX_TMPDIR", &private)]);
        assert!(!ran, "$TMUX still names yours");
        let (ran, _) = rig.run(&["-S", &rig.root.join("yours/default").to_string_lossy(), "kill-server"], false, &[]);
        assert!(!ran, "named by its socket");
    }

    #[test]
    fn a_private_server_or_any_other_command_runs_the_real_tmux() {
        let rig = Rig::new("pass");
        let mine = rig.root.join("private/sock").to_string_lossy().to_string();
        for (args, in_tmux) in [
            (vec!["-S", mine.as_str(), "kill-server"], true),
            (vec!["-L", "tests", "kill-server"], true),
            (vec!["kill-server"], false),
            (vec!["kill-session", "-t", "mine"], true),
            (vec!["new-session", "-d", "-s", "a"], true),
            (vec!["list-sessions", "-F", "#{session_name}"], true),
        ] {
            let (ran, said) = rig.run(&args, in_tmux, &[]);
            assert!(ran && said.trim() == format!("ran: {}", args.join(" ")), "{args:?}: {said}");
        }
    }

    #[test]
    fn the_user_asking_for_it_lets_it_through() {
        let rig = Rig::new("off");
        let (ran, _) = rig.run(&["kill-server"], true, &[("CUE_TMUX_GUARD", "off")]);
        assert!(ran);
    }

    #[test]
    fn the_env_lines_put_the_guard_first_once_and_name_the_agents_server() {
        let guard = Path::new("/Users/me/Library/Application Support/cue/bin/guard");
        let lines = env_lines(guard, Some("/private/tmp/tmux-501/default,88049,0"));
        assert!(lines.contains(MARK) && lines.contains("export CUE_TMUX_PROTECT='/private/tmp/tmux-501/default'"), "{lines}");
        let run = |path: &str| {
            let out = Command::new("/bin/sh").arg("-c").arg(format!("{lines}{lines}printf %s \"$PATH\"")).env_clear().env("PATH", path).output().unwrap();
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        assert_eq!(run("/usr/bin:/bin"), format!("{}:/usr/bin:/bin", guard.display()), "first, and once");
        assert!(!env_lines(guard, None).contains("CUE_TMUX_PROTECT"), "not in tmux: nothing more to guard");
    }
}
