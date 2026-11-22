//! A file the item and another session both changed in the shared folder, where the other's change can't be taken out
//! by its text (a `sed -i`, an inline script, a write): split hunk by hunk (#658). e7361cace took the whole of trees.rs
//! into #650's commit, other items' half-done edits with it, and main stopped building; since #653 such a file was left
//! out whole, the item's own part with it, so main could miss what the item's other files need.
//!
//! Each hunk of the file's change since HEAD goes to the sessions whose own words hold its text: the commands and
//! edits the item's sessions sent (its holder, former holders, their crews), or those of the sessions that changed the
//! file. Only what they sent counts (tool calls, their replies), never what tools gave back, so a file read whole isn't
//! a claim on it. A hunk both or neither hold can't be told apart: the file is left out as before. A hunk with no line
//! worth telling by (blank lines, braces) goes with the hunk nearest it.

use crate::desk::hub::theirs::theirs::{Own, Theirs};
use serde_json::Value;

/// Whose a hunk is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Who {
    Mine,
    Theirs,
    /// Both or neither hold it.
    Unsure,
    /// No line worth telling it by.
    Plain,
}

/// A line that says something: three letters or digits at least (not a brace, a blank, `Ok(())`).
fn telling(l: &str) -> bool {
    l.chars().filter(|c| c.is_alphanumeric()).count() >= 3
}

/// Whose `lines` are, by which text holds them (each line trimmed).
fn whose(lines: &[&str], mine: &str, theirs: &str) -> Who {
    let key: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| telling(l)).collect();
    if key.is_empty() {
        return Who::Plain;
    }
    let n = key.len();
    let (m, t) = (key.iter().filter(|l| mine.contains(**l)).count(), key.iter().filter(|l| theirs.contains(**l)).count());
    match (m, t) {
        (m, t) if m == n && t < n => Who::Mine,
        (m, t) if t == n && m < n => Who::Theirs,
        (m, 0) if m > 0 => Who::Mine,
        (0, t) if t > 0 => Who::Theirs,
        _ => Who::Unsure,
    }
}

/// `git diff -U0`'s hunk headers: (index of the first old line, old count, index of the first new line, new count).
fn hunk_heads(diff: &str) -> Vec<(usize, usize, usize, usize)> {
    let side = |s: &str| -> Option<(usize, usize)> {
        let (start, count) = match s.split_once(',') {
            Some((a, b)) => (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?),
            None => (s.parse::<usize>().ok()?, 1),
        };
        // A count of 0 names the line it comes after; else its first line, from 1.
        Some((if count == 0 { start } else { start.saturating_sub(1) }, count))
    };
    diff.lines()
        .filter_map(|l| {
            let mut parts = l.strip_prefix("@@ ")?.split(' ');
            let (o, n) = (side(parts.next()?.strip_prefix('-')?)?, side(parts.next()?.strip_prefix('+')?)?);
            Some((o.0, o.1, n.0, n.1))
        })
        .collect()
}

/// `head` with only the hunks of `diff` (HEAD to `now`, `-U0`) that `mine`'s text holds, not `theirs`'s. None when a
/// hunk can't be told apart, or the hunks don't fit the files.
pub(crate) fn split(head: &str, now: &str, diff: &str, mine: &str, theirs: &str) -> Option<String> {
    let (old, new): (Vec<&str>, Vec<&str>) = (head.split_inclusive('\n').collect(), now.split_inclusive('\n').collect());
    let heads = hunk_heads(diff);
    if heads.is_empty() {
        return None;
    }
    let mut who: Vec<Who> = Vec::new();
    for &(o, b, n, d) in &heads {
        let (gone, added) = (old.get(o..o + b)?, new.get(n..n + d)?);
        let w = match whose(added, mine, theirs) {
            Who::Plain => whose(gone, mine, theirs),
            w => w,
        };
        if w == Who::Unsure {
            return None;
        }
        who.push(w);
    }
    // A plain hunk goes with the nearest one told (by its line in HEAD).
    let told: Vec<usize> = (0..who.len()).filter(|i| who[*i] != Who::Plain).collect();
    if told.is_empty() {
        return None;
    }
    let settled: Vec<Who> = (0..who.len())
        .map(|i| if who[i] != Who::Plain { who[i] } else { who[*told.iter().min_by_key(|j| heads[**j].0.abs_diff(heads[i].0)).unwrap_or(&told[0])] })
        .collect();
    let mut out = String::with_capacity(head.len());
    let mut at = 0;
    for (&(o, b, n, d), w) in heads.iter().zip(&settled) {
        if o < at {
            return None;
        }
        out.extend(old[at..o].iter().copied());
        out.extend(if *w == Who::Mine { &new[n..n + d] } else { &old[o..o + b] }.iter().copied());
        at = o + b;
    }
    out.extend(old.get(at..)?.iter().copied());
    Some(out)
}

/// Each string in `v`, and those of a string that's JSON itself (a Codex call's arguments), appended to `out`.
fn strings(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => {
            if s.starts_with('{') {
                if let Ok(inner) = serde_json::from_str::<Value>(s) {
                    return strings(&inner, out);
                }
            }
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// What the sessions of `logs` sent from `since` (ms) on: their tool calls and replies, never what came back to them
/// (a file they read). Claude Code's, Codex's and Pi's logs alike.
pub(crate) fn sent_text(logs: &[String], since: u64) -> String {
    let mut out = String::new();
    for log in logs {
        let Ok(text) = std::fs::read_to_string(log) else { continue };
        for line in text.lines().filter(|l| l.contains("assistant") || l.contains("_call")) {
            let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
            let at = crate::logs::transcript::stamp(&e);
            if at != 0 && at < since {
                continue;
            }
            let sent = e.get("type").and_then(Value::as_str) == Some("assistant")
                || e.pointer("/message/role").and_then(Value::as_str) == Some("assistant")
                || e.pointer("/payload/type").and_then(Value::as_str).is_some_and(|t| matches!(t, "function_call" | "custom_tool_call" | "local_shell_call"));
            if sent {
                strings(e.get("message").or_else(|| e.get("payload")).unwrap_or(&e), &mut out);
            }
        }
    }
    out
}

/// Of `path` in `folder` (changed since HEAD, `since` its last commit not the item's own), the item's part told hunk
/// by hunk: None when it can't be.
pub(crate) fn own_by_hunk(theirs: &Theirs, folder: &str, path: &str, since: u64) -> Option<Own> {
    let abs = format!("{}/{path}", folder.trim_end_matches('/'));
    let their_logs = theirs.logs_of(&abs);
    if their_logs.is_empty() || theirs.mine_logs.is_empty() {
        return None;
    }
    let git = |args: &[&str]| crate::desk::hub::landing::git::git("", folder, &[], args);
    let now = std::fs::read_to_string(&abs).ok()?;
    let head = git(&["show", &format!("HEAD:./{path}")])?;
    let diff = git(&["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "HEAD", "--", path])?;
    let mine = sent_text(&theirs.mine_logs, since);
    let other = sent_text(&their_logs, since);
    let part = split(&head, &now, &diff, &mine, &other)?;
    Some(if part == head { Own::Nothing } else if part == now { Own::Whole } else { Own::Part(part.into_bytes()) })
}

#[cfg(test)]
mod tests {
    use super::split;

    fn diff_of(head: &str, now: &str) -> String {
        let dir = std::env::temp_dir().join(format!("cue-by-hunk-{}-{}", std::process::id(), head.len() * 31 + now.len()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, head).unwrap();
        std::fs::write(&b, now).unwrap();
        let o = std::process::Command::new(crate::safe_git::git_bin()).args(["diff", "--no-index", "-U0", "--no-color"]).arg(&a).arg(&b).output().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    const HEAD: &str = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n";

    #[test]
    fn each_hunk_goes_to_whose_commands_hold_it() {
        let now = "fn a() {\n    one();\n    mine_added();\n}\n\nfn b() {\n    two_changed_by_them();\n}\n";
        let d = diff_of(HEAD, now);
        let mine = "python3 - <<'EOF'\ns=s.replace('one();\\n', 'one();\\n    mine_added();\\n')\nEOF";
        let theirs = "sed -i '' 's/two();/two_changed_by_them();/' x.rs";
        assert_eq!(split(HEAD, now, &d, mine, theirs).as_deref(), Some("fn a() {\n    one();\n    mine_added();\n}\n\nfn b() {\n    two();\n}\n"));
        assert_eq!(split(HEAD, now, &d, theirs, mine).as_deref(), Some("fn a() {\n    one();\n}\n\nfn b() {\n    two_changed_by_them();\n}\n"));
    }

    #[test]
    fn a_hunk_both_or_neither_hold_cant_be_told() {
        let now = "fn a() {\n    one();\n    mine_added();\n}\n\nfn b() {\n    two();\n}\n";
        let d = diff_of(HEAD, now);
        assert_eq!(split(HEAD, now, &d, "mine_added();", "mine_added();"), None, "both");
        assert_eq!(split(HEAD, now, &d, "other", "else"), None, "neither");
    }

    #[test]
    fn a_removal_goes_by_what_it_removed() {
        let head = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n\nfn c() {\n    three();\n}\n";
        // Theirs drops fn c; mine adds a line and a blank one to fn a.
        let now = "fn a() {\n    one();\n\n    mine_added();\n}\n\nfn b() {\n    two();\n}\n";
        let d = diff_of(head, now);
        let got = split(head, now, &d, "mine_added();", "s.replace('\\nfn c() {\\n    three();\\n}\\n', '')\nfn c() {\nthree();").unwrap();
        assert_eq!(got, "fn a() {\n    one();\n\n    mine_added();\n}\n\nfn b() {\n    two();\n}\n\nfn c() {\n    three();\n}\n");
    }

    #[test]
    fn a_plain_hunk_goes_with_the_nearest() {
        let head = "alpha();\nbeta();\n";
        let now = "alpha();\nmine_new();\nbeta();\n}\n";
        let d = diff_of(head, now);
        assert_eq!(split(head, now, &d, "mine_new();", "other").as_deref(), Some(now));
        assert_eq!(split(head, now, &d, "other", "mine_new();").as_deref(), Some(head));
    }

    #[test]
    fn a_last_line_without_its_newline_is_kept_as_it_is() {
        let head = "a line\nlast one";
        let now = "a line changed\nlast one";
        let d = diff_of(head, now);
        assert_eq!(split(head, now, &d, "a line changed", "nothing").as_deref(), Some(now));
        assert_eq!(split(head, now, &d, "nothing", "a line changed").as_deref(), Some(head));
    }
}
