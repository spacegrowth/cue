//! Checks over Cue's own source (every `.rs` file under src/ but this folder, whose code and patterns
//! name the shapes it refuses): every lock goes through `locking::lock` with a name no other lock has,
//! and none is taken where it stays held while the next one is taken.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Files that may still call `.lock()` themselves: the terminal, the socket server and the window's
/// commands are being reworked in a parallel track (stability plan, Phase 0 track B); each entry goes
/// when that track moves its file onto `locking::lock`. None of them has a bare lock today.
const BARE_ALLOWED: &[&str] = &["terminal/term.rs", "server.rs", "ipc/"];

/// Every source file but this folder's, by its path under src/, with its text.
fn sources() -> Vec<(String, String)> {
    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![];
    rs_files(&src, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| (p.strip_prefix(&src).unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .filter(|(rel, _)| !rel.starts_with("locking/"))
        .collect()
}

/// For each line of `lines`, whether it's test code: an item under `#[cfg(test)]`, through its closing brace.
fn test_only(lines: &[&str]) -> Vec<bool> {
    let mut test = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "#[cfg(test)]" {
            i += 1;
            continue;
        }
        let mut depth = 0i32;
        let mut j = i;
        while j < lines.len() {
            test[j] = true;
            depth += lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
            // The item ends where its braces close, or on its own line when it has none (a `static`).
            if j > i && depth <= 0 && (lines[j].contains('}') || lines[j].trim_end().ends_with(';')) {
                break;
            }
            j += 1;
        }
        i = j + 1;
    }
    test
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// The lines (from 1) of `text` that call `.lock()` themselves, outside comments and test code.
fn bare_locks(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let test = test_only(&lines);
    lines.iter().enumerate().filter(|(i, l)| !test[*i] && !is_comment(l) && l.contains(".lock()")).map(|(i, _)| i + 1).collect()
}

/// The names `text` gives its locks: the string after each `locking::lock(<lock>, `.
fn lock_names(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !is_comment(l))
        .flat_map(|l| l.match_indices("locking::lock(").map(move |(at, _)| &l[at..]))
        .filter_map(|call| call.split('"').nth(1).map(str::to_string))
        .collect()
}

/// Names used in more than one file, with the files. A lock is used in one file (others go through a
/// function of that file), so a name in two files is two locks that can't be told apart in a log.
fn shared_names(files: &[(String, String)]) -> Vec<String> {
    let mut by_name: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (path, text) in files {
        for name in lock_names(text) {
            by_name.entry(name).or_default().insert(path);
        }
    }
    by_name.into_iter().filter(|(_, f)| f.len() > 1).map(|(n, f)| format!("\"{n}\" names a lock in {}", f.into_iter().collect::<Vec<_>>().join(" and "))).collect()
}

/// Where a lock is taken while another is still held, in the shapes a line can show: a second lock in
/// the same statement, or a lock in the head of a `for`, `match`, `if let` or `while let` (held through
/// the body, Rust 2021) whose body locks again.
fn held_across(path: &str, text: &str) -> Vec<String> {
    let is_lock = |l: &str| l.contains("locking::lock(") || l.contains(".lock()") || l.contains(".st()");
    let lines: Vec<&str> = text.lines().collect();
    let mut bad = vec![];
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if is_comment(t) {
            continue;
        }
        if let Some(first) = ["locking::lock(", ".lock()", ".st()"].iter().filter_map(|p| t.find(p)).min() {
            if is_lock(&t[first + 5..]) {
                bad.push(format!("{path}:{}: two locks in one expression", i + 1));
            }
        }
        let head = ["for ", "match ", "if let ", "while let "].iter().any(|k| t.starts_with(k)) && is_lock(t) && t.ends_with('{');
        if !head {
            continue;
        }
        let mut depth = 0i32;
        for body in &lines[i + 1..] {
            depth += body.matches('{').count() as i32 - body.matches('}').count() as i32;
            if depth < 0 {
                break;
            }
            if is_lock(body) || body.contains(".changed()") {
                bad.push(format!("{path}:{}: a lock in the head of a block that locks again", i + 1));
                break;
            }
        }
    }
    bad
}

#[test]
fn every_lock_goes_through_locking_lock() {
    let bad: Vec<String> = sources()
        .iter()
        .filter(|(path, _)| !BARE_ALLOWED.iter().any(|a| path.starts_with(a)))
        .flat_map(|(path, text)| bare_locks(text).into_iter().map(move |n| format!("{path}:{n}")))
        .collect();
    assert!(bad.is_empty(), "a lock taken with .lock(), not crate::locking::lock(&m, \"its name\"):\n{}", bad.join("\n"));
}

#[test]
fn every_lock_has_a_name_of_its_own() {
    let bad = shared_names(&sources());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The shape behind both hangs that reached the app. Bind the lookup with `let` and let it drop, then do
/// the next thing.
#[test]
fn no_lock_is_taken_in_the_head_of_a_block_or_twice_in_one_expression() {
    let bad: Vec<String> = sources().iter().flat_map(|(path, text)| held_across(path, text)).collect();
    assert!(bad.is_empty(), "lock taken while held (see locking/lint.rs):\n{}", bad.join("\n"));
}

#[test]
fn a_bare_lock_is_found_outside_tests_and_comments_only() {
    let text = "\
fn a() {
    let g = M.lock().unwrap();
}
// M.lock() in a comment
#[cfg(test)]
static T: Mutex<()> = Mutex::new(());
fn b() {
    let _ = N.lock();
}
#[cfg(test)]
mod tests {
    fn t() {
        let g = M.lock().unwrap();
    }
}
fn c() { O.lock(); }
";
    assert_eq!(bare_locks(text), [2, 8, 16]);
}

#[test]
fn a_name_in_two_files_is_found() {
    let one = ("a.rs".to_string(), "let x = crate::locking::lock(&INDEX, \"INDEX\");".to_string());
    let two = ("b.rs".to_string(), "*crate::locking::lock(&INDEX, \"INDEX\") = None;".to_string());
    let own = ("c.rs".to_string(), "crate::locking::lock(&X, \"c X\"); crate::locking::lock(&X, \"c X\");".to_string());
    assert_eq!(shared_names(&[one, two, own]), ["\"INDEX\" names a lock in a.rs and b.rs"]);
}
