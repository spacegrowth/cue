//! The Desk's records: items, who holds them, and decisions. Times are in seconds (the window's Desk shows them so).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Who holds an item: a live session (a plain one, or a lead with a crew), or one of a lead's crew.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Holder {
    /// "session" | "lead" | "crew" | "loop" (an add-on's)
    pub kind: String,
    /// The session's id; a crew member's is its agent id (its lead's session is `lead_sid`).
    pub sid: String,
    pub name: String,
    /// A crew member's lead.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lead_sid: String,
}

impl Holder {
    /// The session that's sent its messages: a crew member's lead, or the holder itself.
    pub fn to(&self) -> &str {
        if self.lead_sid.is_empty() { &self.sid } else { &self.lead_sid }
    }

    /// Whether session `sid` is this holder, or its lead.
    pub fn is(&self, sid: &str) -> bool {
        self.sid == sid || self.lead_sid == sid
    }
}

/// A bug, feature or task on a project's Desk.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DeskItem {
    pub id: String,
    /// The random tag it had before items were numbered ("d7k2"), so old lines and links that name it still find it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alias: String,
    /// Desk identity. The historical field name is kept for saved records and IPC callers.
    pub project: String,
    /// "bug" | "feature" | "task"
    pub kind: String,
    pub title: String,
    /// The words it was added with, once the lead gave it a shorter title (TITLE:) or split it (SPLIT:): shown on its
    /// page as "Your words". Empty while the title is still those words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub words: String,
    /// The item it was split from (SPLIT:), when it was.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub split_from: String,
    /// The big feature it's one piece of (#282): that item's id. The Desk lists it under that feature's name, and
    /// the feature is done once all its pieces are (hub/desk_features.rs). Set by the lead's FEATURE: line.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part_of: String,
    /// Its pieces, when it's a feature with any, oldest first: worked out as it's shown (hub/desk_view.rs), never saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<DeskItem>,
    /// What you'll see when it's done, in plain words: the one line about it shown to you (#382).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub see: String,
    /// The technical check of it being done: told only to the session doing it (#382), not shown to you.
    #[serde(default)]
    pub done_when: String,
    /// Why it's its own item, when a session on another item added it (`ITEM: … | why: …`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// "new" | "queued" | "held" | "working" | "finishing" | "to_check" | "done" as kept. "held" (#385): on the held
    /// shelf, nobody holding it; never started by Cue or the lead, only by your Start (it's new again then).
    /// The displayed "needs_you", "needs_resume" and "paused" states are derived from live conditions.
    pub state: String,
    #[serde(default)]
    pub holder: Option<Holder>,
    /// Sessions that held this item before its current holder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former_holders: Vec<String>,
    /// The holder it moved off when the lead routed it to a new session (`ROUTE: #id -> new` on a held item, #364),
    /// until a session takes it: the new one is sent that holder's hand-off note with it (hub/desk_lead.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<Holder>,
    /// The lead's teammate doing it, when the lead keeps it and says so (`ROUTE: #id -> me | crew: <name>`): the name
    /// the lead gave it, matched to its teammate of that name once there is one (hub/desk_view.rs `shown`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crew: String,
    /// The item it waits on (queued behind it), and why.
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub after_why: String,
    /// Its holder's latest, in one line, while its holder is on it: worked out as it's shown.
    #[serde(default)]
    pub latest: String,
    /// Its holder is at work on this one of the items it holds (hub/desk_stall.rs `now_on`): worked out as it's shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub now_on: bool,
    /// Its holder stopped on it with no DONE and nothing to ask you, and Cue gave the stop to the Desk lead to settle:
    /// (the item's `updated` then, when in ms). Saved, so a restart keeps it (hub/desk_stall.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<(u64, u64)>,
    /// The Desk lead was told of a stop of it (the item's `updated` then): once per item, a later stop is yours (#249).
    /// Saved (hub/desk_stall.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub told: Option<u64>,
    /// What its holder's last turn on it said it waits on (the item's `updated` then, the line): waiting, not
    /// stopped, while the item is unchanged (#249, hub/desk_stall.rs). Saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits: Option<(u64, String)>,
    /// Its holder's turn ended on it saying what it waits on: that line. Worked out as it's shown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub waiting: String,
    /// Its holder stopped on it and the Desk lead is settling it: nothing for you yet. Worked out as it's shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stopped: bool,
    /// What its holder did last while at work on it, and when (#285): its row says so, and reads "Stalled" once that
    /// is `QUIET_MS` old. Worked out as it's shown (hub/desk_stall.rs `act_of`); None when it isn't at work on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Act>,
    /// Its work git said wasn't committed as its holder's turn last ended on it (paths in their folders): a stop then
    /// is the Desk lead's, shown as "not committed", never waiting on you (#213, hub/desk_stall.rs). Saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<String>,
    /// Its holder's open ask, when it waits on you: worked out as it's shown.
    #[serde(default)]
    pub ask: Option<Value>,
    /// The files changed for it, inside its holder's folder: worked out as it's shown (hub/desk.rs `item_files`).
    #[serde(default)]
    pub files: Vec<String>,
    /// Pictures you attached as you added it: where Cue keeps them (uploads.rs `save_kept`), named to whoever takes it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// What its holder made, as it said DONE: the links and files that turn named (a page, a mock, a build), so the
    /// Desk can link to them (parse.rs `made_in`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub made: Vec<String>,
    /// Its designs' canvas: the link a session gave last on a DESIGNS: line, shown as "Designs" on its page.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designs: String,
    /// The option you picked on its mocks ("B"), once you did (hub/desk_act.rs `pick`): the item is done then.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picked: String,
    /// The item that builds what you picked (`picked`): its id. A pick again names it and makes no other.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub built_as: String,
    /// The mocks item whose pick this item builds: its id (the other end of that one's `built_as`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub builds: String,
    /// The commit Looks right made of its files (short; one per repository, space apart), once it did
    /// (hub/desk_commit.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// Looks right's commit while it waits: some of its files were being edited by another session at work, so the
    /// item is done and Cue commits them on its own once they're free (hub/desk_commit.rs `desk_commit_waiting`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_wait: Option<CommitWait>,
    /// Its DONE being finished, saved so it survives the restart an install makes: its commit made, then this Cue
    /// built and installed with it (hub/desk_finish.rs). Kept once it's to check, so Looks right only closes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<Finish>,
    /// The Desk lead said DONE on it for you (you told it it's done) while it was finishing or to check: closed as
    /// Looks right would, at once when it's to check, else as soon as its finishing brings it there (hub/desk_said.rs
    /// lead_done, #337). Saved, so an install's restart keeps it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub close_on_check: bool,
    /// Its own copy of its repository to work in (a git worktree on a branch of its own, from the main line as its
    /// holder started), while it has one: landed as one squash commit at its DONE (hub/desk_trees.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<ItemTree>,
    /// Its place in its Desk's queue as you dragged it (#387), 1 at the top: the next free copy goes to the top one, and
    /// the lead sees what needs routing in this order. None: after those you placed, oldest first. Saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_order: Option<u32>,
    /// Queued for a copy of its own: all of its repository's copies are in use, so it starts once one is free.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tree_wait: bool,
    /// The repository whose copy it's queued for (with `tree_wait`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tree_wait_repo: String,
    /// Your Send back's message to its holder, kept while it waits for a copy (#396): it goes with the item's words
    /// once one is free (hub/desk_trees.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub back_waiting: String,
    /// Waiting for a copy (`tree_wait`): the items using its repository's copies now, as its row says (#401). Only
    /// on what the window is shown (hub/desk_view.rs `shown`), never saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copy_users: Vec<String>,
    /// Started now by the lead (START, #347) while it waited: it works where its holder is when no copy is free,
    /// rather than queue for one again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_copy_wait: bool,
    /// Where each of its commits is: (folder, short hash), for its Files changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_in: Vec<(String, String)>,
    /// Who it went to at work and when (ms): only work begun after that is its (stamped as it's saved: hub/desk.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_from: Option<(String, u64)>,
    /// Its stints with holders before `held_from` (who, from, until; ms), newest last, the last `HELD_KEPT`: an item that
    /// moved on still ends the window of an item its old holder kept (hub/desk_view.rs `held_window`, #302).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_before: Vec<(String, u64, u64)>,
    pub created: u64,
    pub updated: u64,
    #[serde(default)]
    pub done_at: Option<u64>,
    /// "you", or the name of the session whose ITEM: line added it (the lead's, or any session's of the project).
    /// Rows from before leads say "dispatch".
    pub from: String,
    /// A route the lead asked for that Cue couldn't make (no session by that name), until the lead is told.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub route_failed: String,
    /// Why Cue couldn't start a new session for it (a ROUTE to new, or your Start now): it shows "couldn't start"
    /// with a retry until a session takes it or you start it again. The lead is told at once (hub/desk_lead.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_failed: String,
    /// The Desk lead paused it (#365, PAUSE:): when (secs) and why. Its holder keeps it and its place, stopped at a
    /// clean point; it isn't asked "Done?" and the lead isn't told it stopped, until RESUME: (or a new route, or its
    /// DONE) clears it. Shown as "paused" while it's at work (hub/desk.rs `shown`). Saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<(u64, String)>,
    /// Put on the held shelf (#385): when (secs), by whom ("You", or the lead's name as you asked it), and why. Set
    /// while it's "held"; cleared by Start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<(u64, String, String)>,
    /// Your last Restart of it (#291): where it went, or why it couldn't ("couldn't reach cue-251: …"). Saved.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub restart: String,
    /// An `after` the lead asked for that Cue refused, and why (e.g. "#b, which waits on it"), so the item started
    /// now; until the lead is told.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after_refused: String,
    /// A ROUTE to reopen it (with you to check) that Cue didn't act on, and why; until the lead is told.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reopen_refused: String,
    /// Each time its holder said DONE for it: kept, so the lead's chat shows a card for each, after a restart too
    /// (hub/desk_checks.rs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<DeskCheck>,
    /// What its holder said as each turn ended while at work on it, oldest first (the last `SAID_KEPT`): the
    /// session's replies on its page's Thread, between your words and the lead's hand-offs (its Steps).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub said: Vec<Said>,    /// When it first reached each of its six steps (ms; 0: not yet): picked up, building, tests, commit, landing, to
    /// check (#409, hub/desk_steps.rs). The Desk's Progress view shows them, and how long each took. Saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<u64>,
    /// Each time it went back from its finish to be redone (#421): sent back, its landing failed, its work taken out.
    /// Its Progress row draws an arrow back with "round N" and why. Saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backs: Vec<StepBack>,
}

/// One time item went back to be redone (#421, hub/desk_steps.rs): the step it was at, why (as its DONE's card keeps
/// it), when, and that round's step stamps.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StepBack {
    pub from: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
    pub at_ms: u64,
    #[serde(default)]
    pub steps: Vec<u64>,
}

/// What an item's holder did last while at work on it (#285), as its row shows it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Act {
    /// When it last did anything (ms): its latest step, its turn's start, its last words, its crew's latest step.
    pub at_ms: u64,
    /// What it's doing, as the row says it while it moves: "Editing desk.rs".
    pub now: String,
    /// The same once it has stalled: "last read model.rs".
    pub was: String,
    /// When it reads "Stalled" if it does nothing more (ms): `QUIET_MS` after `at_ms`. 0 while its helpers are at it
    /// (it handed work to an agent that runs on): never stalled meanwhile. The window reads it stalled from then on by
    /// its own clock (desk.js `deskStalled`), so the row turns without the hub saying anything new.
    #[serde(default)]
    pub stalls_at: u64,
}

/// How long an item's holder may do nothing while at work on it before the item reads "Stalled" (#285): the one
/// threshold (the window gets `Act::stalls_at`).
pub const QUIET_MS: u64 = 10 * 60 * 1000;

/// How many earlier stints an item keeps (`held_before`).
pub const HELD_KEPT: usize = 8;

impl DeskItem {
    /// Its work starts now with `sid` (ms `now`): the stint before it, if any, is kept in `held_before`.
    pub(crate) fn held_by(&mut self, sid: &str, now: u64) {
        if let Some((by, from)) = self.held_from.replace((sid.to_string(), now)) {
            self.held_before.push((by, from, now));
            let over = self.held_before.len().saturating_sub(HELD_KEPT);
            self.held_before.drain(..over);
        }
    }

    pub(crate) fn remember_holder(&mut self) {
        if let Some(sid) = self.holder.as_ref().map(Holder::to).filter(|sid| !self.former_holders.iter().any(|old| old == sid)) {
            self.former_holders.push(sid.to_string());
        }
    }
}

/// How many of an item's replies it keeps, and how long each is at most (chars): each reply whole on its page (#276),
/// the bound only so a runaway turn can't bloat the Desk's state.
pub const SAID_KEPT: usize = 20;
pub const SAID_CHARS: usize = 20_000;

/// One reply on an item's Thread: when (ms), who said it, its words (the Desk's own lines left out), whether it said
/// DONE for the item, and the session it said it in (its page links to that turn there, #276). What a Stop hook made
/// it write after its answer (a checklist) is `after`, folded under the reply as the chat folds it (#310).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Said {
    pub at: u64,
    pub by: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sid: String,
}

/// A commit waiting for files another session is editing: the item's files to commit (its folder, the path there),
/// the machine they're on ("" here), which of them are busy and who has them, and why it couldn't commit once free.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CommitWait {
    pub files: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub machine: String,
    #[serde(default)]
    pub busy: Vec<BusyFile>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Where an item's finishing is (hub/desk_finish.rs).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Finish {
    /// "commit" (being made) | "line" (in its own copy, waiting in the landing line: `wait` says on what) |
    /// "checking" (its check runs before it lands) | "check_failed" | "review" (its pull request is open: `pr`) |
    /// "pr_failed" (its branch couldn't be pushed or its pull request opened) | "pr_closed" | "landing" (made,
    /// and not on main yet: `wait` says where it is) | "queued" (for
    /// the next install) | "building" | "installing" | "starting" | "word" (the word check found one) | "conflict"
    /// (the merge of its crew's branch) | "error" (git couldn't commit) | "failed" (the build) | "rolled_back" (the
    /// new Cue didn't start) | "checked" (it went to check: nothing to install, or installed).
    pub step: String,
    /// When this step began (ms). For "queued", when it was queued: the install waits for the latest of those.
    pub at: u64,
    /// The commit on the main line that has its work (full), and the repository it's in: what the install builds
    /// and what the installed Cue is asked to have. Empty while none is made (nothing to commit).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repo: String,
    /// When the lead moved it up in the landing line (LAND FIRST, #347; ms): it lands before items it wasn't put
    /// ahead of, the latest moved first.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub first: u64,
    /// What it waits on in the landing line ("the main checkout is in the middle of a merge"), or where its work
    /// is while it isn't on main ("on fix-header, not main").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wait: String,
    /// When it began to wait for its work to be on main (ms): past `desk_finish::WAIT_ASK_MS` it's asked of you. In
    /// review, when its pull request was first seen merged but not on main (hub/desk_pr.rs).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub wait_from: u64,
    /// What it asks you, when it stopped: the window shows it as needing you.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question: String,
    /// The end of the install's log, when the build failed or the new Cue didn't start.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub log: String,
    /// The install it's in (the tag Cue gave it), once one started.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// "Install now": the install starts without waiting for more items or for sessions mid-turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub now: bool,
    /// Files left out of its commit in a shared folder (absolute paths): another session was editing them, or changed
    /// them too in a way that can't be told apart from the item's. They stay in the folder, uncommitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub without: Vec<String>,
    /// The tree its check last passed on (or ran with none to run): a wait in the landing line doesn't check it again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checked: String,
    /// Landed with no check to run (its repository has none): shown as "landed, not checked".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unchecked: bool,
    /// Its pull request's link, once the Desk opened one (step "review", hub/desk_pr.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pr: String,
    /// The branch its pull request goes into ("main"): once merged, the main checkout is brought up to it only while
    /// that's the branch it's on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base: String,
    /// Its project pushes main after each landing: "pushed", or why it couldn't ("" when nothing was pushed).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pushed: String,
    /// The merges of its crew's branches into the main line (main checkout, merge commit): with `commit_in`, what its
    /// own work changed, so an item that changed nothing the app runs isn't installed (#408).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merges: Vec<(String, String)>,
    /// It went to check with nothing to install, its own work outside the app (mocks, notes, plans, #408): its line
    /// says "Mocks · open the canvas", not "In the app" (#436).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mocks: bool,
}

/// An item's own copy of its repository (hub/desk_trees.rs): the main checkout it's from, the copy's folder, its
/// branch ("desk/<item>"), the main line's commit it started from, and which of the repository's copies it is (1..).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ItemTree {
    pub repo: String,
    pub path: String,
    pub branch: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub slot: u32,
}

/// A dropped item's copy that had work in it, kept (moved aside, its branch kept) and listed on its Desk until you
/// remove it: its item, title and Desk, the copy, and when it was dropped (ms).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KeptTree {
    pub id: String,
    pub title: String,
    pub project: String,
    pub repo: String,
    pub path: String,
    pub branch: String,
    pub at: u64,
}

/// A file another session at work is editing, and that session's name.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BusyFile {
    pub path: String,
    pub by: String,
}

/// A DONE for an item: when (ms), who did it, the files it touched (filled in a moment later, read from its logs:
/// hub/desk_checks.rs), and whether it was made in the folder this Cue is built from.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DeskCheck {
    pub at: u64,
    pub by: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cue: bool,
    /// When its work last changed (ms): its newest file here, else when this round began. The installed Cue has it
    /// when it was installed after this, not after the DONE (you install, then say DONE).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub edited: u64,
    /// Sent back after it: why, in your words, and how many pictures went with them.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub back: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub back_pics: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A call someone made about the Desk's work: the lead's (a route, a DECIDED line), a holder's DECIDED line, or
/// yours (giving an item to a session).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DeskDecision {
    pub id: String,
    pub project: String,
    #[serde(default)]
    pub item_id: Option<String>,
    /// "You", the lead's name, or the holder's name.
    pub by: String,
    /// Who made it, and is asked to undo it ("" for yours).
    pub sid: String,
    pub text: String,
    #[serde(default)]
    pub why: String,
    pub at: u64,
    /// "none" | "can" | "asked" | "one_way"
    pub undo: String,
    /// A judgment call (a DECIDED line, the lead's or a holder's): the only kind the Decisions list shows. The rest
    /// (routes, gives, sessions started, contexts kept, Cue's hand-off asks) are steps, shown on their item.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub call: bool,
    /// A housekeeping call (`DECIDED (minor): …`: a folder, a route, an order, a copy): kept, but the Decisions list
    /// shows it only under "Show all". The rest are key product calls.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minor: bool,
    /// The message associated with this step, for showing its existing session delivery state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeskDelivery>,
    /// What the lead did on its own (#360), shown in its chat as it happens and, for a while, in the Dispatcher's
    /// panel, and kept in its "What the lead moved" list (#374): "close" | "take_copy" | "land_first" | "start" |
    /// "send_back" | "answer" | "call" (one of its DECIDED lines that's dispatch work, not a product choice: #420); "" for
    /// any other step.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub act: String,
}

/// Words that make one of the Desk lead's calls dispatch work (routing, starting, order, sending back, clashes,
/// landing, copies, closing) rather than a product choice: kept, but behind "Show all" (#403).
const LEAD_DISPATCH: &[&str] = &[
    "route", "routes", "routed", "routing", "re-route", "reroute", "re-routed", "hand-off", "handoff", "hand off", "handed",
    "send back", "sent back", "send-back", "sends back", "sent", "clash", "clashes", "conflict", "conflicts", "merge", "merged",
    "land", "lands", "landed", "landing", "commit", "commits", "committed", "push", "pushed", "copy", "copies", "worktree",
    "resume", "resumes", "resumed", "hold", "held", "pause", "paused", "restart", "restarted", "start", "starts", "started",
    "wait", "waits", "queue", "queued", "order", "filed", "told", "close", "closes", "closed", "routes first", "takes over",
    "goes to", "go to", "went to", "gave", "give", "gives", "given", "send", "sends", "new session",
];
/// The same for an item holder's calls, which are mostly about what it built (and may well name routing as a
/// feature): plain notes of sending work back, a landing clash or a hand-off, and of how it worked rather than what
/// the owner gets (#420): its copy, branch, rebases and builds, its tests, its teammates and reviews, its mocks' canvas, its repo.
const HOLDER_DISPATCH: &[&str] = &[
    "sent back", "sent it back", "landing clash", "merge clash", "merge conflict", "handed off", "hand-off to", "re-routed",
    "working copy", "worktree", "branch", "commit message", "rewriting main", "own repo", "crate", "debug files", "teammate",
    "teammates", "the review", "review request", "reviewer", "finding", "findings", "canvas", "as your pick", "no change to cue",
    "left as they are", "left as it is", "alone", "rebase", "rebased", "rebasing", "build cache", "install built", "built from", "the test", "the tests", "new test", "new tests", "tests file", "its test", "its tests",
];
/// A source file named in a call ("`desk_finish.rs`"): how it was built, not what the owner gets.
const CODE_FILES: &[&str] = &[".rs", ".js", ".mjs", ".ts", ".css", ".html", ".json", ".toml", ".swift", ".sh", ".py"];

/// Whether a call's words `text` are dispatch work, not a choice worth the owner's glance: `lead`, the Desk lead's
/// (its job is dispatching, so an item number or any dispatch word marks it); else an item holder's.
pub fn dispatch_call(text: &str, lead: bool) -> bool {
    let lower = text.to_lowercase();
    let t = format!(" {} ", lower.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '#')).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "));
    let has = |w: &&str| t.contains(&format!(" {w} "));
    // An item number ("#12") or a session's name ("cue-402"): who gets what.
    let named = |w: &str| {
        let num = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
        w.strip_prefix('#').is_some_and(num) || w.rsplit_once('-').is_some_and(|(a, n)| !a.is_empty() && a.chars().any(|c| c.is_alphabetic()) && num(n))
    };
    if lead {
        t.split(' ').any(named) || LEAD_DISPATCH.iter().any(has)
    } else {
        let file = |w: &str| {
            let w = w.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'));
            CODE_FILES.iter().any(|x| w.len() > x.len() && w.ends_with(x))
        };
        // "the #145 test": a test it changed.
        let ws: Vec<&str> = t.split(' ').collect();
        let its_test = ws.windows(2).any(|p| (p[1] == "test" || p[1] == "tests") && p[0].strip_prefix('#').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())));
        HOLDER_DISPATCH.iter().any(has) || lower.split_whitespace().any(file) || its_test
    }
}

/// A call as compared with another for being said again: its words, any case, spaces and end stop aside.
pub fn call_words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches(['.', '!']).to_lowercase()
}

impl DeskDecision {
    /// A call that's dispatch work (`dispatch_call`; one noted as yours, like a Send back note, always is) goes behind
    /// "Show all", off the Decisions list (#403). `lead`: whether the Desk lead made it.
    /// A lead's dispatch call is one of its moves too, listed in Dispatcher Actions (#420).
    pub fn sort_call(&mut self, lead: bool) {
        if self.call && (self.sid.is_empty() || dispatch_call(&self.text, lead)) {
            self.minor = true;
        }
        if self.call && self.minor && lead && !self.sid.is_empty() && self.act.is_empty() {
            self.act = "call".into();
        }
    }

    /// Whether this is call `c` said again: the same words by the same session on the same item and Desk.
    pub fn same_call(&self, project: &str, item: Option<&str>, sid: &str, text: &str) -> bool {
        self.call && self.project == project && self.item_id.as_deref() == item && self.sid == sid && call_words(&self.text) == call_words(text)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeskDelivery {
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    pub at_ms: u64,
}

/// Every project's Desk, as Cue holds it while it runs (saved as it changes: db/desk.rs).
#[derive(Default, Debug)]
pub struct Desk {
    /// Independent Desks. Older project-keyed Desks resolve through the fallback in `project_of`.
    pub spaces: HashMap<String, DeskSpace>,
    /// Sessions being selected as leads, reserved across concurrent Desk changes.
    pub lead_claims: HashMap<String, String>,
    pub items: Vec<DeskItem>,
    pub decisions: Vec<DeskDecision>,
    /// Each Desk's lead, keyed by its identity.
    pub lead: HashMap<String, Lead>,
    /// Decisions made since Cue started, for unique ids.
    pub next: u64,
    /// Projects whose lead is being started or resumed, with the messages that arrived while it was starting, sent
    /// once it's up.
    pub starting: HashMap<String, Vec<Waiting>>,
    /// Each project's new items its lead hasn't been told of yet: told in one message a moment after the first.
    pub fresh: HashMap<String, Vec<String>>,
    /// Sessions whose context the lead said to keep as it is (CONTEXT: … -> keep): session → (when it said so, its
    /// decision, whether it was at work then). Cue asks no hand-off note of it for the idle stretch it was in then,
    /// or, said while it worked, the one after that turn (stamped as the turn ends: hub/desk.rs; hub/cache_tick.rs).
    pub keep: HashMap<String, (u64, String, bool)>,
    /// The highest item number given since Cue started, so one dropped meanwhile isn't given again.
    pub item_high: std::sync::atomic::AtomicU64,
    /// Each project's sessions on its Desk, in the order they joined: the ones its lead may route to (saved:
    /// db/desk.rs). You add and remove them; a session given an item joins, as does one you say yes to using.
    pub members: HashMap<String, Vec<String>>,
    /// Sessions whose compacting failed (an error, or no end in 15 minutes), and when: a session on a Desk with a live
    /// lead is asked for a hand-off note instead, and goes on fresh from it (hub/cache_tick.rs full_due). Not saved.
    pub compact_failed: HashMap<String, u64>,
    /// Items a lead said DONE on that its DONE couldn't close, and how each was ("state@updated") when Cue told it
    /// why (hub/desk_said.rs lead_done): a lead saying it again of an unchanged item isn't answered again. Not saved.
    pub done_told: HashMap<String, String>,
    /// A holder's turn that said DONE while its background agents or crew were still at work (#393), by its session:
    /// acted on once they've finished (hub/desk_heard.rs `held_done`), at its next turn's end or as its background
    /// agents are seen done (hub/activity.rs `watch_helpers`). Not saved.
    pub done_held: HashMap<String, String>,
    /// What a project's lead is to be told of its routes Cue couldn't make (hub/desk_said.rs `route_answers`, #248) when it
    /// couldn't be sent then, as the lead was asking the user something: told in its next Desk block instead, and
    /// cleared once that reaches it (`desk_told`). Not saved.
    pub routes_held: HashMap<String, String>,
    /// Items whose holder's turn ended on them with no DONE, and Cue asked it once ("Done? …"): item → (that holder,
    /// the item's `updated` then). The turn that answers isn't asked again; the item changed since (sent again, routed)
    /// is a new stall (hub/desk_stall.rs). Not saved.
    pub nudged: HashMap<String, (String, u64)>,
    /// Items whose holder said what's left of them, or what they wait on (item → (that holder, the item's `updated`
    /// then)): not asked "Done?" again until the item changes (hub/desk_stall.rs). Not saved.
    pub left_said: HashMap<String, (String, u64)>,
    /// When Cue last sent each working item to its holder (item → (holder, ms)): a turn that ended before it isn't a
    /// stall on it. Not saved: after a restart, `held_from` stands in.
    pub sent_at: HashMap<String, (String, u64)>,
    /// What builds and installs this Cue with a finished item's commit: the app's own (self_install.rs) unless a test
    /// put another in. Not saved.
    pub installer: InstallerSlot,
    /// The program a pull request is opened and read with: GitHub's gh unless a test put a fake in. Not saved.
    pub gh: String,
    /// When Cue last read each item's pull request (ms): it looks once a minute. Not saved.
    pub pr_looked: HashMap<String, u64>,
    /// Reads of each item's pull request that failed in a row: it says so only from the third. Not saved.
    pub pr_misses: HashMap<String, u32>,
    /// How long a push, a fetch or a gh call may take (ms; 0 for a minute): a test's shorter. Not saved.
    pub net_cap_ms: u64,
    /// Finishing items whose commit is being made now, and "tick" while the finishing tick runs: one run of each at a
    /// time (hub/desk_finish.rs). Not saved.
    pub finish_claims: HashSet<String>,
    /// Each main checkout's HEAD as Cue last looked for a commit that takes a finished item's work back out
    /// (hub/desk_finish.rs `taken_out_tick`). Not saved: the first look after a start only notes it.
    pub seen_heads: HashMap<String, String>,
    /// Dropped items' copies kept with their work, listed on their Desk (saved: db/desk.rs).
    pub kept: Vec<KeptTree>,
    /// Circles in the waits list (hub/desk_waits.rs), by their key: when each was found, and whether the lead was told.
    /// Dropped once the circle is gone. Not saved: after a restart a circle still standing is found and told again.
    pub circles: HashMap<String, (u64, bool)>,
    /// When each Desk's list of what its lead moved (#374) was last cleared (ms; saved: db/desk.rs).
    pub moves_cleared: HashMap<String, u64>,
    /// Each repository's copies whose work is already on its main line, found once a launch for you to remove
    /// (hub/desk_copies.rs, #376). Not saved.
    pub old_copies: HashMap<String, crate::hub::OldCopies>,
}

/// How an install is going, as the installer says: its tag and commit, its step ("building" | "installing" |
/// "starting" | "installed" | "failed" | "rolled_back"), its log's end, and when (ms).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstallStatus {
    pub tag: String,
    pub commit: String,
    pub step: String,
    pub log: String,
    pub at: u64,
}

/// What builds this Cue from a commit and installs it (the app restarts into it), and says how that's going.
pub trait Installer: Send + Sync {
    /// Whether the running Cue was built from `commit` of `repo`, or from one that has it.
    fn has(&self, repo: &str, commit: &str) -> bool;
    /// An install of `commit` of `repo` begun, called `tag`.
    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String>;
    /// The latest install's status, if there's been one.
    fn status(&self) -> Option<InstallStatus>;
    /// The check `commit` of `repo` has to pass before it lands on the main line: None when the repository has no
    /// check, else what it said (an error with the end of its output when it failed).
    fn check(&self, _repo: &str, _commit: &str) -> Option<Result<(), String>> {
        None
    }
    /// Whether `of` of `repo` has `commit`: it is that commit or one after it.
    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool;
}

/// The installer a Desk uses: None for the app's own.
#[derive(Clone, Default)]
pub struct InstallerSlot(pub Option<std::sync::Arc<dyn Installer>>);

impl std::fmt::Debug for InstallerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "InstallerSlot(set)" } else { "InstallerSlot(app)" })
    }
}

/// A Desk's identity and workspace, independent of its lead and items.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeskSpace {
    pub id: String,
    pub name: String,
    pub project: String,
    pub folder: String,
}

/// The session that runs a project's Desk, and the name it went by when last seen (shown once it has ended).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lead {
    pub sid: String,
    pub name: String,
}

/// A message to a lead held while it starts (or resumes, or changes), and where its outcome goes: the lead's id once
/// it's sent, or why it wasn't (whoever sent it waits on that; dropped unanswered, it wasn't sent).
#[derive(Debug)]
pub struct Waiting {
    /// Your words: sent after the Desk's summary as built for the lead it reaches (which may not be the one there was
    /// when you sent them).
    pub text: String,
    pub reply: std::sync::mpsc::Sender<Result<String, String>>,
}

impl Desk {
    pub fn project_of<'a>(&'a self, id: &'a str) -> &'a str {
        self.spaces.get(id).map_or(id, |d| d.project.as_str())
    }

    pub fn name_of<'a>(&'a self, id: &'a str) -> &'a str {
        self.spaces.get(id).map_or(id, |d| d.name.as_str())
    }

    /// Every Desk this session participates in, with its legacy project Desk as a fallback.
    pub fn scopes(&self, sid: &str, project: &str) -> Vec<String> {
        let mut ids: Vec<String> = self.members.iter().filter(|(_, m)| m.iter().any(|s| s == sid)).map(|(id, _)| id.clone())
            .chain(self.lead_of(sid))
            .chain(self.items.iter().filter(|i| i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| i.project.clone())).collect();
        if ids.is_empty() && !project.is_empty() {
            ids.extend(self.spaces.values().filter(|d| d.project == project).map(|d| d.id.clone()));
            let legacy = self.lead.contains_key(project) || self.members.get(project).is_some_and(|m| !m.is_empty()) || self.items.iter().any(|i| i.project == project);
            if ids.is_empty() || legacy {
                ids.push(project.to_string());
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }
    /// The project whose Desk session `sid` leads.
    pub fn lead_of(&self, sid: &str) -> Option<String> {
        self.lead.iter().find(|(_, l)| l.sid == sid).map(|(p, _)| p.clone())
    }

    /// `project`'s lead's session.
    pub fn lead_sid(&self, project: &str) -> Option<&str> {
        self.lead.get(project).map(|l| l.sid.as_str())
    }

    /// Whether session `sid` is on `project`'s Desk.
    pub fn on_desk(&self, project: &str, sid: &str) -> bool {
        self.members.get(project).is_some_and(|m| m.iter().any(|s| s == sid))
    }

    /// Session `sid` on `project`'s Desk from now: true when it wasn't before.
    pub fn join(&mut self, project: &str, sid: &str) -> bool {
        if sid.is_empty() || self.on_desk(project, sid) {
            return false;
        }
        self.members.entry(project.to_string()).or_default().push(sid.to_string());
        true
    }

    /// Session `sid` off `project`'s Desk: true when it was on it.
    pub fn leave(&mut self, project: &str, sid: &str) -> bool {
        let Some(m) = self.members.get_mut(project) else { return false };
        let before = m.len();
        m.retain(|s| s != sid);
        before != m.len()
    }

    /// The item `id` names: by its number, or by the tag it had before (`alias`).
    /// Calls kept from before #403 sorted as new ones are (#420: and those said again). A lead's: one by a Desk's lead, or by a session that made
    /// a lead's own moves (an earlier lead), or with no item, or on an item its maker didn't hold. Those that changed.
    pub fn sort_calls(&mut self) -> Vec<String> {
        let held: HashMap<&str, &Holder> = self.items.iter().filter_map(|i| i.holder.as_ref().map(|h| (i.id.as_str(), h))).collect();
        let leads: HashSet<String> = self.lead.values().map(|l| l.sid.clone()).chain(self.decisions.iter().filter(|d| !d.act.is_empty() && d.act != "call").map(|d| d.sid.clone())).collect();
        let mut changed = Vec::new();
        // A call said again (a session repeats its DECIDED lines in later replies): the first stays, the rest go behind
        // "Show all" (#420).
        let mut seen: HashSet<(String, Option<String>, String, String)> = HashSet::new();
        for d in self.decisions.iter_mut().filter(|d| d.call) {
            let was = (d.minor, d.act.clone());
            if !seen.insert((d.project.clone(), d.item_id.clone(), d.sid.clone(), call_words(&d.text))) {
                d.minor = true;
            } else {
                // An item no longer loaded and a maker not known as a lead: taken as its holder's (the narrower rule).
                let lead = leads.contains(&d.sid)
                    || match d.item_id.as_deref() {
                        None => true,
                        Some(i) => held.get(i).is_some_and(|h| !h.is(&d.sid)) || (!held.contains_key(i) && self.items.iter().any(|x| x.id == i)),
                    };
                d.sort_call(lead);
            }
            if (d.minor, d.act.as_str()) != (was.0, was.1.as_str()) {
                changed.push(d.id.clone());
            }
        }
        changed
    }

    pub fn item(&self, id: &str) -> Option<&DeskItem> {
        self.items.iter().find(|i| i.id == id).or_else(|| self.items.iter().find(|i| !i.alias.is_empty() && i.alias == id))
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut DeskItem> {
        let at = self.items.iter().position(|i| i.id == id).or_else(|| self.items.iter().position(|i| !i.alias.is_empty() && i.alias == id))?;
        self.items.get_mut(at)
    }

    /// The id an item goes by now, for `id` as a line named it (an old tag gives its number); unknown ones as they are.
    pub fn canon(&self, id: &str) -> String {
        self.item(id).map_or_else(|| id.to_string(), |i| i.id.clone())
    }

    /// Items from before numbering (random tags) numbered now, oldest first, after the highest number there is; each
    /// keeps its tag as its `alias`, and what named it (an item's `after`, a decision's item) names its number. The
    /// (old, new) ids changed, to save.
    pub fn number_old(&mut self) -> Vec<(String, String)> {
        let mut old: Vec<usize> = (0..self.items.len()).filter(|&k| self.items[k].id.parse::<u64>().is_err()).collect();
        old.sort_by_key(|&k| (self.items[k].created, k));
        let mut high = super::next_number(self.items.iter().map(|i| i.id.as_str()), 0) - 1;
        let mut changed = vec![];
        for k in old {
            high += 1;
            let it = &mut self.items[k];
            let was = std::mem::replace(&mut it.id, high.to_string());
            if it.alias.is_empty() {
                it.alias = was.clone();
            }
            changed.push((was, it.id.clone()));
        }
        for (was, now) in &changed {
            for it in self.items.iter_mut().filter(|i| i.after.as_deref() == Some(was.as_str())) {
                it.after = Some(now.clone());
            }
            for d in self.decisions.iter_mut().filter(|d| d.item_id.as_deref() == Some(was.as_str())) {
                d.item_id = Some(now.clone());
            }
        }
        changed
    }

    /// A new item's id: the next plain number ("12"), one past the highest any item has (any project's: ids are
    /// shared, so "#12" is one item everywhere). Items from before had random ones ("d7k2"): `number_old` numbers them.
    pub fn new_id(&self) -> String {
        use std::sync::atomic::Ordering;
        let n = super::next_number(self.items.iter().map(|i| i.id.as_str()), self.item_high.load(Ordering::Relaxed));
        self.item_high.fetch_max(n, Ordering::Relaxed);
        n.to_string()
    }

    /// A new decision's id: "k<ms>-<n>".
    pub fn decision_id(&mut self, now_ms: u64) -> String {
        self.next += 1;
        format!("k{now_ms}-{}", self.next)
    }
}

fn is_zero_u64(n: &u64) -> bool {
    *n == 0
}
