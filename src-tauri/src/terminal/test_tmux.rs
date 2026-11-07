//! Tests run tmux on a server of their own, never yours: `/tmp/cue-test-tmux-<pid>` (short enough for a
//! socket's path). A session a failing test leaves can't show up in Cue as one of your terminal tabs, and
//! a test can't reach, let alone kill, the server your own terminals live on.
//!
//! tmux picks its server from $TMUX before $TMUX_TMPDIR, and a test run started inside tmux (an agent's
//! terminal) inherits your $TMUX. So the test binary drops it before `main`, before any test thread can
//! run tmux (`ISOLATE`), and every tmux command a test runs checks that it's still so (`check`, from
//! focus::bin), failing loudly instead of reaching your server.

use std::path::{Path, PathBuf};

/// This test process's tmux folder.
fn mine() -> PathBuf {
    Path::new("/tmp").join(format!("cue-test-tmux-{}", std::process::id()))
}

/// Run by the loader before `main`: tmux, from here on, has only this process's own folder to go by.
extern "C" fn isolate() {
    std::env::remove_var("TMUX");
    std::env::remove_var("TMUX_PANE");
    std::env::set_var("TMUX_TMPDIR", mine());
}

#[used]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
#[cfg_attr(target_os = "linux", link_section = ".init_array")]
static ISOLATE: extern "C" fn() = isolate;

/// Before a test's tmux command: still isolated (a panic naming why, not a command on your server), and
/// this process's folder there (made again if a test run of an older Cue, which removed every run's
/// folder but its own, took it). The first time, the servers of earlier test runs that have exited are
/// taken down; a run still going (another `cargo test` at the same time) keeps its own.
pub fn check() {
    let dir = mine();
    assert!(std::env::var_os("TMUX").is_none(), "a test would run tmux on the server in $TMUX (yours): the test binary's isolation didn't run");
    assert_eq!(std::env::var_os("TMUX_TMPDIR").map(PathBuf::from), Some(dir.clone()), "a test would run tmux outside its own folder");
    let _ = std::fs::create_dir_all(&dir);
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| take_down_ended(Path::new("/tmp")));
}

/// The test-run folders under `root` whose process has exited: their servers and folders go. A run still going
/// (another `cargo test` at the same time) keeps its own, and nothing that isn't a run's folder is touched.
fn take_down_ended(root: &Path) {
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let pid = e.file_name().to_string_lossy().strip_prefix("cue-test-tmux-").and_then(|p| p.parse::<i32>().ok());
        if pid.is_some_and(|p| p != std::process::id() as i32 && !crate::server::alive(p)) {
            take_down(&e.path());
        }
    }
}

/// An exited run's servers (each by its own socket, never by $TMUX) and its folder.
fn take_down(folder: &Path) {
    for s in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let sock = s.path().join("default");
        let _ = std::process::Command::new(super::focus::find_bin("tmux")).args(["-S", &sock.to_string_lossy(), "kill-server"]).env_remove("TMUX").output();
    }
    let _ = std::fs::remove_dir_all(folder);
}

#[cfg(test)]
mod tests {
    #[test]
    fn tests_run_tmux_on_their_own_server_from_the_start() {
        // Before any tmux command of this process: isolated already.
        assert!(std::env::var_os("TMUX").is_none());
        assert_eq!(std::env::var_os("TMUX_TMPDIR").map(std::path::PathBuf::from), Some(super::mine()));
        super::check();
        assert!(super::mine().is_dir());
    }

    /// Two test runs at once (two `cargo test`s in one checkout): a run clears only the tmux servers of runs that have
    /// ended, never one still going (that would end its sessions mid-test).
    #[test]
    fn a_test_run_clears_only_the_tmux_servers_of_runs_that_ended() {
        use std::process::Command;
        let root = std::env::temp_dir().join(format!("cue-left-tmux-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut going = Command::new("sleep").arg("30").spawn().unwrap();
        let mut ended = Command::new("true").spawn().unwrap();
        ended.wait().unwrap();
        let dir = |pid: u32| root.join(format!("cue-test-tmux-{pid}"));
        for pid in [going.id(), ended.id()] {
            std::fs::create_dir_all(dir(pid).join("tmux-1")).unwrap();
        }
        std::fs::create_dir_all(root.join("unrelated")).unwrap();
        super::take_down_ended(&root);
        let kept = dir(going.id()).exists();
        let _ = going.kill();
        let _ = going.wait();
        assert!(kept, "a run still going keeps its server");
        assert!(!dir(ended.id()).exists(), "an ended run's is cleared");
        assert!(root.join("unrelated").exists(), "nothing else is touched");
        let _ = std::fs::remove_dir_all(&root);
    }
}
