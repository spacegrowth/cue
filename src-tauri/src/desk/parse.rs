//! The Desk's lines in a reply, read at its turn's end (as crew_log's DECIDED lines are): the lead's ITEM, ROUTE, ASK
//! and CONTEXT, and a holder's DONE. Each is a line of its own, its word in capitals; a list mark or bold around it is fine;
//! a line in a code block (``` or ~~~) or a `>` quote only shows one, so it's left alone. DECIDED lines are
//! crew_log's `calls_in`.

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A new item.
    Item { kind: String, title: String, see: String, done_when: String, why: String },
    /// Item `id` ("new": the reply's last ITEM) to the session named `to`, now or `after` another item; `kind`, when
    /// said: the lead's call on what it is ("bug" | "feature" | "task"; an item you add has none till then); `crew`,
    /// when said: the lead's teammate doing it (with `to` "me").
    Route { id: String, to: String, after: Option<String>, why: String, kind: Option<String>, crew: Option<String> },
    /// A question for you, with its choices (none: answered in words).
    /// `item`: the item it's about (`ASK: #<id> | <question> | …`), shown on that item as waiting on you.
    Ask { question: String, choices: Vec<String>, item: Option<String> },
    /// Items done, as their holder says.
    Done { ids: Vec<String> },
    /// Item `id`'s designs are at `url` (its canvas), as any session of its project says; a newer one replaces it.
    Designs { id: String, url: String },
    /// Item `id`'s two lines, from the lead (#382): what you'll see when it's done, in plain words, and (when said)
    /// the technical check its session runs: `SEE: #<id> | <what you'll see> | done when: <check>`.
    See { id: String, see: String, done_when: String },
    /// Item `id` retitled by the lead: a short plain `title` for the words it was added with.
    Title { id: String, title: String },
    /// Item `id` holds more than one problem: it takes the first of `titles`, and each of the rest becomes an item of
    /// its own.
    Split { id: String, titles: Vec<String> },
    /// What to do with session `who`'s context ("me": the lead's own): `what` is "keep" (others to come), lower case.
    Context { who: String, what: String, why: String },
    /// The lead cut a link in a circle of waits: item `id` no longer waits on item `on` (#250).
    Cut { id: String, on: String, why: String },
    /// The lead folded item `id`, a repeat of item `into`, into it: its words and pictures go to `into`, and it goes.
    Merge { id: String, into: String, why: String },
    /// The lead's note to whoever holds item `id`, at any stage (#338): `NOTE: #<id> | <words>`.
    Note { id: String, text: String },
    /// The lead unsticking item `id` itself (#347): `what` is "close" (`CLOSE: #<id> | commit: <sha> | why: …`: its
    /// work is already on main; `commit` when said), "take_copy" (`TAKE COPY: #<id> | why: …`: its copy back from a
    /// stuck or finished item), "land_first" (`LAND FIRST: #<id> | why: …`: up in the landing line) or "start"
    /// (`START: #<id> | why: …`: a waiting item, now), "pause" (`PAUSE: #<id> | why: …`: an item at work stops at a
    /// clean point and keeps its place, #365), "resume" (`RESUME: #<id> | why: …`: a paused one carries on) or
    /// "hold" (`HOLD: #<id> | why: …`: onto the held shelf, as the user asked, #385; "#new": the reply's last ITEM).
    Unstick { what: String, id: String, commit: String, why: String },
    /// The lead's answer to the landing clean-up question item `id`'s holder asked it (#349): `ANSWER: #<id> | <words>`.
    Answer { id: String, text: String },
    /// That question passed on to the user (#349): `PASS: #<id> | <why>` (the why may be left out).
    Pass { id: String, why: String },
    /// The lead pausing (`PAUSE: why: …`, on) or resuming (`RESUME: why: …`) its whole Desk (#366).
    Pause { on: bool, why: String },
}

/// The Desk lines in `text`, in order.
pub fn lines(text: &str) -> Vec<Line> {
    read(text).filter_map(one).collect()
}

/// The lines of `text` that look like a ROUTE but don't read as one, as written (#248): "ROUTE #12 -> web" (no
/// colon), "ROUTE: #12 — web", "route: #12 to web", "ROUTE:". The lead is told Cue couldn't read them, so a route in
/// the wrong form never just does nothing.
pub fn unread_routes(text: &str) -> Vec<String> {
    read(text).filter(|t| route_like(t) && !matches!(one(t), Some(Line::Route { .. }))).map(str::to_string).collect()
}

/// Whether `line` is meant as a route (#248): its first word is "route" (any case), then a colon, or an item (#…)
/// and an arrow, a dash or "to" ("ROUTE #12 -> web", "route #12 to web"). Prose that starts so ("Route #3 is
/// blocked") isn't; nor one in backticks, nor the form shown with its blanks (`ROUTE: #<id> -> …`).
pub fn route_like(line: &str) -> bool {
    let Some(rest) = after_route(line) else { return false };
    if let Some(body) = rest.strip_prefix(':') {
        let body = body.trim().trim_start_matches('*').trim();
        return !(body.starts_with('<') || body.trim_start_matches('#').starts_with('<'));
    }
    let Some(r) = rest.strip_prefix('#') else { return false };
    let r = r.trim_start_matches(|c: char| c.is_ascii_alphanumeric()).trim_start();
    ["->", "→", "=>", "—", "–", "to "].iter().any(|a| r.get(..a.len()).is_some_and(|h| h.eq_ignore_ascii_case(a)))
}

/// What's wrong with `line`, a route Cue couldn't read (`unread_routes`), as the lead is told to fix it (#248).
pub fn route_fix(line: &str) -> &'static str {
    let rest = after_route(line).unwrap_or("");
    if !unmarked(line).trim_start_matches('*').starts_with("ROUTE") {
        return "write ROUTE in capitals";
    }
    match rest.strip_prefix(':') {
        None => "put a colon after ROUTE",
        Some(body) if !["->", "→", "=>"].iter().any(|a| body.contains(a)) => "put -> between the item and the session",
        Some(_) => "give the item as #<id> and the session's name after ->",
    }
}

/// What follows `line`'s first word when it's "route" (any case, bold or not), trimmed at its start.
fn after_route(line: &str) -> Option<&str> {
    let l = unmarked(line);
    let end = l.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(l.len());
    let (word, rest) = l.split_at(end);
    word.eq_ignore_ascii_case("route").then(|| rest.trim_start_matches('*').trim_start())
}

/// `line` without its list mark ("- ", "* ", "1. ", "2) ") or leading bold, trimmed (#248).
pub fn unmarked(line: &str) -> &str {
    let l = line.trim().trim_start_matches(['-', '*', '+', ' ']).trim();
    let digits = l.find(|c: char| !c.is_ascii_digit()).unwrap_or(l.len());
    match l[digits..].strip_prefix(['.', ')']).filter(|r| digits > 0 && r.starts_with(char::is_whitespace)) {
        Some(r) => r.trim_start_matches(['-', '*', '+', ' ']).trim(),
        None => l,
    }
}

/// The lines of `text` the Desk reads, trimmed: not those in a code block (``` or ~~~) or a `>` quote.
fn read(text: &str) -> impl Iterator<Item = &str> {
    let mut fenced = false;
    text.lines().filter_map(move |line| {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            return None;
        }
        (!fenced && !t.starts_with('>')).then_some(t)
    })
}

/// One line the Desk reads (trimmed), as a Desk line if it is one.
fn one(t: &str) -> Option<Line> {
    // List marks (numbered too) and bold only: a line in backticks (`ITEM: …`) is an example, not a line to act on.
    let l = unmarked(t);
    let (word, body) = l.split_once(':')?;
    let body = body.trim().trim_start_matches("**").trim().trim_end_matches('*').trim();
    if body.is_empty() || body.starts_with('<') {
        return None;
    }
    match word.trim_end_matches('*').trim() {
        "ITEM" => item(body),
        // A route in any case ("Route: #3 -> web"): its #id and arrow say it's one (#248).
        "ROUTE" => route(body),
        "ASK" => ask(body),
        "DONE" => done(body),
        "CONTEXT" => context(body),
        "DESIGNS" => designs(body),
        "TITLE" => title(body),
        "SEE" => see(body),
        "SPLIT" => split(body),
        "CUT" => cut(body),
        "MERGE" => merge(body),
        "NOTE" => note(body),
        "ANSWER" => match note(body)? {
            Line::Note { id, text } => Some(Line::Answer { id, text }),
            _ => None,
        },
        "PASS" => pass(body),
        "CLOSE" => unstick("close", body),
        "TAKE COPY" => unstick("take_copy", body),
        "LAND FIRST" => unstick("land_first", body),
        "START" => unstick("start", body),
        "HOLD" => unstick("hold", body),
        // With an item (#365) it pauses that item; with none (#366), the whole Desk.
        "PAUSE" => unstick("pause", body).or_else(|| whole_desk(true, body)),
        "RESUME" => unstick("resume", body).or_else(|| whole_desk(false, body)),
        _ => None,
    }
}

/// `PAUSE: why: …` / `RESUME: why: …` with no item: the whole Desk (#366). None for one naming an item.
fn whole_desk(on: bool, body: &str) -> Option<Line> {
    (!body.trim_start().starts_with('#')).then(|| Line::Pause { on, why: labelled(body, "why").unwrap_or(body).to_string() })
}

/// `CLOSE: #<id> | commit: <sha> | why: …` and the other lines that unstick an item (#347): Line::Unstick. Its why may
/// be written with no label; a commit only on CLOSE.
fn unstick(what: &str, body: &str) -> Option<Line> {
    let ps = parts(body);
    let head = ps.first()?;
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let (mut commit, mut why) = (String::new(), String::new());
    for p in &ps[1..] {
        match (labelled(p, "commit"), labelled(p, "why")) {
            (Some(c), _) if what == "close" => commit = c.trim_matches('`').to_string(),
            (_, Some(w)) => why = w.to_string(),
            _ if why.is_empty() => why = p.to_string(),
            _ => {}
        }
    }
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(Line::Unstick { what: what.into(), id: id_of(head)?, commit: commit.to_ascii_lowercase(), why })
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
    let title = rest.iter().find(|s| labelled(s, "done when").is_none() && why_of(s).is_none() && see_of(s).is_none()).map(|s| s.to_string()).unwrap_or_default();
    let see = rest.iter().find_map(|s| see_of(s)).unwrap_or("").to_string();
    let done_when = rest.iter().find_map(|s| labelled(s, "done when")).unwrap_or("").to_string();
    let why = rest.iter().find_map(|s| why_of(s)).unwrap_or("").to_string();
    (!title.is_empty()).then_some(Line::Item { kind, title, see, done_when, why })
}

/// `part` without its "you'll see:" label (or "you will see:", "see:"; a curly quote too), with its colon, so a
/// title that starts "See …" stays the title (#382).
fn see_of(part: &str) -> Option<&str> {
    let p = part.replace('’', "'");
    ["you'll see", "you will see", "see"].iter().find_map(|l| {
        let rest = p.get(..l.len()).filter(|h| h.eq_ignore_ascii_case(l)).map(|_| &p[l.len()..])?;
        rest.trim_start().starts_with(':').then(|| {
            // The same cut in `part` itself: the curly quote is 3 bytes to the straight one's 1.
            let skip = part.len() - rest.len();
            part[skip..].trim_start().trim_start_matches(':').trim()
        })
    })
}

/// "#111 | <what you'll see> | done when: <check>": item 111's two lines (#382). Its see may be labelled or not.
fn see(body: &str) -> Option<Line> {
    let p = parts(body);
    let head = p.first()?;
    if !head.starts_with('#') {
        return None;
    }
    let id = id_of(head)?;
    let done_when = p[1..].iter().find_map(|s| labelled(s, "done when")).unwrap_or("").to_string();
    let see = p[1..].iter().find_map(|s| see_of(s)).or_else(|| p[1..].iter().copied().find(|s| labelled(s, "done when").is_none())).map(|s| unwrapped(s).to_string()).unwrap_or_default();
    (!see.is_empty() || !done_when.is_empty()).then_some(Line::See { id, see, done_when })
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
    let mut p = parts(body);
    // About one item: `ASK: #<id> | <question> | …`.
    let item = p.first().filter(|f| p.len() > 1 && f.trim().starts_with('#') && f.trim().len() > 1 && !f.trim().contains(' ')).and_then(|f| id_of(f));
    if item.is_some() {
        p.remove(0);
    }
    let question = p.first()?.to_string();
    // "choices: A / B", and as leads also write them: "A | B" (the parts after it), "A; B", "A, B", "A or B".
    let choices = p[1..].iter().position(|s| labelled(s, "choices").is_some()).map(|at| {
        let first = labelled(p[1 + at], "choices").unwrap_or("");
        std::iter::once(first).chain(p[2 + at..].iter().copied()).flat_map(split_choices).collect()
    });
    (!question.is_empty()).then_some(Line::Ask { question, choices: choices.unwrap_or_default(), item })
}

/// One part of an ASK's choices, split on `/` or `;`; with neither, on commas or " or ".
fn split_choices(part: &str) -> Vec<String> {
    let clean = |x: &str| x.trim().trim_matches(['"', '`', '*', '.']).trim().to_string();
    let by = |seps: &[&str]| -> Vec<String> {
        let mut out = vec![part.to_string()];
        for sep in seps {
            out = out.iter().flat_map(|x| x.split(sep).map(str::to_string).collect::<Vec<_>>()).collect();
        }
        out.iter().map(|x| clean(x)).filter(|x| !x.is_empty()).collect()
    };
    let main = by(&["/", ";"]);
    if main.len() > 1 { main } else { by(&[",", " or "]) }
}

fn done(body: &str) -> Option<Line> {
    let ids: Vec<String> = body.split([',', ' ']).filter(|w| w.trim().starts_with('#')).filter_map(id_of).collect();
    (!ids.is_empty()).then_some(Line::Done { ids })
}

/// "#111 | <title>": the item and its new title.
fn title(body: &str) -> Option<Line> {
    let (id, titles) = id_and_titles(body)?;
    let title = titles.into_iter().next()?;
    Some(Line::Title { id, title })
}

/// "#111 | <title> | <title> …": the item and the two or more titles it's split into.
fn split(body: &str) -> Option<Line> {
    let (id, titles) = id_and_titles(body)?;
    (titles.len() >= 2).then_some(Line::Split { id, titles })
}

/// "#231 -> #228 | why: …": item 231 no longer waits on item 228.
fn cut(body: &str) -> Option<Line> {
    let p = parts(body);
    let head = p.first()?;
    let (a, b) = head.split_once("->").or_else(|| head.split_once('→'))?;
    let (a, b) = (a.trim(), b.trim());
    if !a.starts_with('#') || !b.starts_with('#') {
        return None;
    }
    let why = p[1..].iter().find_map(|s| labelled(s, "why")).unwrap_or("").to_string();
    Some(Line::Cut { id: id_of(a)?, on: id_of(b)?, why })
}

/// "#320 into #319 | why: …" (or "#320 -> #319"): item 320 folded into item 319.
/// `NOTE: #<id> | <words>` (the words may hold `|` too): Line::Note.
fn note(body: &str) -> Option<Line> {
    let (head, text) = body.split_once('|')?;
    let head = head.trim();
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(Line::Note { id: id_of(head)?, text })
}

/// `PASS: #<id> | why: <words>` (the why's label, or the why, may be left out): Line::Pass.
fn pass(body: &str) -> Option<Line> {
    let (head, why) = body.split_once('|').unwrap_or((body, ""));
    let head = head.trim();
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let why = why.trim();
    let why = labelled(why, "why").unwrap_or(why).to_string();
    Some(Line::Pass { id: id_of(head)?, why })
}

fn merge(body: &str) -> Option<Line> {
    let p = parts(body);
    let head = p.first()?;
    let (a, b) = [" into ", "->", "→", "=>"].iter().find_map(|sep| head.split_once(sep))?;
    let (a, b) = (a.trim(), b.trim());
    if !a.starts_with('#') || !b.starts_with('#') {
        return None;
    }
    let (id, into) = (id_of(a)?, id_of(b)?);
    let why = p[1..].iter().find_map(|s| labelled(s, "why")).unwrap_or("").to_string();
    (id != into).then_some(Line::Merge { id, into, why })
}

/// "#id | a | b": the id (it must start with #) and the non-empty parts after it, quotes and bold taken off.
fn id_and_titles(body: &str) -> Option<(String, Vec<String>)> {
    let p = parts(body);
    let head = p.first()?;
    if !head.starts_with('#') {
        return None;
    }
    let id = id_of(head)?;
    let titles = p[1..].iter().map(|t| unwrapped(t).to_string()).filter(|t| !t.is_empty()).collect::<Vec<_>>();
    (!titles.is_empty()).then_some((id, titles))
}

/// `t` without the quotes, backticks or bold wrapped around the whole of it (`"a"`, `**a**`): a mark only at one end
/// is the title's own (`"Looks right" is loud` keeps its opening quote). Bold may have lost its close already (lines()
/// takes stars off the line's end), so opening stars go on their own.
fn unwrapped(t: &str) -> &str {
    let mut t = t.trim();
    if !t.ends_with('*') {
        t = t.trim_start_matches('*').trim();
    }
    loop {
        let Some(c) = t.chars().next().filter(|c| matches!(c, '"' | '`' | '*')) else { return t };
        if t.len() < 2 || !t.ends_with(c) {
            return t;
        }
        t = t[1..t.len() - 1].trim();
    }
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
