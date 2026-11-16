//! The Desk's lines in a reply, read at its turn's end (as crew_log's DECIDED lines are): the lead's ITEM, ROUTE and
//! ASK, and a holder's DONE. Each is a line of its own, its word in capitals; a list mark or bold around it is fine;
//! a line in a code block (``` or ~~~) or a `>` quote only shows one, so it's left alone. DECIDED lines are
//! crew_log's `calls_in`.

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A new item.
    Item { kind: String, title: String, done_when: String },
    /// Item `id` ("new": the reply's last ITEM) to the session named `to`, now or `after` another item.
    Route { id: String, to: String, after: Option<String>, why: String },
    /// A question for you, with its choices (none: answered in words).
    Ask { question: String, choices: Vec<String> },
    /// Items done, as their holder says.
    Done { ids: Vec<String> },
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
    let title = rest.iter().find(|s| labelled(s, "done when").is_none()).map(|s| s.to_string()).unwrap_or_default();
    let done_when = rest.iter().find_map(|s| labelled(s, "done when")).unwrap_or("").to_string();
    (!title.is_empty()).then_some(Line::Item { kind, title, done_when })
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
    Some(Line::Route { id, to, after, why })
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
