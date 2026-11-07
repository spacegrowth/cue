//! Slow facts read from memory (stability plan §2.4): the window's state builds while the disk under a
//! session's folder doesn't answer.

use crate::testing::World;
use std::io::Write;
use std::time::{Duration, Instant};

/// A repository whose `.cue/actions.json` is a pipe nobody writes to yet: reading it waits, as a stat on an
/// automounted /home that stopped answering does, until the test writes `{"project": …}` into it.
struct StalledRepo {
    dir: std::path::PathBuf,
    pipe: std::path::PathBuf,
}

impl StalledRepo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cue-stalled-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join(".cue")).unwrap();
        let pipe = dir.join(".cue/actions.json");
        let c = std::ffi::CString::new(pipe.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "made the pipe");
        StalledRepo { dir, pipe }
    }

    /// The disk answers: the lookup waiting on the pipe reads `text`.
    fn answer(&self, text: &str) {
        std::fs::OpenOptions::new().write(true).open(&self.pipe).unwrap().write_all(text.as_bytes()).unwrap();
    }
}

impl Drop for StalledRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn the_window_builds_while_a_sessions_folder_doesnt_answer() {
    let w = World::new();
    let repo = StalledRepo::new("window");
    let cwd = repo.dir.to_string_lossy().into_owned();
    let own = repo.dir.file_name().unwrap().to_string_lossy().into_owned();
    let start = Instant::now();
    w.session("a").in_folder(&repo.dir).prompt("fix the login");
    let s = w.snapshot();
    assert!(start.elapsed() < Duration::from_secs(2), "the hook and the snapshot waited on the disk: {:?}", start.elapsed());
    assert_eq!(s.state("a"), "working");
    assert_eq!(s.0["projects"][&cwd]["name"], own.as_str(), "unknown for now: the folder's own name");
    repo.answer(r#"{ "project": "shop" }"#);
    let back = Instant::now();
    while crate::logs::repos::facts(&cwd).is_none() {
        assert!(back.elapsed() < Duration::from_secs(5), "the folder's lookup came back");
        crate::logs::repos::refresh();
    }
    // The tick redraws when a refresh brought something new (lib.rs); whether it says so is repos.rs's tests'.
    w.hub().redraw();
    assert_eq!(w.snapshot().0["projects"][&cwd]["name"], "shop", "what the repository says, once the disk answered");
}
