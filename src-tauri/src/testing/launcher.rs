//! A launcher that starts nothing: it records each session Cue asked it to start (the command line, folder,
//! name and where) and answers with a pane of its own (%1, %2, …), or with the refusal a test gave it.

use crate::machines::Machine;
use crate::terminal::focus::NewTab;
use crate::terminal::launch::{Launcher, Place, Started};
use std::sync::Mutex;

/// One start Cue asked for: on this Mac (`place`) or on `machine`.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub line: String,
    pub dir: String,
    pub name: String,
    pub place: Option<Place>,
    pub machine: String,
    /// The pane it was given.
    pub pane: String,
}

#[derive(Default)]
pub struct FakeLauncher {
    launched: Mutex<Vec<Launch>>,
    /// Lines typed into a pane opened empty, by pane.
    typed: Mutex<Vec<(String, String)>>,
    closed: Mutex<Vec<String>>,
    /// What the next starts fail with, until `accept`.
    refusal: Mutex<Option<String>>,
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

    /// The lines typed into panes opened empty: (pane, line).
    pub fn typed(&self) -> Vec<(String, String)> {
        crate::locking::lock(&self.typed, "fake launch typing").clone()
    }

    /// The panes closed, in order.
    pub fn closed(&self) -> Vec<String> {
        crate::locking::lock(&self.closed, "fake launch closes").clone()
    }

    /// Starts fail with `why` from now on (tmux won't open a window, the machine's folder is gone).
    pub fn refuse(&self, why: &str) {
        *crate::locking::lock(&self.refusal, "fake launch refusal") = Some(why.to_string());
    }

    /// Starts succeed again.
    pub fn accept(&self) {
        *crate::locking::lock(&self.refusal, "fake launch refusal") = None;
    }

    /// Record a start (or refuse it): the pane it's given.
    fn start(&self, line: &str, dir: &str, name: &str, place: Option<Place>, machine: &str) -> Result<String, String> {
        if let Some(why) = crate::locking::lock(&self.refusal, "fake launch refusal").clone() {
            return Err(why);
        }
        let mut launched = crate::locking::lock(&self.launched, "fake launches");
        let pane = format!("%{}", launched.len() + 1);
        launched.push(Launch { line: line.into(), dir: dir.into(), name: name.into(), place, machine: machine.into(), pane: pane.clone() });
        Ok(pane)
    }
}

impl Launcher for FakeLauncher {
    fn here(&self, line: &str, dir: &str, name: &str, place: Place) -> Result<NewTab, String> {
        let pane = self.start(line, dir, name, Some(place), "")?;
        let tty = format!("/dev/ttys9{}", &pane[1..]);
        Ok(match place {
            Place::Tab => NewTab { what: "a fake tab".into(), term_program: "iTerm.app".into(), iterm_session_id: format!("cue:{pane}"), tty, ..Default::default() },
            _ => NewTab { what: "Cue".into(), term_program: "tmux".into(), tty, tmux_pane: pane, ..Default::default() },
        })
    }

    fn there(&self, m: &Machine, line: &str, dir: &str, name: &str) -> Result<Started, String> {
        let pane = self.start(line, dir, name, None, &m.name)?;
        Ok(Started { tty: format!("/dev/pts/{}", &pane[1..]), pane, dir: dir.to_string() })
    }

    fn type_line(&self, pane: &str, line: &str) -> Result<(), String> {
        crate::locking::lock(&self.typed, "fake launch typing").push((pane.into(), line.into()));
        Ok(())
    }

    fn close_pane(&self, pane: &str) {
        crate::locking::lock(&self.closed, "fake launch closes").push(pane.into());
    }
}
