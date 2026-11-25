//! What the Desk keeps only while Cue runs: scratch maps that are never saved (db/desk.rs saves and loads none of
//! them), so each starts empty after a restart. Held in `Desk` as `run`, under the same lock.

use std::collections::{HashMap, HashSet};

use super::model::Waiting;

/// The Desk's runtime-only state (`Desk::run`). Not saved.
#[derive(Default, Debug)]
pub struct DeskRun {
    /// When the queue's next wait runs out and a sweep is set for it (ms; 0: none set): desk/hub/turn/queue.rs.
    pub queue_timer: u64,
    /// Sessions being selected as leads, reserved across concurrent Desk changes.
    pub lead_claims: HashMap<String, String>,
    /// Projects whose lead is being started or resumed, with the messages that arrived while it was starting, sent
    /// once it's up.
    pub starting: HashMap<String, Vec<Waiting>>,
    /// Sessions whose context the lead said to keep as it is (CONTEXT: … -> keep): session → (when it said so, its
    /// decision, whether it was at work then). Cue asks no hand-off note of it for the idle stretch it was in then,
    /// or, said while it worked, the one after that turn (stamped as the turn ends: desk/hub/core/desk.rs; hub/cache_tick.rs).
    pub keep: HashMap<String, (u64, String, bool)>,
    /// Sessions whose compacting failed (an error, or no end in 15 minutes), and when: a session on a Desk with a live
    /// lead is asked for a hand-off note instead, and goes on fresh from it (hub/cache_tick.rs full_due). Not saved.
    pub compact_failed: HashMap<String, u64>,
    /// Items a lead said DONE on that its DONE couldn't close, and how each was ("state@updated") when Cue told it
    /// why (desk/hub/turn/said.rs lead_done): a lead saying it again of an unchanged item isn't answered again. Not saved.
    pub done_told: HashMap<String, String>,
    /// A holder's turn that said DONE while its background agents or crew were still at work (#393), by its session:
    /// acted on once they've finished (desk/hub/turn/heard.rs `held_done`), at its next turn's end or as its background
    /// agents are seen done (hub/activity.rs `watch_helpers`). Not saved.
    pub done_held: HashMap<String, String>,
    /// What a project's lead is to be told of its routes Cue couldn't make (desk/hub/turn/said.rs `route_answers`, #248) when it
    /// couldn't be sent then, as the lead was asking the user something: told in its next Desk block instead, and
    /// cleared once that reaches it (`desk_told`). Not saved.
    pub routes_held: HashMap<String, String>,
    /// A lead's ASKs Cue sent back to it to decide itself while its Desk's Away was in force (#471), by the lead's
    /// session: (the question, its item). Its next calls are marked with them (desk/hub/away/away.rs `answered_by_calls`).
    /// Not saved.
    pub away_asked: HashMap<String, Vec<(String, Option<String>)>>,
    /// Items whose holder's turn ended on them with no DONE, and Cue asked it once ("Done? …"): item → (that holder,
    /// the item's `updated` then). The turn that answers isn't asked again; the item changed since (sent again, routed)
    /// is a new stall (desk/hub/stall/stall.rs). Not saved.
    pub nudged: HashMap<String, (String, u64)>,
    /// Items whose holder said what's left of them, or what they wait on (item → (that holder, the item's `updated`
    /// then)): not asked "Done?" again until the item changes (desk/hub/stall/stall.rs). Not saved.
    pub left_said: HashMap<String, (String, u64)>,
    /// Items whose holder the Desk lead was told is stuck mid-turn (desk/hub/stall/stuck.rs): item → (that holder, the
    /// stuck spell: when it last showed output, or the prompt it sits on). Told once a spell; gone once it moves.
    /// Not saved.
    pub stuck_told: HashMap<String, (String, String)>,
    /// What the Desk itself sent each session lately (session → its last few messages): a prompt it takes that isn't
    /// one of these was typed by you (desk/hub/turn/asked.rs). Not saved.
    pub sent: HashMap<String, Vec<String>>,
    /// Your own words to each lead from the Desk lately (lead session → your last few messages): a turn answering one
    /// of them is one you asked (UNPARK, #499). Not saved.
    pub yours: HashMap<String, Vec<String>>,
    /// When Cue last sent each working item to its holder (item → (holder, ms)): a turn that ended before it isn't a
    /// stall on it. Not saved: after a restart, `held_from` stands in.
    pub sent_at: HashMap<String, (String, u64)>,
    /// When Cue last read each item's pull request (ms): it looks once a minute. Not saved.
    pub pr_looked: HashMap<String, u64>,
    /// Reads of each item's pull request that failed in a row: it says so only from the third. Not saved.
    pub pr_misses: HashMap<String, u32>,
    /// Finishing items whose commit is being made now, and "tick" while the finishing tick runs: one run of each at a
    /// time (desk/hub/landing/finish.rs). Not saved.
    pub finish_claims: HashSet<String>,
    /// Each main checkout's HEAD as Cue last looked for a commit that takes a finished item's work back out
    /// (desk/hub/landing/finish.rs `taken_out_tick`). Not saved: the first look after a start only notes it.
    pub seen_heads: HashMap<String, String>,
    /// Circles in the waits list (desk/hub/core/waits.rs), by their key: when each was found, and whether the lead was told.
    /// Dropped once the circle is gone. Not saved: after a restart a circle still standing is found and told again.
    pub circles: HashMap<String, (u64, bool)>,
    /// Each repository's copies whose work is already on its main line, found once a launch for you to remove
    /// (desk/hub/landing/copies.rs, #376). Not saved.
    pub old_copies: HashMap<String, crate::hub::OldCopies>,
    /// Items Cue is starting a new session for right now (#656): from the moment the start is sent off
    /// (desk/hub/lead/checks.rs `desk_later`) until that session has it or couldn't start (desk/hub/lead/run.rs
    /// `desk_start_new`). A second start of the same item meanwhile is dropped, and a route of it to new is told
    /// one is starting. Not saved.
    pub new_starts: HashSet<String>,
    /// The machine each item in `new_starts` is starting on, picked when its start began (#661, "" for this Mac): a
    /// Desk spreading its items over several machines counts it there, so two starting at once go to different ones.
    /// Not saved.
    pub start_machine: HashMap<String, String>,
    /// Each item's session Cue started for it, until that session's first turn ends (#656): item → (session, when).
    /// The lead routing it to new in its first minutes (it read as waiting in a Desk block made before the session
    /// was up) is told it's in one already, and it isn't moved off the session that just began on it. Not saved.
    pub started_for: HashMap<String, (String, u64)>,
    /// The install the Desk shows (`state.desk.install`, desk/hub/landing/install_shown.rs); None when there's been none.
    /// Not saved.
    pub install_shown: Option<serde_json::Value>,
    /// When the install the Desk shows began: (its tag, ms). Not saved.
    pub install_began: Option<(String, u64)>,
    /// The last step under way seen of the install the Desk shows ("building", "installing", "starting"): (its tag,
    /// that step), what a failure says it failed at. Not saved.
    pub install_last_step: Option<(String, String)>,
    /// The Desk whose lead asked for an install (INSTALL: main): (its tag, that Desk). Not saved.
    pub install_lead_desk: Option<(String, String)>,
}
