//! Every message into a session says who it's from and what kind it is where it's sent (#837): read from the source.
//!
//! - Text is handed into a session only by hub/send.rs's `deliver` (and the hosted half of it, hub/hosted.rs), or as
//!   a session's first message when it's started or resumed: no other line calls the primitives that type or hand
//!   text in (`ALLOWED` names each line that may).
//! - `send_to_session` is your words alone: only the places that send what you wrote call it.
//! - Cue's words name their kind at the call: `tell`, `tell_now`, `cue_says` and `desk_send_cue` end with a
//!   `Kind::…` (or pass on a `kind` given them), `send_saved` with a `Said::…` (or a `said`).

use std::collections::HashMap;
use std::path::Path;

/// What types or hands text into a session, and `send_to_session` (yours only).
const PRIMITIVES: [&str; 7] = ["type_into(", "send_direct(", "send_saved_hosted(", "send_to_hosted(", "send_to_hosted_held(", "hosted().send(", "send_to_session("];
/// The lines that may call them, as (file, the call as written there). A file named with "*" may call `what` on any
/// line: where the primitive is made (the terminal's keys, the hosted connections), or where your words are sent.
const ALLOWED: [(&str, &str); 21] = [
    // deliver itself, and typing a command
    ("hub/send.rs", "self.send_saved_hosted(session_id, typed, saved, busy, now, held)"),
    ("hub/send.rs", "self.send_direct(session_id, text, saved, now)"),
    ("hub/send.rs", "self.keys().type_into(&origin, "),
    ("hub/send.rs", "self.send_to_hosted_held(session_id, typed, saved, busy, held)"),
    ("hub/send.rs", "self.keys().type_into(origin, command)"),
    ("hub/send.rs", "self.hosted().send(&origin.session_id, command)"),
    // the hosted half of deliver
    ("hub/hosted.rs", "self.send_to_hosted_held(sid, typed, saved, busy, None)"),
    ("hub/hosted.rs", "self.send_to_hosted(sid, text, &[], busy)"),
    ("hub/hosted.rs", "self.hosted().send(sid, &message)"),
    // a session's first message, started or resumed with it (its sender said whose it is first)
    ("hub/starting.rs", "h.send_to_hosted(&sid, message, saved, false)"),
    ("hub/resuming.rs", "h.hosted().send(&sid, message.trim())"),
    // where the primitives are made
    ("terminal/*", "type_into("),
    ("hosted/*", "send("),
    // your words
    ("ipc/*", "send_to_session("),
    ("server/inbound.rs", "hub.send_to_session(&session_id, &text, &images, now)"),
    ("voice/talk.rs", "h.send_to_session(&sid, &msg, &[], false)"),
    ("hub/team/out.rs", "self.send_to_session(sid, &self.team_words(sid, text), images, now)"),
    ("hub/send.rs", "self.send_saved(session_id, text, saved, now, Said::You)"),
    ("desk/hub/core/desk.rs", "h.send_to_session(&lead, &words, &[], false)"),
    ("ext/*", "send_to_session("),
    ("hub/send.rs", "send_to_session("),
];
/// Calls that must say their kind (last argument), and what may stand there.
const TAGGED: [(&str, &[&str]); 5] = [
    (".tell(", &["Kind::", "kind"]),
    (".tell_now(", &["Kind::", "kind"]),
    ("cue_says(", &["Kind::", "kind"]),
    ("desk_send_cue(", &["Kind::", "kind"]),
    ("send_saved(", &["Said::", "said"]),
];
/// The window's own `tell` (an event to the window, not a message to a session).
const WINDOW_TELL: &str = ".ports.window.tell(";

/// Where the string starting at `i` (a `"`) ends.
fn str_end(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() && b[i] != b'"' {
        i += if b[i] == b'\\' { 2 } else { 1 };
    }
    i
}

/// Where a char literal starting at `i` ends (`'x'`, `'\\x'`); `i` itself for a lifetime.
fn char_end(b: &[u8], i: usize) -> usize {
    if b.get(i + 2) == Some(&b'\'') {
        i + 2
    } else if b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'') {
        i + 3
    } else {
        i
    }
}

/// Where `open` (an opening paren or brace) closes in `s`, strings and char literals skipped.
fn close_of(s: &[u8], open: usize) -> Option<usize> {
    let (mut depth, mut i) = (0i32, open);
    while i < s.len() {
        match s[i] {
            b'"' => i = str_end(s, i),
            b'\'' => i = char_end(s, i),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The last top-level argument of the call whose parens are `open..close`.
fn last_arg(src: &str, open: usize, close: usize) -> &str {
    let b = src.as_bytes();
    let (mut start, mut i) = (open + 1, open + 1);
    while i < close {
        match b[i] {
            b'"' => i = str_end(b, i),
            b'(' | b'[' | b'{' => i = close_of(b, i).unwrap_or(close),
            b',' => start = i + 1,
            _ => {}
        }
        i += 1;
    }
    let a = src[start..close].trim();
    if a.is_empty() && start > open + 1 {
        // A trailing comma: the one before it.
        return last_arg(src, open, start - 1);
    }
    a
}

/// `src` with its `//` comments and its `#[cfg(test)]` items (a file's own tests) blanked, every line kept where it was.
fn code_of(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => i = str_end(b, i),
            b'\'' => i = char_end(b, i),
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = src[i..].find('\n').map_or(b.len(), |n| i + n);
                blank(&mut out, i, end);
                i = end;
            }
            b'#' if src[i..].starts_with("#[cfg(test)]") => {
                // The item it's on: to its `;`, or to the close of its `{`.
                let after = i + "#[cfg(test)]".len();
                let semi = src[after..].find(';').map(|n| after + n);
                let brace = src[after..].find('{').map(|n| after + n);
                let end = match (semi, brace) {
                    (Some(s), Some(o)) if s < o => s + 1,
                    (_, Some(o)) => close_of(b, o).map_or(b.len(), |c| c + 1),
                    (Some(s), None) => s + 1,
                    (None, None) => b.len(),
                };
                blank(&mut out, i, end);
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The line of `code` that `i` is on, and its number.
fn line_at(code: &str, i: usize) -> (&str, usize) {
    let start = code[..i].rfind('\n').map_or(0, |n| n + 1);
    let end = code[i..].find('\n').map_or(code.len(), |n| i + n);
    (&code[start..end], code[..i].matches('\n').count() + 1)
}

/// Line `line` of `path` may call `what` (`ALLOWED`).
fn allowed(path: &str, line: &str, what: &str) -> bool {
    ALLOWED.iter().any(|(file, call)| match file.strip_suffix('*') {
        Some(dir) => path.starts_with(dir) && what.ends_with(call),
        None => *file == path && call.contains(what.trim_end_matches('(')) && line.contains(call),
    })
}

/// Where file `path` (its path under the source root) breaks the rules, as "path:line: why".
pub(crate) fn violations(path: &str, src: &str) -> Vec<String> {
    let code = code_of(src);
    let mut out = Vec::new();
    for p in PRIMITIVES {
        for (i, _) in code.match_indices(p) {
            if code[..i].trim_end().ends_with("fn") {
                continue;
            }
            let (line, n) = line_at(&code, i);
            if !allowed(path, line, p) {
                let why = if p == "send_to_session(" { "sends as yours by `send_to_session(…)` where it isn't your words".to_string() } else { format!("hands text into a session by `{p}…)` outside hub/send.rs's deliver") };
                out.push(format!("{path}:{n}: {why}"));
            }
        }
    }
    for (call, ok) in TAGGED {
        for (i, _) in code.match_indices(call) {
            if code[..i].trim_end().ends_with("fn") || (call == ".tell(" && code[..i + 1].ends_with(&WINDOW_TELL[..WINDOW_TELL.len() - 5])) {
                continue;
            }
            let open = i + call.len() - 1;
            let Some(close) = close_of(code.as_bytes(), open) else { continue };
            let last = last_arg(&code, open, close);
            if !ok.iter().any(|o| last.starts_with(o) || last.contains(&format!("::{o}")) || last == *o || last.ends_with(&format!(".{o}"))) {
                out.push(format!("{path}:{}: `{call}…)` doesn't say its kind (ends with `{last}`)", line_at(&code, i).1));
            }
        }
    }
    out
}

/// Every .rs file under `root`, as (path under it, its text). A link to a folder isn't followed: the add-on links to
/// itself.
fn sources(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    out
}

/// A test's own file, not checked: the test harness (testing/, it fakes sessions and plants what this test looks for),
/// a `*_tests.rs`, or a file in a `tests/` folder its module declares `#[cfg(test)] mod tests;`.
fn is_test(path: &str, all: &HashMap<String, String>) -> bool {
    if path.starts_with("testing/") || path.ends_with("_tests.rs") {
        return true;
    }
    let Some(i) = path.find("/tests/").map(|i| i + 1).or_else(|| path.starts_with("tests/").then_some(0)) else { return false };
    let parent = path[..i].trim_end_matches('/');
    let decl = |f: &str| all.get(f).is_some_and(|s| s.split("#[cfg(test)]").skip(1).any(|after| after.trim_start().starts_with("mod tests;")));
    if parent.is_empty() {
        decl("lib.rs") || decl("mod.rs")
    } else {
        decl(&format!("{parent}/mod.rs")) || decl(&format!("{parent}.rs"))
    }
}

#[test]
fn every_message_into_a_session_goes_by_deliver_and_cues_say_their_kind_where_sent() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut broken = Vec::new();
    for (root, under) in [(crate_dir.join("src"), ""), (crate_dir.join("ext"), "ext/")] {
        if !root.exists() {
            continue;
        }
        let all: HashMap<String, String> = sources(&root).into_iter().collect();
        for (path, src) in &all {
            if !is_test(path, &all) {
                broken.extend(violations(&format!("{under}{path}"), src));
            }
        }
    }
    broken.sort();
    assert!(broken.is_empty(), "messages into a session must be sent with who and what kind, where they're sent:\n{}", broken.join("\n"));
}

#[test]
fn the_rules_catch_a_message_sent_around_deliver_or_with_no_kind() {
    let planted = r#"
fn f(h: &Hub) {
    h.keys().type_into(&o, "hello");
    h.tell(&sid, &format!("Desk item #{id} (bug): go"));
    h.tell_now(&sid, "now", true, Kind::Nudge);
    h.cue_says(&text);
    h.send_saved(&sid, &t, vec![], false);
    // h.tell(&sid, "a comment");
    let s = "a // in a string"; h.tell(&sid, &s);
    h.ports.window.tell("ui-reload", json!({}));
    h.other.window.tell("ui-reload");
    h.send_to_session(&sid, "Your teammate has gone quiet.", &[], false);
    h.tell(
        &sid,
        &words,
        crate::sessions::Kind::Route,
    );
}

#[cfg(test)]
mod tests {
    fn t(h: &Hub) { h.tell(&sid, "a test's own"); }
}

fn g(h: &Hub) {
    h.tell(&sid, "after a file's tests");
}
"#;
    let v = violations("desk/planted.rs", planted);
    let at = |n: usize, what: &str| v.iter().any(|l| l.starts_with(&format!("desk/planted.rs:{n}: ")) && l.contains(what));
    assert!(at(3, "`type_into("), "around deliver: {v:#?}");
    assert!(at(4, "`.tell(…)` doesn't say its kind"), "{v:#?}");
    assert!(at(6, "`cue_says(…)`"), "{v:#?}");
    assert!(at(7, "`send_saved(…)`"), "{v:#?}");
    assert!(at(9, "`.tell(…)`"), "a `//` in a string before the call: {v:#?}");
    assert!(at(11, "`.tell(…)`"), "a tell that isn't the window's own: {v:#?}");
    assert!(at(12, "`send_to_session(…)`"), "Cue's words sent as yours: {v:#?}");
    assert!(at(26, "`.tell(…)`"), "after a file's tests: {v:#?}");
    assert_eq!(v.len(), 8, "the comment, the window's tell, the tagged call and the test's own aren't: {v:#?}");
    // The same calls where they belong, or tagged: nothing.
    assert!(violations("hub/send.rs", "fn deliver() { let direct = self.send_direct(session_id, text, saved, now); }").is_empty());
    assert!(!violations("hub/send.rs", "fn other() { self.send_direct(sid, t, s, n); }").is_empty(), "only deliver's own line");
    assert!(!violations("hub/hosted.rs", "fn other() { self.send_to_hosted(sid, cue_words, &[], false); }").is_empty(), "only the hosted half's own lines");
    assert!(violations("hub/guard.rs", "h.tell(&sid, &words, Kind::Desk);").is_empty());
    assert!(violations("voice/talk.rs", "None => h.send_to_session(&sid, &msg, &[], false),").is_empty(), "your words");
}
