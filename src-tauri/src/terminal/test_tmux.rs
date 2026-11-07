//! Tests run tmux on a server of their own, never yours: `/tmp/cue-test-tmux-<pid>` (short enough for a
//! socket's path). A session a failing test leaves can't show up in Cue as one of your terminal tabs, and
//! a test can't reach, let alone kill, the server your own terminals live on.
//!
//! tmux picks its server from $TMUX before $TMUX_TMPDIR, and a test run started inside tmux (an agent's
//! terminal) inherits your $TMUX. So the test binary drops it before `main`, before any test thread can
//! run tmux (`ISOLATE`), and every tmux command a test runs checks that it's still so (`check`, from
//! focus::exec::bin), failing loudly instead of reaching your server.
//!
//! That server is started with a config of its own (`CONFIG`), not your ~/.tmux.conf, and its windows run
//! /bin/sh, not your login shell: a test that types into a new window waits on a shell that starts in
//! moments, never on your shell's startup files (a second idle, ten under a parallel test run).

use std::path::{Path, PathBuf};

/// This test process's tmux folder.
fn mine() -> PathBuf {
    Path::new("/tmp").join(format!("cue-test-tmux-{}", std::process::id()))
}

thread_local! {
    /// The programs a test stands in on its own thread (`stand_in`), by name: a script, or one not installed.
    static STAND_INS: std::cell::RefCell<std::collections::HashMap<String, String>> = std::cell::RefCell::default();
    /// How long a tmux command may take on this thread before it's cut off (`cap_tmux`).
    static CAP: std::cell::Cell<Option<std::time::Duration>> = const { std::cell::Cell::new(None) };
}

/// While this lives, program `name` (tmux, ps) on this thread is `bin` (focus::exec::bin), never the real one:
/// what a test stands in to make it hang, fail, answer as it says, or be missing. Other threads (other
/// tests) are untouched.
pub struct StandIn(String);

pub fn stand_in(name: &str, bin: &str) -> StandIn {
    STAND_INS.with(|s| s.borrow_mut().insert(name.to_string(), bin.to_string()));
    StandIn(name.to_string())
}

impl Drop for StandIn {
    fn drop(&mut self) {
        STAND_INS.with(|s| s.borrow_mut().remove(&self.0));
    }
}

/// The program this thread's test stands in for `name`, if it does.
pub fn standing_in(name: &str) -> Option<String> {
    STAND_INS.with(|s| s.borrow().get(name).cloned())
}

/// While this lives, a tmux command on this thread is cut off after `cap` instead of focus::exec::CAP: a tmux
/// that never answers fails in moments.
pub struct Cap(());

pub fn cap_tmux(cap: std::time::Duration) -> Cap {
    CAP.with(|c| c.set(Some(cap)));
    Cap(())
}

impl Drop for Cap {
    fn drop(&mut self) {
        CAP.with(|c| c.set(None));
    }
}

/// The cap this thread's test put on tmux, if it did.
pub fn tmux_cap() -> Option<std::time::Duration> {
    CAP.with(|c| c.get())
}

/// Run by the loader before `main`: tmux, from here on, has only this process's own folder to go by.
extern "C" fn isolate() {
    std::env::remove_var("TMUX");
    std::env::remove_var("TMUX_PANE");
    std::env::set_var("TMUX_TMPDIR", mine());
    unsafe { libc::atexit(end) };
}

/// Run as the test process exits: its server (kept up while it ran, `CONFIG`) and its folder go.
extern "C" fn end() {
    take_down(&mine());
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
    ONCE.call_once(|| {
        take_down_ended(Path::new("/tmp"));
        start_server(&dir);
    });
}

/// The test server's settings: kept up with no sessions (tests start and end theirs throughout the run; it's
/// taken down as the process exits, `end`, or by the next run if it was killed), and /bin/sh in every new window.
const CONFIG: &str = "set -s exit-empty off\nset -g default-shell /bin/sh\nset -g default-command ''\n";

/// This process's server, started on `CONFIG` before any test's command could start it on your config.
fn start_server(dir: &Path) {
    let conf = dir.join("tmux.conf");
    std::fs::write(&conf, CONFIG).expect("the test tmux server's config");
    let ok = std::process::Command::new(super::focus::find_bin("tmux")).args(["-f", &conf.to_string_lossy(), "start-server"]).env("SHELL", "/bin/sh").status();
    assert!(ok.as_ref().is_ok_and(|s| s.success()), "the test tmux server didn't start: {ok:?}");
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

    /// A new window runs /bin/sh, whatever your login shell and its startup files are.
    #[test]
    fn the_test_server_runs_a_plain_shell_in_new_windows() {
        if !crate::terminal::focus::tmux_installed() {
            return;
        }
        let shell = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(["show-options", "-gv", "default-shell"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&shell.stdout).trim(), "/bin/sh");
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
