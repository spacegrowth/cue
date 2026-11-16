//! The Desk: each project's items (a bug, a feature, a task), who holds each one, what it waits on, and the
//! decisions made about them. Each project's Desk is run by a lead: a session you pick (or Cue starts) that keeps
//! the list. It writes lines Cue reads at its turn's end (parse.rs), and Cue tells it where things stand before each
//! message (`summary`). The hub keeps the store and does the sending (hub/desk.rs, hub/desk_lead.rs); this module is
//! the data and the words.

pub mod model;
pub mod parse;

pub use model::{Desk, DeskDecision, DeskItem, Holder, Lead, Waiting};

/// How a project's Desk lead works: appended to the system prompt of a lead Cue starts (after the team skill), and
/// sent inside the Desk block to a session you make the lead (a running session can't get a system prompt).
pub const LEAD_SKILL: &str = "# Running this project's Desk (Cue)
You run this project's Desk in Cue: its list of items (bugs, features, tasks). The user tells you what needs doing; \
you do it or hand it out, and say why.
- Hand work out by default (to your crew or to another session of this project), so you stay free to answer the user. \
Do it yourself only when it's small, or when you are the only session.
- Say why in one line each time: who takes it and why them.
- Route new work to a small or warm session over a big cold one.
- Start a fresh teammate rather than messaging one that has finished its work.
- Write CONTEXT: only to differ from what Cue says it will do with a session's context.

Each message from Cue starts with a <desk> block: the items (#id, kind, title, state, who holds it, files) and the \
sessions you can hand work to, with each one's context and cache (\"142k, cache 12m\", or \"cold\"; \"→ Cue will hand \
off at expiry\": Cue asks it for a hand-off note before its cache runs out). The user's own words come after it.

Act by writing lines of their own, outside code blocks, exactly like this:
ITEM: bug | <title> | done when: <how we'll know it's done>      (kind is bug, feature or task)
ROUTE: #<id> -> <session or crew name> | why: <one short reason>
ROUTE: #<id> -> me | why: <reason>      (you do it yourself)
ROUTE: #<id> -> <name> | after: #<other id> | why: <reason>   (it starts once #<other id> is done; use it when \
both would change the same files)
ASK: <question> | choices: <A> / <B> / <C>      (only when you can't decide without the user)
DECIDED: <what you chose> — <why>      (a judgment call you made on your own)
DONE: #<id>      (an item you did yourself is done)
CONTEXT: <session or me> -> keep | why: <reason>      (Cue leaves that session's context as it is while it waits now, or, if it's at work, while it waits after this turn)

In a ROUTE, #new means the ITEM you added in the same reply. Route only to a name the <desk> block lists, or to me; \
never make one up. The session you route to is told the item and asked to say DONE: #<id> when it's done; Cue \
then moves it to the user to check. When Cue tells you of new items, route each one now.
Be short and plain: a line or two a busy person reads at a glance, no headings, no emoji.";

/// An id no item in `taken` has: a letter then three letters or digits ("d7k2").
pub fn new_id(taken: impl Fn(&str) -> bool) -> String {
    const ALL: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut seed = crate::clock::now_ms() ^ (std::process::id() as u64).rotate_left(32);
    loop {
        // A small mix of the clock (splitmix64): ids look random, and the next try differs.
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let mut id = String::with_capacity(4);
        id.push(ALL[(z % 26) as usize] as char);
        for i in 1..4 {
            id.push(ALL[((z >> (i * 8)) % 36) as usize] as char);
        }
        if !taken(&id) {
            return id;
        }
    }
}

/// What the session holding `it` is told when it's handed the item.
pub fn handed_words(it: &DeskItem, why: &str) -> String {
    let mut out = format!("Desk item #{} ({}): {}", it.id, it.kind, it.title);
    if !it.done_when.trim().is_empty() {
        out.push_str(&format!("\nDone when: {}", it.done_when.trim()));
    }
    if !why.trim().is_empty() {
        out.push_str(&format!("\nWhy you: {}", why.trim()));
    }
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
        out.push_str(&format!("New item #{} ({}): {}", it.id, it.kind, it.title));
        if !it.done_when.trim().is_empty() {
            out.push_str(&format!(" | done when: {}", it.done_when.trim()));
        }
        if it.from != "you" && !it.from.is_empty() {
            out.push_str(&format!(" | from {}", it.from));
        }
        out.push('\n');
    }
    out.push_str(if items.len() == 1 { "Route it now: ROUTE it to a session, or -> me." } else { "Route each now: ROUTE it to a session, or -> me." });
    out
}

/// A decision's undo, in the words its decider is sent (as crew_log's `go_back_words` for a lead's own call).
pub fn undo_words(d: &DeskDecision) -> String {
    let what = d.text.trim().trim_end_matches('.');
    let why = if d.why.trim().is_empty() { String::new() } else { format!(" (your reason: {})", d.why.trim().trim_end_matches('.')) };
    let about = d.item_id.as_deref().map(|id| format!(" on #{id}")).unwrap_or_default();
    format!("Go back on your call{about}: you decided {what}{why}. Undo it and take the other way, then carry on; say what that changes.")
}
