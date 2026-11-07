//! A file that doesn't answer: a pipe nobody writes to yet. Opening it waits, as a stat on an automounted
//! /home that stopped answering does, until the test answers.

use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Stall {
    path: PathBuf,
}

impl Stall {
    /// The pipe at `path` (its folder must be there).
    pub fn at(path: &Path) -> Self {
        let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "made the pipe at {}", path.display());
        Stall { path: path.to_path_buf() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The disk answers: whatever waits on the pipe reads `text`.
    pub fn answer(&self, text: &str) {
        std::fs::OpenOptions::new().write(true).open(&self.path).unwrap().write_all(text.as_bytes()).unwrap();
    }

    /// The disk answers (with nothing) every open of the pipe, until `over`: for a reader that opens it
    /// more than once.
    pub fn answer_until(&self, over: impl Fn() -> bool) {
        let c = std::ffi::CString::new(self.path.to_string_lossy().as_bytes()).unwrap();
        while !over() {
            let fd = unsafe { libc::open(c.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK) };
            if fd >= 0 {
                unsafe { libc::close(fd) };
            }
            std::thread::yield_now();
        }
    }
}
