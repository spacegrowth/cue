//! Blanking what isn't code in JavaScript, so braces and parentheses can be matched: comments, string
//! and template text, regex literals (told from division by the token before the `/`).

use super::js::{skip_space_back, word_back};

/// `text` with comments, string, template and regex text blanked to spaces (newlines kept), and
/// template `${…}` kept as code.
pub(super) fn blank_non_code(text: &str) -> Vec<char> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(c.len());
    // Brace depth inside each open `${`: a `}` at depth 0 goes back to the template's text.
    let mut templates: Vec<usize> = vec![];
    let mut i = 0;
    while i < c.len() {
        let blank_to = |out: &mut Vec<char>, end: usize| out.extend(c[out.len()..end].iter().map(|&ch| if ch == '\n' { '\n' } else { ' ' }));
        let at = |k: usize| c.get(i + k).copied().unwrap_or('\0');
        let end = match at(0) {
            '/' if at(1) == '/' => c[i..].iter().position(|&ch| ch == '\n').map_or(c.len(), |p| i + p),
            '/' if at(1) == '*' => c[i + 2..].windows(2).position(|w| w == ['*', '/']).map_or(c.len(), |p| i + 2 + p + 2),
            '\'' | '"' => quoted_end(&c, i, at(0)),
            '/' if regex_can_start(&out) => regex_end(&c, i),
            '`' => template_text_end(&c, i + 1, &mut templates),
            '}' if templates.last() == Some(&0) => {
                templates.pop();
                template_text_end(&c, i + 1, &mut templates)
            }
            ch => {
                if let Some(d) = templates.last_mut() {
                    match ch {
                        '{' => *d += 1,
                        '}' => *d -= 1,
                        _ => {}
                    }
                }
                out.push(ch);
                i += 1;
                continue;
            }
        };
        blank_to(&mut out, end);
        i = end;
    }
    out
}

/// Past a template's text from `i` to its closing backtick, or to a `${` (which opens code, recorded
/// in `templates`).
fn template_text_end(c: &[char], mut i: usize, templates: &mut Vec<usize>) -> usize {
    while i < c.len() {
        match c[i] {
            '\\' => i += 2,
            '`' => return i + 1,
            '$' if c.get(i + 1) == Some(&'{') => {
                templates.push(0);
                return i + 2;
            }
            _ => i += 1,
        }
    }
    c.len()
}

fn quoted_end(c: &[char], mut i: usize, quote: char) -> usize {
    i += 1;
    while i < c.len() && c[i] != '\n' {
        match c[i] {
            '\\' => i += 2,
            q if q == quote => return i + 1,
            _ => i += 1,
        }
    }
    i.min(c.len())
}

/// A `/` starts a regex after an operator or opening bracket, or after `return`/`typeof`, not after a value.
fn regex_can_start(code: &[char]) -> bool {
    let b = skip_space_back(code, code.len());
    match b.checked_sub(1).map(|k| code[k]) {
        None => true,
        Some(p) if "(,=:[!&|?{};+-*%<>~^".contains(p) => true,
        Some(_) => matches!(word_back(code, b).1.as_str(), "return" | "typeof" | "case"),
    }
}

fn regex_end(c: &[char], mut i: usize) -> usize {
    i += 1;
    let mut class = false;
    while i < c.len() && c[i] != '\n' {
        match c[i] {
            '\\' => i += 1,
            '[' => class = true,
            ']' => class = false,
            '/' if !class => return i + 1,
            _ => {}
        }
        i += 1;
    }
    i.min(c.len())
}
