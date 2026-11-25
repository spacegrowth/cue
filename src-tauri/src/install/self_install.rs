//! Installing a new Cue from one commit, from inside the running one: a Desk item is done only when its commit is
//! in the Cue you're using. `start` runs scripts/self-install.sh, detached, which builds that commit from a clean
//! copy, installs it in place of this Cue, starts it, and puts the old one back when the new one doesn't come up.
//! Everything it does is said in <Cue's folder>/install/:
//! - status.json: the step it's at (`Status`), written at every step;
//! - running.json: which commit the running Cue was built from, written by that Cue as it starts (`note_running`):
//!   how the helper knows the new one is up;
//! - lock: the PID of the helper running, one at a time;
//! - log: the last run's whole output.

use crate::server::cue_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// What the helper is doing, from status.json. `step`: "building", "installing", "starting", then "installed",
/// "rolled_back" (the new one didn't say it was running: the old one is back and started again) or "failed". A
/// failed build leaves the running Cue alone; a failure after Cue was quit (swapping the apps) starts the old one
/// again; a failure while rolling back may leave the new app in place, or no Cue running. `log`: the log's last
/// lines, saying why, on "failed" and "rolled_back". `at`: when, in ms.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Status {
    pub tag: String,
    pub commit: String,
    pub step: String,
    pub log: String,
    pub at: u64,
}

/// The helper, built into Cue: the one this Cue runs is its own, never what a checkout has at the moment.
const HELPER: &str = include_str!("../../../scripts/self-install.sh");

/// Taken while one is started, so two at once can't both find the lock free.
static STARTING: Mutex<()> = Mutex::new(());

fn install_dir() -> PathBuf {
    cue_dir().join("install")
}

/// The commit this running Cue was built from (build.rs's CUE_COMMIT); None for a build from outside a git tree.
pub fn running_commit() -> Option<String> {
    Some(option_env!("CUE_COMMIT").unwrap_or("").to_string()).filter(|c| !c.is_empty())
}

/// The commit of the add-on this running Cue was built with (build.rs's CUE_ADDON_COMMIT); None without one.
pub fn running_addon_commit() -> Option<String> {
    Some(option_env!("CUE_ADDON_COMMIT").unwrap_or("").to_string()).filter(|c| !c.is_empty())
}

/// Whether `commit` of the add-on's repository `repo` is in the running Cue: the add-on commit it was built with is
/// that one or one after it.
pub fn has_addon(repo: &str, commit: &str) -> bool {
    running_addon_commit().is_some_and(|running| is_ancestor(repo, commit, &running))
}

/// Said once as Cue starts: install/running.json, {commit, pid, at}. The helper waits for it to name the commit it
/// installed, from a PID that's alive.
pub fn note_running() {
    let _ = write_running(&install_dir(), &running_commit().unwrap_or_default(), std::process::id(), crate::clock::now_ms());
}

fn write_running(dir: &Path, commit: &str, pid: u32, at: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("running.json.tmp");
    std::fs::write(&tmp, serde_json::json!({ "commit": commit, "pid": pid, "at": at }).to_string())?;
    std::fs::rename(tmp, dir.join("running.json"))
}

/// Whether `commit` is in the running Cue: it is the commit this Cue was built from or one before it, in `repo`.
pub fn has(repo: &str, commit: &str) -> bool {
    running_commit().is_some_and(|running| is_ancestor(repo, commit, &running))
}

/// Whether `of` of `repo` has `commit`: it is that commit or one after it.
pub fn is_ancestor(repo: &str, commit: &str, of: &str) -> bool {
    if !looks_like_commit(commit) || !looks_like_commit(of) {
        return false;
    }
    matches!(crate::safe_git::run("", repo, &[], &["merge-base", "--is-ancestor", commit, of]), Ok((_, 0)))
}

/// A commit's name as git prints it: hex, 7 to 64 long. Anything else (an option, a range) is refused.
fn looks_like_commit(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Start the helper installing `commit` from `repo`, detached: it carries on while Cue quits. Err when one is
/// already running, or it couldn't be started.
pub fn start(repo: &str, commit: &str, tag: &str) -> Result<(), String> {
    start_in(&install_dir(), &cue_dir(), repo, commit, tag, &[])
}

/// `start`, in Cue's folder `home` (its install/ folder `dir`), with these environment variables set for the helper.
fn start_in(dir: &Path, home: &Path, repo: &str, commit: &str, tag: &str, env: &[(&str, &str)]) -> Result<(), String> {
    if !looks_like_commit(commit) {
        return Err(format!("not a commit: {commit}"));
    }
    if !Path::new(repo).join(".git").exists() {
        return Err(format!("not a git checkout: {repo}"));
    }
    let _starting = crate::locking::lock(&STARTING, "self_install.starting");
    if let Some(pid) = running_helper(dir) {
        return Err(format!("an install is already running (PID {pid})"));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let script = match std::env::var("CUE_SELF_INSTALL_SCRIPT") {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => {
            let s = dir.join("self-install.sh");
            std::fs::write(&s, HELPER).map_err(|e| e.to_string())?;
            s
        }
    };
    let out = std::fs::File::create(dir.join("helper.out")).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(&script).args([repo, commit, tag]).env("CUE_HOME", home).envs(env.iter().copied()).current_dir(dir);
    cmd.stdin(Stdio::null()).stdout(out).stderr(err);
    // A session of its own: no terminal, not in Cue's process group, so nothing Cue's quitting sends it reaches it.
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start the install: {e}"))?;
    // The helper writes its own PID there too as it starts; written here first, so the next `start` sees it at once.
    // Written whole, then moved into place, so a look at it never finds it half-written.
    let tmp = dir.join(format!("lock.{}", std::process::id()));
    if std::fs::write(&tmp, child.id().to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("lock"));
    }
    // Reaped when it ends, if this Cue is still running then.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The PID of the helper running, from the lock; a lock whose PID is gone is stale, as good as none.
fn running_helper(dir: &Path) -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(dir.join("lock")).ok()?.trim().parse().ok()?;
    (pid > 0 && crate::server::alive(pid)).then_some(pid)
}

/// What the helper last said, from install/status.json; None before the first install. An install.sh run that
/// stopped without saying how it ended reads as failed (`over_if_stale`).
pub fn status() -> Option<Status> {
    let dir = install_dir();
    status_in(&dir).map(|s| over_if_stale(s, log_written_ms(&dir), crate::clock::now_ms()))
}

/// The tag ./install.sh writes in status.json when it installs the app (run by you, or a session, not Cue).
pub const BY_HAND: &str = "install.sh";

/// How long an install.sh run may say nothing (status.json and its log untouched) before it's taken as over (ms).
const BY_HAND_QUIET_MS: u64 = 30 * 60_000;

/// `s`, or, when it's install.sh's still building, installing or starting with nothing written for `BY_HAND_QUIET_MS`
/// (`log_ms`: when its log was last written), failed: it was stopped (a closed terminal, a kill) before it could say
/// so, and must not hold the Desk's installs back for good.
pub fn over_if_stale(mut s: Status, log_ms: u64, now: u64) -> Status {
    if s.tag == BY_HAND && matches!(s.step.as_str(), "building" | "installing" | "starting") && now.saturating_sub(s.at.max(log_ms)) > BY_HAND_QUIET_MS {
        s.step = "failed".into();
        if s.log.is_empty() {
            s.log = "install.sh stopped without saying how it ended".into();
        }
    }
    s
}

/// When install/log was last written (ms; 0 when there's none).
fn log_written_ms(dir: &Path) -> u64 {
    std::fs::metadata(dir.join("log")).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

/// The end of install/log (its last 64 KB), the latest install's output; "" when there's none.
pub fn log_end() -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(install_dir().join("log")) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(64 * 1024)));
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn status_in(dir: &Path) -> Option<Status> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("status.json")).ok()?).ok()
}

#[cfg(test)]
#[path = "self_install_tests.rs"]
mod tests;
