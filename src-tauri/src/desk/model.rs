//! The Desk's records: items, who holds them, and decisions. Times are in seconds (the window's Desk shows them so).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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
    pub project: String,
    /// "bug" | "feature" | "task"
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub done_when: String,
    /// "new" | "queued" | "working" | "to_check" | "done" as kept; the window's "needs_you" is worked out as it's
    /// shown (its holder waits on you).
    pub state: String,
    #[serde(default)]
    pub holder: Option<Holder>,
    /// The item it waits on (queued behind it), and why.
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub after_why: String,
    /// Its holder's latest, in one line: worked out as it's shown.
    #[serde(default)]
    pub latest: String,
    /// Its holder's open ask, when it waits on you: worked out as it's shown.
    #[serde(default)]
    pub ask: Option<Value>,
    /// The files its holder is changing, as last seen.
    #[serde(default)]
    pub files: Vec<String>,
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
    /// An `after` the lead asked for that Cue refused, and why (e.g. "#b, which waits on it"), so the item started
    /// now; until the lead is told.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after_refused: String,
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
}

/// Every project's Desk, as Cue holds it while it runs (saved as it changes: db/desk.rs).
#[derive(Default, Debug)]
pub struct Desk {
    pub items: Vec<DeskItem>,
    pub decisions: Vec<DeskDecision>,
    /// Each project's lead.
    pub lead: HashMap<String, Lead>,
    /// Decisions made since Cue started, for unique ids.
    pub next: u64,
    /// Projects whose lead is being started or resumed, with the messages that arrived while it was starting, sent
    /// once it's up.
    pub starting: HashMap<String, Vec<Waiting>>,
    /// Each project's new items its lead hasn't been told of yet: told in one message a moment after the first.
    pub fresh: HashMap<String, Vec<String>>,
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
    /// The project whose Desk session `sid` leads.
    pub fn lead_of(&self, sid: &str) -> Option<String> {
        self.lead.iter().find(|(_, l)| l.sid == sid).map(|(p, _)| p.clone())
    }

    /// `project`'s lead's session.
    pub fn lead_sid(&self, project: &str) -> Option<&str> {
        self.lead.get(project).map(|l| l.sid.as_str())
    }

    pub fn item(&self, id: &str) -> Option<&DeskItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut DeskItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// A new item's id.
    pub fn new_id(&self) -> String {
        super::new_id(|id| self.item(id).is_some())
    }

    /// A new decision's id: "k<ms>-<n>".
    pub fn decision_id(&mut self, now_ms: u64) -> String {
        self.next += 1;
        format!("k{now_ms}-{}", self.next)
    }
}
