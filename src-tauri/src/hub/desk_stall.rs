//! An item whose holder's turn ended on it with no DONE: it waits on you (shown with the holder's last line), and Cue
//! asks the holder once, "Done? Write DONE: #N, or say what's left". The turn that answers isn't asked again, nor one
//! that ends on an ask; once a turn says what's left or what it waits on, none is while the item is unchanged.
//! Waiting isn't stopping (#249): a turn that ends saying it waits (on a build, tests, a helper, another commit) shows
//! the item as waiting on that, and the Desk lead isn't told; so is a turn that ends with its own tests or build still
//! running in the background (#435). A stop with no question for you is the lead's, each time: it waits on you only
//! when its holder asked you something, the lead asks you (ASK: #id …), or the Desk has no lead at hand.

use super::desk::{ask_of, at_work, crew_parts, Effects};
use super::*;
use crate::desk::{DeskItem, Holder};
use crate::sessions::Session;

/// The live session of `it`'s holder when its turn ended on `it` and it's at its prompt: its last reply came after
/// the item last went to it, and none of its crew is still at work. None for a crew member or a project's lead.
pub(super) fn stalled<'a>(st: &'a Store, it: &DeskItem, h: &Holder) -> Option<&'a Session> {
    // A crew member's turns aren't its lead's; a project's lead's turns are the Desk's routing.
    if crew_parts(h).is_some() || st.desk.lead_of(&h.sid).is_some() {
        return None;
    }
    let s = st.sessions.get(&h.sid).filter(|s| matches!(s.state.as_str(), "waiting" | "stopped") && !crew_at_work(s))?;
    let held = it.held_from.as_ref().filter(|(by, _)| *by == h.sid).map_or(it.updated * 1000, |(_, at)| *at);
    let from = st.desk.sent_at.get(&it.id).filter(|(by, _)| *by == h.sid).map_or(held, |(_, at)| held.max(*at));
    s.thread.iter().rev().find(|e| e.role == "agent").filter(|e| e.at_ms > from).map(|_| s)
}

/// One of session `s`'s crew is still at work: its turn ended, but the work handed to them goes on.
fn crew_at_work(s: &Session) -> bool {
    s.team.iter().any(|t| t.working())
}

/// Session `sid` (itself, not a crew member of it) is still at it as its turn ends: its background agents run on, or
/// one of its crew is at work. A DONE it says then waits (the item stays at work), and it isn't asked if it's done.
pub(super) fn still_at_it(st: &Store, sid: &str) -> bool {
    let h = Holder { kind: "session".into(), sid: sid.to_string(), ..Default::default() };
    at_work(st, &h) || st.sessions.get(sid).is_some_and(crew_at_work)
}

/// The last line of a session's last words.
pub(super) fn last_line(s: &Session) -> String {
    s.thread.iter().rev().find(|e| e.role == "agent").and_then(|e| e.text.lines().map(str::trim).rfind(|l| !l.is_empty())).unwrap_or("").chars().take(160).collect()
}

/// The item session `sid` (itself, not a crew member of it) is on now, of the ones at work with it (#176): the one the
/// last message it was sent names first. A message naming no item at all, to a session at work on just one: that one
/// (#395: typed in its own chat, it can only be about it). Else none.
pub(super) fn now_on(st: &Store, sid: &str) -> Option<String> {
    let held: Vec<&DeskItem> = st.desk.items.iter().filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty())).collect();
    // The message it answers: the newest one to it. Its tag is the held item it names first (a hand-off, a message
    // from an item's page, Cue's "Done? Write DONE: #id"); none named, untagged.
    let last = st.sessions.get(sid)?.thread.iter().rev().find(|e| e.role == "you" && !e.decision)?;
    let named = held.iter().filter_map(|i| names_at(&last.text, &i.id).map(|at| (at, i))).min_by_key(|(at, _)| *at).map(|(_, i)| i.id.clone());
    if named.is_some() || held.len() != 1 || st.desk.lead_of(sid).is_some() {
        return named;
    }
    // Untagged: on the one it holds, unless the message names another item (one queued for it, say).
    let other = st.desk.items.iter().any(|i| names_at(&last.text, &i.id).is_some());
    (!other).then(|| held[0].id.clone())
}

/// Item `id`, queued, is the one session `sid` (itself) is on anyway (#342): it's on no item at work, and the last
/// message it was sent names `id` (the lead or you started it while it waited for a copy or for the item before).
pub(super) fn on_queued(st: &Store, sid: &str, id: &str) -> bool {
    st.desk.lead_of(sid).is_none()
        && now_on(st, sid).is_none()
        && st.sessions.get(sid).and_then(|s| s.thread.iter().rev().find(|e| e.role == "you" && !e.decision)).is_some_and(|e| names_at(&e.text, id).is_some())
}

/// Where `text` first names item `id` as "#id" (not the start of a longer number or name), if it does.
fn names_at(text: &str, id: &str) -> Option<usize> {
    let want = format!("#{id}");
    text.match_indices(&want).map(|(at, _)| at).find(|at| !text[at + want.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
}

/// Of `on` (the items at work with session `sid` as its turn ended), the ones its words in `text` are about: the ones
/// its DONE lines name, and the one it's on now (`now`, from before this turn's lines changed anything). Untagged
/// words are on none: they show only in the session's own chat.
pub(super) fn about(st: &Store, text: &str, on: &[String], now: Option<&str>) -> Vec<String> {
    use crate::desk::parse::{self, Line};
    let done: Vec<String> = parse::lines(text).into_iter().filter_map(|l| if let Line::Done { ids } = l { Some(ids) } else { None }).flatten().map(|i| st.desk.canon(&i)).collect();
    on.iter().filter(|id| done.contains(id) || now == Some(id.as_str())).cloned().collect()
}

/// The items at work with session `sid` itself (not a crew member of it) since before this turn's end: the ones its
/// turn could stall on. None for a project's lead: its turns are the Desk's routing, not an item's work.
pub(super) fn at_work_with(st: &Store, sid: &str) -> Vec<String> {
    if st.desk.lead_of(sid).is_some() {
        return Vec::new();
    }
    st.desk
        .items
        .iter()
        .filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty()))
        .filter(|i| i.held_from.as_ref().is_some_and(|(by, _)| by == sid))
        .map(|i| i.id.clone())
        .collect()
}

/// What Cue asks a holder whose turn ended on item `id` with no DONE.
pub(super) fn stall_words(id: &str) -> String {
    format!("Done? Write DONE: #{id}, or say what's left")
}

/// The turn says what's left of its work, or what it waits on: nothing to ask it while its item is unchanged.
fn says_whats_left(text: &str) -> bool {
    let t = text.to_lowercase().replace('\u{2019}', "'");
    ["what's left", "left to do", "still to do", "remaining:", "not done yet", "blocked on", "waiting on", "waits on", "waiting for", "waits for"].iter().any(|w| t.contains(w))
}

/// What the turn ends saying it waits on, in one line (#249): a line near its end that waits on something ("Waiting
/// on the test run"), or says work it started runs on ("still compiling", "The build for 57d72d4e has started").
pub(super) fn waits_line(text: &str) -> Option<String> {
    const WAITS: [&str; 8] = ["waiting on", "waits on", "waiting for", "waits for", "wait for", "blocked on", "blocked by", "once the"];
    const WORK: [&str; 10] = ["build", "test", "compil", "check", "install", "helper", "agent", "teammate", "review", "commit"];
    const RUNS: [&str; 13] = ["started", "is running", "are running", "still running", "in progress", "under way", "underway", "kicked off", "compiling", "still building", "to finish", "finishes", "lands"];
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !["DONE:", "ITEM:", "DESIGNS:", "DECIDED:", "DECISION:", "ROUTE:", "ASK:"].iter().any(|p| l.starts_with(p)))
        .collect();
    lines.iter().rev().take(3).find_map(|l| {
        let t = l.to_lowercase().replace('\u{2019}', "'");
        let waits = WAITS.iter().any(|w| t.contains(w)) || (WORK.iter().any(|w| t.contains(w)) && RUNS.iter().any(|w| t.contains(w)));
        waits.then(|| l.trim_start_matches(['-', '*', '>', ' ']).replace("**", "").replace('`', "").trim().chars().take(160).collect())
    })
}

/// What a holder's turn asks you, in one line (#475), if anything: the question on its last line (its Desk lines,
/// DECIDED and the like, left out), even with a word on what happens next after it ("Which one: A, B or C? Once
/// it's built I'll say DONE."), or a line saying it waits on you ("I'm waiting for your pick of A, B or C").
pub(crate) fn question_for_you(text: &str) -> Option<String> {
    let mut fenced = false;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| {
            if l.starts_with("```") {
                fenced = !fenced;
                return false;
            }
            !fenced && !l.is_empty() && !super::desk_heard::desk_line_start(l)
        })
        .collect();
    let last = *lines.last()?;
    if let Some(q) = question_in(last) {
        return Some(q);
    }
    // A question, then one sentence on what comes after it.
    if let Some(at) = last.rfind("? ") {
        let after = last[at + 2..].trim().to_lowercase().replace('\u{2019}', "'");
        const NEXT: [&str; 8] = ["once ", "then ", "after ", "when ", "if ", "i'll ", "i will ", "as soon as "];
        let one = after.trim_end_matches(['.', '!']).matches(". ").count() == 0;
        if one && NEXT.iter().any(|w| after.starts_with(w)) {
            if let Some(q) = question_in(&last[..=at]) {
                return Some(q);
            }
        }
    }
    // It says it waits on you.
    let w = waits_line(text)?;
    let t = w.to_lowercase().replace('\u{2019}', "'");
    ["for your ", "on your ", "for you ", "on you ", "for you.", "on you.", "for you to", "until you "].iter().any(|y| t.contains(y)).then_some(w)
}

/// The turn ends on a question for you: nothing to ask it this time.
fn ends_on_a_question(text: &str) -> bool {
    text.lines().map(str::trim).rfind(|l| !l.is_empty()).is_some_and(|l| l.ends_with('?') && !to_lead_line(l))
}

/// `line` is a TO LEAD: line (#474): words for the Desk's lead.
fn to_lead_line(line: &str) -> bool {
    matches!(crate::desk::parse::lines(line).first(), Some(crate::desk::parse::Line::ToLead { .. }))
}

/// How long the Desk lead has to settle a cleanup card before it turns to you (desk_cleanup.rs).
pub(super) const STOP_WAIT_MS: u64 = 10 * 60 * 1000;

/// The question a turn ended on, in one plain line: the last sentence of its last line, when that ends on "?".
pub(super) fn question_in(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    // A question in its TO LEAD: line is the lead's (#474), not yours.
    if to_lead_line(line) {
        return None;
    }
    let line = line.trim_start_matches(['-', '*', '>', ' ']).replace("**", "").replace('`', "");
    let line = line.trim();
    if !line.ends_with('?') {
        return None;
    }
    // Its last sentence: after the last ". ", "! " or ": " before the question.
    let body = &line[..line.len() - 1];
    let from = [". ", "! ", "? "].iter().filter_map(|p| body.rfind(p).map(|at| at + p.len())).max().unwrap_or(0);
    let q: String = line[from..].trim().chars().take(200).collect();
    (q.len() > 1).then_some(q)
}

/// How a holder's stop on an item (its turn ended on it, no DONE, no ask) is shown.
pub(super) enum Stop {
    /// Its turn ended saying what it waits on (#249): shown as waiting on that, nothing for you or the lead.
    Waits(String),
    /// It waits on you with one plain line: the question its turn ended on, or that the lead couldn't settle it.
    Asks(String),
    /// The Desk lead is settling it: nothing for you yet.
    Lead,
}

/// Stall `s` on `it` as it's shown at `now`: its turn ended on a question for you (Answer), or the lead has it, until
/// its time is up or when the Desk has no lead at hand to settle it.
pub(super) fn stop_shown(st: &Store, it: &DeskItem, s: &Session, now: u64) -> Stop {
    let last = s.thread.iter().rev().find(|e| e.role == "agent").map(|e| e.text.as_str()).unwrap_or("");
    // Its tests or build still run in the background (#435): still at work on them, nothing for you or the lead.
    if !s.running.is_empty() && question_in(last).is_none() {
        return Stop::Waits(s.running.clone());
    }
    // It said what it waits on: waiting, not stopped, its work committed or not (#249).
    if let Some((_, w)) = it.waits.as_ref().filter(|(u, _)| *u == it.updated && question_in(last).is_none()) {
        return Stop::Waits(w.clone());
    }
    // Its work isn't committed: it isn't in your app, so it isn't yours yet. The lead has it, its question too (#213).
    if !it.loose.is_empty() {
        return match lead_live(st, &it.project) {
            true => Stop::Lead,
            false => Stop::Asks(format!("{}, and this Desk has no lead to settle it: say what's next", not_committed(&it.loose))),
        };
    }
    if let Some(q) = question_in(last) {
        return Stop::Asks(q);
    }
    if !lead_live(st, &it.project) {
        return Stop::Asks("It stopped, and this Desk has no lead to settle it: say what's next".into());
    }
    // A stop with no question for you is the lead's to settle, each time and however long it takes (#435): it
    // routes it on, or asks you (ASK: #id …) when you must decide.
    let _ = now;
    Stop::Lead
}

/// `project`'s Desk lead is running (not ended or parked).
pub(super) fn lead_live(st: &Store, project: &str) -> bool {
    st.desk.lead_sid(project).and_then(|l| st.sessions.get(l)).is_some_and(|s| s.state != "stopped")
}

/// "Not committed: <its files>", as a stop with work git says isn't committed is shown (#213).
pub(super) fn not_committed(files: &[String]) -> String {
    let names: Vec<&str> = files.iter().take(3).map(|f| f.rsplit('/').next().unwrap_or(f)).collect();
    let more = if files.len() > 3 { format!(" and {} more", files.len() - 3) } else { String::new() };
    format!("Not committed: {}{more}", names.join(", "))
}

/// What the Desk lead is told of a stop on item `it` by `holder`, whose last words were `last`.
fn stop_words(it: &DeskItem, holder: &str, last: &str) -> String {
    if !it.loose.is_empty() {
        return format!(
            "#{id} ({title}) stopped, not committed: {holder} ended its turn on it with no DONE, and its work isn't \
             committed ({files}), so it isn't in the user's app. Its last line: {last}\n\
             Settle it: ROUTE #{id} to {holder} again with what's left as the why (Cue commits its work when it says \
             DONE: #{id}), or route it on; only if the user must decide, ASK: #{id} | <question> | choices: … Until \
             then it isn't shown as waiting on the user.",
            id = it.id,
            title = it.title,
            files = it.loose.join(", "),
        );
    }
    format!(
        "#{id} ({title}) stopped: {holder} ended its turn on it with no DONE and no question for the user. Its last line: {last}\n\
         Settle it: ROUTE #{id} to {holder} again with what to do next as the why, or route it on; only if the user must \
         decide, ASK: #{id} | <question> | choices: … Until then it isn't shown as waiting on the user.",
        id = it.id,
        title = it.title,
    )
}

/// Session `sid`'s turn ended, saying `text`, with items `on` at work with it before: each still at work with it (no
/// DONE for it) stalled there, and Cue asks it once, unless it's still at it (its agents or crew run on), it asks you, or the
/// turn says what's left. A turn that says what it waits on isn't a stop: the item shows it, no ask, no notice (#249).
/// A stall already asked of is answered by this turn: not asked again. One its answer (or what's
/// left) doesn't settle goes to the Desk lead (#182), unless it asks you.
pub(super) fn stalls_asked(st: &mut Store, sid: &str, text: &str, on: &[String], now: u64, fx: &mut Effects) {
    let still: Vec<(String, u64)> = on
        .iter()
        // An item on a paused Desk (#366) stopped because it was told to: not a stall.
        .filter_map(|id| st.desk.item(id).filter(|i| i.state == "working" && i.paused.is_none() && !super::desk_pause::is_paused(&i.project) && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty())).map(|i| (i.id.clone(), i.updated)))
        .collect();
    // This holder's asks for items that aren't stalled with it now are over.
    st.desk.nudged.retain(|id, (by, _)| by != sid || still.iter().any(|(s, _)| s == id));
    st.desk.left_said.retain(|id, (by, _)| by != sid || still.iter().any(|(s, _)| s == id));
    if still.is_empty() {
        return;
    }
    let h = Holder { kind: "session".into(), sid: sid.to_string(), ..Default::default() };
    let left = says_whats_left(text);
    let busy = still_at_it(st, sid);
    let asks = ask_of(st, &h).is_some() || ends_on_a_question(text);
    // Its own tests or build still running in the background (#435): waiting on them, whatever its words say.
    let running = st.sessions.get(sid).map(|s| s.running.clone()).filter(|r| !r.is_empty());
    // Its TO LEAD: words just went to the lead (#474): it waits on the lead's answer, not stopped.
    let asked_lead = || crate::desk::parse::lines(text).iter().any(|l| matches!(l, crate::desk::parse::Line::ToLead { .. })).then(|| "waiting on the Desk lead's answer".to_string());
    let waits = running.or_else(|| waits_line(text)).or_else(asked_lead);
    let mut ask = Vec::new();
    // Its turn asks you something (#475): each item still at work with it waits on you with that, through its later
    // turns, until you answer (`DeskItem::asks`). A card it asks with is shown as itself.
    let yours = if ask_of(st, &h).is_none() { question_for_you(text) } else { None };
    for (id, updated) in still {
        // Its work not committed: a question it ends on is the lead's to settle first (#213, below).
        if let Some(q) = yours.as_ref().filter(|_| st.desk.item(&id).is_some_and(|i| i.loose.is_empty())) {
            if let Some(it) = st.desk.item_mut(&id) {
                (it.asks, it.waits) = (Some((q.clone(), now)), None);
            }
            st.desk.nudged.remove(&id);
            st.desk.left_said.remove(&id);
            fx.items.push(id.clone());
            continue;
        }
        // Still waiting on your answer to what it asked earlier: nothing to ask it, nothing for the lead.
        if st.desk.item(&id).is_some_and(|i| i.asks.is_some()) {
            continue;
        }
        // What this turn says it waits on, if anything: the item shows it until it changes or a turn says otherwise.
        if let Some(it) = st.desk.item_mut(&id) {
            it.waits = waits.clone().map(|w| (updated, w));
        }
        // A question its turn ended on, with its work not committed, is the lead's to settle, not yours (#213).
        let loose = st.desk.item(&id).is_some_and(|i| !i.loose.is_empty());
        if loose && !busy && ask_of(st, &h).is_none() && ends_on_a_question(text) {
            st.desk.nudged.remove(&id);
            to_lead(st, sid, &id, now, fx);
            continue;
        }
        let quiet = busy || asks;
        // Waiting isn't stopping: not asked "Done?", and the lead isn't told (#249).
        if waits.is_some() {
            st.desk.left_said.insert(id.clone(), (sid.to_string(), updated));
            st.desk.nudged.remove(&id);
            continue;
        }
        // It said what's left of it, or what it waited on: not asked "Done?" until the item changes (sent again, routed;
        // #168). A turn that no longer waits is a stop the lead hears of; told already (what's left), this is a no-op.
        if st.desk.left_said.get(&id).is_some_and(|(by, at)| by == sid && *at == updated) {
            if !quiet {
                to_lead(st, sid, &id, now, fx);
            }
            continue;
        }
        if left {
            st.desk.left_said.insert(id.clone(), (sid.to_string(), updated));
            st.desk.nudged.remove(&id);
            if !quiet {
                to_lead(st, sid, &id, now, fx);
            }
            continue;
        }
        // Asked of this stall already: this turn is its answer. Still no DONE: the lead settles it.
        if st.desk.nudged.get(&id).is_some_and(|(_, at)| *at == updated) {
            st.desk.nudged.remove(&id);
            if !quiet {
                to_lead(st, sid, &id, now, fx);
            }
            continue;
        }
        if quiet {
            continue;
        }
        st.desk.nudged.insert(id.clone(), (sid.to_string(), updated));
        ask.push(stall_words(&id));
    }
    if !ask.is_empty() {
        fx.sends.push((sid.to_string(), ask.join("\n")));
    }
}

/// Holder `sid`'s stop on item `id` to the Desk lead to settle, once while the item is unchanged; kept on the item
/// (saved) so the row shows the lead has it, and turns to you when its time is up. The lead is told once per item
/// (#249): a later stop of it is yours at once, with no notice.
fn to_lead(st: &mut Store, sid: &str, id: &str, now: u64, fx: &mut Effects) {
    let Some(it) = st.desk.item(id) else { return };
    if it.stop.is_some_and(|(u, _)| u == it.updated) {
        return;
    }
    let project = it.project.clone();
    let last = st.sessions.get(sid).map(last_line).unwrap_or_default();
    let holder = st.sessions.get(sid).map(super::desk::shown_name).unwrap_or_else(|| sid.to_string());
    let words = stop_words(it, &holder, &last);
    let lead = st.desk.lead_sid(&project).filter(|_| lead_live(st, &project)).map(str::to_string);
    let Some(it) = st.desk.item_mut(id) else { return };
    it.stop = Some((it.updated, now));
    fx.said.push(id.to_string());
    // Each stop is the lead's (#435): told of every one, once while the item is unchanged.
    fx.stopped.push(id.to_string());
    if let Some(lead) = lead {
        it.told = Some(it.updated);
        fx.sends.push((lead, words));
    }
}

/// What holder `h` did last while at work on its item, and when (#285): its latest step, else its last words; for a
/// crew member, its own latest step. None for a holder that asks you something (waiting on you, never stalled), or a
/// crew member not at work.
pub(super) fn act_of(st: &Store, h: &Holder) -> Option<crate::desk::Act> {
    if let Some((lead, agent)) = crew_parts(h) {
        let t = st.sessions.get(lead)?.team.iter().find(|t| (t.agent_id == agent || t.call == agent) && t.working())?;
        return Some(act_words(&t.step, t.step_ms.max(t.started_ms), false));
    }
    let s = st.sessions.get(&h.sid)?;
    // Deciding: it asks you (a permission, a question): waiting on you, never stalled.
    if s.state == "deciding" {
        return None;
    }
    let doing = if s.state == "working" { s.doing.as_str() } else { "" };
    // Its last sign of life: its latest step, its turn's start (its state's), its last words, its crew's latest steps.
    // A message sent to it isn't: Cue's own "Done?" would read as work.
    let said = s.thread.iter().rev().find(|e| e.role == "agent").map_or(0, |e| e.at_ms);
    let crew = s.team.iter().filter(|t| t.working()).map(|t| t.step_ms.max(t.started_ms)).max().unwrap_or(0);
    let at = s.since_ms.max(if doing.is_empty() { 0 } else { s.doing_ms }).max(said).max(crew);
    // Handed to agents that run on (an Agent call under way, or background agents Cue can't read): theirs is the
    // work, as for being stuck (activity.rs). Its crew's steps are read, so a quiet crew stalls it.
    let helped = crew == 0 && (doing.starts_with("Delegating") || s.state == "agent");
    if doing.is_empty() {
        let words = last_line(s);
        let now = match s.state.as_str() {
            "agent" if !s.driven_by.is_empty() => format!("Waiting on {}", s.driven_by),
            _ if !words.is_empty() => words.clone(),
            _ => "Starting".into(),
        };
        let was = if words.is_empty() { "nothing said yet".into() } else { format!("last said: {words}") };
        return Some(crate::desk::Act { at_ms: at, now, was, stalls_at: stalls_at(at, helped) });
    }
    Some(act_words(doing, at, helped))
}

/// A step ("Reading model.rs", "Running: cargo test", "Thinking…") as an item's row says it: now, and once stalled.
pub(super) fn act_words(step: &str, at: u64, helped: bool) -> crate::desk::Act {
    let step = step.trim();
    let (now, was) = if let Some(rest) = step.strip_prefix("Running: ") {
        (format!("Running {rest}"), format!("last ran {rest}"))
    } else if let Some(rest) = step.strip_prefix("Searching the web: ") {
        (format!("Searching the web for {rest}"), format!("last searched the web for {rest}"))
    } else if let Some(rest) = step.strip_prefix("Searching: ") {
        (format!("Searching for {rest}"), format!("last searched for {rest}"))
    } else if let Some(rest) = step.strip_prefix("Finding files: ") {
        (format!("Finding files: {rest}"), format!("last looked for {rest}"))
    } else if let Some(rest) = step.strip_prefix("Delegating: ") {
        (format!("Handed off: {rest}"), format!("last handed off {rest}"))
    } else if let Some(rest) = step.strip_prefix("Editing ") {
        (step.to_string(), format!("last edited {rest}"))
    } else if let Some(rest) = step.strip_prefix("Reading ") {
        (step.to_string(), format!("last read {rest}"))
    } else if let Some(rest) = step.strip_prefix("Updating ") {
        (step.to_string(), format!("last updated {rest}"))
    } else if let Some(rest) = step.strip_prefix("Using ") {
        (step.to_string(), format!("last used {rest}"))
    } else if step.starts_with("Thinking") {
        ("Thinking".to_string(), "thinking".to_string())
    } else if step.starts_with("Writing") {
        ("Writing its reply".to_string(), "writing its reply".to_string())
    } else if step.is_empty() {
        ("Starting".to_string(), "nothing done yet".to_string())
    } else {
        (step.trim_end_matches('…').to_string(), format!("last: {}", step.trim_end_matches('…')))
    };
    crate::desk::Act { at_ms: at, now, was, stalls_at: stalls_at(at, helped) }
}

/// When an item whose holder last did anything at `at` reads "Stalled": `QUIET_MS` on; never (0) while its helpers
/// are at it.
fn stalls_at(at: u64, helped: bool) -> u64 {
    if helped { 0 } else { at + crate::desk::model::QUIET_MS }
}

#[cfg(test)]
mod tests {
    use super::act_words;
    use crate::desk::model::QUIET_MS;

    #[test]
    fn a_step_reads_as_what_it_does_now_and_what_it_did_last() {
        let words = |step: &str| { let a = act_words(step, 1000, false); (a.now, a.was) };
        assert_eq!(words("Editing desk.rs"), ("Editing desk.rs".into(), "last edited desk.rs".into()));
        assert_eq!(words("Reading model.rs"), ("Reading model.rs".into(), "last read model.rs".into()));
        assert_eq!(words("Running: cargo test"), ("Running cargo test".into(), "last ran cargo test".into()));
        assert_eq!(words("Searching: fn shown"), ("Searching for fn shown".into(), "last searched for fn shown".into()));
        assert_eq!(words("Thinking…"), ("Thinking".into(), "thinking".into()));
        assert_eq!(words("Writing…"), ("Writing its reply".into(), "writing its reply".into()));
    }

    #[test]
    fn it_stalls_ten_quiet_minutes_after_its_last_step_unless_helpers_are_at_it() {
        assert_eq!(QUIET_MS, 10 * 60 * 1000);
        assert_eq!(act_words("Editing desk.rs", 5000, false).stalls_at, 5000 + QUIET_MS);
        assert_eq!(act_words("Delegating: review", 5000, true).stalls_at, 0, "helpers at work never stall");
    }
}
