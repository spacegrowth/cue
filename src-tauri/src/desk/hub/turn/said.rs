//! What the Desk reads from its lead at a turn's end (desk/hub/core/desk.rs): the lead's lines (ITEM, ROUTE, DONE, TITLE,
//! SPLIT, CUT, MERGE, KEEP…), and `desk_turn_ended`, which reads every turn's lines.

use crate::desk::hub::prelude::*;
use crate::desk::parse::{self, Line};
use crate::desk::DeskItem;
use crate::desk::hub::core::desk::{Effects, resolve, secs, shown_name};
use crate::desk::hub::turn::route::{By, decide, decide_call, note, route, waits_on};
use crate::desk::hub::turn::heard::{designs_said, desk_line_start, holder_said, kind_of, new_item, replied, session_items};

/// The first "#id" in `text` that's an item of `project`.
fn item_named(st: &Store, project: &str, text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter_map(|w| w.strip_prefix('#')).map(str::to_ascii_lowercase).find(|id| st.desk.item(id).is_some_and(|i| i.project == project))
}

/// An ITEM: line said by session `by` (named `from`): a new item of `project`, or the one of that kind and title
/// not done yet, with you to check too (the line said again). The item's id, and whether it's new.
#[allow(clippy::too_many_arguments)]
pub(crate) fn item_said(st: &mut Store, project: &str, kind: &str, title: &str, see: &str, done_when: &str, from: &str, now: u64, fx: &mut Effects) -> (String, bool) {
    let same_kind = kind_of(kind);
    let open = |i: &&DeskItem| i.project == project && i.kind == same_kind && i.state != "done" && i.title.trim().eq_ignore_ascii_case(title.trim());
    if let Some(open) = st.desk.items.iter().find(open) {
        return (open.id.clone(), false);
    }
    let mut it = new_item(st, project, kind, title, done_when, from, now);
    it.see = see.trim().into();
    let id = it.id.clone();
    fx.items.push(id.clone());
    st.desk.items.push(it);
    (id, true)
}

/// Lead `lsid` of `project` said `text` at its turn's end: its ITEM, ROUTE, ASK and DECIDED lines.
/// `repos`: the repositories its COPY lines name, found before the store was taken (desk/hub/landing/named_copy.rs).
/// `loops`: each project its LOOP lines name, with its folder on this Mac when Cue knows one (Hub::desk_folder).
#[allow(clippy::too_many_arguments)]
fn lead_said(st: &mut Store, project: &str, lsid: &str, text: &str, repos: &[(String, Result<(String, String), String>)], loops: &[(String, Option<String>)], changes: &crate::desk::hold_check::Changes, now: u64, fx: &mut Effects) {
    let name = st.sessions.get(lsid).map(shown_name).or_else(|| st.desk.lead.get(project).map(|l| l.name.clone())).unwrap_or_default();
    let by = By { name: &name, sid: lsid };
    let mut last_new: Option<String> = None;
    // What the lead is told of its routes, in one message once its lines are read (#248).
    let mut answers: Vec<String> = Vec::new();
    let (away, mut back) = (crate::desk::hub::away::away::away_in_force(st, project, now), Vec::new());
    for line in parse::lines(text) {
        match line {
            Line::Item { kind, title, see, done_when, .. } => last_new = Some(item_said(st, project, &kind, &title, &see, &done_when, &name, now, fx).0),
            route @ Line::Route { .. } => route_line(st, project, &by, route, last_new.as_deref(), now, fx, &mut answers),
            Line::Ask { question, choices, item } => ask_said(st, project, lsid, (question, choices, item), away.then_some(&mut back), fx),
            Line::Context { who, what, why } if what == "keep" => keep(st, project, &by, &who, &why, now, fx),
            Line::Done { ids } => lead_done(st, project, lsid, &ids, fx),
            Line::Title { id, title } => retitle(st, project, &by, &id, &title, now, fx),
            Line::See { id, see, done_when } => {
                let id = st.desk.canon(&id);
                if let Err(no) = see_said(st, project, &id, &see, &done_when, now, fx) {
                    answers.push(no);
                }
            }
            Line::Split { id, titles } => split(st, project, &by, &id, &titles, now, fx, &mut answers),
            Line::Cut { id, on, why } => cut(st, project, &by, &id, &on, &why, now, fx, &mut answers),
            Line::Merge { id, into, why } => merge(st, project, &by, &id, &into, &why, now, fx, &mut answers),
            Line::Note { id, text } => {
                let id = st.desk.canon(&id);
                if let Err(no) = note(st, project, &by, &id, &text, now, fx) {
                    answers.push(no);
                }
            }
            Line::Unstick { what, id, commit, why } => {
                // PARK: #new parks the reply's last ITEM (#385). LAND FIRST may name another Desk's item on the same code (#540).
                let id = if what == "land_first" && id != "new" {
                    match crate::desk::across::named(st, project, &id) {
                        Ok(id) => id,
                        Err(no) => {
                            let id = crate::desk::across::split(&id).1;
                            answers.push(format!("Refused: LAND FIRST #{id}: #{id} {no}. You can move up an item on this repository, or ask the user (ASK: #{id} | …)."));
                            continue;
                        }
                    }
                } else {
                    new_or_canon(st, &id, last_new.as_deref())
                };
                if let Err(no) = crate::desk::hub::stall::unstick::unstick(st, project, &by, &what, &id, &commit, &why, now, fx) {
                    answers.push(no);
                }
            }
            Line::Feature { id, name, pieces } => {
                if let Err(no) = crate::desk::hub::turn::features::feature_said(st, project, &by, id, &name, &pieces, last_new.as_deref(), now, fx) {
                    answers.push(no);
                }
            }
            Line::Copy { id, repo, branch, why } => copy_line(st, project, &by, (new_or_canon(st, &id, last_new.as_deref()), repo, branch, why), repos, now, fx, &mut answers),
            Line::Hold { id, area, till, why } => {
                let id = new_or_canon(st, &id, last_new.as_deref());
                answers.push(crate::desk::holds::hold_said(st, project, &by, &id, area, till, why, changes, now, fx));
            }
            Line::Unhold { id } => {
                let id = st.desk.canon(&id);
                answers.push(crate::desk::holds::unhold_said(st, project, &id, fx));
            }
            Line::Tell { to, item, text } => answers.push(crate::desk::tell::tell_said(st, project, &name, lsid, to, item, &text, now, fx)),
            Line::Disagree { to, item, text } => answers.push(crate::desk::escalate::disagree_said(st, project, &name, lsid, &to, item, &text, now, fx)),
            Line::Answer { id, text } => cleanup_said(st, project, &by, &id, Some(text), now, fx, &mut answers),
            Line::Lands { part, to, steps, why } => fx.lands.push(crate::desk::hub::lead::lands::Proposal { lsid: lsid.to_string(), desk: project.to_string(), part, to, steps, why }),
            Line::Pause { on, why } => fx.pause.push((project.to_string(), on, name.clone(), lsid.to_string(), why)),
            Line::Pass { id, .. } => cleanup_said(st, project, &by, &id, None, now, fx, &mut answers),
            Line::Loop(d) => loop_said(st, project, &by, d, loops, now, fx, &mut answers),
            Line::Context { .. } | Line::Designs { .. } | Line::IfPicked { .. } | Line::ToLead { .. } | Line::Check { .. } => {}
        }
    }
    for line in parse::unread_routes(text) {
        answers.push(format!("Couldn't read this line as a route: {line}\nWrite it as ROUTE: #<id> -> <session> | why: <reason> ({}).", parse::route_fix(&line)));
    }
    if !back.is_empty() {
        answers.push(crate::desk::hub::away::away::sent_back_words(&back));
    }
    route_answers(st, project, lsid, answers, fx);
    lead_calls(st, project, &by, text, back, now, fx);
}

/// The lead's LOOP line (#583): a loop drafted for project `d.project`, kept on the Desk as one of the lead's moves
/// with the draft a loop card reads, in that project's folder (`loops`). Nothing starts: you press Start loop on its
/// card. One with no folder Cue knows, or no goal or done when, is refused and the lead is told why.
#[allow(clippy::too_many_arguments)]
fn loop_said(st: &mut Store, project: &str, by: &By, d: crate::desk::parse::LoopDraft, loops: &[(String, Option<String>)], now: u64, fx: &mut Effects, answers: &mut Vec<String>) {
    let Some(folder) = loops.iter().find(|(p, _)| *p == d.project).and_then(|(_, f)| f.clone()) else {
        answers.push(format!("Refused: LOOP: {}: Cue knows no folder of a project by that name on this Mac. You can name the project as its sessions are shown, or ask the user where it is.", d.project));
        return;
    };
    if d.goal.is_empty() || d.done_when.is_empty() {
        answers.push(format!("Refused: LOOP: {}: a loop needs goal: and done when:. Write the line again with both.", d.project));
        return;
    }
    let mut draft = json!({ "name": d.name, "project": d.project, "folder": folder, "goal": d.goal, "done_when": d.done_when, "check": d.check,
        "decisions": if d.decisions.is_empty() { "one-way" } else { d.decisions.as_str() } });
    for (k, v) in [("rounds", d.rounds.map(Value::from)), ("time_min", d.time_min.map(Value::from)), ("spend_usd", d.spend_usd.map(Value::from))] {
        if let Some(v) = v {
            draft[k] = v;
        }
    }
    // Said again in a later reply: the card is there already.
    if st.desk.decisions.iter().any(|x| x.project == project && x.act == "loop" && x.draft.as_ref() == Some(&draft)) {
        return;
    }
    let name = if d.name.is_empty() { "a loop".to_string() } else { format!("\"{}\"", d.name) };
    decide(st, project, None, by.name, by.sid, &format!("Drafted {name} for {}, for you to start", d.project), "", now, fx);
    if let Some(x) = st.desk.decisions.last_mut() {
        x.act = "loop".into();
        x.undo = "none".into();
        x.draft = Some(draft);
    }
}

/// The lead's ROUTE line (`#new`: the reply's last ITEM). On another machine (#500) only a new session starts there,
/// and only a route made (not refused, so not answered) says on its item where it starts.
#[allow(clippy::too_many_arguments)]
fn route_line(st: &mut Store, project: &str, by: &By, line: Line, last_new: Option<&str>, now: u64, fx: &mut Effects, answers: &mut Vec<String>) {
    let Line::Route { id, to, after, why, kind, on, in_dir, .. } = line else { return };
    let id = match last_new.filter(|_| id == "new") {
        Some(n) => n.to_string(),
        None => id,
    };
    let canon = st.desk.canon(&id);
    let set = match crate::desk::hub::landing::remote_item::route_on(st, project, &canon, &to, on.as_deref(), in_dir.as_deref()) {
        Err(no) => return answers.push(no),
        Ok(set) => set,
    };
    let before = answers.len();
    route_said(st, project, by, (id, to, after, why, kind), last_new, now, fx, answers);
    if let (Some(set), true) = (set, answers.len() == before) {
        if let Some(it) = st.desk.item_mut(&canon) {
            it.on_machine = set;
            fx.items.push(canon);
        }
    }
}

/// An item named on a lead's line: `#new` is the reply's last ITEM (`last_new`), else its number from now.
fn new_or_canon(st: &Store, id: &str, last_new: Option<&str>) -> String {
    match last_new.filter(|_| id == "new") {
        Some(n) => n.to_string(),
        None => st.desk.canon(id),
    }
}

/// The lead's COPY line (`id`, `repo`, `branch`, `why`), its repository looked up in `repos` before the store was
/// taken (desk/hub/landing/named_copy.rs); a refusal into `answers`.
#[allow(clippy::too_many_arguments)]
fn copy_line(st: &mut Store, project: &str, by: &By, (id, repo, branch, why): (String, String, String, String), repos: &[(String, Result<(String, String), String>)], now: u64, fx: &mut Effects, answers: &mut Vec<String>) {
    let found = repos.iter().find(|(r, _)| *r == repo).map(|(_, f)| f.clone()).unwrap_or_else(|| Err("Cue didn't look it up".into()));
    if let Err(no) = crate::desk::hub::landing::named_copy::copy_said(st, project, by, &id, found, &branch, &why, now, fx) {
        answers.push(no);
    }
}

/// The lead's ROUTE line (`id`, `to`, `after`, `why`, `kind`), `#new` naming the reply's last ITEM (`last_new`); what
/// it's told of it into `answers`.
#[allow(clippy::too_many_arguments)]
fn route_said(st: &mut Store, project: &str, by: &By, line: (String, String, Option<String>, String, Option<String>), last_new: Option<&str>, now: u64, fx: &mut Effects, answers: &mut Vec<String>) {
    let (id, to, after, why, kind) = line;
    // An old tag ("#d7k2") names the item by its number from now.
    let id = if id == "new" { last_new.map(str::to_string) } else { Some(st.desk.canon(&id)) };
    let Some(id) = id else {
        answers.push("Refused: ROUTE #new has no ITEM line before it in this reply. You can add the ITEM: line first, or route the item by its number.".into());
        return;
    };
    // After another Desk's item (#540): only one on the same code (desk/across.rs).
    let after = match after.as_deref() {
        Some("new") => last_new.map(str::to_string),
        Some(a) => match crate::desk::across::named(st, project, a) {
            Ok(a) => Some(a),
            Err(why) => {
                let a = crate::desk::across::split(a).1;
                answers.push(format!("Refused: #{id} can't wait after #{a}: #{a} {why}. You can route #{id} without after:, or ask the user (ASK: #{id} | …)."));
                return;
            }
        },
        None => None,
    };
    // The lead says what it is as it routes it (before, so whoever takes it is told).
    if let (Some(k), Some(it)) = (kind, st.desk.item_mut(&id).filter(|i| i.project == project)) {
        it.kind = k;
        fx.items.push(id.clone());
    }
    // A route made isn't answered: the lead knows what it said, and each answer costs it a turn.
    if let Some(said) = route(st, project, by, &id, &to, after, &why, now, fx).filter(|s| !s.starts_with("Taken:")) {
        answers.push(said);
    }
}

/// Lead `lsid`'s ASK line: a card asking you, or, while the owner is away (`back`: Some), kept to send back to it to
/// decide itself (#471). About an item of this Desk's: it shows on that item as waiting on you (`shown`).
type Ask = (String, Vec<String>, Option<String>);
fn ask_said(st: &Store, project: &str, lsid: &str, (question, choices, item): Ask, back: Option<&mut Vec<(String, Option<String>)>>, fx: &mut Effects) {
    let item = item.map(|i| st.desk.canon(&i)).filter(|i| st.desk.item(i).is_some_and(|x| x.project == project));
    match back {
        Some(back) => back.push((question, item)),
        None => fx.asks.push((lsid.to_string(), project.to_string(), question, choices, item)),
    }
}

/// Lead `by`'s DECIDED lines in `text`, each a call on the Desk. Those made in place of ASKs sent back before this
/// turn are marked with them; this turn's ASKs sent back (`back`, #471) wait for its next calls.
fn lead_calls(st: &mut Store, project: &str, by: &By, text: &str, back: Vec<(String, Option<String>)>, now: u64, fx: &mut Effects) {
    let first = st.desk.decisions.len();
    for (what, why, minor) in crate::sessions::crew_log::calls_marked(text) {
        let item = item_named(st, project, &what).or_else(|| item_named(st, project, &why));
        decide_call(st, project, item, by.name, by.sid, (&what, &why, minor), true, now, fx);
    }
    crate::desk::hub::away::away::answered_by_calls(st, by.sid, first, fx);
    if !back.is_empty() {
        st.desk.run.away_asked.entry(by.sid.to_string()).or_default().extend(back);
    }
}

/// The lead's ANSWER (Some) or PASS (None) on item `id`'s landing clean-up question (desk/hub/lead/cleanup.rs); told when
/// none waits on it.
#[allow(clippy::too_many_arguments)]
fn cleanup_said(st: &mut Store, project: &str, by: &By, id: &str, answer: Option<String>, now: u64, fx: &mut Effects, told: &mut Vec<String>) {
    let id = st.desk.canon(id);
    match crate::desk::hub::lead::cleanup::card_for(st, project, &id) {
        Some(card) => {
            // Answered, it's one of the things the lead did on its own (#360): a step on the item.
            if let Some(a) = &answer {
                decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Answered #{id}'s session: {a}"), "", now, fx);
                if let Some(d) = st.desk.decisions.last_mut() {
                    d.undo = "none".into();
                    d.act = "answer".into();
                }
            }
            fx.cleanup.push((card, answer));
            // It answers what its holder asked you too, if it asked (#559).
            if st.desk.item_mut(&id).and_then(|i| i.asks.take()).is_some() {
                fx.items.push(id.clone());
            }
        }
        // No card, but its holder asked the user something (#559): the lead's answer goes to it, as a note would,
        // and the item no longer waits on the user.
        None if answer.is_some() && st.desk.item(&id).is_some_and(|i| i.project == project && i.asks.is_some()) => {
            if let Err(no) = note(st, project, by, &id, &format!("Answer to your question: {}", answer.unwrap_or_default()), now, fx) {
                told.push(no);
            }
        }
        None => told.push(format!("No question from #{id} waits on you: nothing to answer or pass.")),
    }
}

/// The lead cut a link in a circle (CUT:): item `id` no longer waits on item `on` (desk/hub/core/waits.rs). Its commit, when
/// that's what waited, lands at the next finishing tick. Noted as its decision; the lead told when there's no such
/// link.
#[allow(clippy::too_many_arguments)]
fn cut(st: &mut Store, project: &str, by: &By, id: &str, on: &str, why: &str, now: u64, fx: &mut Effects, told: &mut Vec<String>) {
    let id = st.desk.canon(id);
    if !st.desk.item(&id).is_some_and(|i| i.project == project) {
        told.push(format!("#{id} not cut: this Desk has no item #{id}"));
        return;
    }
    match crate::desk::hub::core::waits::cut_in(st, &id, on, now) {
        Ok(done) => {
            fx.items.extend(done.items);
            fx.sends.extend(done.sends);
            fx.news.extend(done.news);
            decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Cut its wait on #{}", st.desk.canon(on)), why, now, fx);
        }
        Err(e) => told.push(format!("Not cut: {e}")),
    }
}

/// The lead folded item `id`, a repeat of item `into`, into it (MERGE:): its words and pictures are added to `into`'s
/// (its holder, at work on it, told them), anything queued after it waits on `into` instead, and it leaves the list.
/// Only one not begun: no session is started for it (one at work on it, or to check, stays; the lead is told why).
#[allow(clippy::too_many_arguments)]
fn merge(st: &mut Store, project: &str, by: &By, id: &str, into: &str, why: &str, now: u64, fx: &mut Effects, told: &mut Vec<String>) {
    let (id, into) = (st.desk.canon(id), st.desk.canon(into));
    let note = |told: &mut Vec<String>, why: String| told.push(format!("#{id} not merged: {why}"));
    if id == into {
        return;
    }
    let Some(copy) = st.desk.item(&id).filter(|i| i.project == project).cloned() else { return note(told, format!("this Desk has no item #{id}")) };
    let Some(target) = st.desk.item(&into).filter(|i| i.project == project).cloned() else { return note(told, format!("this Desk has no item #{into}")) };
    if !matches!(copy.state.as_str(), "new" | "queued") {
        let who = copy.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        return note(told, if copy.state == "done" { "it's done already".into() } else { format!("{who} has begun it already") });
    }
    if target.state == "done" {
        return note(told, format!("#{into} is done; route #{id} instead"));
    }
    if waits_on(st, &into, &id) {
        return note(told, format!("#{into} waits on it; CUT that wait first"));
    }
    let said = if copy.words.is_empty() { copy.title.clone() } else { copy.words.clone() };
    let added: Vec<String> = copy.images.iter().filter(|p| !target.images.contains(p)).cloned().collect();
    if let Some(t) = st.desk.item_mut(&into) {
        if t.words.is_empty() {
            t.words = t.title.clone();
        }
        t.words = format!("{}\n\nFrom #{id}: {said}", t.words.trim_end());
        t.images.extend(added.iter().cloned());
        t.updated = secs(now);
    }
    // Its holder at work on it hears what came with the repeat.
    if let Some(h) = target.holder.as_ref().filter(|_| target.state == "working") {
        let pics = added.iter().map(|p| format!(" `{p}`")).collect::<String>();
        fx.sends.push((h.to().to_string(), format!("Desk item #{id} was folded into #{into}, which you have. It said: {said}{}", if pics.is_empty() { String::new() } else { format!("\nPictures:{pics}") })));
    }
    for it in st.desk.items.iter_mut().filter(|i| i.after.as_deref() == Some(id.as_str())) {
        it.after = Some(into.clone());
        it.updated = secs(now);
        fx.items.push(it.id.clone());
    }
    st.desk.items.retain(|i| i.id != id);
    fx.dropped.push(id.clone());
    fx.items.push(into.clone());
    decide(st, project, Some(into.clone()), by.name, by.sid, &format!("Folded #{id} into it"), why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
}

/// What lead `lsid` of `project` is told of the lines of the turn that just ended (#248), in one message: each
/// refused route, each SPLIT, MERGE or CUT not made, and each line Cue couldn't read (once, when said twice). A repeat said again later is answered
/// again: the lead is never left thinking a refused route was made. Not "Asked:" (the user's card is in the lead's
/// name, and it hears the answer). With the lead asking the user something (a card from this turn, or before), a
/// message can't reach it: it's told in its next Desk block instead (`routes_held`).
fn route_answers(st: &mut Store, project: &str, lsid: &str, answers: Vec<String>, fx: &mut Effects) {
    let mut out: Vec<String> = Vec::new();
    for said in answers.into_iter().filter(|s| !s.starts_with("Asked:")) {
        if !out.contains(&said) {
            out.push(said);
        }
    }
    if out.is_empty() || st.sessions.get(lsid).is_none() {
        return;
    }
    let asking = fx.uses.iter().any(|u| u.lead == lsid)
        || fx.asks.iter().any(|a| a.0 == lsid)
        || fx.lands.iter().any(|p| p.lsid == lsid)
        || st.sessions.state(lsid).as_deref() == Some("deciding");
    if asking {
        let held = st.desk.run.routes_held.entry(project.to_string()).or_default();
        for line in out {
            held.push_str(&line);
            held.push('\n');
        }
    } else {
        fx.sends.push((lsid.to_string(), out.join("\n")));
    }
}

/// The lead gave item `id` a short plain title (TITLE:): the words it was added with are kept (`words`), shown on its
/// page. Not for a done item.
fn retitle(st: &mut Store, project: &str, by: &By, id: &str, title: &str, now: u64, fx: &mut Effects) {
    let id = st.desk.canon(id);
    let Some(it) = st.desk.item_mut(&id).filter(|i| i.project == project && i.state != "done") else { return };
    if it.title == title.trim() {
        return;
    }
    if it.words.is_empty() {
        it.words = it.title.clone();
    }
    it.title = title.trim().to_string();
    it.updated = secs(now);
    fx.items.push(id.clone());
    decide(st, project, Some(id), by.name, by.sid, &format!("Named it \"{}\"", title.trim()), "", now, fx);
}

/// The lead gave item `id` its two lines (SEE:, #382): what you'll see when it's done, and the check its session runs.
/// Either may be left out to keep what it has. Told when there's no such item.
fn see_said(st: &mut Store, project: &str, id: &str, see: &str, done_when: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let it = st.desk.item_mut(id).filter(|i| i.project == project).ok_or_else(|| format!("Refused: SEE #{id}: this Desk has no item #{id}."))?;
    let (see, done_when) = (see.trim(), done_when.trim());
    let mut changed = false;
    if !see.is_empty() && it.see != see {
        it.see = see.into();
        changed = true;
    }
    if !done_when.is_empty() && it.done_when != done_when {
        it.done_when = done_when.into();
        changed = true;
    }
    if changed {
        it.updated = secs(now);
        fx.items.push(id.to_string());
    }
    Ok(())
}

/// The lead split item `id` (SPLIT:): it takes the first title, and each of the rest becomes a new item of the same
/// kind, with the same words and pictures, from whoever added it; the lead is told their numbers to route them. Only
/// one not given out yet: work begun on it stays one item (the lead is told why).
#[allow(clippy::too_many_arguments)]
fn split(st: &mut Store, project: &str, by: &By, id: &str, titles: &[String], now: u64, fx: &mut Effects, told: &mut Vec<String>) {
    let id = st.desk.canon(id);
    let Some(it) = st.desk.item(&id).filter(|i| i.project == project).cloned() else {
        told.push(format!("#{id} not split: this Desk has no item #{id}"));
        return;
    };
    if it.holder.is_some() || it.state != "new" {
        let who = it.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        let why = if it.state == "done" { "it's done already".to_string() } else { format!("{who} has it already; add the other part as an ITEM") };
        told.push(format!("#{id} not split: {why}"));
        return;
    }
    let words = if it.words.is_empty() { it.title.clone() } else { it.words.clone() };
    let mut ids = vec![];
    for t in &titles[1..] {
        let mut n = new_item(st, project, &it.kind, t, "", &it.from, now);
        n.words = words.clone();
        n.images = it.images.clone();
        n.split_from = id.clone();
        ids.push(n.id.clone());
        fx.items.push(n.id.clone());
        fx.news.push((project.to_string(), n.id.clone()));
        st.desk.items.push(n);
    }
    retitle(st, project, by, &id, &titles[0], now, fx);
    if let Some(o) = st.desk.item_mut(&id) {
        o.words = words;
        // Told again with its new title (with the new ones), to route it with them.
        fx.news.push((project.to_string(), id.clone()));
    }
    let list = ids.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ");
    decide(st, project, Some(id.clone()), by.name, by.sid, &format!("Split off {list}"), "", now, fx);
    for n in &ids {
        decide(st, project, Some(n.clone()), by.name, by.sid, &format!("Split from #{id}"), "", now, fx);
    }
}

/// The lead said DONE on `ids`. Its DONE sends to check what it holds itself at work (holder_said), and closes for
/// you, as Looks right would, an item finishing or to check (#337: you told it it's done): at once when it's to check,
/// else once its finishing brings it there, its commit and install still made. For any other item Cue tells it, in
/// one short message shown in its chat, why the item stays as it is ("#69 not closed: Ann has it"), so nothing it
/// says is dropped without a word. Once while the item stays as it is.
fn lead_done(st: &mut Store, project: &str, lsid: &str, ids: &[String], fx: &mut Effects) {
    let mut notes = Vec::new();
    for id in ids {
        let id = st.desk.canon(id);
        let Some(it) = st.desk.item(&id).filter(|i| i.project == project) else {
            notes.push(format!("#{id} not closed: this Desk has no item #{id}"));
            continue;
        };
        let mine = it.holder.as_ref().is_some_and(|h| h.is(lsid));
        if mine && matches!(it.state.as_str(), "working" | "queued") {
            continue;
        }
        if matches!(it.state.as_str(), "to_check" | "finishing") {
            if let Some(it) = st.desk.item_mut(&id) {
                it.close_on_check = true;
            }
            fx.items.push(id.clone());
            fx.closes.push(id);
            continue;
        }
        // Told once while it stays as it is: a change to it (its state, a check, a route) lets the next DONE be told.
        let seen = format!("{}@{}", it.state, it.updated);
        if st.desk.run.done_told.get(&id) == Some(&seen) {
            continue;
        }
        let holder = it.holder.as_ref().map(|h| h.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        let why = match it.state.as_str() {
            "done" => "it's done already".to_string(),
            "working" | "queued" => format!("{holder} has it; its own DONE sends it to your check"),
            _ => "no one has worked on it yet".to_string(),
        };
        st.desk.run.done_told.insert(id.clone(), seen);
        notes.push(format!("#{id} not closed: {why}"));
    }
    if !notes.is_empty() {
        notes.push(DONE_NOTE_FOR_LEAD.to_string());
        fx.sends.push((lsid.to_string(), notes.join("\n")));
    }
}

/// The last line of Cue's note on a lead's DONE it couldn't act on: for the lead, not shown in its chat
/// (ui/desk/lead.js deskDoneNotes).
const DONE_NOTE_FOR_LEAD: &str = "Say DONE for an item only once it's finishing or waiting on the user's check. Don't say DONE for these again.";

/// The lead said to keep session `who`'s context ("me": its own) as it is: Cue asks it for no hand-off note for the
/// idle stretch it's in (cache_tick's Desk rule), until it next works; said while it works, for the idle stretch
/// after this turn (the turn's end stamps it: `keep_from_now`). A decision of the lead's, which can be undone.
#[allow(clippy::too_many_arguments)]
fn keep(st: &mut Store, project: &str, by: &By, who: &str, why: &str, now: u64, fx: &mut Effects) {
    let sid = if who.eq_ignore_ascii_case("me") {
        Some(by.sid.to_string())
    } else {
        // A session, not one of a lead's crew: a crew member's context is its lead's.
        resolve(st, project, who).filter(|h| h.kind != "crew").map(|h| h.sid)
    };
    let Some(sid) = sid else { return };
    let name = if sid == by.sid { "its own".to_string() } else { st.sessions.get(&sid).map(|s| format!("{}'s", shown_name(s))).unwrap_or_default() };
    decide(st, project, None, by.name, by.sid, &format!("Kept {name} context as it is"), why, now, fx);
    let id = fx.decisions.last().cloned().unwrap_or_default();
    // The lead's own turn is ending as it says so: its idle stretch starts now.
    let working = sid != by.sid && st.sessions.get(&sid).is_some_and(|s| s.state == "working");
    st.desk.run.keep.insert(sid, (now, id, working));
}

/// The lines any session's turn may say, lead or not: DESIGNS:, IF PICKED: (#543) and TO LEAD:.
fn any_session_said(st: &mut Store, sid: &str, text: &str, fx: &mut Effects) {
    designs_said(st, sid, text, fx);
    crate::desk::hub::turn::heard::next_said(st, sid, text, fx);
    crate::desk::hub::turn::heard::to_lead_said(st, sid, text, fx);
}

impl Hub {
    /// Session `sid`'s turn ended, saying `text`: a lead's lines (ITEM, ROUTE, ASK, DECIDED, and DONE for what it
    /// keeps), any session's ITEM lines, and a holder's (DONE, DECIDED) acted on; an item it ended on with no DONE
    /// is asked about once (desk/hub/stall/stall.rs). A lead's Desk changes sent before it are heard (desk/feed.rs).
    /// `turn`: its parts, so a reply keeps what a Stop hook made it write apart from its answer.
    /// A turn just ended, so a holder may have stopped: when each holder last wrote is looked at first (off the store's
    /// lock), so its item reads as still at work or to resume at once, not a second later (view.rs wrote_just_now).
    pub(crate) fn desk_turn_ended(&self, sid: &str, text: &str, turn: &[crate::model::TurnPart]) {
        self.look_at_holder_logs(now_ms());
        self.desk_turn_ended_said(sid, text, turn);
    }

    /// `desk_turn_ended`'s work: the turn's lines acted on.
    fn desk_turn_ended_said(&self, sid: &str, text: &str, turn: &[crate::model::TurnPart]) {
        self.keep_from_now(sid);
        self.desk_feed_turn_ended(sid);
        // Away mode ends by itself (its end time, its budget): noticed as a Desk session's turn ends.
        self.desk_away_check(sid);
        // Most turns say none of the Desk's lines and end on no item at work: then the store is only read.
        // A route in the wrong form counts too: the lead is told it couldn't be read (#248).
        let said = text.lines().any(|l| desk_line_start(l) || parse::route_like(l));
        let (held, done_held) = self.read(|st| (crate::desk::hub::stall::stall::at_work_with(st, sid), st.desk.run.done_held.contains_key(sid)));
        // Items it was answering your question on (#475): settled as its turn ends (desk/hub/turn/asked.rs).
        let asked: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.asked.is_some() && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| i.id.clone()).collect());
        if !said && held.is_empty() && !done_held && asked.is_empty() {
            return;
        }
        // Each item at work with it: its work git says isn't committed yet, asked with the store let go (#213).
        let loose: Vec<(String, Vec<String>)> = held.iter().chain(asked.iter().filter(|a| !held.contains(a))).map(|id| (id.clone(), self.desk_uncommitted(id))).collect();
        let from_disk = self.feature_pieces_on_disk(text);
        let from = self.read(|st| st.sessions.get(sid).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone())).unwrap_or_default();
        let repos = crate::desk::hub::landing::named_copy::repos_said(text, &from);
        // The folder of each project a LOOP line names (#583), found before the store is taken.
        let loops: Vec<(String, Option<String>)> = if said {
            parse::lines(text).into_iter().filter_map(|l| if let Line::Loop(d) = l { Some(d.project) } else { None }).map(|p| { let f = self.desk_folder(&p); (p, f) }).collect()
        } else {
            Vec::new()
        };
        // A HOLD lists the items whose copies already changed a held file (#540): their changes read off disk here.
        let changes = if said && parse::lines(text).iter().any(|l| matches!(l, Line::Hold { .. })) {
            crate::desk::hold_check::copies_changed(self.read(crate::desk::hold_check::open_copies))
        } else {
            Vec::new()
        };
        let now = now_ms();
        let fx = self.write(|st| {
            crate::desk::hub::turn::features::bring_back(st, from_disk);
            let mut fx = Effects::default();
            // A DONE it said while its crew or agents worked: they're done now, so it goes ahead (#393).
            crate::desk::hub::turn::heard::held_done(st, sid, now, &mut fx);
            for (id, files) in &loose {
                if let Some(it) = st.desk.item_mut(id) {
                    it.loose = files.clone();
                }
            }
            // What it was at work on as its turn ended: what this turn's DONE lines don't close stalled there.
            // Its reply goes only to the items it's tagged with (#176): the one the message it answers names, and
            // those its DONE lines name; untagged, to none. A turn that ends untagged with one item at work still
            // stalls there: Cue's ask names the item, so the answer to it is tagged.
            let held = crate::desk::hub::stall::stall::at_work_with(st, sid);
            let mut on = crate::desk::hub::stall::stall::about(st, text, &held, crate::desk::hub::stall::stall::now_on(st, sid).as_deref());
            replied(st, sid, text, turn, &on, now, &mut fx);
            crate::desk::hub::turn::asked::answered(st, sid, text, &loose, now, &mut fx);
            if on.is_empty() && held.len() == 1 {
                on = held;
            }
            if !said {
                crate::desk::hub::stall::stall::stalls_asked(st, sid, text, &on, now, &mut fx);
                return fx;
            }
            any_session_said(st, sid, text, &mut fx);
            let lead = st.desk.lead_of(sid);
            match &lead {
                Some(project) => lead_said(st, project, sid, text, &repos, &loops, &changes, now, &mut fx),
                None => session_items(st, sid, text, now, &mut fx),
            }
            if st.desk.items.iter().any(|i| i.holder.as_ref().is_some_and(|h| h.is(sid))) {
                holder_said(st, sid, text, lead.is_none(), now, &mut fx);
            }
            crate::desk::hub::stall::stall::stalls_asked(st, sid, text, &on, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Session `sid`'s background agents have finished and it's at its prompt: a DONE it said while they worked goes
    /// ahead now (#393). Only read unless there's one.
    pub(crate) fn desk_helpers_done(&self, sid: &str) {
        if !self.read(|st| st.desk.run.done_held.contains_key(sid)) {
            return;
        }
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            crate::desk::hub::turn::heard::held_done(st, sid, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Session `sid`'s turn ended: a keep the lead said while it worked holds from now, for the idle stretch this
    /// starts (until its next reply). Only read unless there's one.
    fn keep_from_now(&self, sid: &str) {
        if !self.read(|st| st.desk.run.keep.get(sid).is_some_and(|k| k.2)) {
            return;
        }
        let now = now_ms();
        self.write(|st| {
            if let Some(k) = st.desk.run.keep.get_mut(sid).filter(|k| k.2) {
                (k.0, k.2) = (now, false);
            }
        });
    }
}
