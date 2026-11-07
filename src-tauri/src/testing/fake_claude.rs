//! A stand-in for Claude Code in its headless mode (hosted/), for tests: a shell script that plays a script of
//! steps (say a line, wait for one of Cue's, write to its error output, pause, exit), notes the arguments it
//! was started with and every line Cue writes to it, and then, like Claude, keeps reading until its input
//! closes. No model, no network, no ~/.claude: what a test checks is Cue's side of the conversation.

use serde_json::Value;
use std::path::PathBuf;

pub struct FakeClaude {
    dir: PathBuf,
    steps: Vec<String>,
}

impl FakeClaude {
    pub fn new() -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cue-fake-claude-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        FakeClaude { dir, steps: vec![] }
    }

    /// Write `line` to its output.
    pub fn say(self, line: Value) -> Self {
        self.step(format!("printf '%s\\n' {}", q(&line.to_string())))
    }

    /// Write `text` to its output as it is (not JSON: a shell's greeting, say).
    pub fn say_text(self, text: &str) -> Self {
        self.step(format!("printf '%s\\n' {}", q(text)))
    }

    /// Write `text` to its error output.
    pub fn complain(self, text: &str) -> Self {
        self.step(format!("printf '%s\\n' {} >&2", q(text)))
    }

    /// Wait for a line from Cue that contains `part` (noting every line read on the way).
    pub fn wait_for(self, part: &str) -> Self {
        self.step(format!("upto {}", q(part)))
    }

    pub fn pause_ms(self, ms: u64) -> Self {
        self.step(format!("sleep {}", ms as f64 / 1000.0))
    }

    /// Start a long `sleep` of its own in the background, its pid noted (`child_pid`): something it started,
    /// which closing the session must not leave behind.
    pub fn start_child(self) -> Self {
        self.step("sleep 60 & echo $! > \"$DIR/child\"".into())
    }

    /// Ignore the polite request to end (SIGTERM), as a stuck process would.
    pub fn ignore_term(self) -> Self {
        self.step("trap '' TERM".into())
    }

    /// Exit with `code` there and then, input still open.
    pub fn exit(self, code: i32) -> Self {
        self.step(format!("exit {code}"))
    }

    fn step(mut self, s: String) -> Self {
        self.steps.push(s);
        self
    }

    /// The script, written: the program to start in Claude's place.
    pub fn program(&self) -> PathBuf {
        let path = self.dir.join("claude");
        let script = format!(
            "#!/bin/sh\nDIR={dir}\nprintf '%s\\n' \"$@\" > \"$DIR/args\"\nprintf '%s\\n' \"${{CUE_HOSTED:-}}\" \"${{TMUX:-}}\" > \"$DIR/env\"\n: > \"$DIR/input\"\n\
             one() {{ IFS= read -r l || exit 0; printf '%s\\n' \"$l\" >> \"$DIR/input\"; }}\n\
             upto() {{ while one; do case \"$l\" in *\"$1\"*) return ;; esac; done; }}\n\
             {steps}\nwhile one; do :; done\n",
            dir = q(&self.dir.to_string_lossy()),
            steps = self.steps.join("\n"),
        );
        std::fs::write(&path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The arguments it was started with.
    pub fn args(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("args")).unwrap_or_default().lines().map(str::to_string).collect()
    }

    /// What it found set: (CUE_HOSTED, TMUX), each empty when unset.
    pub fn env(&self) -> (String, String) {
        let text = std::fs::read_to_string(self.dir.join("env")).unwrap_or_default();
        let mut l = text.lines().map(str::to_string);
        (l.next().unwrap_or_default(), l.next().unwrap_or_default())
    }

    /// The pid of the child `start_child` started.
    pub fn child_pid(&self) -> Option<i32> {
        std::fs::read_to_string(self.dir.join("child")).ok()?.trim().parse().ok()
    }

    /// Every line Cue wrote to it so far, parsed.
    pub fn heard(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("input")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }
}

impl Drop for FakeClaude {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Single-quoted for sh.
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
