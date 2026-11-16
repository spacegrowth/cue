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
    /// The random tag it had before items were numbered ("d7k2"), so old lines and links that name it still find it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alias: String,
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
    /// The lead's teammate doing it, when the lead keeps it and says so (`ROUTE: #id -> me | crew: <name>`): the name
    /// the lead gave it, matched to its teammate of that name once there is one (hub/desk.rs `shown`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crew: String,
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
    /// The commit Looks right made of its files (short; one per repository, space apart), once it did
    /// (hub/desk_commit.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// Looks right's commit while it waits: some of its files were being edited by another session at work, so the
    /// item is done and Cue commits them on its own once they're free (hub/desk_commit.rs `desk_commit_waiting`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_wait: Option<CommitWait>,
    /// Where each of its commits is: (folder, short hash), for its Files changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_in: Vec<(String, String)>,
    /// Who it went to at work and when (ms): only work begun after that is its (stamped as it's saved: hub/desk.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_from: Option<(String, u64)>,
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
    /// A ROUTE to reopen it (with you to check) that Cue didn't act on, and why; until the lead is told.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reopen_refused: String,
    /// Each time its holder said DONE for it: kept, so the lead's chat shows a card for each, after a restart too
    /// (hub/desk_checks.rs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<DeskCheck>,
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
    /// why (hub/desk.rs lead_done): a lead saying it again of an unchanged item isn't answered again. Not saved.
    pub done_told: HashMap<String, String>,
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
