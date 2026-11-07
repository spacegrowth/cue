// Each test file that uses this takes only what it needs.
#![allow(dead_code)]


//! The real `cue hook` binary against a fake Cue: a socket of the test's own (cue.sock in a private CUE_HOME)
//! that keeps what each connection sends and answers as the test says. Your Cue is never reached: the hook
//! finds its socket only through CUE_HOME, and a hook run from a tmux pane would look up that pane, so
//! TMUX is cleared too.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub struct FakeCue {
    pub home: PathBuf,
    listener: Option<UnixListener>,
}

impl FakeCue {
    /// A Cue that's running: its socket takes connections.
    pub fn running() -> Self {
        let mut c = Self::absent();
        c.start();
        c
    }

    /// No Cue: nothing listens on its socket.
    pub fn absent() -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let home = std::env::temp_dir().join(format!("cue-hook-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        FakeCue { home, listener: None }
    }

    fn sock(&self) -> PathBuf {
        self.home.join("cue.sock")
    }

    /// Cue (re)starts: its socket is there again.
    pub fn start(&mut self) {
        let _ = std::fs::remove_file(self.sock());
        let l = UnixListener::bind(self.sock()).unwrap();
        l.set_nonblocking(true).unwrap();
        self.listener = Some(l);
    }

    /// Cue quits: its socket is gone.
    pub fn stop(&mut self) {
        self.listener = None;
        let _ = std::fs::remove_file(self.sock());
    }

    /// `cue hook <args>` started, `stdin` written to it and closed.
    pub fn hook(&self, args: &[&str], stdin: &[u8], env: &[(&str, &str)]) -> Child {
        let mut c = Command::new(env!("CARGO_BIN_EXE_cue"));
        c.arg("hook").args(args).env("CUE_HOME", &self.home).env_remove("TMUX").env_remove("TMUX_PANE").env_remove("CUE_DRIVEN_BY").env_remove("CUE_HOOK_RECONNECT_FOR");
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        child
    }

    /// `cue hook <args>` run to its end with `stdin`.
    pub fn run(&self, args: &[&str], stdin: &Value) -> Output {
        self.hook(args, stdin.to_string().as_bytes(), &[]).wait_with_output().unwrap()
    }

    /// The next connection to Cue (10 s at most).
    #[track_caller]
    pub fn accept(&self) -> UnixStream {
        let l = self.listener.as_ref().expect("Cue isn't running");
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            match l.accept() {
                Ok((s, _)) => {
                    s.set_nonblocking(false).unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                    return s;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < end => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => panic!("no connection to Cue came: {e}"),
            }
        }
    }

    /// Every message sent on the connections made so far (each read to its end), in order.
    pub fn received(&self) -> Vec<Value> {
        let Some(l) = self.listener.as_ref() else { return vec![] };
        let mut out = vec![];
        while let Ok((mut s, _)) = l.accept() {
            // A connection whose hook has exited already: macOS refuses options on it (EINVAL), and reading
            // it to its end needs none.
            let _ = s.set_nonblocking(false);
            let mut text = String::new();
            let _ = s.read_to_string(&mut text);
            out.extend(text.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("Cue can't read {l:?}: {e}"))));
        }
        out
    }
}

impl Drop for FakeCue {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// The one line a connection sent first.
pub fn first_line(s: &UnixStream) -> Value {
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not a message: {line:?} ({e})"))
}

/// What the hook printed, as JSON (Null when it printed nothing).
pub fn printed(o: &Output) -> Value {
    let text = String::from_utf8_lossy(&o.stdout);
    if text.trim().is_empty() { Value::Null } else { serde_json::from_str(text.trim()).unwrap_or_else(|e| panic!("printed {text:?}: {e}")) }
}
