//! Does your message to a Desk's lead ask it to unpark item `id` (UNPARK, #499, #853)? Your own words, typed or said:
//! "unpark #810", "un-park 810", "bring eight one zero back", "unpark eight ten", or "unpark it" when the lead's reply
//! before was about that item. A message that names the item without asking for it back, or asks without naming it,
//! isn't one.

/// Words that ask for an item back on their own ("start" and "resume" too: with the item named, that's the ask).
const ASKS: &[&str] = &["unpark", "unparked", "unparking", "unhold", "restore", "resume", "start", "reopen"];

/// Speech-to-text's "unpark": "un park", "on park" (read as two words).
const ASK_PAIRS: &[(&str, &str)] = &[("un", "park"), ("on", "park"), ("un", "hold"), ("take", "off"), ("out", "of")];

/// Words that stand for the item the lead was just talking about.
const POINTERS: &[&str] = &["it", "that", "this"];

/// Whether `message` asks for item `id` back. `before`: the lead's reply your message answers, if any (for "it").
/// `parked`: the ids of every parked item, so "it" counts only when that reply was about this one parked item.
pub(crate) fn asks_to_unpark(message: &str, id: &str, before: Option<&str>, parked: &[String]) -> bool {
    let words = words(message);
    if !asks(&words) {
        return false;
    }
    let named = numbers(&words);
    if named.iter().any(|n| n == id) {
        return true;
    }
    // "unpark it": no item named, and the lead's reply before named this parked item and no other.
    named.is_empty()
        && words.iter().any(|w| POINTERS.contains(&w.as_str()))
        && before.is_some_and(|b| {
            let about: Vec<String> = mentioned(b).into_iter().filter(|m| parked.contains(m)).collect();
            !about.is_empty() && about.iter().all(|m| m == id)
        })
}

/// `text` in lower-case words: letters and digits, "#810" kept as "810", "un-park" split as "un" "park".
fn words(text: &str) -> Vec<String> {
    let lower = text.to_lowercase().replace("un-park", "unpark").replace("un-hold", "unhold");
    lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

fn asks(words: &[String]) -> bool {
    let has = |w: &str| words.iter().any(|x| x == w);
    words.iter().any(|w| ASKS.contains(&w.as_str()))
        || words.windows(2).any(|p| ASK_PAIRS.iter().any(|(a, b)| p[0] == *a && p[1] == *b) && (p[1] != "of" || has("park") || has("parked")))
        // "bring #810 back", "bring it back", "put it back", "back in".
        || (has("back") && (has("bring") || has("put") || has("get") || has("want")))
        || words.windows(2).any(|p| p[0] == "back" && (p[1] == "in" || p[1] == "on"))
}

/// The item numbers in `words`: "810" as written, and spoken numbers read either way, "eight one zero" and "eight ten"
/// as 810, "eight hundred and ten" as 810. A run of number words gives the whole run read together, and each word alone.
fn numbers(words: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut run: Vec<&str> = vec![];
    for w in words.iter().map(String::as_str).chain(std::iter::once("")) {
        if is_number(w) || (w == "and" && !run.is_empty() && run.last() == Some(&"hundred")) {
            run.push(w);
            continue;
        }
        if !run.is_empty() {
            out.extend(read_run(&run));
            run.clear();
        }
    }
    out.sort();
    out.dedup();
    out
}

fn is_number(w: &str) -> bool {
    w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty() || ones(w).is_some() || teen(w).is_some() || tens(w).is_some() || w == "hundred"
}

fn ones(w: &str) -> Option<u32> {
    ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"].iter().position(|x| *x == w).map(|i| i as u32).or((w == "oh").then_some(0))
}

fn teen(w: &str) -> Option<u32> {
    ["ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"].iter().position(|x| *x == w).map(|i| 10 + i as u32)
}

fn tens(w: &str) -> Option<u32> {
    ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"].iter().position(|x| *x == w).map(|i| 20 + 10 * i as u32)
}

/// A run of number words and digits: read as groups put side by side ("eight" "ten" → 810, "twelve" "thirty four" →
/// 1234), a "hundred" taking the group before it ("eight hundred and ten" → 810); and each word alone.
fn read_run(run: &[&str]) -> Vec<String> {
    let mut alone: Vec<String> = run.iter().filter_map(|w| group_of(w)).collect();
    let mut groups: Vec<String> = vec![];
    let mut i = 0;
    while i < run.len() {
        let w = run[i];
        if w == "and" {
            i += 1;
            continue;
        }
        if w == "hundred" {
            // "eight hundred" + what's under a hundred after it.
            let lead = groups.pop().and_then(|g| g.parse::<u32>().ok()).unwrap_or(1);
            let (rest, used) = under_hundred(&run[i + 1..]);
            groups.push((lead * 100 + rest).to_string());
            i += 1 + used;
            continue;
        }
        if let Some(t) = tens(w) {
            // "thirty four" → 34.
            if let Some(o) = run.get(i + 1).and_then(|n| ones(n)).filter(|o| *o > 0) {
                groups.push((t + o).to_string());
                i += 2;
                continue;
            }
        }
        if let Some(g) = group_of(w) {
            groups.push(g);
        }
        i += 1;
    }
    alone.push(groups.concat());
    alone
}

/// What's under a hundred at the start of `rest` ("and ten", "fifty two", "seven"): its value and the words it took.
fn under_hundred(rest: &[&str]) -> (u32, usize) {
    let skip = usize::from(rest.first() == Some(&"and"));
    let r = &rest[skip..];
    match r.first().copied() {
        Some(w) if teen(w).is_some() => (teen(w).unwrap(), skip + 1),
        Some(w) if tens(w).is_some() => match r.get(1).and_then(|n| ones(n)).filter(|o| *o > 0) {
            Some(o) => (tens(w).unwrap() + o, skip + 2),
            None => (tens(w).unwrap(), skip + 1),
        },
        Some(w) if ones(w).is_some() => (ones(w).unwrap(), skip + 1),
        _ => (0, 0),
    }
}

/// One word's digits: "810" → "810", "eight" → "8", "ten" → "10", "thirty" → "30".
fn group_of(w: &str) -> Option<String> {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()) {
        return Some(w.to_string());
    }
    ones(w).or_else(|| teen(w)).or_else(|| tens(w)).map(|n| n.to_string())
}

/// The items `text` names as "#810".
fn mentioned(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split('#')
        .skip(1)
        .map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>())
        .filter(|n| !n.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::asks_to_unpark;

    fn ask(m: &str) -> bool {
        asks_to_unpark(m, "810", None, &["810".into()])
    }

    #[test]
    fn typed_with_a_hash_or_a_plain_number() {
        assert!(ask("unpark #810"));
        assert!(ask("Unpark 810 please"));
        assert!(ask("un-park #810, I want it now"));
        assert!(ask("Can you bring #810 back?"));
        assert!(ask("start 810 now"));
    }

    #[test]
    fn said_by_voice() {
        assert!(ask("Unpark eight one zero."));
        assert!(ask("unpark eight ten"));
        assert!(ask("Un park eight hundred and ten"));
        assert!(ask("on park eight hundred ten"));
        assert!(ask("bring eight one zero back"));
        assert!(ask("Unpark 8 10."));
    }

    #[test]
    fn it_when_the_reply_before_was_about_that_item() {
        let parked = ["810".to_string(), "812".to_string()];
        let before = "#810 Dark sidebar is parked; #900 is at work.";
        assert!(asks_to_unpark("unpark it", "810", Some(before), &parked));
        assert!(asks_to_unpark("OK, bring it back", "810", Some(before), &parked));
        // The reply was about two parked items: "it" is no one of them.
        assert!(!asks_to_unpark("unpark it", "810", Some("#810 and #812 are parked."), &parked));
        assert!(!asks_to_unpark("unpark it", "810", Some("#812 is parked."), &parked));
        assert!(!asks_to_unpark("unpark it", "810", None, &parked));
    }

    #[test]
    fn not_without_asking_or_naming_it() {
        assert!(!ask("How is #810 doing?"));
        assert!(!ask("How is the Desk doing?"));
        assert!(!ask("unpark #811"));
        assert!(!ask("unpark eight one one"));
        assert!(!ask("unpark it"));
        assert!(!ask("Leave 810 parked"));
    }
}
