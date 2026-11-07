//! Where Cue starts a session's agent: a command line (`cd <folder> && claude …`, terminal::focus's lines)
//! typed into a new tmux window or terminal tab on this Mac, or a tmux window on one of your machines. + New,
//! Resume, Fork, Continue in a new session, Start fresh, Move to Cue and the panes rebuilt after a restart all
//! start theirs here. One trait, so tests can stand in a launcher that records what it was asked to start and
//! answers as told (testing::FakeLauncher); `Tabs` is the real one.

use crate::machines::Machine;
use crate::terminal::focus::{self, NewTab};

/// Where on this Mac a session starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// A tmux session of its own, shown in Cue's own terminal (Settings → New sessions in tmux).
    Tmux,
    /// A window of Cue's tmux tabs session, shown in a terminal tab ("Runs in iTerm" with tmux installed).
    TmuxTab,
    /// A plain terminal tab (no tmux here).
    Tab,
}

impl Place {
    /// Where a session goes when `tmux` says (+ New's choice; None: Settings' default): in tmux either way when
    /// it's installed, "Runs in iTerm" only adding a terminal tab showing it.
    pub fn chosen(tmux: Option<bool>) -> Place {
        let installed = focus::tmux_installed();
        match tmux.map_or_else(crate::config::tmux_sessions, |t| t && installed) {
            true => Place::Tmux,
            false if installed => Place::TmuxTab,
            false => Place::Tab,
        }
    }
}

/// A session started on a machine: its tmux pane, that pane's tty, and its folder's full path there.
pub struct Started {
    pub pane: String,
    pub tty: String,
    pub dir: String,
}

pub trait Launcher: Send + Sync {
    /// Start `line` on this Mac, in `dir`, named `name`, at `place`. An empty line opens the window only, for
    /// `type_line` later.
    fn here(&self, line: &str, dir: &str, name: &str, place: Place) -> Result<NewTab, String>;
    /// Start `line` in a new window of Cue's tmux session on machine `m`, in `dir` there.
    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String>;
    /// Type `line` into a pane `here` opened empty, and press Enter.
    fn type_line(&self, pane: &str, line: &str) -> Result<(), String>;
    /// Close a pane `here` opened (its window, and its session if that was the last one).
    fn close_pane(&self, pane: &str);
}

/// The terminals and tmux on this Mac and your machines.
pub struct Tabs;

impl Launcher for Tabs {
    fn here(&self, line: &str, dir: &str, name: &str, place: Place) -> Result<NewTab, String> {
        match place {
            Place::Tmux => focus::open_in_tmux(line, dir, name),
            Place::TmuxTab => focus::open_in_tmux_tab(line, dir, name),
            Place::Tab => focus::open_tab_with(line),
        }
    }

    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String> {
        crate::machines::open_in_tmux(m, line, dir, name).map(|(pane, tty, dir)| Started { pane, tty, dir })
    }

    fn type_line(&self, pane: &str, line: &str) -> Result<(), String> {
        focus::tmux_type_line(pane, line)
    }

    fn close_pane(&self, pane: &str) {
        focus::tmux_kill_pane(pane)
    }
}
