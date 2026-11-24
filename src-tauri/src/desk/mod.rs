//! The Desk: each project's items (a bug, a feature, a task), who holds each one, what it waits on, and the
//! decisions made about them. Each project's Desk is run by a lead: a session you pick (or Cue starts) that keeps
//! the list. It writes lines Cue reads at its turn's end (parse.rs), and Cue tells it where things stand before each
//! message (`summary`). The hub keeps the store and does the sending (desk/hub/core/desk.rs, desk/hub/lead/run.rs); this module is
//! the data and the words.

pub(crate) mod across;
pub mod escalate;
pub mod feed;
pub(crate) mod hold_check;
pub mod holds;
pub(crate) mod hub;
pub(crate) mod lands;
pub(crate) mod model_pick;
pub(crate) mod machine_pref;
pub(crate) mod one_way;
pub mod model;
pub mod copies;
pub mod installer;
pub mod tell;
pub(crate) mod move_desk;
pub mod parse;
pub mod state;

pub use installer::{InstallStatus, Installer};
pub use model::{Act, BusyFile, CommitWait, Desk, DeskDecision, DeskItem, Finish, Holder, ItemTree, CopyOn, InRepo, KeptTree, OnMachine, RemoteWork, Lead, StartFull, Stint, Waiting};

/// `text` without the `<desk …>…</desk>` block a message to a lead starts with (lead/send.rs `desk_message`): the words
/// you sent it. `text` itself when it has none.
pub fn without_block(text: &str) -> &str {
    let t = text.trim_start();
    if !(t.starts_with("<desk ") || t.starts_with("<desk>")) {
        return text;
    }
    match t.find("\n</desk>") {
        Some(end) => t[end + "\n</desk>".len()..].trim_start(),
        None => text,
    }
}

/// `text` holds a `<desk …>…</desk>` block (one message to a lead, or several kept together, #627).
pub fn has_block(text: &str) -> bool {
    blocks(text).next().is_some()
}

/// Where each `<desk …>…</desk>` block in `text` is (from, to, the whitespace after it included): one at its start,
/// or after a blank line (where messages kept together join).
fn blocks(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut from = 0;
    std::iter::from_fn(move || {
        while from < text.len() {
            let at = text[from..].find("<desk").map(|i| from + i)?;
            let starts = text[..at].trim_end_matches([' ', '\t']).is_empty() || text[..at].ends_with("\n\n");
            let opens = text[at + 5..].starts_with([' ', '>']);
            match text[at..].find("\n</desk>").filter(|_| starts && opens) {
                Some(end) => {
                    let close = at + end + "\n</desk>".len();
                    let after = close + (text[close..].len() - text[close..].trim_start().len());
                    from = after;
                    return Some((at, after));
                }
                None => from = at + 5,
            }
        }
        None
    })
}

/// `text` with every `<desk …>…</desk>` block taken out and `block` (a summary made now) put first, once.
pub fn with_fresh_block(text: &str, block: &str) -> String {
    let mut rest = String::new();
    let mut last = 0;
    for (from, to) in blocks(text).collect::<Vec<_>>() {
        rest.push_str(&text[last..from]);
        last = to;
    }
    rest.push_str(&text[last..]);
    format!("{block}\n\n{}", rest.trim())
}

/// Routing policy in every Desk summary, including for already-running leads.
pub(crate) const ROUTING_GUIDANCE: &str = concat!(
    "For independent work, prefer a suitable idle Desk session over a busy one. ",
    "When routing several items together, count a session you just assigned as busy. ",
    "If items would change the same files, sequence them with after: #<id>, even when another session is idle. ",
    "For several overlapping items, chain each one after the previous item in that sequence; a session holds one item at a time, so items routed to a busy session wait in the queue for it.",
);

/// The LOOP: line in the lead's skill (#583): only in a build with the add-on, which draws its card and runs loops.
#[cfg(feature = "ext")]
macro_rules! lead_loop_line {
    () => {
        "LOOP: <project> | name: <2 to 4 words> | goal: <the full instructions for the AI that runs it> | done when: <plain words> | \
check: <a shell command that passes when the work is right, or none> | rounds: <1 to 10> | time: <minutes> | spend: <dollars> | \
decisions: one-way, every or none | folder: <its full path, when you know it>      (only when the user asks for a loop, for this project or another of theirs: \
Cue finds that project's folder, by its name or its repository's, and shows the draft in your chat with a Start loop button; it starts only when the \
user presses it, never by you. Give goal and done when always, a check and limits whenever you can)\n"
    };
}
#[cfg(not(feature = "ext"))]
macro_rules! lead_loop_line {
    () => {
        ""
    };
}

/// How a project's Desk lead works: appended to the system prompt or sent inside the Desk block.
pub const LEAD_SKILL: &str = concat!("# Running this project's Desk (Cue)
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
- Call a piece of work an item, as the Desk shows it, in titles, you'll-see lines and what you tell the user: never a card \
(\"two items on one machine\", not \"two cards\"). Cue never shows the word card: a question the user answers is a question.
- An item that won't change the repo (mocks, a doc, a plan, work on another service): its done when \
says \"nothing in the repo changes\". It takes no copy and goes to the user to check with no commit.

Each message from Cue starts with a <desk> block. It shows who runs the Desk, what needs routing, other open items, \
and the sessions that can take work. A pending hand-off shows the session's context size and cache time. \
An item marked needs resume is still assigned and its holder is idle. A queued item with an after reference waits for that dependency. \
The user's own words come after the block.

Act by writing lines of their own, outside code blocks, exactly like this:
ITEM: bug | <title> | you'll see: <what the user sees when it's done> | done when: <the check>      (kind is bug, feature or task)
ITEM: feature | <title> | you'll see: … | done when: … | from: #<id> | picked: <option>      (an item that carries \
on from #<id>, as when the user picks an option in your chat and you close #<id> and file what builds it: #<id> then \
says which item builds their pick, and the new one links back. Leave picked out for a follow-on that isn't a pick)
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
ROUTE: #<id> -> new | model: <name> | why: <reason>   (the new session starts on that model, as Claude Code names it: fable, opus, sonnet, haiku, or a full id; without model: it starts on the Desk's default, which the user sets in the Desk's settings. Pick a model only when the work calls for it)
ROUTE: #<id> -> new | on: <machine> | in: <folder there> | why: <reason>   (the new session starts on another of the user's machines, in this repository's clone there, from the latest main; on: this Mac starts it here. Without on:, it starts where the user chose for the item, else where the Desk's sessions start (its settings). Cue remembers the folder, so in: is needed only the first time. Before a Desk's first item on a machine, ask the user (ASK: #<id> | Start it on <machine>, in <folder>? | Yes | No, this Mac), and route it there only once they say yes. An item given to a session on another machine, this way or by name, is worked on a branch of its own there and lands here at its DONE like any other)
ROUTE: #<id> -> new | repo: <repository folder, or a project Cue knows> | why: <reason>   (the item's code lives in another repository on this Mac: the new session starts in that repository, the item works in a copy of it and lands on that repository's main line at its DONE, by that repository's own landing settings. With a session's name instead of new, that session gets a copy of that repository to work in. Use COPY for a branch that stays off the main line)
ROUTE: #<id> -> <name> | after: #<other id> | why: <reason>   (it starts once #<other id> has landed, or once the user checked it, by the Desk's setting (\"waiting items start\" in How work lands); use it whenever \
an item really waits on another, as when both would change the same files: the Desk shows the user what each waiting item unblocks)
One item per session, at work or waiting for the user's check: a session keeps its item until the user's Looks right. \
A route to a busy session queues the item for it, with no holder yet: it starts there once that session is free, or in \
a fresh session with a hand-off after 20 minutes. An item after another starts once that one lands, or gets Looks right, as the Desk is set.
ASK: <question> | choices: <A> / <B> / <C>      (only when you can't decide without the user; give each choice a \
short line on what it does after a dash, and mark the one you suggest: choices: Merge now (Recommended) — it lands \
today / Wait for #12 — it lands after the fix; no / or ; inside a choice)
ASK: #<id> | <question> | choices: <A> / <B>      (the same, about one item: it shows on that item as waiting on the user)
DECIDED: <what you chose> — <why>      (a key judgment call you made on your own that the user may want to undo: what gets built, how it looks or behaves, what's dropped; not for routing, starting or giving work: Cue lists those itself)
DECIDED (minor): <what you chose> — <why>      (housekeeping: folders, routing, ordering, copies; kept, but off the user's Decisions list)
DONE: #<id>      (the user tells you an item is done)
NOTE: #<id> | <words>      (a message to whoever holds the item, at any stage, even finishing or with the user to check: \
Cue sends it and shows it on the item; the item stays as it is; an item on another Desk takes it too)
ANSWER: #<id> | <a choice's number, or your words>      (when Cue brings you an item holder's question about clearing \
the way for its landing (leftover edits, stray copies): answer it yourself; Cue gives your answer to it as the user's. \
Also for an item waiting on the user with its holder's question, when you know the answer: it goes to the holder, and \
the item stops waiting on the user; a NOTE to its holder does the same)
PASS: #<id> | why: <reason>      (that question is one only the user can decide: Cue shows it to them)
CLOSE: #<id> | commit: <sha> | why: <reason>      (an item whose work is already on main: Cue checks the commit is \
on the main line, then closes it as done; leave out commit when Cue already knows it, or when the item has no code \
at all: mocks, a doc, a plan, its done when saying no code)
TAKE COPY: #<id> | why: <reason>      (take back the copy of a stuck or finished item: its work stays on its branch, \
the item waiting longest for a copy gets it, and a stuck item is new again for you to route)
LAND FIRST: #<id> | why: <reason>      (move an item waiting in the landing line up, so it lands next)
COPY FIRST: #<id> | why: <reason>      (move an item waiting for a free copy of its repository to the front of that \
line, so it gets the next copy that frees; an item at work without a copy waits in that line too)
after: and LAND FIRST may name another Desk's item on this repository too, as #<id> or <Desk's name>#<id>: \
the item then waits for it, or it lands next, as within this Desk.
START: #<id> | why: <reason>      (start a waiting item now: not after the item it waits on, nor for a free copy; on one at work \
that only waits for a copy, it moves it to the front of that line)
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
HOLD: #<id> | area: <files or globs, comma-separated> | till: <it lands, or a time> | why: <reason>      (the item \
will move or rewrite that part of the code: Cue keeps the hold for the repository, and every Desk on it sees it in its \
Desk block and under its header till the item lands, is dropped or parks)
UNHOLD: #<id>      (lift that hold early)
TELL DESK: <Desk's name> | <words>      (to another Desk's lead on this repository, e.g. to agree who lands first; \
start the words with #<id> when it's about one of your items. Its answer comes back to you the same way)
TELL DESKS: <words>      (to every other Desk's lead on this repository: a heads-up before you change shared code)
MOVE: #<id> -> <Desk's name> | why: <reason>      (only when the user asks for an item to go to another Desk: it \
leaves your list for that Desk's with its notes, copy and branch; a session at work on it stops and writes its \
hand-off, which the session that Desk's lead starts for it is sent. Cue tells that lead it came)
DISAGREE: <Desk's name> | <your side>      (only when you and that Desk's lead can't agree an order: Cue puts one \
question to the user on both Desks, your side and its lead's last words to you quoted; start your side with #<id> when \
it's about one of your items. The user's answer comes to you both)
DROP: #<id> | why: <reason>      (take an item off the list in any state, e.g. when the user says to drop it or it's no \
longer wanted: whoever holds it stops (or never starts it), its copy goes (kept if it holds work), what waited on it \
goes ahead, and it shows as dropped with your reason, not done. Never DONE or CLOSE an item to get rid of it)
PAUSE: why: <reason>      (no #<id>: pause the whole Desk, only when the user asks: every session stops at a safe point, its work \
kept in its copy; queued items don't start and what you route is held)
RESUME: why: <reason>      (no #<id>: pick a paused Desk back up: sessions carry on, held routes go out)
FEATURE: <name> | #<id> #<id> …      (items that are pieces of one big feature: Cue adds a feature item of that \
name and the Desk lists the pieces under it, with how many are done; it's done once they all are. Route the pieces, \
not the feature. FEATURE: #<id> | #<id> … puts them under an item that's there already; #new is the ITEM you added \
in this reply; say it again to add more pieces)
", lead_loop_line!(), "\
DESIGNS: #<id> <canvas link>      (where an item's designs are; say it again when they move)
IF PICKED: #<id> | <option or any> | <bug|feature|task> | <title> | you'll see: … | done when: … | by: <me, new or a \
session's name>      (what follows an option of an item that asks the user to pick; their pick closes it, and Cue files \
and routes these itself, me being its holder. Its holder usually says them)
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
Cue's messages of Desk changes end with their numbers, (Desk changes 41–44): skip any change whose number you've already seen.
When Cue tells you an item stopped (its session ended its turn with no DONE and no question), settle it: ROUTE it \
to that session again with what to do next as the why, or route it on. ASK: #<id> only if the user must decide; \
until then the user doesn't see it as waiting on them.
A ROUTE Cue makes isn't answered. One it can't make is, in one message after your turn (or in your next Desk block, \
while you're asking the user something): \"Refused: #<id> <why>. You can …\"; a line that looks like a route but isn't in \
the form above comes back as one it couldn't read. Write it again, correctly.
An item that's finishing (its commit and install) and stopped on a question: send it back to its holder with what to fix \
as the why, route it to another session (or new), or ASK: #<id>. While its commit or install is under way, it stays as it is: a ROUTE back to its holder with a why goes to it as a note, \
and NOTE: #<id> reaches it too.
Before an item will move or rewrite a part of the code other Desks might touch (a refactor, a rename, a folder \
move), take a HOLD on that part. Before routing into a part another Desk holds, wait after its item, or ask its lead \
with TELL DESK. Agree the order between you, and answer another lead's band with TELL DESK. Cue asks the user only \
when you can't agree (DISAGREE), a band goes unanswered for 15 minutes, or a hold stands an hour past its till; \
never ASK the user about a heads-up you settled.
Your job is to steer: when the Desk is stuck (an item stopped, a copy held by an item that's done, the landing line in \
the wrong order, an item waiting on nothing that still matters), unstick it yourself with the lines above; don't wait \
for the user to. When you lack a control for it, tell the user what you couldn't do and add an ITEM for that control.
In your own words, name an item by its kind and number, as the user sees it: \"Bug #2\", \"Task #1\" (in the lines above, \
keep #<id>). Be short and plain: a line or two a busy person reads at a glance, no headings, no emoji.");

/// The number after the highest of `ids` that are plain numbers, and after `high` (the highest given before).
pub fn next_number<'a>(ids: impl Iterator<Item = &'a str>, high: u64) -> u64 {
    ids.filter_map(|id| id.parse::<u64>().ok()).fold(high, u64::max) + 1
}

/// What a holder writes next to its DONE for item `id` (#526, three steps since #668): what's shown to whoever checks
/// it, as steps someone who has never used the app can follow. It never says "the owner": the screen shows that phrase
/// as your name or "you" (owner.rs `on_screen`), and a send back's words must read the same on the item as in the session.
pub fn check_words(id: &str) -> String {
    format!(
        "With your DONE, on a line of its own, write what whoever checks it does, from what you actually built, as three \
         steps someone who has never used the app can follow: CHECK: #{id} | Where: <the screen, page or settings pane, by \
         the name it shows> | Do: <what to click or type there, with the button's real name> | Expect: <what they should \
         see happen>. Say what works now, not what the bug was (\"the header stays at the top as you scroll\", not \"fixed the \
         header jumping\"), in plain words: no code names and no words like branch, commit, worktree, hook, landing or copy. \
         When there's nothing anyone can try: CHECK: #{id} | Nothing to try: <why, e.g. a change nobody can see; tests show it works>."
    )
}

/// Desk and git words that mean nothing to whoever checks an item (#668), found as whole words in its check line.
const CHECK_JARGON: &[&str] = &["copy", "landing", "landed", "holder", "spec", "branch", "worktree", "commit", "committed", "hook", "the owner"];

/// What in check line `c` isn't plain (#668): the Desk words it uses, and whether it names code (backticks, a file, a
/// `path::name`, a snake_case or camelCase name). Empty when it's plain.
pub fn check_unplain(c: &crate::desk::model::Check) -> (Vec<&'static str>, bool) {
    let all = format!("{} {} {} {}", c.text, c.at, c.act, c.expect);
    let low = all.to_lowercase();
    let whole = |w: &str| low.match_indices(w).any(|(n, _)| {
        let before = low[..n].chars().next_back().map_or(true, |c| !c.is_alphanumeric());
        let after = low[n + w.len()..].chars().next().map_or(true, |c| !c.is_alphanumeric());
        before && after
    });
    let words = CHECK_JARGON.iter().copied().filter(|w| whole(w)).collect();
    let code = all.contains('`') || all.contains("::") || all.split(|c: char| c.is_whitespace() || "(),;\"'".contains(c)).any(|t| {
        let t = t.trim_end_matches(['.', ':', '!', '?']);
        let file = t.rsplit_once('.').is_some_and(|(a, ext)| !a.is_empty() && ["rs", "js", "mjs", "ts", "css", "html", "json", "swift", "py", "md", "toml"].contains(&ext));
        let snake = t.contains('_') && t.split('_').filter(|p| !p.is_empty()).count() > 1 && t.chars().all(|c| c.is_alphanumeric() || c == '_');
        // camelCase: a capital between small letters, past the second letter ("deskItemSide", not "eBay" or "macOS").
        let ch: Vec<char> = t.chars().collect();
        let camel = ch.first().is_some_and(|c| c.is_lowercase()) && ch.iter().all(|c| c.is_alphanumeric()) && (2..ch.len().saturating_sub(1)).any(|k| ch[k - 1].is_lowercase() && ch[k].is_uppercase() && ch[k + 1].is_lowercase());
        file || snake || camel
    });
    (words, code)
}

/// What Cue sends item `id`'s holder about its check line `c` once it's said DONE (#668): none given, a one-line Try
/// with no steps, or words that aren't plain; None when it's fine (three plain steps, or nothing to try and why).
pub fn check_back_words(id: &str, c: Option<&crate::desk::model::Check>) -> Option<String> {
    let fix = match c {
        None => "it has no check line".to_string(),
        Some(c) if c.kind == "try" && c.at.is_empty() => "it's one line, not the three steps".to_string(),
        Some(c) => {
            let (words, code) = check_unplain(c);
            let mut why = Vec::new();
            if !words.is_empty() {
                why.push(format!("it uses {} (Desk words someone checking won't know: say what they see on screen instead)", words.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(", ")));
            }
            if code {
                why.push("it names code (say the button, page or setting by the name the app shows)".to_string());
            }
            if why.is_empty() {
                return None;
            }
            why.join(", and ")
        }
    };
    Some(format!(
        "Your DONE for #{id} is in, but its check line needs another go: {fix}. Write it again on a line of its own (it \
         replaces the one before; nothing else to redo): CHECK: #{id} | Where: <the screen or page, by the name it shows> | \
         Do: <what to click or type> | Expect: <what they should see happen>, or CHECK: #{id} | Nothing to try: <why>. Plain \
         words someone who has never used the app can follow, saying what works now rather than what was wrong."
    ))
}

/// How the holder of item `id`, with the user to check, says it isn't done after all (#627).
pub fn not_done_words(id: &str) -> String {
    format!(
        "If there's more to do on it after all (the user wants more, or you found something left), say NOT DONE: #{id} | <what's \
         left> on a line of its own: Cue puts it back in progress (in line for a copy when none is free). Else it stays \
         with the user to check."
    )
}

/// Which tests a session runs while it works (#778; docs/desk-routing-rules.md, "Tests while you work"), in every item's
/// brief: as its Desk's Tests setting says (#780, desk/lands.rs: "all", "changed", "changed_full").
pub fn tests_rule(tests: &str) -> &'static str {
    match tests {
        "changed" => "Tests: run only the tests for the files you changed and for code that uses them, never the full suite.",
        "changed_full" => "Tests: while you work, run only the tests for the files you changed and for code that uses them. \
Run the full suite once, just before your DONE: your work lands from there.",
        _ => "Tests: run the tests for what you change as you work, and the full suite before you say done.",
    }
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
    out.push_str(&format!(
        "\nWhen you ask the owner to pick between options, say what follows each, a line apiece: IF PICKED: #{} | <option, \
         or any> | <bug|feature|task> | <title> | you'll see: <…> | done when: <…> | by: <me, new, or a session's name>. \
         A pick closes #{0}, and Cue files and routes what follows it.",
        it.id
    ));
    // Its Desk's Tests setting (#780): config is held in memory, so this never reads a file under the store's lock.
    let tests = it.tree.as_ref().map_or_else(|| crate::desk::lands::tests_of(&it.project, "", false), |t| crate::desk::lands::tests_of(&it.project, &t.repo, t.card_tests));
    out.push_str(&format!("\n{}", tests_rule(tests)));
    out.push_str(&format!("\nSay DONE: #{} on a line of its own when it's done.", it.id));
    out.push_str(&format!("\n{}", check_words(&it.id)));
    out.push_str("\nTo ask the owner something (a pick, a yes or no, something only they can tell you), end your reply with that question as its last line, ending in \"?\".");
    if let Some(t) = &it.tree {
        out.push_str(&format!("\n{}", crate::hub::tree_line(t, tests)));
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

/// What a lead is told of a new item it hasn't routed yet (added by you, or by a session's ITEM: line): one line.
/// What it's to do with them is said once per message (`new_items_tail`).
pub fn new_item_words(it: &DeskItem) -> String {
    let mut out = format!("New item #{}{}: {}", it.id, kind_words(it), it.title);
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
    // Said in the row itself: the lead hears of a new item in the feed, not in the Desk's summary.
    if it.kind.is_empty() {
        out.push_str(" | no kind yet");
    }
    out
}

/// What the lead is to do with new items `items` (those in one message), said once after them: SPLIT/TITLE, SEE,
/// and route them now.
pub fn new_items_tail(items: &[&DeskItem]) -> String {
    let mut out = String::new();
    if items.iter().any(|i| i.from == "you" && i.words.is_empty() && i.split_from.is_empty()) {
        out.push_str("If one of the owner's items holds two separate problems, SPLIT it first (Cue tells you the new numbers); if its title isn't short and plain, TITLE it. ");
    }
    if items.iter().any(|i| i.see.trim().is_empty() || i.done_when.trim().is_empty()) {
        out.push_str("Give one missing its two lines both first: SEE: #<id> | <what the owner sees when it's done> | done when: <the check>. ");
    }
    out.push_str(if items.len() == 1 { "Route it now: ROUTE it to a session, or -> new." } else { "Route each now: ROUTE it to a session, or -> new." });
    if items.iter().any(|i| i.kind.is_empty()) {
        out.push_str(" Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.");
    }
    out
}

/// The line ipc `desk_item_act` (and the add-on) puts your pictures in, after your words on an item's page.
pub const YOUR_PICTURES: &str = "\nPictures with it (open each to see it): ";
/// The line those pictures go in on the Desk's message to a session (send-back, note, question): taken off as the
/// Desk sends it (`as_sent`), and the pictures sent as pictures, as a session's own chat sends them (#541).
const SENT_PICTURES: &str = "Pictures you attached: ";

/// Your words from an item's page (`YOUR_PICTURES` and on left out), and the pictures' paths.
pub fn your_words(text: &str) -> (&str, Vec<String>) {
    match text.split_once("\nPictures with it") {
        Some((words, pics)) => (words, backticked(pics)),
        None => (text, vec![]),
    }
}

/// Your words from an item's page as the Desk's message carries them: the words, then the pictures on their own line.
pub fn with_your_pictures(text: &str) -> String {
    let (words, pics) = your_words(text);
    if pics.is_empty() {
        return words.trim().to_string();
    }
    format!("{}\n{SENT_PICTURES}{}", words.trim(), pics.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(" "))
}

/// A Desk message as it's sent: its pictures line taken off, and those pictures, to send as pictures.
pub fn as_sent(message: &str) -> (String, Vec<crate::uploads::Saved>) {
    let mut pics = vec![];
    let words: Vec<&str> = message
        .split('\n')
        .filter(|l| match l.strip_prefix(SENT_PICTURES) {
            Some(rest) => {
                pics.extend(backticked(rest));
                false
            }
            None => true,
        })
        .collect();
    let saved = pics.into_iter().map(|p| crate::uploads::Saved { mime: crate::uploads::mime_of(&p).into(), path: p }).collect();
    (words.join("\n"), saved)
}

/// The paths written in backticks in `text`.
fn backticked(text: &str) -> Vec<String> {
    text.split('`').skip(1).step_by(2).map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()
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
