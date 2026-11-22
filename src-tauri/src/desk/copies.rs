//! An item's copies of its repository (desk/hub/landing): its own copy here, its work on another machine, the copy
//! the lead asked for, and a dropped item's kept copy.

use super::DeskItem;
use serde::{Deserialize, Serialize};

/// An item's own copy of its repository (desk/hub/landing/trees.rs): the main checkout it's from, the copy's folder, its
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
    /// A branch the lead named for it (COPY, #557), in a repository of its choosing (the add-on's, say): its work is
    /// committed on that branch and stays there. Never landed on a main line, and the branch is never removed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep: bool,
    /// A copy of Cue's add-on (#565): the main checkout of the Cue it's built into. At its DONE its branch is merged
    /// into the add-on's main line and that Cue installed with it, as Cue's own items land.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub addon_of: String,
    /// On a branch of its own in the project's own folder ("A branch", desk/hub/landing/in_place.rs): `path` is the
    /// main checkout itself, never removed or moved; at the end it's switched back onto the main line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_place: bool,
    /// In place: the main line the folder was switched from, as its holder is told it (`tree_line`), so the words
    /// never ask git while the Desk is held. Saved; empty in a tree saved before it was kept.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub main_line: String,
    /// In place: the commit its work landed as on the main line while the folder couldn't go back onto it yet, and the
    /// folder's files as landed (a tree). It finishes once the folder is back (desk/hub/landing/trees.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub landed: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub landed_work: String,
    /// A copy of this Cue's own source, noted as the copy is made (desk/hub/landing/trees.rs `desk_tree_ready`): its
    /// holder is told to run the tests for what it changed. Saved, so the words never read a file while the Desk is held.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub card_tests: bool,
}

impl ItemTree {
    /// What its holder is told of testing, after the words on its copy: in this Cue's own source, to run the tests
    /// for what it changed (tests/run.sh card) and leave the full run to after landing; nothing in any other project.
    pub(crate) fn card_tests_line(&self) -> &'static str {
        if self.card_tests {
            " Before you say done, run tests/run.sh card: it runs the tests for what you changed. Don't run the full suite: it runs after landing."
        } else {
            ""
        }
    }
}

/// An item's work on another machine (#500, docs/desk-remote-items-plan.md).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteWork {
    /// The machine (its name in Settings → Machines) and the folder of the clone there.
    pub machine: String,
    pub folder: String,
    /// Its branch there ("desk/<id>"), the remote it's pushed to, and the main line it starts from and lands on.
    pub branch: String,
    pub remote: String,
    pub onto: String,
    /// Its last commit as Cue last pushed it, and when (ms; 0: not yet).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha: String,
    #[serde(default)]
    pub pushed_at: u64,
    /// Why its DONE waits ("Machine not answering", "Waiting for its branch"), and when Cue last tried (ms).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wait: String,
    #[serde(default)]
    pub tried_at: u64,
    /// Since when its machine hasn't answered (ms): worked out as it's shown, never saved.
    #[serde(default)]
    pub quiet_since: u64,
    /// The lead was told its machine is quiet (once per quiet spell). Saved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet_told: bool,
    /// Tries in a row its DONE stopped on something that won't mend by itself (desk/hub/landing/remote_item.rs).
    #[serde(default)]
    pub fails: u32,
    /// What its holder's last DONE named (#596): the branch and the last commit Cue lands ("": not named).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said_branch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said_commit: String,
}

/// Where a new session for an item starts when not on this Mac: the machine and the folder of the clone there.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OnMachine {
    pub machine: String,
    pub folder: String,
}

/// The copy the lead asked for an item (`COPY: #<id> | in: <repo> | branch: <name>`, #557): the main checkout of the
/// repository it's cut from, and the branch it works on (desk/hub/landing/named_copy.rs).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CopyOn {
    pub repo: String,
    pub branch: String,
    /// The repository is Cue's add-on (`in: add-on`, #565): the main checkout of the Cue it's built into. Its work
    /// then lands on the add-on's main line and that Cue is installed with it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cue: String,
}

/// The repository the lead routed an item to (`ROUTE: #<id> -> new | repo: <folder or project>`, #635): its main
/// checkout, where a new session for it starts and its copy is made, and the project Cue knows that repository as,
/// whose landing settings and never-commit words its landing there follows (desk/hub/landing/other_repo.rs).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct InRepo {
    pub repo: String,
    pub project: String,
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
    /// The main line's commit it started from ("" before #608: read from its branch then), so what it changed can be shown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base: String,
    /// The item as it was when dropped (#608): its page opens on it, with its holder's chat, and Pick up again brings
    /// it back. None for one kept before then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<Box<DeskItem>>,
}
