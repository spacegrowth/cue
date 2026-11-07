//! Cue's hooks on a machine (+ New → Machine), for the agents installed there. Cue's own hook is a
//! Mac program, so a machine gets a small POSIX script instead, ~/.cue/cue-hook: it drops each hook
//! event into ~/.cue/in/ and, for a permission question, waits for Cue's answer in ~/.cue/out/ while
//! Cue is watching. For a session with the status list on (named in ~/.cue/status-on, which Cue keeps
//! up to date), a message sent and a reply ending wait a moment too, for its list (statuslist.rs). On the Mac, one SSH stream per machine picks the events up and hands each to Cue
//! as a local hook would, so everything after that is the same as on this Mac. Nothing listens on the
//! network. When Cue isn't watching, the script leaves at once and the agent asks in its terminal.
//!
//! The scripts put on the machine live in `scripts`, setting them up in `setup`, the SSH stream that
//! watches a machine in `watch`, and handing each event to Cue in `events`.

mod events;
mod scripts;
mod setup;
mod watch;
#[cfg(test)]
mod tests;

pub(crate) use events::log_raw;
pub(crate) use scripts::hostname;
pub use setup::{push_status_on, setup};
pub use watch::{on_new_session, on_reached, unwatch, watch, watch_all};
/// The tests' World runs a machine's stream itself.
#[cfg(test)]
pub(crate) use watch::stream;
