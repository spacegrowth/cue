//! Layer A of the stability plan (docs/dev/stability-plan.md §3): the hub driven headless, as the app
//! drives it, with everything outside it stood in for. A `World` owns a hub on a private CUE_HOME, a
//! clock it can run ahead (clock::fake), a terminal that records what was typed and answers as told
//! (`FakeTerminal`), Notification Center (`FakeNotes`), the hub's follow-up work queued to run when the World says (`Deferred`), and
//! transcripts written as Claude Code and Codex write them (`Transcript`). Hook events and control
//! messages go through server::inbound::apply, exactly as a socket connection hands them over; what the
//! window would show is read back from the same `list` a client gets (`Snap`).
//!
//! Machines over SSH are played by `FakeSsh` (machines::ssh), the hub served on a socket of its own by
//! `World::serve`. Left for the Terminal port (plan §6 step 5): the pane reads of hub/activity.rs and the
//! built-in terminal (term.rs, whose ssh tab is still the system's) still reach the real thing; the clock seam covers the hub's own waits
//! and `now_ms`, not tokio's timers, `Instant` deadlines elsewhere, or file times read through `elapsed()`.

mod agent;
mod later;
mod notes;
mod served;
mod snap;
mod ssh;
mod stall;
mod terminal;
pub(crate) mod transcript;
mod world;

mod scenarios;
mod tests;

pub use agent::{Agent, Ask};
pub use later::Deferred;
pub use notes::FakeNotes;
pub use served::Served;
pub use snap::Snap;
pub use ssh::{until, FakeSsh, RUNS_IT};
pub use stall::Stall;
pub use terminal::{FakeTerminal, Key, Takes};
pub use transcript::Transcript;
pub use world::World;
