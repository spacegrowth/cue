//! A launcher that starts nothing: it records each session Cue asked it to start (the command line, folder,
//! name, and the machine) and answers with a pane of its own (%1, %2, …).

use crate::machines::Machine;
use crate::terminal::focus::NewTab;
use crate::terminal::launch::{Launcher, Started};
use std::sync::Mutex;

/// One start Cue asked for: a tab on this Mac (`machine` empty; its folder and name are in `line` only) or on `machine`.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub line: String,
    pub dir: String,
    pub name: String,
    pub machine: String,
    /// The pane it was given.
    pub pane: String,
}

#[derive(Default)]
pub struct FakeLauncher {
    launched: Mutex<Vec<Launch>>,
}

impl FakeLauncher {
    /// Every start so far, in order.
    pub fn launched(&self) -> Vec<Launch> {
        crate::locking::lock(&self.launched, "fake launches").clone()
    }

    /// The last start.
    #[track_caller]
    pub fn last(&self) -> Launch {
        self.launched().pop().expect("nothing was started")
    }

    /// Record a start: the pane it's given.
    fn start(&self, line: &str, dir: &str, name: &str, machine: &str) -> Result<String, String> {
        let mut launched = crate::locking::lock(&self.launched, "fake launches");
        let pane = format!("%{}", launched.len() + 1);
        launched.push(Launch { line: line.into(), dir: dir.into(), name: name.into(), machine: machine.into(), pane: pane.clone() });
        Ok(pane)
    }
}

impl Launcher for FakeLauncher {
    fn here(&self, line: &str) -> Result<NewTab, String> {
        let pane = self.start(line, "", "", "")?;
        let tty = format!("/dev/ttys9{}", &pane[1..]);
        Ok(NewTab { what: "a fake tab".into(), term_program: "iTerm.app".into(), iterm_session_id: format!("cue:{pane}"), tty, ..Default::default() })
    }

    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String> {
        let pane = self.start(line, dir, name, &m.name)?;
        Ok(Started { tty: format!("/dev/pts/{}", &pane[1..]), pane, dir: dir.to_string() })
    }
}
