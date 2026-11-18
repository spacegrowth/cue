//! The Desk: each project's items (a bug, a feature, a task), who holds each one, what it waits on, and the
//! decisions made about them. Each project's Desk is run by a lead: a session you pick (or Cue starts) that keeps
//! the list. It writes lines Cue reads at its turn's end (parse.rs), and Cue tells it where things stand before each
//! message (`summary`). The hub keeps the store and does the sending (hub/desk.rs, hub/desk_lead.rs); this module is
//! the data and the words.

pub mod model;
pub mod parse;

pub use model::{BusyFile, CommitWait, Desk, DeskDecision, DeskItem, Finish, Holder, InstallStatus, Installer, Lead, Waiting};

/// How a project's Desk lead works: appended to the system prompt of a lead Cue starts (after the team skill), and
/// sent inside the Desk block to a session you make the lead (a running session can't get a system prompt).
pub const LEAD_SKILL: &str = "# Running this project's Desk (Cue)
You run this project's Desk in Cue: its list of items (bugs, features, tasks). The user tells you what needs doing; \
you hand it out to the project's sessions, and say why. You only dispatch, so you stay free to answer the user:
- Never do an item's work yourself, start teammates or subagents, or run anything in the background.
- Route each item to a session of this project; when none fits, to a new one (-> new), which Cue starts.
- Say why in one line each time: who takes it and why them.
- When a session is about to hand off, prefer another suitable session for new work.
- The session that did an item reviews its own work; you don't.
- Write CONTEXT: only to differ from what Cue says it will do with a session's context.

Each message from Cue starts with a <desk> block. It shows who runs the Desk, what needs routing, other open items, \
and the sessions that can take work. A pending hand-off shows the session's context size and cache time. \
An item marked needs resume is still assigned and its holder is idle. A queued item with an after reference waits for that dependency. \
The user's own words come after the block.

Act by writing lines of their own, outside code blocks, exactly like this:
ITEM: bug | <title> | done when: <how we'll know it's done>      (kind is bug, feature or task)
TITLE: #<id> | <short plain title>      (an item whose title isn't short and plain, as the user's often aren't: \
a few words saying what they'll see; Cue keeps their words on the item)
SPLIT: #<id> | <title> | <title>      (an item that holds two or more separate problems, one plain title each: the \
first stays #<id>, Cue numbers the rest and tells you them; route them all then, not before)
ROUTE: #<id> -> <session name> | kind: bug | why: <one short reason>      (kind is bug, feature or task; \
give it when the item has none yet, as items the user adds don't)
ROUTE: #<id> -> new | why: <reason>      (no session fits: Cue starts a new session of this project for it)
ROUTE: #<id> -> <name> | after: #<other id> | why: <reason>   (it starts once #<other id> is done; use it whenever \
an item really waits on another, as when both would change the same files: the Desk shows the user what each waiting item unblocks)
ASK: <question> | choices: <A> / <B> / <C>      (only when you can't decide without the user)
ASK: #<id> | <question> | choices: <A> / <B>      (the same, about one item: it shows on that item as waiting on the user)
DECIDED: <what you chose> — <why>      (a judgment call you made on your own that the user may want to undo; not for routing, starting or giving work: Cue lists those itself)
DONE: #<id>      (the user tells you an item is done)
DESIGNS: #<id> <canvas link>      (where an item's designs are; say it again when they move)
CONTEXT: <session or me> -> keep | why: <reason>      (Cue leaves that session's context as it is while it waits now, or, if it's at work, while it waits after this turn)

In a ROUTE, #new means the ITEM you added in the same reply. Route to a session on the Desk (the <desk> block lists them), or to new; \
never make one up. A ROUTE to another session of the project makes Cue ask the user first (don't ask it yourself): a yes \
adds that session to the Desk and makes the route, a no leaves the item as it was. The session you route to is told the item and asked to say DONE: #<id> when it's done; Cue \
then moves it to the user to check. To reopen one that's to check, route it again to the session holding it, \
with what's left as the why. When Cue tells you of new items, route each one now; first TITLE or SPLIT one that needs it.
When Cue tells you an item stopped (its session ended its turn with no DONE and no question), settle it: ROUTE it \
to that session again with what to do next as the why, or route it on. ASK: #<id> only if the user must decide; \
until then the user doesn't see it as waiting on them.
In your own words, name an item by its kind and number, as the user sees it: \"Bug #2\", \"Task #1\" (in the lines above, \
keep #<id>). Be short and plain: a line or two a busy person reads at a glance, no headings, no emoji.";

/// The number after the highest of `ids` that are plain numbers, and after `high` (the highest given before).
pub fn next_number<'a>(ids: impl Iterator<Item = &'a str>, high: u64) -> u64 {
    ids.filter_map(|id| id.parse::<u64>().ok()).fold(high, u64::max) + 1
}

/// What the session holding `it` is told when it's handed the item.
pub fn handed_words(it: &DeskItem, why: &str) -> String {
    let mut out = format!("Desk item #{}{}: {}", it.id, kind_words(it), it.title);
    if !it.done_when.trim().is_empty() {
        out.push_str(&format!("\nDone when: {}", it.done_when.trim()));
    }
    if !it.images.is_empty() {
        out.push_str(&format!("\nPictures with it (open each to see it): {}", pictures(it)));
    }
    if !why.trim().is_empty() {
        out.push_str(&format!("\nWhy you: {}", why.trim()));
    }
    out.push_str(&format!(
        "\nFixes and next steps for it stay in #{0}: do them, or say them in your reply; don't add new items for them. \
         Add one only for something truly separate, with why in one line: \
         ITEM: <bug|feature|task> | <title> | done when: <…> | why: <how it's separate from #{0}>",
        it.id
    ));
    out.push_str(&format!("\nIf you put its designs on a canvas, or move them, say DESIGNS: #{} <canvas link> on a line of its own.", it.id));
    out.push_str(&format!("\nSay DONE: #{} on a line of its own when it's done.", it.id));
    match &it.holder {
        Some(h) if h.kind == "crew" => format!("For your teammate {}: {out}", h.name),
        _ => out,
    }
}

/// What a lead is told of new items it hasn't routed yet (added by you, or by a session's ITEM: line).
pub fn new_items_words(items: &[DeskItem]) -> String {
    let mut out = String::new();
    for it in items {
        out.push_str(&format!("New item #{}{}: {}", it.id, kind_words(it), it.title));
        if !it.done_when.trim().is_empty() {
            out.push_str(&format!(" | done when: {}", it.done_when.trim()));
        }
        if !it.images.is_empty() {
            out.push_str(&format!(" | pictures: {}", pictures(it)));
        }
        if it.from != "you" && !it.from.is_empty() {
            out.push_str(&format!(" | from {}", it.from));
        }
        if !it.why.trim().is_empty() {
            out.push_str(&format!(" | why separate: {}", it.why.trim()));
        }
        if !it.split_from.is_empty() {
            out.push_str(&format!(" | split from #{}", it.split_from));
        }
        out.push('\n');
    }
    if items.iter().any(|i| i.from == "you" && i.words.is_empty() && i.split_from.is_empty()) {
        out.push_str("If one of yours holds two separate problems, SPLIT it first (Cue tells you the new numbers); if its title isn't short and plain, TITLE it. ");
    }
    out.push_str(if items.len() == 1 { "Route it now: ROUTE it to a session, or -> new." } else { "Route each now: ROUTE it to a session, or -> new." });
    if items.iter().any(|i| i.kind.is_empty()) {
        out.push_str(" Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.");
    }
    out
}

/// An item's pictures, each path in backticks (the window shows a path written so as the picture).
fn pictures(it: &DeskItem) -> String {
    it.images.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(" ")
}

/// " (bug)" for an item's kind, "" for one with none yet.
pub fn kind_words(it: &DeskItem) -> String {
    if it.kind.is_empty() { String::new() } else { format!(" ({})", it.kind) }
}

/// A decision's undo, in the words its decider is sent (as crew_log's `go_back_words` for a lead's own call).
pub fn undo_words(d: &DeskDecision) -> String {
    let what = d.text.trim().trim_end_matches('.');
    let why = if d.why.trim().is_empty() { String::new() } else { format!(" (your reason: {})", d.why.trim().trim_end_matches('.')) };
    let about = d.item_id.as_deref().map(|id| format!(" on #{id}")).unwrap_or_default();
    format!("Go back on your call{about}: you decided {what}{why}. Undo it and take the other way, then carry on; say what that changes.")
}
