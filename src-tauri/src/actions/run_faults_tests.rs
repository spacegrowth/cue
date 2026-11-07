//! Actions that misbehave (stability plan §4, Extensions / actions): one that prints far more than a log
//! keeps, one that never exits and shrugs off Ctrl-C, one whose folder is gone. Run as a child of Cue's
//! (no tmux) and in tmux.

use super::*;

/// A scratch Cue folder with a project folder in it, CUE_HOME pointed at it while it lives.
struct Scratch {
    home: PathBuf,
    folder: String,
    _g: std::sync::MutexGuard<'static, ()>,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let g = crate::db::test_turn();
        let home = std::env::temp_dir().join(format!("cue-act-faults-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("proj")).unwrap();
        std::env::set_var("CUE_HOME", &home);
        let folder = home.join("proj").to_string_lossy().into_owned();
        Scratch { home, folder, _g: g }
    }

    fn spec<'a>(&'a self, id: &'a str, run: &'a str) -> Spec<'a> {
        Spec { folder: &self.folder, machine: "", id, name: id, run, keep: true }
    }

    fn name(&self, id: &str) -> String {
        format!("{}-{}", session_name(&self.folder, "", id, id), std::process::id())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// `done` within `secs` seconds, looked at every tenth of one.
fn within(secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if done() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    done()
}

/// Prints 10 MB of numbered lines, then its last line.
const TEN_MB: &str = "awk 'BEGIN { for (i = 0; i < 170000; i++) printf \"line %07d %s\\n\", i, \"................................................\" }'; echo the-end";
/// Never exits, and takes no notice of Ctrl-C (from when it says it's ready).
const STUBBORN: &str = "trap '' INT; echo ready; while :; do sleep 1; done";

#[test]
fn a_child_action_printing_10_mb_logs_its_end() {
    let s = Scratch::new("plain-big");
    let name = s.name("Big");
    plain_start(&s.spec("Big", TEN_MB), &name).unwrap();
    assert!(within(30, || plain_list().iter().any(|x| x.session == name && !x.running)), "it ended");
    let size = std::fs::metadata(plain_out(&name)).unwrap().len();
    let raw = plain_log(&name).unwrap();
    let log = log("", &name, 3).unwrap();
    assert!(size >= 10 << 20, "all its output kept in its file: {size}");
    assert!(raw.len() <= 512 * 1024, "a log read is the end of it, not all 10 MB: {}", raw.len());
    assert_eq!(log.lines().last(), Some("the-end"), "{log}");
    assert!(log.starts_with("line 0169998"), "{log}");
}

#[test]
fn a_child_action_that_ignores_ctrl_c_is_killed_by_stop() {
    let s = Scratch::new("plain-stubborn");
    let name = s.name("Stubborn");
    plain_start(&s.spec("Stubborn", STUBBORN), &name).unwrap();
    assert!(within(10, || log("", &name, 1).is_ok_and(|l| l == "ready")), "it started");
    let t0 = std::time::Instant::now();
    assert!(plain_stop(&name));
    let took = t0.elapsed();
    let seen = plain_list().into_iter().find(|x| x.session == name).unwrap();
    assert!(!seen.running && seen.stopped, "ended, as stopped");
    assert_eq!(seen.exit, Some(128 + libc::SIGKILL), "killed once Ctrl-C did nothing");
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}

#[test]
fn an_action_whose_folder_is_gone_isnt_started() {
    let s = Scratch::new("gone");
    let gone = format!("{}/not-there", s.folder);
    let spec = Spec { folder: &gone, machine: "", id: "Gone", name: "Gone", run: "true", keep: false };
    assert_eq!(plain_start(&spec, &s.name("Gone")).unwrap_err(), format!("{gone} isn't a folder"));
    assert_eq!(start(&spec).unwrap_err(), format!("{gone} isn't a folder"));
}

/// tmux's session `name`, killed when this goes (a failing test leaves nothing running).
struct Session(String);

impl Drop for Session {
    fn drop(&mut self) {
        let _ = tmux_here(&["kill-session", "-t", &format!("={}", self.0)]);
    }
}

#[test]
fn an_action_in_tmux_printing_10_mb_logs_its_end_and_its_exit() {
    if !crate::terminal::focus::tmux_installed() {
        return;
    }
    let s = Scratch::new("tmux-big");
    let started = start(&s.spec("Big", &format!("{TEN_MB}; exit 4"))).unwrap();
    let _kill = Session(started.session.clone());
    let ended = || list("").into_iter().find(|x| x.session == started.session).filter(|x| !x.running);
    assert!(within(60, || ended().is_some_and(|x| x.exit.is_some())), "it ended: {:?}", ended().map(|x| x.state()));
    assert_eq!(ended().unwrap().exit, Some(4));
    let log = log("", &started.session, 2).unwrap();
    assert!(log.ends_with("the-end"), "{log}");
}

#[test]
fn an_action_in_tmux_that_ignores_ctrl_c_is_killed_by_stop() {
    if !crate::terminal::focus::tmux_installed() {
        return;
    }
    let s = Scratch::new("tmux-stubborn");
    let started = start(&s.spec("Stubborn", STUBBORN)).unwrap();
    let _kill = Session(started.session.clone());
    let running = || list("").into_iter().any(|x| x.session == started.session && x.running);
    assert!(within(30, || log("", &started.session, 1).is_ok_and(|l| l == "ready")), "it started");
    stop(&s.folder, "", &started.session).unwrap();
    assert!(!running(), "stopped, Ctrl-C notwithstanding");
    let seen = list("").into_iter().find(|x| x.session == started.session).unwrap();
    assert!(seen.stopped, "marked as stopped: {:?}", seen.state());
    assert_eq!(stop(&s.folder, "", &started.session), Ok(()), "stopping one that's stopped is nothing");
}

/// An action's output file of 2 GB (sparse, made in a moment): a log read takes its end at once, never the
/// whole file.
#[test]
fn a_log_read_of_a_huge_output_takes_only_its_end() {
    use std::io::{Seek, Write};
    let s = Scratch::new("plain-huge");
    let name = s.name("Huge");
    plain_start(&s.spec("Huge", "true"), &name).unwrap();
    assert!(within(10, || plain_list().iter().any(|x| x.session == name && !x.running)), "it ended");
    let mut f = std::fs::OpenOptions::new().write(true).open(plain_out(&name)).unwrap();
    f.set_len(2 << 30).unwrap();
    f.seek(std::io::SeekFrom::End(0)).unwrap();
    f.write_all(b"\nthe last line\n").unwrap();
    let t0 = std::time::Instant::now();
    let raw = plain_log(&name).unwrap();
    let took = t0.elapsed();
    assert!(took < std::time::Duration::from_millis(100), "read {took:?}");
    assert!(raw.len() <= 512 * 1024 && raw.ends_with("the last line\n"), "{} bytes", raw.len());
}

/// A file cut shorter after its length was looked at is read from its start; one that grew since is read up
/// to where it was.
#[test]
fn a_log_tail_copes_with_a_file_cut_or_grown_meanwhile() {
    let path = std::env::temp_dir().join(format!("cue-act-tail-{}", std::process::id()));
    std::fs::write(&path, "short\n").unwrap();
    let mut f = std::fs::File::open(&path).unwrap();
    assert_eq!(tail(&mut f, 10 << 20, 1024).as_deref(), Some("short\n"), "cut to 6 bytes after a look said 10 MB");
    std::fs::write(&path, "0123456789later").unwrap();
    let mut f = std::fs::File::open(&path).unwrap();
    assert_eq!(tail(&mut f, 10, 4).as_deref(), Some("6789"), "grown past the look: up to where it was");
    let _ = std::fs::remove_file(&path);
}
