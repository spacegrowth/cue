//! Layer A of the stability plan (docs/dev/stability-plan.md §3): the hub driven headless, as the app
//! drives it, with everything outside it stood in for. A `World` owns a hub on a private CUE_HOME, a
//! clock it can run ahead (clock::fake), a terminal that records what was typed and answers as told
//! (`FakeTerminal`), a launcher that records each session started (`FakeLauncher`), the window's events (`FakeWindow`), Notification Center (`FakeNotes`), the hub's follow-up work queued to run when the World says (`Deferred`), and
//! transcripts written as Claude Code and Codex write them (`Transcript`). Hook events and control
//! messages go through server::inbound::apply, exactly as a socket connection hands them over; what the
//! window would show is read back from the same `list` a client gets (`Snap`).
//!
//! Machines over SSH are played by `FakeSsh` (machines::ssh_program, the built-in terminal's client included),
//! the hub served on a socket of its own by `World::serve`. The clock seam covers the hub's own waits
//! and `now_ms`, not tokio's timers, `Instant` deadlines elsewhere, or file times read through `elapsed()`.

mod agent;
mod crew;
mod launcher;
mod later;
mod notes;
mod process;
mod served;
mod snap;
mod ssh;
mod stall;
mod terminal;
pub(crate) mod transcript;
mod window;
mod world;

mod scenarios;
mod tests;

pub use agent::{Agent, Ask};
pub use crew::Crew;
pub use launcher::FakeLauncher;
pub use later::Deferred;
pub use notes::FakeNotes;
pub use process::AgentProcess;
pub use served::Served;
pub use snap::Snap;
pub use ssh::{until, FakeSsh, RUNS_IT};
pub use stall::Stall;
pub use terminal::{FakeTerminal, Key, Takes};
pub use transcript::Transcript;
pub use window::FakeWindow;
pub use world::{Client, World};
