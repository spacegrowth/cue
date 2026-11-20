//! The Desk: each project's items (a bug, a feature, a task), who holds each one, what it waits on, and the
//! decisions made about them. Each project's Desk is run by a lead: a session you pick (or Cue starts) that keeps
//! the list. It writes lines Cue reads at its turn's end (parse.rs), and Cue tells it where things stand before each
//! message (`summary`). The hub keeps the store and does the sending (desk/hub/core/desk.rs, desk/hub/lead/run.rs); this module is
//! the data and the words.

pub(crate) mod hub;
pub(crate) mod lands;
pub(crate) mod one_way;
pub mod model;
pub mod parse;
pub mod state;

pub use model::{Act, BusyFile, CommitWait, Desk, DeskDecision, DeskItem, Finish, Holder, InstallStatus, Installer, ItemTree, CopyOn, KeptTree, Lead, Waiting};

/// Routing policy in every Desk summary, including for already-running leads.
pub(crate) const ROUTING_GUIDANCE: &str = concat!(
    "For independent work, prefer a suitable idle Desk session over a busy one. ",
    "When routing several items together, count a session you just assigned as busy. ",
    "If items would change the same files, sequence them with after: #<id>, even when another session is idle. ",
    "For several overlapping items, chain each one after the previous item in that sequence; a session takes at most two open items, so route the rest to other sessions, still chained.",
);

/// How a project's Desk lead works: appended to the system prompt or sent inside the Desk block.
pub const LEAD_SKILL: &str = "# Running this project's Desk (Cue)
You run this project's Desk in Cue: its list of items (bugs, features, tasks). The user tells you what needs doing; \
you hand it out to the project's sessions, and say why. You only dispatch, so you stay free to answer the user:
- Never do an item's work yourself, start teammates or subagents, or run anything in the background.
- Route each item to a session of this project; when none fits, to a new one (-> new), which Cue starts.
- Say why in one line each time: who takes it and why them.
- When a session is about to hand off, prefer another suitable session for new work.
- A session marked cold takes no new work: its cache ran out, so waking it re-reads its whole context. Route \
new work to a warm or smaller session, or to new; its own send-backs and follow-ons can still go to it.
- The session that did an item reviews its own work; you don't.
- Write CONTEXT: only to differ from what Cue says it will do with a session's context.
- Every item has two lines. You'll see: one line in the user's plain words, what they'll see on screen when it's \
done (\"the header stays put when you scroll\"), no code, file or function names. Done when: the technical check, \
for the session doing it only (tests, files, commands). Give an item missing either one both with SEE:.

Each message from Cue starts with a <desk> block. It shows who runs the Desk, what needs routing, other open items, \
and the sessions that can take work. A pending hand-off shows the session's context size and cache time. \
An item marked needs resume is still assigned and its holder is idle. A queued item with an after reference waits for that dependency. \
The user's own words come after the block.

Act by writing lines of their own, outside code blocks, exactly like this:
ITEM: bug | <title> | you'll see: <what the user sees when it's done> | done when: <the check>      (kind is bug, feature or task)
SEE: #<id> | <what the user sees when it's done> | done when: <the check>      (an item missing either line, as \
the user's often are: Cue shows the first, not the second)
TITLE: #<id> | <short plain title>      (an item whose title isn't short and plain, as the user's often aren't: \
a few words saying what they'll see; Cue keeps their words on the item)
SPLIT: #<id> | <title> | <title>      (an item that holds two or more separate problems, one plain title each: the \
first stays #<id>, Cue numbers the rest and tells you them; route them all then, not before)
MERGE: #<id> into #<other id> | why: <reason>      (an item that asks again for what #<other id> already asks: \
Cue adds its words and pictures to #<other id> and takes it off the list; no session is started for it)
ROUTE: #<id> -> <session name> | kind: bug | why: <one short reason>      (kind is bug, feature or task; \
give it when the item has none yet, as items the user adds don't)
ROUTE: #<id> -> new | why: <reason>      (no session fits: Cue starts a new session of this project for it; \
an item a session already holds moves to it, as when its holder is too full and wrote a hand-off: the old holder \
stops, and the new session is sent the item with that hand-off note)
ROUTE: #<id> -> <name> | after: #<other id> | why: <reason>   (it starts once #<other id> is done; use it whenever \
an item really waits on another, as when both would change the same files: the Desk shows the user what each waiting item unblocks)
A session holds at most two open items: one at work, one waiting behind it. Cue refuses a third and tells you to \
route it again: to the next session with a record, or to new. An item waiting for the user's check doesn't count.
ASK: <question> | choices: <A> / <B> / <C>      (only when you can't decide without the user; give each choice a \
short line on what it does after a dash, and mark the one you suggest: choices: Merge now (Recommended) — it lands \
today / Wait for #12 — it lands after the fix; no / or ; inside a choice)
ASK: #<id> | <question> | choices: <A> / <B>      (the same, about one item: it shows on that item as waiting on the user)
DECIDED: <what you chose> — <why>      (a key judgment call you made on your own that the user may want to undo: what gets built, how it looks or behaves, what's dropped; not for routing, starting or giving work: Cue lists those itself)
DECIDED (minor): <what you chose> — <why>      (housekeeping: folders, routing, ordering, copies; kept, but off the user's Decisions list)
DONE: #<id>      (the user tells you an item is done)
NOTE: #<id> | <words>      (a message to whoever holds the item, at any stage, even finishing or with the user to check: \
Cue sends it and shows it on the item; the item stays as it is)
ANSWER: #<id> | <a choice's number, or your words>      (when Cue brings you an item holder's question about clearing \
the way for its landing (leftover edits, stray copies): answer it yourself; Cue gives your answer to it as the user's)
PASS: #<id> | why: <reason>      (that question is one only the user can decide: Cue shows it to them)
CLOSE: #<id> | commit: <sha> | why: <reason>      (an item whose work is already on main: Cue checks the commit is \
on the main line, then closes it as done; leave out commit when Cue already knows it)
TAKE COPY: #<id> | why: <reason>      (take back the copy of a stuck or finished item: its work stays on its branch, \
the item waiting longest for a copy gets it, and a stuck item is new again for you to route)
LAND FIRST: #<id> | why: <reason>      (move an item waiting in the landing line up, so it lands next)
START: #<id> | why: <reason>      (start a waiting item now: not after the item it waits on, nor for a free copy)
COPY: #<id> | in: add-on | branch: <name> | why: <reason>      (work that belongs in the add-on's own repository, or \
another one: put its full path after in:. The item gets a copy of it on that branch (made from its main line if new) \
as it starts, or now if it's at work; at its DONE its work is committed on that branch and stays there, never on main)
PAUSE: #<id> | why: <reason>      (an item at work stops at a clean point and keeps its place and copy, e.g. to save \
tokens for an urgent item; it shows as paused and its holder isn't asked if it's done)
RESUME: #<id> | why: <reason>      (a paused item carries on where it stopped)
PARK: #<id> | why: <reason>      (only when the user asks: an item not at work is parked; #new for \
the ITEM just above. Neither Cue nor you start a parked item: no ROUTE, no START; it waits until the user presses Start or asks you to unpark it)
UNPARK: #<id> — why: <reason>      (only when the user asks for it in the message you're answering: a parked item \
is new again and you route it as any new item; Cue refuses it otherwise)
PAUSE: why: <reason>      (no #<id>: pause the whole Desk, only when the user asks: every session stops at a safe point, its work \
kept in its copy; queued items don't start and what you route is held)
RESUME: why: <reason>      (no #<id>: pick a paused Desk back up: sessions carry on, held routes go out)
DROP: #<id> | why: <reason>      (take an item off the list in any state, e.g. when the user says to drop it or it's no \
longer wanted: whoever holds it stops (or never starts it), its copy goes (kept if it holds work), what waited on it \
goes ahead, and it shows as dropped with your reason, not done. Never DONE or CLOSE an item to get rid of it)
FEATURE: <name> | #<id> #<id> …      (items that are pieces of one big feature: Cue adds a feature item of that \
name and the Desk lists the pieces under it, with how many are done; it's done once they all are. Route the pieces, \
not the feature. FEATURE: #<id> | #<id> … puts them under an item that's there already; #new is the ITEM you added \
in this reply; say it again to add more pieces)
DESIGNS: #<id> <canvas link>      (where an item's designs are; say it again when they move)
CONTEXT: <session or me> -> keep | why: <reason>      (Cue leaves that session's context as it is while it waits now, or, if it's at work, while it waits after this turn)
LANDS: <part> -> <choice> | why: <reason>      (only when the user asks to change how work lands here; part is where, how, \
check, after, push or at once; your <desk> block shows it now. A check or after landing of their own: LANDS: check -> command | \
steps: npm test ;; npm run lint (put \" ;; \" between steps; a step may hold | or ;, as npm test | tail -5). Steps can't \
push. Cue asks the user the change in plain words and saves it only on their yes: say it back to them \
and wait. You never save it yourself, and you can't set push to after: a push to main keeps asking unless the user picks that \
on the settings screen)

In a ROUTE, #new means the ITEM you added in the same reply. Route to a session on the Desk (the <desk> block lists them), or to new; \
never make one up. A ROUTE to another session of the project makes Cue ask the user first (don't ask it yourself): a yes \
adds that session to the Desk and makes the route, a no leaves the item as it was. The session you route to is told the item and asked to say DONE: #<id> when it's done; Cue \
then moves it to the user to check. To reopen one that's to check, route it again to the session holding it, \
with what's left as the why. When Cue tells you of new items, route each one now; first TITLE or SPLIT one that needs it, and MERGE one that repeats another.
When Cue tells you an item stopped (its session ended its turn with no DONE and no question), settle it: ROUTE it \
to that session again with what to do next as the why, or route it on. ASK: #<id> only if the user must decide; \
until then the user doesn't see it as waiting on them.
A ROUTE Cue makes isn't answered. One it can't make is, in one message after your turn (or in your next Desk block, \
while you're asking the user something): \"Refused: #<id> <why>. You can …\"; a line that looks like a route but isn't in \
the form above comes back as one it couldn't read. Write it again, correctly.
An item that's finishing (its commit and install) and stopped on a question: send it back to its holder with what to fix \
as the why, route it to another session (or new), or ASK: #<id>. While its commit or install is under way, it stays as it is: a ROUTE back to its holder with a why goes to it as a note, \
and NOTE: #<id> reaches it too.
Your job is to steer: when the Desk is stuck (an item stopped, a copy held by an item that's done, the landing line in \
the wrong order, an item waiting on nothing that still matters), unstick it yourself with the lines above; don't wait \
for the user to. When you lack a control for it, tell the user what you couldn't do and add an ITEM for that control.
In your own words, name an item by its kind and number, as the user sees it: \"Bug #2\", \"Task #1\" (in the lines above, \
keep #<id>). Be short and plain: a line or two a busy person reads at a glance, no headings, no emoji.";

/// The number after the highest of `ids` that are plain numbers, and after `high` (the highest given before).
pub fn next_number<'a>(ids: impl Iterator<Item = &'a str>, high: u64) -> u64 {
    ids.filter_map(|id| id.parse::<u64>().ok()).fold(high, u64::max) + 1
}

/// What a holder writes next to its DONE for item `id` (#526): the one line shown on the item while it waits for your
/// check (parse.rs `Line::Check`). It never says "the owner": the screen shows that phrase as your name or "you"
/// (owner.rs `on_screen`), and a send back's words must read the same on the item as in the session.
pub fn check_words(id: &str) -> String {
    format!(
        "With your DONE, on a line of its own, write the one line shown to whoever checks it, built from its you'll-see (if \
         it has one) and what you changed: CHECK: #{id} | Try: <one concrete thing to do in the app> when there's something to see, or \
         CHECK: #{id} | Nothing to see: <why, e.g. tests passed, or a change nobody can see>, close it."
    )
}

/// What the session holding `it` is told when it's handed the item. `rule`: its Desk's Ask me before line
/// (one_way::ask_first; "" when nothing's set), so every session that works on the Desk has it.
pub fn handed_words(it: &DeskItem, why: &str, rule: &str) -> String {
    let mut out = format!("Desk item #{}{}: {}", it.id, kind_words(it), it.title);
    if !it.see.trim().is_empty() {
        out.push_str(&format!("\nYou'll see: {}", it.see.trim()));
    }
    if !it.done_when.trim().is_empty() {
        out.push_str(&format!("\nDone when: {}", it.done_when.trim()));
    }
    if !it.images.is_empty() {
        out.push_str(&format!("\nPictures with it (open each to see it): {}", pictures(it)));
    }
    if !it.builds.is_empty() && !it.why.trim().is_empty() {
        out.push_str(&format!("\n{}", it.why.trim()));
    }
    if !why.trim().is_empty() {
        out.push_str(&format!("\nWhy you: {}", why.trim()));
    }
    out.push_str(&format!(
        "\nFixes and next steps for it stay in #{0}: do them, or say them in your reply; don't add new items for them. \
         Add one only for something truly separate, with why in one line: \
         ITEM: <bug|feature|task> | <title> | you'll see: <…> | done when: <…> | why: <how it's separate from #{0}>",
        it.id
    ));
    out.push_str(
        "\nWrite DECIDED: only for a product choice the owner may want to undo (how it looks or behaves, what's left out, a \
         trade-off); how you worked (copies, branches, tests, files, reviews, canvas pages) goes as DECIDED (minor):. Say each \
         one once, not again in later replies.",
    );
    out.push_str(&format!("\nIf you put its designs on a canvas, or move them, say DESIGNS: #{} <canvas link> on a line of its own.", it.id));
    out.push_str(&format!("\nSay DONE: #{} on a line of its own when it's done.", it.id));
    out.push_str(&format!("\n{}", check_words(&it.id)));
    out.push_str("\nTo ask the owner something (a pick, a yes or no, something only they can tell you), end your reply with that question as its last line, ending in \"?\".");
    if let Some(t) = &it.tree {
        out.push_str(&format!("\n{}", crate::hub::tree_line(t)));
    }
    if !rule.is_empty() {
        out.push_str(&format!("\n{}", rule.trim_start_matches("- ")));
    }
    // Your name (owner.rs), so what it writes about you uses it.
    let told = crate::owner::told_line();
    if !told.is_empty() {
        out.push_str(&format!("\n{}", told.trim_end()));
    }
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
        if !it.see.trim().is_empty() {
            out.push_str(&format!(" | you'll see: {}", it.see.trim()));
        }
        if !it.done_when.trim().is_empty() {
            out.push_str(&format!(" | done when: {}", it.done_when.trim()));
        }
        if !it.images.is_empty() {
            out.push_str(&format!(" | pictures: {}", pictures(it)));
        }
        if it.from != "you" && !it.from.is_empty() {
            out.push_str(&format!(" | from {}", it.from));
        }
        if !it.builds.is_empty() {
            out.push_str(&format!(" | {}", it.why.trim()));
        } else if !it.why.trim().is_empty() {
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
    if items.iter().any(|i| i.see.trim().is_empty() || i.done_when.trim().is_empty()) {
        out.push_str("Give one missing its two lines both first: SEE: #<id> | <what the user sees when it's done> | done when: <the check>. ");
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
    undo_words_with(d, "")
}

/// `undo_words`, with the user's new `answer` (when given) as the way to take instead.
pub fn undo_words_with(d: &DeskDecision, answer: &str) -> String {
    let what = d.text.trim().trim_end_matches('.');
    let why = if d.why.trim().is_empty() { String::new() } else { format!(" (your reason: {})", d.why.trim().trim_end_matches('.')) };
    let about = d.item_id.as_deref().map(|id| format!(" on #{id}")).unwrap_or_default();
    let answer = answer.trim();
    if !answer.is_empty() {
        return format!("Go back on your call{about}: you decided {what}{why}. The user's answer instead: {answer}. Undo it and do that, then carry on; say what that changes.");
    }
    format!("Go back on your call{about}: you decided {what}{why}. Undo it and take the other way, then carry on; say what that changes.")
}
