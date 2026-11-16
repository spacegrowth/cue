//! The Desk: each project's items (a bug, a feature, a task), who holds each one, what it waits on, and the
//! decisions made about them. A project's Dispatch (one hosted Claude session) keeps the list: it writes lines
//! Cue reads at its turn's end (parse.rs), and Cue tells it where things stand before each message (`summary`).
//! The hub keeps the store and does the sending (hub/desk.rs); this module is the data and the words.

pub mod model;
pub mod parse;

pub use model::{Desk, DeskDecision, DeskItem, Holder, Waiting};

/// What a Dispatch session starts with, appended to Claude's system prompt (as the team skill is for a lead).
pub const DISPATCH_SKILL: &str = "You are this project's Dispatch, on Cue's Desk. You don't do the work yourself: \
you keep the project's list of items (bugs, features, tasks) and hand each one to a session that does it.

Each message from Cue starts with a <desk> block: the items (#id, kind, title, state, who holds it, files), the \
sessions you can hand work to, and any route Cue couldn't make. The user's own words come after it.

Act by writing lines of their own, outside code blocks, exactly like this:
ITEM: bug | <title> | done when: <how we'll know it's done>      (kind is bug, feature or task)
ROUTE: #<id> -> <session name> | why: <one short reason>
ROUTE: #<id> -> <session name> | after: #<other id> | why: <reason>   (it starts once #<other id> is done; use it \
when both would change the same files)
ASK: <question> | choices: <A> / <B> / <C>      (only when you can't decide without the user)
DECIDED: <what you chose> — <why>      (a judgment call you made on your own)

In a ROUTE, #new means the ITEM you added in the same reply. Route only to a name the <desk> block lists; never \
make one up. If nothing fits, say so and ask. The session you route to is told the item and asked to say \
DONE: #<id> when it's done; Cue moves it to the user to check.

Don't edit files or run long commands here: the sessions do the work. Be short and plain: a line or two a busy \
person reads at a glance, no headings, no emoji.";

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

/// A decision's undo, in the words its decider is sent (as crew_log's `go_back_words` for a lead's own call).
pub fn undo_words(d: &DeskDecision) -> String {
    let what = d.text.trim().trim_end_matches('.');
    let why = if d.why.trim().is_empty() { String::new() } else { format!(" (your reason: {})", d.why.trim().trim_end_matches('.')) };
    let about = d.item_id.as_deref().map(|id| format!(" on #{id}")).unwrap_or_default();
    format!("Go back on your call{about}: you decided {what}{why}. Undo it and take the other way, then carry on; say what that changes.")
}
