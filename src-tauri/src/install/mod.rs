//! Putting a new Cue in place of the running one: a full install from one commit (self_install.rs), or, when only
//! the window's files changed, those files reloaded in place (live_ui.rs).

pub(crate) mod live_ui;
pub(crate) mod self_install;
