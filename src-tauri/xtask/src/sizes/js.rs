//! JavaScript function lengths, by a brace-aware scan: comments, strings, template text and regex
//! literals blanked first (template `${…}` kept as code), braces matched, and each `{` that opens a
//! function body told by what's before it: `function name(…)`, `name(…)` (a method), or `… => `.
//! Named from the `name =`/`name:` it's assigned to, else "(anonymous)". Not a parser: a regex literal
//! after an unusual token could be read as division; the counts are for finding long functions.

use super::js_lex::blank_non_code;
use super::Found;

pub(super) fn functions(text: &str) -> Vec<Found> {
    let code = blank_non_code(text);
    let line_of = line_index(&code);
    let mut out = vec![];
    for (open, close) in brace_pairs(&code) {
        if let Some((start, name)) = function_head(&code, open) {
            let (first, last) = (line_of[start], line_of[close]);
            out.push(Found { line: first, name, lines: last + 1 - first, test: false });
        }
    }
    out.sort_by_key(|f| f.line);
    out
}

/// For each char, its line (from 1).
fn line_index(code: &[char]) -> Vec<usize> {
    let mut line = 1;
    code.iter().map(|&c| {
        let at = line;
        if c == '\n' {
            line += 1;
        }
        at
    }).collect()
}

/// Every `{` with its matching `}`.
fn brace_pairs(code: &[char]) -> Vec<(usize, usize)> {
    let mut open = vec![];
    let mut pairs = vec![];
    for (i, &c) in code.iter().enumerate() {
        match c {
            '{' => open.push(i),
            '}' => {
                if let Some(o) = open.pop() {
                    pairs.push((o, i));
                }
            }
            _ => {}
        }
    }
    pairs
}

const NOT_FUNCTIONS: &[&str] = &["if", "for", "while", "switch", "catch", "with", "return", "typeof"];

/// Whether the `{` at `open` opens a function body: where its head starts, and its name.
fn function_head(code: &[char], open: usize) -> Option<(usize, String)> {
    let before = skip_space_back(code, open);
    if before >= 2 && code[before - 2] == '=' && code[before - 1] == '>' {
        let params_end = skip_space_back(code, before - 2);
        let params_start = if code.get(params_end.wrapping_sub(1)) == Some(&')') { matching_open(code, params_end - 1)? } else { word_back(code, params_end).0 };
        let (start, name) = assigned_name(code, strip_async(code, params_start));
        return Some((start, name));
    }
    if before == 0 || code[before - 1] != ')' {
        return None;
    }
    let paren = matching_open(code, before - 1)?;
    let (word_start, word) = word_back(code, skip_space_back(code, paren));
    match word.as_str() {
        w if NOT_FUNCTIONS.contains(&w) => None,
        "function" => Some(assigned_name(code, strip_async(code, word_start))),
        "" => None,
        name => {
            let (kw_start, kw) = word_back(code, skip_space_back(code, word_start));
            let start = if kw == "function" || kw == "async" { kw_start } else { word_start };
            Some((start, name.to_string()))
        }
    }
}

/// The name a function expression starting at `start` is given (`name = …`, `name: …`), with where
/// that name starts; "(anonymous)" otherwise.
fn assigned_name(code: &[char], start: usize) -> (usize, String) {
    let b = skip_space_back(code, start);
    if b > 0 && (code[b - 1] == ':' || (code[b - 1] == '=' && !matches!(code.get(b.wrapping_sub(2)), Some('=' | '!' | '<' | '>')))) {
        let (name_start, name) = word_back(code, skip_space_back(code, b - 1));
        if !name.is_empty() {
            return (name_start, name);
        }
    }
    (start, "(anonymous)".into())
}

fn strip_async(code: &[char], at: usize) -> usize {
    let (s, w) = word_back(code, skip_space_back(code, at));
    if w == "async" { s } else { at }
}

pub(super) fn skip_space_back(code: &[char], mut i: usize) -> usize {
    while i > 0 && code[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

/// The identifier ending at `end` (exclusive), with where it starts; empty when there's none.
pub(super) fn word_back(code: &[char], end: usize) -> (usize, String) {
    let mut s = end;
    while s > 0 && (code[s - 1].is_alphanumeric() || code[s - 1] == '_' || code[s - 1] == '$') {
        s -= 1;
    }
    (s, code[s..end].iter().collect())
}

/// The `(` that the `)` at `close` closes.
fn matching_open(code: &[char], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        match code[i] {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    fn found(text: &str) -> Vec<(usize, String, usize)> {
        super::functions(text).into_iter().map(|f| (f.line, f.name, f.lines)).collect()
    }

    #[test]
    fn declarations_methods_and_arrows_are_measured_and_named() {
        let text = "function a(x) {\n  if (x) {\n  }\n}\nconst b = async (y) => {\n  s = '}' + `${ {k: 1}.k }` + /}/.source;\n};\nconst o = {\n  m(z) {\n  },\n};\nlist.map(e => {\n});\n";
        assert_eq!(found(text), [(1, "a".into(), 4), (5, "b".into(), 3), (9, "m".into(), 2), (12, "(anonymous)".into(), 2)]);
    }
}
