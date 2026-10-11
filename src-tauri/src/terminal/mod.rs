//! How Cue reaches a session's terminal: finding and focusing it, the built-in terminal, dialogs read off
//! the pane, the "!" shell line and dictation.

pub(crate) mod dialog;
pub(crate) mod dictation;
pub(crate) mod focus;
pub(crate) mod shell;
pub(crate) mod term;
