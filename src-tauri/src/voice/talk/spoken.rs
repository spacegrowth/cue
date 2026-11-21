//! Hands-free: which session you named at the start of what you said ("ask four ninety-seven…"), from the
//! words and numbers heard, and the rest that goes to it.

use super::target;
use serde_json::Value;

/// A word as heard, or a number (said as digits or words: "497", "four ninety-seven", "four nine seven" each come
/// as a run of numbers that reads 497 put together), with the word of what was said it came from.
#[derive(Debug, Clone, PartialEq)]
enum Heard {
    Word(String),
    Num(String),
}

/// `text` as words and numbers, each with the index of the word (split on spaces) it came from.
fn heard_words(text: &str) -> Vec<(Heard, usize)> {
    const UNITS: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
    const TEENS: [&str; 10] = ["ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
    const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    let unit = |w: &str| UNITS.iter().position(|u| *u == w);
    let teen = |w: &str| TEENS.iter().position(|u| *u == w).map(|n| n + 10);
    let ten = |w: &str| TENS.iter().position(|u| *u == w).map(|n| n * 10 + 20);
    // Pieces: lower case, split at - · / and the like, nothing but letters and digits.
    let pieces: Vec<(String, usize)> = text
        .split_whitespace()
        .enumerate()
        .flat_map(|(i, w)| w.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()).map(move |p| (p.to_lowercase(), i)).collect::<Vec<_>>())
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        let (w, at) = (&pieces[i].0, pieces[i].1);
        let next = |k: usize| pieces.get(i + k).map(|p| p.0.as_str()).unwrap_or("");
        let (tok, used) = if w.chars().all(|c| c.is_ascii_digit()) {
            (Heard::Num(w.clone()), 1)
        } else if let Some(t) = ten(w) {
            match unit(next(1)).filter(|u| *u > 0) {
                Some(u) => (Heard::Num((t + u).to_string()), 2),
                None => (Heard::Num(t.to_string()), 1),
            }
        } else if let Some(t) = teen(w) {
            (Heard::Num(t.to_string()), 1)
        } else if let Some(u) = unit(w) {
            if next(1) == "hundred" {
                // "four hundred (and) ninety seven"
                let mut k = 2 + usize::from(next(2) == "and");
                let rest = if let Some(t) = ten(next(k)) {
                    k += 1;
                    match unit(next(k)).filter(|u| *u > 0) {
                        Some(u) => { k += 1; t + u }
                        None => t,
                    }
                } else if let Some(t) = teen(next(k)).or_else(|| unit(next(k))) {
                    k += 1;
                    t
                } else {
                    k = 2;
                    0
                };
                (Heard::Num((u * 100 + rest).to_string()), k)
            } else {
                (Heard::Num(u.to_string()), 1)
            }
        } else {
            (Heard::Word(w.clone()), 1)
        };
        out.push((tok, pieces[i + used - 1].1.max(at)));
        i += used;
    }
    out
}

/// Where `key` (a session's name as heard) ends when it's said at `from` in `said`: a number in it may be said
/// as a run of numbers that reads it put together ("4", "97" for 497).
fn said_at(said: &[(Heard, usize)], from: usize, key: &[Heard]) -> Option<usize> {
    let mut j = from;
    for k in key {
        match k {
            Heard::Word(w) => {
                if said.get(j).map(|s| &s.0) != Some(&Heard::Word(w.clone())) {
                    return None;
                }
                j += 1;
            }
            Heard::Num(n) => {
                let mut acc = String::new();
                while acc.len() < n.len() {
                    let Some((Heard::Num(d), _)) = said.get(j) else { break };
                    acc.push_str(d);
                    j += 1;
                }
                if acc != *n {
                    return None;
                }
            }
        }
    }
    (j > from).then_some(j)
}

/// Hands-free: the session you named at the start of what you said ("ask four ninety-seven how far along it is",
/// "tell cue 497 to run the tests"), and what goes to it, the rest ("How far along it is", "Run the tests"). A
/// session is named by its name or its folder, or by the number in either. No name heard: the chat open in Cue,
/// else the session that moved last, with all you said. None when no session runs.
pub fn spoken_pick(snap: &Value, open: Option<&str>, said: &str) -> Option<(String, String)> {
    const LEAD: [&str; 10] = ["ask", "tell", "to", "for", "with", "hey", "please", "ok", "okay", "session"];
    let heard = heard_words(said);
    let from = heard.iter().take_while(|(h, _)| matches!(h, Heard::Word(w) if LEAD.contains(&w.as_str()))).count();
    let running = snap["sessions"].as_array().into_iter().flatten().filter(|s| s["state"] != "stopped");
    // The longest name said; the same length, the session that moved last.
    let mut best: Option<(usize, u64, String)> = None;
    for s in running {
        let names = [s["name"].as_str().map(|n| n.trim_start_matches("[Exec] ").trim_start_matches("[Lead] ")), s["cwd"].as_str().and_then(|c| c.rsplit('/').find(|p| !p.is_empty()))];
        let mut keys: Vec<Vec<Heard>> = Vec::new();
        for n in names.into_iter().flatten() {
            let k: Vec<Heard> = heard_words(n).into_iter().map(|h| h.0).collect();
            keys.extend(k.iter().filter(|h| matches!(h, Heard::Num(_))).map(|h| vec![h.clone()]));
            if !k.is_empty() {
                keys.push(k);
            }
        }
        let since = s["since_ms"].as_u64().unwrap_or(0);
        for k in &keys {
            if let Some(end) = said_at(&heard, from, k) {
                if best.as_ref().is_none_or(|b| end > b.0 || (end == b.0 && since > b.1)) {
                    best = Some((end, since, s["session_id"].as_str().unwrap_or("").to_string()));
                }
            }
        }
    }
    let Some((end, _, sid)) = best else {
        let sid = target(snap, open)["sid"].as_str()?.to_string();
        return Some((sid, said.trim().to_string()));
    };
    // The rest, from the word after the name; "to" or "that" after it goes too ("tell 497 to run the tests").
    let words: Vec<&str> = said.split_whitespace().collect();
    let mut rest = &words[(heard[end - 1].1 + 1).min(words.len())..];
    while let Some(w) = rest.first().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()) {
        if w.is_empty() || w == "to" || w == "that" {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    let msg = rest.join(" ").trim_start_matches(|c: char| c == ',' || c == ':' || c.is_whitespace()).to_string();
    let mut c = msg.chars();
    let msg = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
    Some((sid, msg))
}
