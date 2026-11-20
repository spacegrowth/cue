//! Who uses Cue: the Mac account's first name, read at run time (never written down anywhere). The screen shows it
//! wherever Cue's words, or a lead's, say "the owner" ("you" when the account has no name), and the Desk lead and
//! the sessions are told it, so what they write uses it.

use serde_json::Value;
use std::sync::OnceLock;

/// The Mac account's first name: the first word of its full name. None when it has none.
pub(crate) fn first_name() -> Option<String> {
    #[cfg(test)]
    {
        return test_name::get();
    }
    #[allow(unreachable_code)]
    {
        static NAME: OnceLock<Option<String>> = OnceLock::new();
        NAME.get_or_init(|| first_word(&full_name())).clone()
    }
}

/// The account's full name ("" when none): the user database's name field.
fn full_name() -> String {
    // SAFETY: getpwuid returns a pointer into a static record (or null); read once, at the first ask.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_gecos.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().into_owned()
    }
}

/// The first word of a full name, when it has a letter (a name field can hold commas and numbers).
fn first_word(full: &str) -> Option<String> {
    let w = full.split(|c: char| c.is_whitespace() || c == ',').find(|w| !w.is_empty())?;
    w.chars().any(char::is_alphabetic).then(|| w.to_string())
}

/// `text` as the screen shows it: "the owner" is your first name, or "you" without one.
pub(crate) fn on_screen(text: &str) -> String {
    said(text, first_name().as_deref())
}

/// `text` for a lead or a session: "the owner" is your first name when there is one, else as it was.
pub(crate) fn in_prompt(text: &str) -> String {
    match first_name() {
        Some(n) => said(text, Some(&n)),
        None => text.to_string(),
    }
}

/// The line that tells a lead or a session your name ("" without one).
pub(crate) fn told_line() -> String {
    first_name().map_or(String::new(), |n| format!("The user is {n}. In everything you write for them or about them (items, DECIDED and ASK lines, notes, replies), call them {n}, or \"you\" when speaking to them; never \"the owner\" or \"the user\".\n"))
}

/// Every string in `v` as the screen shows it (on_screen).
pub(crate) fn on_screen_json(v: &mut Value) {
    let name = first_name();
    walk(v, name.as_deref());
}

fn walk(v: &mut Value, name: Option<&str>) {
    match v {
        Value::String(s) if s.contains("he owner") => *s = said(s, name),
        Value::Array(a) => a.iter_mut().for_each(|x| walk(x, name)),
        Value::Object(o) => o.values_mut().for_each(|x| walk(x, name)),
        _ => {}
    }
}

/// `text` with each "the owner" / "The owner" (a whole phrase, not "the owners") as `name`, or as "you" without one:
/// "the owner's" is "your", "the owner is" "you are", and so on.
pub(crate) fn said(text: &str, name: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let at = [rest.find("the owner"), rest.find("The owner")].into_iter().flatten().min();
        let Some(at) = at else {
            out.push_str(rest);
            return out;
        };
        let (before, after) = (&rest[..at], &rest[at + "the owner".len()..]);
        out.push_str(before);
        let starts = before.chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let ends = after.chars().next().is_none_or(|c| !c.is_alphanumeric());
        if !(starts && ends) {
            out.push_str(&rest[at..at + "the owner".len()]);
            rest = after;
            continue;
        }
        let cap = rest[at..].starts_with('T') || out.trim_end().is_empty() || out.trim_end().ends_with(['.', '!', '?']);
        rest = match name {
            Some(n) => {
                out.push_str(n);
                after
            }
            None => {
                let (word, left) = you_form(after);
                out.push_str(&if cap { capital(word) } else { word.to_string() });
                left
            }
        };
    }
}

/// "you" and what follows "the owner" (`after`), fixed to go with it: (the words, what's left).
fn you_form(after: &str) -> (&'static str, &str) {
    for (from, to) in [("'s", "your"), ("’s", "your"), (" is", "you are"), (" was", "you were"), (" has", "you have"), (" doesn't", "you don't"), (" wants", "you want"), (" needs", "you need"), (" asks", "you ask"), (" says", "you say")] {
        if let Some(left) = after.strip_prefix(from) {
            if left.chars().next().is_none_or(|c| !c.is_alphanumeric()) {
                return (to, left);
            }
        }
    }
    ("you", after)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// In tests, the name is one a test sets (none by default): never the account's.
#[cfg(test)]
pub(crate) mod test_name {
    use std::cell::RefCell;
    thread_local!(static NAME: RefCell<Option<String>> = const { RefCell::new(None) });
    pub(crate) fn get() -> Option<String> {
        NAME.with(|n| n.borrow().clone())
    }
    pub(crate) fn set(name: Option<&str>) {
        NAME.with(|n| *n.borrow_mut() = name.map(str::to_string));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_owner_is_your_name() {
        let n = Some("Robin");
        assert_eq!(said("Waits for the owner.", n), "Waits for Robin.");
        assert_eq!(said("The owner is away; the owner's call.", n), "Robin is away; Robin's call.");
        assert_eq!(said("the owners and the ownership stay", n), "the owners and the ownership stay");
        assert_eq!(said("nothing here", n), "nothing here");
    }

    #[test]
    fn without_a_name_it_is_you() {
        assert_eq!(said("Waits for the owner.", None), "Waits for you.");
        assert_eq!(said("The owner is away until 9.", None), "You are away until 9.");
        assert_eq!(said("Held for the owner's answer. The owner has it.", None), "Held for your answer. You have it.");
        assert_eq!(said("Paused: the owner asked", None), "Paused: you asked");
        assert_eq!(said("ask the owner isolated", None), "ask you isolated");
        assert_eq!(said("the owner asked", None), "You asked", "a sentence starts with it");
    }

    #[test]
    fn a_full_name_gives_its_first_word() {
        assert_eq!(first_word("Robin Q Example"), Some("Robin".into()));
        assert_eq!(first_word("  Robin,,,"), Some("Robin".into()));
        assert_eq!(first_word(""), None);
        assert_eq!(first_word("1234"), None);
    }

    #[test]
    fn the_screen_and_the_lead_get_it() {
        test_name::set(Some("Robin"));
        let mut v = json!({ "items": [{ "title": "Ask the owner", "n": 3 }], "log": ["The owner was away"] });
        on_screen_json(&mut v);
        assert_eq!(v, json!({ "items": [{ "title": "Ask Robin", "n": 3 }], "log": ["Robin was away"] }));
        assert_eq!(in_prompt("waits for the owner"), "waits for Robin");
        assert!(told_line().contains("call them Robin"));
        test_name::set(None);
        assert_eq!(in_prompt("waits for the owner"), "waits for the owner");
        assert_eq!(on_screen("waits for the owner"), "waits for you");
        assert_eq!(told_line(), "");
    }
}
