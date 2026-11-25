//! Where Cue resumes a session it ran when you move it out of Cue (⋯ → Move to terminal): a command line (`cd <folder> &&
//! claude …`, terminal::focus's lines) in a new tab of your terminal app on this Mac, or a tmux window on one of your
//! machines (that machine's own terminal). One trait, so tests can stand in a launcher that records what it was asked to
//! start and answers as told (testing::FakeLauncher); `Tabs` is the real one.

use crate::machines::Machine;
use crate::terminal::focus::{self, NewTab};

/// A session started on a machine: its tmux pane, that pane's tty, and its folder's full path there.
pub struct Started {
    pub pane: String,
    pub tty: String,
    pub dir: String,
}

pub trait Launcher: Send + Sync {
    /// Start `line` (it `cd`s to its folder itself) in a new plain tab of your terminal app on this Mac.
    fn here(&self, line: &str) -> Result<NewTab, String>;
    /// Start `line` in a new window of Cue's tmux session on machine `m`, in `dir` there.
    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String>;
}

/// The terminals on this Mac and tmux on your machines.
pub struct Tabs;

impl Launcher for Tabs {
    fn here(&self, line: &str) -> Result<NewTab, String> {
        focus::open_tab_with(line)
    }

    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String> {
        crate::machines::open_in_tmux(m, line, dir, name).map(|(pane, tty, dir)| Started { pane, tty, dir })
    }
}
