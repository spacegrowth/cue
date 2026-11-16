//! The Desk's lines in a reply, read at its turn's end (as crew_log's DECIDED lines are): the lead's ITEM, ROUTE, ASK
//! and CONTEXT, and a holder's DONE. Each is a line of its own, its word in capitals; a list mark or bold around it is fine;
//! a line in a code block (``` or ~~~) or a `>` quote only shows one, so it's left alone. DECIDED lines are
//! crew_log's `calls_in`.

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A new item.
    Item { kind: String, title: String, done_when: String, why: String },
    /// Item `id` ("new": the reply's last ITEM) to the session named `to`, now or `after` another item; `kind`, when
    /// said: the lead's call on what it is ("bug" | "feature" | "task"; an item you add has none till then); `crew`,
    /// when said: the lead's teammate doing it (with `to` "me").
    Route { id: String, to: String, after: Option<String>, why: String, kind: Option<String>, crew: Option<String> },
    /// A question for you, with its choices (none: answered in words).
    Ask { question: String, choices: Vec<String> },
    /// Items done, as their holder says.
    Done { ids: Vec<String> },
    /// Item `id`'s designs are at `url` (its canvas), as any session of its project says; a newer one replaces it.
    Designs { id: String, url: String },
    /// What to do with session `who`'s context ("me": the lead's own): `what` is "keep" (others to come), lower case.
    Context { who: String, what: String, why: String },
}

/// The Desk lines in `text`, in order.
pub fn lines(text: &str) -> Vec<Line> {
    let mut fenced = false;
    text.lines()
        .filter_map(|line| {
            let t = line.trim();
            if t.starts_with("```") || t.starts_with("~~~") {
                fenced = !fenced;
                return None;
            }
            if fenced || t.starts_with('>') {
                return None;
            }
            // List marks and bold only: a line in backticks (`ITEM: …`) is an example, not a line to act on.
            let l = t.trim_start_matches(['-', '*', '+', ' ']).trim();
            let (word, body) = l.split_once(':')?;
            let body = body.trim().trim_start_matches("**").trim().trim_end_matches('*').trim();
            if body.is_empty() || body.starts_with('<') {
                return None;
            }
            match word.trim_end_matches('*').trim() {
                "ITEM" => item(body),
                "ROUTE" => route(body),
                "ASK" => ask(body),
                "DONE" => done(body),
                "CONTEXT" => context(body),
                "DESIGNS" => designs(body),
                _ => None,
            }
        })
        .collect()
}

/// The parts of `body` between `|`, trimmed.
fn parts(body: &str) -> Vec<&str> {
    body.split('|').map(str::trim).collect()
}

/// `part` without its label ("done when:", any case), if it has it.
fn labelled<'a>(part: &'a str, label: &str) -> Option<&'a str> {
    let head = part.get(..label.len())?;
    head.eq_ignore_ascii_case(label).then(|| part[label.len()..].trim_start().trim_start_matches(':').trim())
}

/// An item id as written ("#d7k2", "d7k2."), lower case.
fn id_of(s: &str) -> Option<String> {
    let id: String = s.trim().trim_start_matches('#').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!id.is_empty()).then(|| id.to_ascii_lowercase())
}

fn item(body: &str) -> Option<Line> {
    let p = parts(body);
    let first = p.first()?.to_ascii_lowercase();
    let (kind, rest) = match first.as_str() {
        "bug" | "feature" | "task" => (first.clone(), &p[1..]),
        _ => ("task".to_string(), &p[..]),
    };
    // "why:" with its colon, so a title that starts "Why …" stays the title.
    fn why_of(s: &str) -> Option<&str> {
        labelled(s, "why").filter(|_| s[3..].trim_start().starts_with(':'))
    }
    let title = rest.iter().find(|s| labelled(s, "done when").is_none() && why_of(s).is_none()).map(|s| s.to_string()).unwrap_or_default();
    let done_when = rest.iter().find_map(|s| labelled(s, "done when")).unwrap_or("").to_string();
    let why = rest.iter().find_map(|s| why_of(s)).unwrap_or("").to_string();
    (!title.is_empty()).then_some(Line::Item { kind, title, done_when, why })
}

fn route(body: &str) -> Option<Line> {
    let p = parts(body);
    let head = p.first()?;
    let (id, to) = ["->", "→", "=>"].iter().find_map(|a| head.split_once(a))?;
    let id = id_of(id)?;
    let to = to.trim().trim_start_matches('@').trim_matches(['"', '`', '*']).trim().to_string();
    if to.is_empty() {
        return None;
    }
    let after = p[1..].iter().find_map(|s| labelled(s, "after")).and_then(id_of);
    let why = p[1..].iter().find_map(|s| labelled(s, "why")).unwrap_or("").to_string();
    let kind = p[1..].iter().find_map(|s| labelled(s, "kind")).map(|k| k.trim_matches(['"', '`', '*', '.']).to_ascii_lowercase()).filter(|k| matches!(k.as_str(), "bug" | "feature" | "task"));
    let crew = p[1..].iter().find_map(|s| labelled(s, "crew")).map(|c| c.trim_start_matches('@').trim_matches(['"', '`', '*']).trim().to_string()).filter(|c| !c.is_empty());
    Some(Line::Route { id, to, after, why, kind, crew })
}

fn ask(body: &str) -> Option<Line> {
    let p = parts(body);
    let question = p.first()?.to_string();
    let choices = p[1..]
        .iter()
        .find_map(|s| labelled(s, "choices"))
        .map(|c| c.split('/').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();
    (!question.is_empty()).then_some(Line::Ask { question, choices })
}

fn done(body: &str) -> Option<Line> {
    let ids: Vec<String> = body.split([',', ' ']).filter(|w| w.trim().starts_with('#')).filter_map(id_of).collect();
    (!ids.is_empty()).then_some(Line::Done { ids })
}

/// "#107 https://…" (the link may be in <> or (), or a markdown link): the item and its first web link.
fn designs(body: &str) -> Option<Line> {
    let (id, rest) = body.split_once(char::is_whitespace)?;
    if !id.starts_with('#') {
        return None;
    }
    let id = id_of(id)?;
    let at = rest.find("https://").or_else(|| rest.find("http://"))?;
    let url: String = rest[at..].chars().take_while(|c| !c.is_whitespace() && !matches!(c, '>' | ')' | ']' | '"' | '`' | '*')).collect();
    let url = url.trim_end_matches(['.', ',', ';']).to_string();
    (url.len() > "https://".len()).then_some(Line::Designs { id, url })
}

fn context(body: &str) -> Option<Line> {
    let p = parts(body);
    let (who, what) = ["->", "→", "=>"].iter().find_map(|a| p.first()?.split_once(a))?;
    let who = who.trim().trim_start_matches('@').trim_matches(['"', '`', '*']).trim().to_string();
    let what = what.split_whitespace().next().unwrap_or("").trim_matches(['"', '`', '*', '.']).to_ascii_lowercase();
    if who.is_empty() || what.is_empty() {
        return None;
    }
    let why = p[1..].iter().find_map(|s| labelled(s, "why")).unwrap_or("").to_string();
    Some(Line::Context { who, what, why })
}

/// Kinds of file a holder makes to be looked at: a page or mock, a picture, a document, a build.
const MADE: &[&str] = &["html", "htm", "pdf", "md", "png", "jpg", "jpeg", "gif", "svg", "webp", "mp4", "mov", "app", "dmg", "zip"];

/// What a turn names that was made, in order, each once (four at most): its web links, and with `paths` (a session on
/// this Mac: another machine's paths mean nothing here) the files it names by a full path (`/…` or `~/…`) whose kind
/// is one made to be looked at. A markdown link counts by its address.
pub fn made_in(text: &str, paths: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let tail: &[char] = &[')', ']', '>', '.', ',', ';', ':', '!', '?', '\'', '"', '`', '*', '_'];
    for word in text.split_whitespace() {
        let found = match word.find("https://").or_else(|| word.find("http://")) {
            Some(at) => Some(word[at..].trim_end_matches(tail)),
            None if paths => {
                let w = word.trim_start_matches(['(', '[', '<', '\'', '"', '`', '*', '_']).trim_end_matches(tail).trim_end_matches('/');
                let kind = w.rsplit_once('.').map(|(_, k)| k.to_ascii_lowercase()).unwrap_or_default();
                ((w.starts_with('/') || w.starts_with("~/")) && !w.contains("://") && MADE.contains(&kind.as_str())).then_some(w)
            }
            None => None,
        };
        if let Some(f) = found.filter(|f| f.len() > 8 && !out.iter().any(|o| o == f)) {
            out.push(f.to_string());
            if out.len() == 4 {
                break;
            }
        }
    }
    out
}
