//! Which of the Desk's calls are dispatch work rather than a choice worth your glance, and a call's words as
//! compared for being said again.

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
/// A source file named in a call ("`desk/hub/landing/finish.rs`"): how it was built, not what the owner gets.
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
