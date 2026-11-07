//! How Cue reaches a session's terminal: finding and focusing it, the built-in terminal, dialogs read off
//! the pane, the "!" shell line, Cue's own shells, and dictation.

pub(crate) mod dialog;
pub(crate) mod dictation;
pub(crate) mod speak;
pub(crate) mod focus;
pub(crate) mod shell;
pub(crate) mod shells;
pub(crate) mod input;
pub(crate) mod keys;
pub(crate) mod term;
pub(crate) mod tmux_guard;
#[cfg(test)]
pub(crate) mod fake_tmux;
#[cfg(test)]
pub(crate) mod test_tmux;
