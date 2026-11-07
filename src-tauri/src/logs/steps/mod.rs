//! What a session did, step by step, read from its Claude Code transcript for the chat's step lines
//! (the window's Active pane, and anything else that asks): the agent's words between steps, and each
//! tool call as one line (what it did, how it went). Read only while someone has the session open:
//! each feed remembers how far into the file it got and reads just the new lines, and a feed nobody
//! asked for in a couple of minutes is dropped. Nothing is saved; the transcript is the only copy.
//!
//! A step's full output or diff isn't part of the feed: `detail` fetches it when you open the step.
//!
//! Pi's session log (~/.pi/agent/sessions/…) and Codex's (~/.codex/sessions/…/rollout-*.jsonl) are read
//! the same way: each of their entries is first put in the shape Claude Code's transcript uses
//! (`from_pi`, `from_codex`), so all three share everything below.
//!
//! SHORTCUT: viewers poll (every 0.5 s while the session works) rather than being
//! pushed new steps. Fine for a few viewers, since a poll with nothing new reads no file and answers
//! with just a version; push new steps instead if many viewers watch at once.
//! SHORTCUT: only the last 4 MB of a transcript is read, so very old turns of a huge session aren't
//! there. That's past anything the chat shows; read further back on demand if that changes.
//!
//! Where things live: the feed and its reading of new lines (`feed`), what it makes of each line
//! (`observe`, `take`), its background work (`background`, `helper`), the open feeds and what's asked of
//! them (`queries`, `history`, `windows`), a tool call as a step line (`step_line`) and opened
//! (`detail`), the turns for Insights (`turn_rows`), the edited files (`edits`), and the other agents'
//! logs (`kind`, `codex`, `pi`).

mod background;
mod codex;
mod detail;
mod edits;
mod feed;
mod helper;
mod history;
mod kind;
mod model;
mod observe;
mod pi;
mod queries;
mod step_line;
mod take;
mod turn_rows;
mod windows;
#[cfg(test)]
mod tests;

pub use detail::detail;
pub use edits::edited_files_by;
#[cfg(test)]
pub use helper::helper_path_for_test;
pub use history::{history, history_helper};
pub use kind::harness_of;
pub use model::Usage;
pub use queries::{agent_count, agent_name, context_pct, handoff_note, insight_state, last_prompt, last_say, model_of, running_background, running_helpers, set_window, steps, take_task_failures};
#[cfg(test)]
pub use queries::{is_handoff_note, HANDOFF_ASK_START};
pub use turn_rows::{turn_rows, TurnRow};
pub use windows::InsightState;

use std::time::Duration;

/// A first look reads this much from the end of the transcript (older turns are past what the chat shows).
const START_BYTES: u64 = 4 * 1024 * 1024;
const FORGET: Duration = Duration::from_secs(120);
const LINE_CHARS: usize = 400;
