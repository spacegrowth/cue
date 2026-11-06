//! Reading Rust source for the lint, line by line: comments (`//` and `/* */`, nested) blanked out, string
//! literals kept or blanked, and which lines are test code. Not a parser: enough that a lock named in a
//! comment, a string or a test isn't taken for one in the app.

/// `text` with every comment blanked to spaces, and with string and char literals blanked too unless
/// `keep_strings` (their quotes stay). Lines stay where they were.
pub(super) fn code_only(text: &str, keep_strings: bool) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let blank = |ch: char| if ch == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < c.len() {
        let at = |k: usize| c.get(i + k).copied().unwrap_or('\0');
        let literal_end = if at(0) == '/' && at(1) == '/' {
            let end = c[i..].iter().position(|&ch| ch == '\n').map_or(c.len(), |p| i + p);
            out.extend(c[i..end].iter().map(|&ch| blank(ch)));
            i = end;
            continue;
        } else if at(0) == '/' && at(1) == '*' {
            let end = block_comment_end(&c, i);
            out.extend(c[i..end].iter().map(|&ch| blank(ch)));
            i = end;
            continue;
        } else if let Some(end) = raw_string_end(&c, i) {
            Some(end)
        } else if at(0) == '"' {
            Some(string_end(&c, i))
        } else if at(0) == '\'' {
            char_end(&c, i)
        } else {
            None
        };
        match literal_end {
            Some(end) => {
                out.extend(c[i..end].iter().enumerate().map(|(k, &ch)| if keep_strings || k == 0 || k == end - i - 1 { ch } else { blank(ch) }));
                i = end;
            }
            None => {
                out.push(c[i]);
                i += 1;
            }
        }
    }
    out
}

/// Past the `/* … */` starting at `i` (nested ones count, as in Rust).
fn block_comment_end(c: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < c.len() {
        if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    c.len()
}

/// Past the `"…"` starting at `i`, escapes included.
fn string_end(c: &[char], mut i: usize) -> usize {
    i += 1;
    while i < c.len() {
        match c[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    c.len()
}

/// Past a raw string (`r"…"`, `r#"…"#`, `br"…"`) starting at `i`, if one does.
fn raw_string_end(c: &[char], i: usize) -> Option<usize> {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    if i > 0 && ident(c[i - 1]) {
        return None;
    }
    let mut j = i + usize::from(c[i] == 'b');
    if c.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let hashes = c[j..].iter().take_while(|&&ch| ch == '#').count();
    j += hashes;
    if c.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < c.len() {
        if c[j] == '"' && c[j + 1..].iter().take(hashes).filter(|&&ch| ch == '#').count() == hashes {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(c.len())
}

/// Past a char literal starting at `i` (`'x'`, `'\n'`, `'\''`); None for a lifetime (`'a`).
fn char_end(c: &[char], i: usize) -> Option<usize> {
    if c.get(i + 1) == Some(&'\\') {
        return c[i + 3..].iter().position(|&ch| ch == '\'').map(|p| i + 3 + p + 1);
    }
    (c.get(i + 2) == Some(&'\'')).then_some(i + 3)
}

/// For each line of `lines`, whether it's test code: an item under `#[cfg(test)]` (on the line before it
/// or the same line), through its closing brace.
pub(super) fn test_only(lines: &[&str]) -> Vec<bool> {
    const ATTR: &str = "#[cfg(test)]";
    let mut test = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if !t.starts_with(ATTR) {
            i += 1;
            continue;
        }
        let item_here = !t[ATTR.len()..].trim().is_empty();
        let mut depth = 0i32;
        let mut j = i;
        while j < lines.len() {
            test[j] = true;
            depth += lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
            // The item ends where its braces close, or on its own line when it has none (a `static`).
            if (j > i || item_here) && depth <= 0 && (lines[j].contains('}') || lines[j].trim_end().ends_with(';')) {
                break;
            }
            j += 1;
        }
        i = j + 1;
    }
    test
}

#[test]
fn comments_go_and_strings_stay_or_go() {
    let text = "a.lock(); // b.lock()\n/* c.lock() /* nested */ d.lock() */ e(\"f.lock() // not a comment\", '\"', 'x, r#\"g\"h\"#);\nlet s = 1; /* open\nstill.lock() */ x.lock();";
    let kept = code_only(text, true);
    let lines: Vec<&str> = kept.lines().collect();
    assert_eq!(lines[0].trim_end(), "a.lock();");
    assert!(lines[1].trim_start().starts_with("e(\"f.lock() // not a comment\", '\"', 'x, r#\"g\"h\"#);"), "{}", lines[1]);
    assert_eq!(lines[2].trim_end(), "let s = 1;");
    assert_eq!(lines[3].trim(), "x.lock();", "a comment over two lines, with code after it");
    let blanked = code_only(text, false);
    assert!(!blanked.lines().nth(1).unwrap().contains("f.lock()"), "a string's text is blanked");
    assert_eq!(blanked.lines().count(), 4);
}

#[test]
fn test_items_are_found_on_their_own_line_or_after_the_attribute() {
    let lines = ["fn a() {}", "#[cfg(test)]", "mod t {", "  x();", "}", "#[cfg(test)] fn one() { y(); }", "fn b() {}", "#[cfg(test)]", "static S: u8 = 0;", "fn c() {}"];
    assert_eq!(test_only(&lines), [false, true, true, true, true, true, false, true, true, false]);
}
