//! The Desk's lines in a reply, read at its turn's end (as crew_log's DECIDED lines are): the lead's ITEM, ROUTE, ASK
//! and CONTEXT, and a holder's DONE. Each is a line of its own, its word in capitals; a list mark or bold around it is fine;
//! a line in a code block (``` or ~~~) or a `>` quote only shows one, so it's left alone. DECIDED lines are
//! crew_log's `calls_in`.

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// A new item. `follows`: the item it carries on from (`| from: #<id>`, #606), with `picked` the option you picked
    /// there (`| picked: <option>`), when the lead files a build item for your pick itself; both empty otherwise.
    Item { kind: String, title: String, see: String, done_when: String, why: String, follows: String, picked: String },
    /// Item `id` ("new": the reply's last ITEM) to the session named `to`, now or `after` another item; `kind`, when
    /// said: the lead's call on what it is ("bug" | "feature" | "task"; an item you add has none till then); `crew`,
    /// when said: the lead's teammate doing it (with `to` "me").
    /// `on`, when said: the machine a new session for it starts on (`| on: <machine>`), with `in` the folder of the
    /// clone there (`| in: <folder>`, else the one this Desk used there last).
    Route { id: String, to: String, after: Option<String>, why: String, kind: Option<String>, crew: Option<String>, on: Option<String>, in_dir: Option<String>, model: Option<String>, repo: Option<String> },
    /// A question for you, with its choices (none: answered in words).
    /// `item`: the item it's about (`ASK: #<id> | <question> | …`), shown on that item as waiting on you.
    Ask { question: String, choices: Vec<String>, item: Option<String> },
    /// Items done, as their holder says.
    Done { ids: Vec<String> },
    /// Item `id`, with you to check, isn't done after all, as its holder says (#627): `NOT DONE: #<id> | <what's left>`.
    /// Cue puts it back in progress (in line for a copy when none is free). `text`: what's left, maybe empty.
    NotDone { id: String, text: String },
    /// What comes after option `option` ("any": whichever) of item `id` is picked (#543), as its holder says:
    /// `IF PICKED: #<id> | <option> | <kind> | <title> | you'll see: … | done when: … | by: <me | new | name>`.
    IfPicked { id: String, next: crate::desk::model::FollowUp },
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
    /// A big feature's pieces (#282): `FEATURE: <name> | #a #b …` makes a feature item named so (`id` None), and
    /// `FEATURE: #<id> | #a #b …` makes item `id` the feature; either way each of `pieces` ("new": the reply's last
    /// ITEM) is listed under it.
    Feature { id: Option<String>, name: String, pieces: Vec<String> },
    /// The lead's note to whoever holds item `id`, at any stage (#338): `NOTE: #<id> | <words>`.
    Note { id: String, text: String },
    /// The lead unsticking item `id` itself (#347): `what` is "close" (`CLOSE: #<id> | commit: <sha> | why: …`: its
    /// work is already on main; `commit` when said), "take_copy" (`TAKE COPY: #<id> | why: …`: its copy back from a
    /// stuck or finished item), "land_first" (`LAND FIRST: #<id> | why: …`: up in the landing line), "copy_first"
    /// (`COPY FIRST: #<id> | why: …`: to the front of the line for a free copy, #572) or "start"
    /// (`START: #<id> | why: …`: a waiting item, now), "pause" (`PAUSE: #<id> | why: …`: an item at work stops at a
    /// clean point and keeps its place, #365), "resume" (`RESUME: #<id> | why: …`: a paused one carries on) or
    /// "hold" (`PARK: #<id> | why: …`, or the old `HOLD:`: parked, as the user asked, #385/#391; "#new": the reply's last ITEM)
    /// or "unpark" (`UNPARK: #<id> — why: …`: back to be routed, only when the user asked for it in the message the
    /// turn answers, #499) or "drop" (`DROP: #<id> | why: …`: gone in any state, as its Drop does, #544).
    Unstick { what: String, id: String, commit: String, why: String },
    /// The lead's answer to the landing clean-up question item `id`'s holder asked it (#349): `ANSWER: #<id> | <words>`.
    Answer { id: String, text: String },
    /// That question passed on to the user (#349): `PASS: #<id> | <why>` (the why may be left out).
    Pass { id: String, why: String },
    /// The lead pausing (`PAUSE: why: …`, on) or resuming (`RESUME: why: …`) its whole Desk (#366).
    Pause { on: bool, why: String },
    /// The lead proposing a change to how work lands (#381): `LANDS: <part> -> <choice> | steps: <one> ;; <two> | why: …`.
    /// Cue asks you; only your yes saves it (desk/hub/lead/lands.rs).
    Lands { part: String, to: String, steps: Vec<String>, why: String },
    /// A session's message for its Desk's lead (#474): `TO LEAD: [#<id>[ |]] <words>`. `id`: the item it's about, when
    /// named; without one, Cue takes the item the session is on.
    ToLead { id: Option<String>, text: String },
    /// What you check on item `id` once it's done (#526), as its holder says next to its DONE:
    /// `CHECK: #<id> | Try: <one thing to do in the app>` (`kind` "try") or `CHECK: #<id> | Nothing to see: <why>`
    /// (`kind` "nothing"). `text`: the words after the label. Its three steps (#668):
    /// `CHECK: #<id> | Where: <screen> | Do: <what to click or type> | Expect: <what happens>` (`kind` "try", `text` the
    /// Do, `steps` [where, do, expect]); `steps` is empty for the one-line forms.
    Check { id: String, kind: String, text: String, steps: Vec<String> },
    /// The lead giving item `id` a copy of another repository on a branch it names (#557):
    /// `COPY: #<id> | in: <add-on, or a repository's folder> | branch: <name> | why: …`.
    Copy { id: String, repo: String, branch: String, why: String },
    /// The lead's item `id` holds part of the repository's code for every Desk on it (#540):
    /// `HOLD: #<id> | area: <globs, comma-separated> | till: <words> | why: …` (desk/holds.rs). A HOLD with no `area:`
    /// is PARK's first name.
    Hold { id: String, area: Vec<String>, till: String, why: String },
    /// That hold lifted early (#540): `UNHOLD: #<id>`.
    Unhold { id: String },
    /// To another Desk's lead on the repository (#540, desk/tell.rs): `TELL DESK: <Desk> | <words>` (`to` Some), or
    /// to every other one, `TELL DESKS: <words>`. `item`: the #id the words start with.
    Tell { to: Option<String>, item: Option<String>, text: String },
    /// The lead can't agree with another Desk's lead on the repository (#540, desk/escalate.rs): `DISAGREE: <Desk> |
    /// <its side>`. Cue asks the user, both sides quoted. `item`: the #id the words start with.
    Disagree { to: String, item: Option<String>, text: String },
    /// A loop the lead drafts for you to start (#583): `LOOP: <project> | name: … | goal: … | done when: … | check: … |
    /// rounds: … | time: … | spend: … | decisions: … | folder: …`. Cue finds the project's folder and shows it as a card with
    /// Start loop; nothing starts until you press it.
    Loop(LoopDraft),
    /// The lead moving item `id` to another Desk, named `to` (#639, desk/move_desk.rs): `MOVE: #<id> -> <Desk> | why: …`.
    /// It goes with its notes, copy and branch, and its holder's hand-off.
    Move { id: String, to: String, why: String },
    /// The Desk lead installing what's on main now, through Cue so the Desk shows it (#867): `INSTALL: main`, or a
    /// plain `INSTALL:`.
    Install,
}

/// What a LOOP: line says (#583). A part with no label of its own goes on the one before it, so a goal or a check
/// may hold a "|" (`check: npm test | tail -5`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoopDraft {
    pub project: String,
    pub name: String,
    pub goal: String,
    pub done_when: String,
    /// The check command; "" when the line says none (the loop's lead proposes one at its start).
    pub check: String,
    pub rounds: Option<u32>,
    pub time_min: Option<u32>,
    pub spend_usd: Option<f64>,
    /// "one-way" | "every" | "none"; "" when not said.
    pub decisions: String,
    /// The folder the line names (`folder: ~/development/web`), "" when none: Cue finds the project's own then.
    pub folder: String,
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
    if word.trim_end_matches('*').trim() == "INSTALL" {
        return install(body);
    }
    if body.is_empty() || body.starts_with('<') {
        return None;
    }
    match word.trim_end_matches('*').trim() {
        "ITEM" => item(body),
        // A route in any case ("Route: #3 -> web"): its #id and arrow say it's one (#248).
        "ROUTE" => route(body),
        "ASK" => ask(body),
        "DONE" => done(body),
        "NOT DONE" => not_done(body),
        "CHECK" => check(body),
        "CONTEXT" => context(body),
        "DESIGNS" => designs(body),
        "IF PICKED" => if_picked(body),
        "TITLE" => title(body),
        "SEE" => see(body),
        "SPLIT" => split(body),
        "CUT" => cut(body),
        "MERGE" => merge(body),
        "NOTE" => note(body),
        "FEATURE" => feature(body),
        "ANSWER" => match note(body)? {
            Line::Note { id, text } => Some(Line::Answer { id, text }),
            _ => None,
        },
        "PASS" => pass(body),
        "CLOSE" => unstick("close", body),
        "TAKE COPY" => unstick("take_copy", body),
        "LAND FIRST" => unstick("land_first", body),
        "COPY FIRST" => unstick("copy_first", body),
        "START" => unstick("start", body),
        "PARK" => unstick("hold", body),
        // HOLD with an area holds code for every Desk on the repository (#540); without one it's PARK's first name
        // (#385), which still works.
        "HOLD" => hold(body).or_else(|| unstick("hold", body)),
        "UNHOLD" => unhold(body),
        w @ ("TELL DESK" | "TELL DESKS") => crate::desk::tell::parse(w, body),
        "DISAGREE" => crate::desk::escalate::parse(body),
        // UNPARK (#499): its why after a dash ("UNPARK: #12 — why: …") or a "|" as the other lines have it.
        "UNPARK" => unstick("unpark", &dashed(body)),
        // DROP (#544): the item is gone, as its Drop does; its why after a dash or a "|".
        "DROP" => unstick("drop", &dashed(body)),
        // With an item (#365) it pauses that item; with none (#366), the whole Desk.
        "PAUSE" => unstick("pause", body).or_else(|| whole_desk(true, body)),
        "RESUME" => unstick("resume", body).or_else(|| whole_desk(false, body)),
        "LANDS" => lands(body),
        "TO LEAD" => to_lead(body),
        "COPY" => copy(body),
        "LOOP" => loop_line(body),
        "MOVE" => crate::desk::move_desk::parse(body),
        _ => None,
    }
}

/// `INSTALL: main`, `INSTALL: main | why: …` or a plain `INSTALL:` (#867): Line::Install. Anything else after it (the line
/// explained in prose) isn't one.
fn install(body: &str) -> Option<Line> {
    let (head, _) = body.split_once('|').unwrap_or((body, ""));
    let head = head.trim().trim_matches(['`', '*']).trim();
    (head.is_empty() || head.eq_ignore_ascii_case("main")).then_some(Line::Install)
}

/// `LOOP: <project> | name: … | goal: … | done when: … | check: … | rounds: … | time: … | spend: … | decisions: …`
/// (#583): None without a project.
fn loop_line(body: &str) -> Option<Line> {
    const LABELS: [&str; 9] = ["name", "goal", "done when", "check", "rounds", "time", "spend", "decisions", "folder"];
    let ps = parts(body);
    let project = ps.first()?.trim_matches(['`', '*', '"']).trim();
    if project.is_empty() || project.contains(':') {
        return None;
    }
    let mut got: Vec<(&str, String)> = Vec::new();
    for p in &ps[1..] {
        let label = p.split_once(':').and_then(|(k, _)| LABELS.iter().find(|l| k.trim().eq_ignore_ascii_case(l)));
        match (label, got.last_mut()) {
            (Some(l), _) => got.push((l, labelled(p, l).unwrap_or_default().to_string())),
            (None, Some(last)) => last.1 = format!("{} | {p}", last.1),
            (None, None) => {}
        }
    }
    let said = |l: &str| got.iter().rev().find(|(k, _)| *k == l).map(|(_, v)| v.trim().to_string()).unwrap_or_default();
    let digits = |v: String| v.trim_start_matches('$').split(|c: char| !(c.is_ascii_digit() || c == '.')).next().unwrap_or("").to_string();
    let check = said("check").trim_matches('`').to_string();
    let decisions = said("decisions").to_ascii_lowercase().replace("oneway", "one-way");
    Some(Line::Loop(LoopDraft {
        project: project.to_string(),
        name: said("name"),
        goal: said("goal"),
        done_when: said("done when"),
        check: if ["none", "no", "-", "\"\""].contains(&check.to_ascii_lowercase().as_str()) { String::new() } else { check },
        rounds: digits(said("rounds")).parse().ok(),
        time_min: digits(said("time")).parse().ok(),
        spend_usd: digits(said("spend")).parse().ok(),
        decisions: if ["one-way", "every", "none"].contains(&decisions.as_str()) { decisions } else { String::new() },
        folder: said("folder").trim_matches('`').to_string(),
    }))
}

/// `COPY: #<id> | in: <repo> | branch: <name> | why: …` (#557): a branch name git would take, no spaces.
fn copy(body: &str) -> Option<Line> {
    let ps = parts(body);
    let head = ps.first()?;
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let (mut repo, mut branch, mut why) = (String::new(), String::new(), String::new());
    for p in &ps[1..] {
        if let Some(r) = labelled(p, "in") {
            repo = r.trim_matches('`').to_string();
        } else if let Some(b) = labelled(p, "branch") {
            branch = b.trim_matches('`').to_string();
        } else if let Some(w) = labelled(p, "why") {
            why = w.to_string();
        }
    }
    let ok = |b: &str| !b.is_empty() && !b.starts_with(['-', '/']) && !b.contains("..") && b.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if repo.is_empty() || !ok(&branch) {
        return None;
    }
    Some(Line::Copy { id: id_of(head)?, repo, branch, why })
}

/// `HOLD: #<id> | area: <globs> | till: <words> | why: …` (#540): None without an `area:` part (then it's a PARK).
fn hold(body: &str) -> Option<Line> {
    let ps = parts(body);
    let head = ps.first()?;
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let (mut area, mut till, mut why) = (None, String::new(), String::new());
    for p in &ps[1..] {
        if let Some(a) = labelled(p, "area") {
            area = Some(a.split(',').map(|g| g.trim().trim_matches('`').to_string()).filter(|g| !g.is_empty()).collect());
        } else if let Some(t) = labelled(p, "till") {
            till = t.to_string();
        } else if let Some(w) = labelled(p, "why") {
            why = w.to_string();
        }
    }
    Some(Line::Hold { id: id_of(head)?, area: area?, till, why })
}

/// `UNHOLD: #<id>` (#540); anything after a "|" is left alone.
fn unhold(body: &str) -> Option<Line> {
    let head = *parts(body).first()?;
    (head.starts_with('#') && !head.contains(' ')).then(|| id_of(head)).flatten().map(|id| Line::Unhold { id })
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
    // LAND FIRST may name another Desk's item (#540): "loops#533", "loops #533".
    let other = what == "land_first" && !head.starts_with('#') && head.contains('#');
    if !other && (!head.starts_with('#') || head.contains(' ')) {
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
    Some(Line::Unstick { what: what.into(), id: if other { item_ref(head)? } else { id_of(head)? }, commit: commit.to_ascii_lowercase(), why })
}

/// `body` with a dash between its item and its why ("#12 — why: …") read as a "|" (#499).
fn dashed(body: &str) -> String {
    match body.split_once(char::is_whitespace) {
        Some((head, rest)) if head.starts_with('#') => {
            let rest = rest.trim_start();
            match rest.strip_prefix(['—', '–', '-']) {
                Some(r) => format!("{head} | {}", r.trim_start()),
                None => body.to_string(),
            }
        }
        _ => body.to_string(),
    }
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

/// An item `after:` or LAND FIRST names (#540): its id as `id_of` reads it, or "<Desk>#<id>" when a Desk's name is
/// written before it ("loops#533", "loops #533"); desk/across.rs reads that back and checks it.
fn item_ref(s: &str) -> Option<String> {
    let s = s.trim().trim_matches(['`', '*']);
    match s.split_once('#') {
        Some((desk, rest)) if !desk.trim().is_empty() => Some(format!("{}#{}", desk.trim().to_ascii_lowercase(), id_of(rest)?)),
        _ => id_of(s),
    }
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
    // "from: #593" and "picked: B" with their colons and the from's #, so a title "From …" or "Picked …" stays the title.
    fn from_of(s: &str) -> Option<String> {
        labelled(s, "from").filter(|v| s[4..].trim_start().starts_with(':') && v.starts_with('#')).and_then(id_of)
    }
    fn picked_of(s: &str) -> Option<&str> {
        labelled(s, "picked").filter(|_| s[6..].trim_start().starts_with(':'))
    }
    let title = rest.iter().find(|s| labelled(s, "done when").is_none() && why_of(s).is_none() && see_of(s).is_none() && from_of(s).is_none() && picked_of(s).is_none()).map(|s| s.to_string()).unwrap_or_default();
    let see = rest.iter().find_map(|s| see_of(s)).unwrap_or("").to_string();
    let done_when = rest.iter().find_map(|s| labelled(s, "done when")).unwrap_or("").to_string();
    let why = rest.iter().find_map(|s| why_of(s)).unwrap_or("").to_string();
    let follows = rest.iter().find_map(|s| from_of(s)).unwrap_or_default();
    let picked = rest.iter().find_map(|s| picked_of(s)).map(|p| p.trim_start_matches("option ").trim_start_matches("Option ").trim().to_string()).unwrap_or_default();
    (!title.is_empty()).then_some(Line::Item { kind, title, see, done_when, why, follows, picked })
}

/// "#12 | B | feature | <title> | you'll see: … | done when: … | by: me" (#543): what follows option B of item 12. Its
/// kind may be left out (a task), and its by (whoever held #12).
fn if_picked(body: &str) -> Option<Line> {
    let p = parts(body);
    let (head, option) = (p.first()?, p.get(1)?);
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let option = option.trim().trim_start_matches("option ").trim_start_matches("Option ").trim();
    let option = if option.eq_ignore_ascii_case("any") || option == "*" { "any".to_string() } else { crate::desk::hub::turn::act::letter_of(option)? };
    let by = p[2..].iter().find_map(|s| labelled(s, "by").filter(|_| s[2..].trim_start().starts_with(':'))).unwrap_or("me").trim().trim_start_matches('@').to_string();
    let rest: Vec<&str> = p[2..].iter().copied().filter(|s| !(labelled(s, "by").is_some() && s[2..].trim_start().starts_with(':'))).collect();
    let Some(Line::Item { kind, title, see, done_when, .. }) = item(&rest.join(" | ")) else { return None };
    let by = if by.is_empty() { "me".to_string() } else { by };
    Some(Line::IfPicked { id: id_of(head)?, next: crate::desk::model::FollowUp { option, kind, title, see, done_when, by } })
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
    let after = p[1..].iter().find_map(|s| labelled(s, "after")).and_then(item_ref);
    let why = p[1..].iter().find_map(|s| labelled(s, "why")).unwrap_or("").to_string();
    let kind = p[1..].iter().find_map(|s| labelled(s, "kind")).map(|k| k.trim_matches(['"', '`', '*', '.']).to_ascii_lowercase()).filter(|k| matches!(k.as_str(), "bug" | "feature" | "task"));
    let crew = p[1..].iter().find_map(|s| labelled(s, "crew")).map(|c| c.trim_start_matches('@').trim_matches(['"', '`', '*']).trim().to_string()).filter(|c| !c.is_empty());
    let clean = |x: &str| x.trim_matches(['"', '`', '*']).trim().to_string();
    // Short labels, so only with their colon: "on: box", never "one reason".
    fn colon<'a>(s: &'a str, l: &str) -> Option<&'a str> {
        labelled(s, l).filter(|_| s.get(l.len()..).is_some_and(|r| r.trim_start().starts_with(':')))
    }
    let on = p[1..].iter().find_map(|s| colon(s, "on")).map(clean).filter(|m| !m.is_empty());
    let in_dir = p[1..].iter().find_map(|s| colon(s, "in")).map(clean).filter(|m| !m.is_empty());
    let model = p[1..].iter().find_map(|s| colon(s, "model")).map(clean).filter(|m| !m.is_empty());
    // Another repository (#635): its folder, or the project Cue knows it as.
    let repo = p[1..].iter().find_map(|s| colon(s, "repo")).map(clean).filter(|m| !m.is_empty());
    Some(Line::Route { id, to, after, why, kind, crew, on, in_dir, model, repo })
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

/// One of an ASK's choices as the card shows it (#450): its label and the line on what it does, written after a dash
/// ("Merge now — it lands today"); "(Recommended)" anywhere in the label marks the lead's suggestion, kept as the
/// label's suffix (as Claude Code marks one) so the card picks it for you.
pub fn ask_choice(choice: &str) -> (String, String) {
    let (label, what) = [" — ", " – ", " -- ", " - "].iter().find_map(|d| choice.split_once(d)).unwrap_or((choice, ""));
    let low = label.to_lowercase();
    let label = match low.find("(recommended)") {
        Some(at) => format!("{} (Recommended)", format!("{}{}", &label[..at], &label[at + "(recommended)".len()..]).split_whitespace().collect::<Vec<_>>().join(" ")),
        None => label.trim().to_string(),
    };
    (label, what.trim().to_string())
}

/// A choice's label as the lead wrote it: without the "(Recommended)" the card marks it with.
pub fn plain_choice(label: &str) -> &str {
    let t = label.trim_end();
    t.len().checked_sub("(recommended)".len()).filter(|&at| t.is_char_boundary(at) && t[at..].eq_ignore_ascii_case("(recommended)")).map_or(t, |at| t[..at].trim_end())
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

/// What a DONE line for item `id` in `text` names of its work on another machine (#596): its branch and its last
/// commit, from "DONE: #<id> | branch: <branch> | commit: <sha>" (labels in any case; either may be missing, then "").
/// None: no DONE line names `id`.
pub fn done_names(text: &str, id: &str) -> Option<(String, String)> {
    read(text).find_map(|t| {
        let (word, body) = unmarked(t).split_once(':')?;
        if word.trim_end_matches('*').trim() != "DONE" {
            return None;
        }
        let mut p = body.split('|');
        let ids = p.next()?;
        if !ids.split([',', ' ']).filter(|w| w.trim().starts_with('#')).filter_map(id_of).any(|i| i == id) {
            return None;
        }
        let (mut branch, mut commit) = (String::new(), String::new());
        for part in p {
            let Some((label, v)) = part.split_once(':') else { continue };
            let v = v.trim().trim_matches(['`', '*', '"', '.']).trim().to_string();
            match label.trim().to_ascii_lowercase().as_str() {
                "branch" => branch = v,
                "commit" => commit = v.to_ascii_lowercase(),
                _ => {}
            }
        }
        Some((branch, commit))
    })
}

/// "#627 | <what's left>" (or "#627 — …", "#627: …", or "#627" alone): Line::NotDone.
fn not_done(body: &str) -> Option<Line> {
    let rest = body.trim().strip_prefix('#')?;
    let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
    let id = id_of(&rest[..end])?;
    let text = rest[end..].trim_start_matches([':', ',', '.', ')']).trim().trim_start_matches(['|', ':', '-', '—']).trim();
    Some(Line::NotDone { id, text: text.to_string() })
}

/// "#111 | Try: <thing>" or "#111 | Nothing to see: <why>" (labels in any case; the words may hold `|`), or its three
/// steps (#668): "#111 | Where: <screen> | Do: <what to do> | Expect: <what happens>", each needed.
fn check(body: &str) -> Option<Line> {
    let (head, rest) = body.split_once('|')?;
    let head = head.trim();
    if !head.starts_with('#') || head.contains(' ') {
        return None;
    }
    let id = id_of(head)?;
    // The label and its colon: "Try:", "Nothing to see:" (or "Nothing to try:"), "Where:", any case.
    let said = |rest: &str, label: &str| -> Option<String> {
        let rest = rest.trim();
        rest.get(..label.len()).filter(|h| h.eq_ignore_ascii_case(label)).and_then(|_| rest[label.len()..].trim_start().strip_prefix(':')).map(|t| t.trim().to_string())
    };
    if said(rest, "where").is_some() {
        // Each part after a "|" starts a step when it's labelled; one that isn't is the step before's words, with its "|".
        let mut steps = [String::new(), String::new(), String::new()];
        let mut at: Option<usize> = None;
        for part in rest.split('|') {
            let label = ["where", "do", "expect"].iter().position(|l| said(part, l).is_some());
            match (label, at) {
                (Some(n), _) if steps[n].is_empty() => {
                    steps[n] = said(part, ["where", "do", "expect"][n]).unwrap_or_default();
                    at = Some(n);
                }
                (_, Some(n)) => steps[n] = format!("{} | {}", steps[n], part.trim()),
                _ => return None,
            }
        }
        if steps.iter().any(|s| s.trim().is_empty()) {
            return None;
        }
        return Some(Line::Check { id, kind: "try".into(), text: steps[1].clone(), steps: steps.to_vec() });
    }
    let (kind, text) = match said(rest, "try") {
        Some(t) => ("try", t),
        None => ("nothing", said(rest, "nothing to see").or_else(|| said(rest, "nothing to try"))?),
    };
    (!text.is_empty()).then(|| Line::Check { id, kind: kind.into(), text, steps: vec![] })
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
/// `TO LEAD: #<id> | <words>`, `TO LEAD: #<id> <words>` or `TO LEAD: <words>` (#474): Line::ToLead.
fn to_lead(body: &str) -> Option<Line> {
    let body = body.trim();
    let (id, text) = match body.strip_prefix('#') {
        Some(rest) => {
            let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
            let id = id_of(&rest[..end])?;
            let text = rest[end..].trim_start_matches([':', ',', '.', ')']).trim().trim_start_matches(['|', ':', '-', '—']).trim();
            (Some(id), text)
        }
        None => (None, body),
    };
    (!text.is_empty()).then(|| Line::ToLead { id, text: text.to_string() })
}

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

/// `LANDS: <part> -> <choice> | steps: <one> ;; <two> | why: <reason>` (#381): steps only for your own steps. Read by
/// its ` | steps:` and ` | why:` markers, so a step may hold `|` or `;` (`npm test | tail -5`, `cd web; make`); steps
/// are split only on ` ;; ` (lands::STEP_SEP). A why without its label (`| the user asked`) still counts.
fn lands(body: &str) -> Option<Line> {
    let lower = body.to_ascii_lowercase();
    let mark = |label: &str| lower.find(&format!("| {label}:")).or_else(|| lower.find(&format!("|{label}:")));
    let (steps_at, why_at) = (mark("steps"), mark("why"));
    let head_end = [steps_at, why_at].into_iter().flatten().min().unwrap_or(body.len());
    // What follows a marker, up to the other one when it comes after.
    let after = |at: Option<usize>, label: &str, other: Option<usize>| {
        at.map(|i| {
            let end = other.filter(|o| *o > i).unwrap_or(body.len());
            let seg = body[i..end].trim_start_matches('|').trim();
            labelled(seg, label).unwrap_or(seg).to_string()
        })
    };
    let (head, mut why) = match body[..head_end].split_once('|') {
        Some((h, w)) if why_at.is_none() => (h, w.trim().to_string()),
        _ => (&body[..head_end], String::new()),
    };
    let (part, to) = head.split_once("->")?;
    let (part, to) = (part.trim().to_string(), to.trim().to_string());
    if part.is_empty() || to.is_empty() {
        return None;
    }
    let steps = after(steps_at, "steps", why_at)
        .map(|s| s.split(crate::desk::lands::STEP_SEP.trim()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(w) = after(why_at, "why", steps_at) {
        why = w;
    }
    Some(Line::Lands { part, to, steps, why })
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

/// "<name> | #a #b, #c" or "#id | #a #b": a feature and its pieces, two or more (#282). The name may be bold or quoted.
fn feature(body: &str) -> Option<Line> {
    let p = parts(body);
    let head = unwrapped(p.first()?);
    let pieces: Vec<String> = p[1..].iter().flat_map(|s| s.split(|c: char| c.is_whitespace() || c == ',')).filter_map(|w| w.trim_matches(['*', '`', '.']).strip_prefix('#')).filter_map(id_of_bare).collect();
    let (id, name) = match head.strip_prefix('#') {
        Some(h) if !h.contains(char::is_whitespace) => (Some(id_of_bare(h)?), String::new()),
        _ => (None, head.to_string()),
    };
    let mut seen = Vec::new();
    for x in pieces {
        if !seen.contains(&x) && id.as_ref() != Some(&x) {
            seen.push(x);
        }
    }
    (!seen.is_empty() && (id.is_some() || !name.is_empty())).then_some(Line::Feature { id, name, pieces: seen })
}

/// "253" or "new" (a piece's id without its #), lower case; None for anything else.
fn id_of_bare(w: &str) -> Option<String> {
    let w = w.trim_end_matches([')', ':', ';']).to_ascii_lowercase();
    (!w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric())).then_some(w)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn steps_why(body: &str) -> (String, String, Vec<String>, String) {
        match lands(body) {
            Some(Line::Lands { part, to, steps, why }) => (part, to, steps, why),
            _ => panic!("not read: {body}"),
        }
    }

    #[test]
    fn lands_steps_may_hold_a_pipe_or_a_semicolon_and_split_only_on_the_separator() {
        let (part, to, steps, why) = steps_why("check -> command | steps: npm test | tail -5 ;; cd web; make lint | why: the user asked");
        assert_eq!((part.as_str(), to.as_str(), why.as_str()), ("check", "command", "the user asked"));
        assert_eq!(steps, ["npm test | tail -5", "cd web; make lint"]);
        let (_, _, steps, why) = steps_why("after -> command | why: they want a deploy | steps: a; b");
        assert_eq!((steps, why.as_str()), (vec!["a; b".to_string()], "they want a deploy"), "why first, steps after");
        let (_, to, steps, why) = steps_why("check -> none | the user asked");
        assert_eq!((to.as_str(), steps.len(), why.as_str()), ("none", 0, "the user asked"), "a why without its label");
        assert!(lands("check | why: x").is_none(), "no arrow");
    }
    #[test]
    fn a_done_names_its_branch_and_commit() {
        let text = "All in.\nDONE: #a1 | branch: desk/a1 | commit: `ABC123`\nCHECK: #a1 | Try: it";
        assert_eq!(done_names(text, "a1"), Some(("desk/a1".into(), "abc123".into())));
        assert_eq!(done_names("DONE: #a1", "a1"), Some((String::new(), String::new())));
        assert_eq!(done_names("DONE: #a1 | Branch: b", "b2"), None);
        assert_eq!(done_names("```\nDONE: #a1 | commit: abc\n```", "a1"), None);
        assert!(matches!(&lines("DONE: #a1 | branch: desk/a1 | commit: abc")[..], [Line::Done { ids }] if ids == &["a1".to_string()]));
    }
}
