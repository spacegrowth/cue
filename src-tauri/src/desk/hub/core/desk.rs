//! The Desk in the hub (desk/ has its records and words): `Effects` and the helpers the desk/hub/ area files share,
//! restoring the Desk and setting its lead, Cue's asks to the lead and your answers, and `desk_done`. Which topic is
//! in which file: docs/dev/desk-files.md. The lead itself (picking it, starting it, what it's told before each
//! message) is desk/hub/lead/run.rs.
//! A change is made under the store's lock and leaves `Effects`: its saves and messages, done once it's let go.

use crate::sessions::{Kind, Said};
use crate::desk::hub::prelude::*;
use crate::desk::{DeskDecision, DeskItem, Holder};
use crate::sessions::Session;
use crate::desk::hub::core::members::UseAsk;
pub(crate) use crate::desk::hub::turn::route::{By, decide, give_started, handoff_words, moved_from_of, release, route};
pub(crate) use crate::desk::hub::core::view::{held_window, item_sources, shown};
use crate::desk::hub::turn::act::{letter_of, pick, pick_letters};

/// What a change to the Desk leaves to do with the store let go.
#[derive(Default)]
pub(crate) struct Effects {
    /// Items to save as they are now, and items dropped.
    pub(crate) items: Vec<String>,
    /// Items whose holder's words were only kept on them (replied): saved, never counted as sent or started.
    pub(crate) said: Vec<String>,
    pub(crate) dropped: Vec<String>,
    pub(crate) decisions: Vec<String>,
    /// Messages to sessions: (session, text).
    pub(crate) sends: Vec<DeskSend>,
    /// Your Send backs (#396): (item, its message in `sends`), kept on the item if it has to wait for a copy.
    pub(crate) backs: Vec<(String, String)>,
    /// Your pick on a mocks item whose holder waits on its question (#281): (that ask, its question, the answer),
    /// answered with the option picked. A pick always changes an item, so `items` is never empty with these.
    pub(crate) answers: Vec<(String, String, String)>,
    /// A lead's questions for you: (its session, its project, question, choices, the item it's about).
    pub(crate) asks: Vec<(String, String, String, Vec<String>, Option<String>)>,
    /// A lead's proposed changes to how work lands (LANDS:), each asked of you (desk/hub/lead/lands.rs).
    pub(crate) lands: Vec<crate::desk::hub::lead::lands::Proposal>,
    /// New items for their project's lead to route: (project, item), rows on its Desk's feed as they're saved.
    pub(crate) news: Vec<(String, String)>,
    /// Items a lead gave to a new session (`-> new`): each started a moment later (desk/hub/lead/run.rs).
    pub(crate) starts: Vec<NewStart>,
    /// Sessions that joined a Desk: (project, session), to save.
    pub(crate) joined: Vec<(String, String)>,
    /// Routes to sessions not on the Desk: you're asked first, in the lead's name (`desk_ask`).
    pub(crate) uses: Vec<UseAsk>,
    /// Items just said DONE: the files their check card shows, read a moment later (desk/hub/lead/checks.rs).
    pub(crate) checked: Vec<String>,
    /// Items whose stop just went to the Desk lead: shown again once the lead's time to settle it is up (desk/hub/stall/stall.rs).
    pub(crate) stopped: Vec<String>,
    /// Your Restarts (#291): (item, the session it goes to, its name), sent like `sends`, and how each went noted
    /// on the item and its step.
    pub(crate) restarts: Vec<(String, String, String)>,
    /// Sessions whose `sends` go now, mid-turn (a Pause, #365): handed over as Send now does, so it reads it at once.
    pub(crate) now: Vec<String>,
    /// Dropped items that had their own copy: kept or removed (desk/hub/landing/trees.rs).
    pub(crate) trees_gone: Vec<DeskItem>,
    /// Items the lead said DONE on for you (`DeskItem::close_on_check`): closed as Looks right would once to check.
    pub(crate) closes: Vec<String>,
    /// Items the lead said are on main already (CLOSE, #347): closed once git says their commit is on the main line.
    pub(crate) on_main: Vec<crate::desk::hub::stall::unstick::OnMain>,
    /// The lead's word on a landing clean-up question it had first (desk/hub/lead/cleanup.rs): (its card, the answer; None:
    /// passed on to you).
    pub(crate) cleanup: Vec<(String, Option<String>)>,
    /// The lead moved an item up in the landing line (LAND FIRST): the line runs now.
    pub(crate) land: bool,
    /// The lead's PAUSE (true) or RESUME (false) of its Desk (#366): (Desk, on, its name, its session, why).
    pub(crate) pause: Vec<(String, bool, String, String, String)>,
    /// The lead's INSTALL: main (#867): (Desk, the lead's session), installed through Cue a moment later and answered.
    pub(crate) install: Vec<(String, String)>,
    /// Steps whose message is in `sends` (your notes and send-backs, the lead's NOTE: #437): (step, session, item), so
    /// how each send went is kept on its step (`desk_went`), and the item's page can say so (and offer Retry).
    pub(crate) went: Vec<(String, String, String)>,
    /// A session's TO LEAD: words (#474): (Desk, the session, the message, its own words), sent to that Desk's lead as
    /// it is then; its words quoted back to it when they can't be.
    pub(crate) to_lead: Vec<(String, String, String, String)>,
    /// Holds taken or lifted (#540, desk/holds.rs), to save; those whose item ended are added as the Desk saves.
    pub(crate) holds: Vec<crate::desk::holds::HoldSave>,
    /// Bands to other Desks' leads (#540, desk/tell.rs): (that Desk, the message, the sending lead's session), sent as
    /// Cue's own words to it; and the bands sent or answered, to save.
    pub(crate) tells: Vec<(String, String, String)>,
    pub(crate) bands: Vec<crate::desk::tell::Band>,
    /// Questions to the user when Desks' leads can't settle it (#540, desk/escalate.rs), each on all its Desks.
    pub(crate) to_user: Vec<crate::desk::escalate::ToUser>,
}


/// Item `id` of `project` to a new session, as the lead (`by`, `by_sid`) asked, with its why.
pub(crate) struct NewStart {
    pub(crate) project: String,
    pub(crate) id: String,
    pub(crate) why: String,
    pub(crate) by: String,
    pub(crate) by_sid: String,
}


pub(crate) fn secs(ms: u64) -> u64 {
    ms / 1000
}

/// What a session goes by on the Desk: the name you gave it, Claude's, else its folder's.
pub(crate) fn shown_name(s: &Session) -> String {
    if !s.name.is_empty() {
        return s.name.clone();
    }
    match s.claude_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => crate::model::folder_of(&s.origin.cwd),
    }
}

/// A crew holder's (lead session, agent id).
pub(crate) fn crew_parts(h: &Holder) -> Option<(&str, &str)> {
    (h.kind == "crew" && !h.lead_sid.is_empty()).then_some((h.lead_sid.as_str(), h.sid.as_str()))
}

/// The sessions `project`'s items may go to: the project's own, live, oldest first.
pub(crate) fn routable<'a>(st: &'a Store, project: &str) -> Vec<&'a Session> {
    let mut ours: Vec<&Session> = st.sessions.iter().filter(|s| s.project == st.desk.project_of(project) && st.desk.lead_of(&s.origin.session_id).is_none_or(|id| id == project)).collect();
    ours.sort_by_key(|s| s.since_ms);
    ours
}

/// Desk `scope`'s Ask me before line for the sessions it hands work to (desk/one_way.rs; "" when nothing's set): its
/// project's, so two Desks on one project share it.
/// Then how to reach the Desk's lead (#474), named as it is now: every hand-off says so, so a session needn't look it up.
pub(crate) fn rule_of(st: &Store, scope: &str) -> String {
    let rule = crate::desk::one_way::ask_first(&crate::desk::one_way::kinds(st.desk.project_of(scope)));
    let reach = reach_lead(st, scope);
    if rule.is_empty() { reach } else { format!("{}\n{reach}", rule.trim_end()) }
}

/// How a session on Desk `scope` messages its lead (#474): the lead's name now, and the TO LEAD: line Cue delivers to
/// whoever leads it when it's written (so a lead's hand-off changes nothing for the session).
pub(crate) fn reach_lead(st: &Store, scope: &str) -> String {
    let how = "write a line of its own: TO LEAD: <your words> (add #<id> first for another item); Cue delivers it to whoever runs the Desk then, no lookups needed.";
    match st.desk.lead.get(scope) {
        Some(l) => {
            let name = st.sessions.get(&l.sid).map(shown_name).filter(|n| !n.is_empty()).unwrap_or_else(|| l.name.clone());
            format!("Your Desk lead is {name}. To message it, {how}")
        }
        None => format!("To message your Desk's lead, {how}"),
    }
}

/// Live session `sid` as a holder: a lead when it has a crew.
pub(crate) fn holder_of(st: &Store, sid: &str) -> Option<Holder> {
    st.sessions.get(sid).map(|s| Holder { kind: if s.team.is_empty() { "session".into() } else { "lead".into() }, sid: sid.to_string(), name: shown_name(s), lead_sid: String::new() })
}

/// The session of `project` a ROUTE names (`to`): by name (yours or Claude's), by "sid:<start of its id>" or the
/// start of its id (six characters or more), by folder, else a crew member by the name its lead gave it.
pub(crate) fn resolve(st: &Store, project: &str, to: &str) -> Option<Holder> {
    let want = to.trim().trim_start_matches('@').to_lowercase();
    let id = want.strip_prefix("sid:").unwrap_or(&want).trim().to_string();
    let all = routable(st, project);
    let session = |s: &Session| holder_of(st, &s.origin.session_id).unwrap_or_default();
    if let Some(s) = all.iter().find(|s| s.name.to_lowercase() == want || s.claude_name.as_deref().is_some_and(|n| n.to_lowercase() == want)) {
        return Some(session(s));
    }
    if id.len() >= 6 {
        if let Some(s) = all.iter().find(|s| s.origin.session_id.to_lowercase().starts_with(&id)) {
            return Some(session(s));
        }
    }
    if let Some(s) = all.iter().find(|s| crate::model::folder_of(&s.origin.cwd).to_lowercase() == want) {
        return Some(session(s));
    }
    all.iter().find_map(|s| {
        let t = s.team.iter().find(|t| t.name.to_lowercase() == want)?;
        let agent = if t.agent_id.is_empty() { &t.call } else { &t.agent_id };
        Some(Holder { kind: "crew".into(), sid: agent.clone(), name: t.name.clone(), lead_sid: s.origin.session_id.clone() })
    })
}

/// Holder `h` at work: its session mid-turn (or asking, or driven by its agents), or for a crew member, that teammate.
pub(crate) fn at_work(st: &Store, h: &Holder) -> bool {
    match crew_parts(h) {
        Some((lead, agent)) => st.sessions.get(lead).is_some_and(|s| s.team.iter().any(|t| (t.agent_id == agent || t.call == agent) && t.working())),
        None => st.sessions.get(&h.sid).is_some_and(|s| matches!(s.state.as_str(), "working" | "deciding" | "agent")),
    }
}

/// An open ask (not a "your turn" card) of the holder's: for a crew member, its own.
pub(crate) fn ask_of<'a>(st: &'a Store, h: &Holder) -> Option<&'a Item> {
    let agent = crew_parts(h).map(|(_, a)| a);
    st.items.iter().find(|i| {
        i.status == "pending" && i.kind != "waiting" && i.lead_has.is_none() && i.origin.session_id == h.to() && agent.is_none_or(|a| i.agent_id == a)
    })
}

/// An ask as an item shows it: { item_id, sid, text, choices }.
pub(crate) fn ask_view(i: &Item) -> Value {
    let q = i.tool_input.pointer("/questions/0");
    let text = q.and_then(|q| q["question"].as_str()).map(str::to_string).unwrap_or_else(|| crate::hub::summary(i));
    let choices: Vec<String> = q.and_then(|q| q["options"].as_array()).map(|o| o.iter().filter_map(|x| x["label"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    // Its kind: a question (Answer, or Pick from its choices), or a tool to allow (Allow).
    json!({ "item_id": i.id, "sid": i.origin.session_id, "text": text, "choices": choices, "kind": i.kind })
}

/// The first line of a session's last words.
/// At work on a turn: only this turn's words (#484: what it said last round isn't what it's doing now).
pub(crate) fn last_words(s: &Session) -> String {
    let from = if s.state == "working" { s.since_ms } else { 0 };
    s.thread.iter().rev().find(|e| e.role == "agent" && e.at_ms >= from).and_then(|e| e.text.lines().map(str::trim).find(|l| !l.is_empty())).unwrap_or("").chars().take(160).collect()
}

/// How a note that was still going when Cue stopped went.
pub(crate) const STOPPED: &str = "failed: Cue stopped before it went";

impl Hub {
    /// The Desk as it was saved (with_ports).
    pub(crate) fn desk_restore(st: &mut Store) {
        let (items, decisions, leads) = crate::db::desk_load(now_ms());
        st.desk.spaces = crate::db::desk_spaces().into_iter().map(|d| (d.id.clone(), d)).collect();
        st.desk.items = items;
        st.desk.decisions = decisions;
        st.desk.kept = crate::db::desk_kept();
        st.desk.moves_cleared = crate::db::desk_moves_cleared().into_iter().collect();
        // Items from before numbering get their numbers once, saved under them.
        let changed = st.desk.number_old();
        crate::db::desk_put_numbered(&st.desk, &changed);
        // The change feed: its marks and its rows not heard yet. Sending starts again from "heard" (desk/feed.rs).
        st.desk.feed = crate::db::desk_feed_load();
        for f in st.desk.feed.values_mut() {
            f.sent = f.heard.clone();
        }
        st.desk.lead = leads.into_iter().map(|(p, sid, name)| (p, crate::desk::Lead { sid, name })).collect();
        // Calls from before #403 (once the leads are known): the lead's dispatch work sorted off the Decisions list, as new ones are.
        for id in st.desk.sort_calls() {
            if let Some(d) = st.desk.decisions.iter().find(|d| d.id == id) {
                crate::db::desk_put_decision(d);
            }
        }
        // A note still going when Cue stopped never went (#437): its step says so and offers Retry.
        for d in st.desk.decisions.iter_mut() {
            if let Some(del) = d.delivery.as_mut().filter(|del| del.went == "sending") {
                del.went = STOPPED.into();
                crate::db::desk_put_decision(d);
            }
        }
        // Holds (#540): those whose item ended while Cue was off (or is no longer loaded) are gone.
        st.desk.holds = crate::db::desk_holds();
        let ended = crate::desk::holds::ended(st);
        if let Err(e) = crate::db::desk_save_holds(&ended) {
            eprintln!("cue: desk: couldn't drop ended holds: {e}");
        }
        // Bands between Desks (#540): the last day's.
        st.desk.bands = crate::db::desk_bands(now_ms().saturating_sub(crate::desk::tell::BAND_KEEP_MS));
        st.desk.members.clear();
        for (p, sid) in crate::db::desk_members() {
            st.desk.join(&p, &sid);
        }
    }

    /// Session `sid` (going by `name`) leads `project`'s Desk from now: kept and saved.
    pub(crate) fn desk_put_lead(&self, project: &str, sid: &str, name: &str) {
        self.desk_lead_to(project, Some(crate::desk::Lead { sid: sid.to_string(), name: name.to_string() }));
    }

    /// `project`'s lead put back as it was before a change of lead that failed (`before`: None when it had none).
    pub(crate) fn desk_put_back_lead(&self, project: &str, before: Option<crate::desk::Lead>) {
        let failed = self.desk_lead_sid(project).unwrap_or_default();
        let back = before.as_ref().map(|l| l.sid.clone());
        self.desk_lead_to(project, before);
        // Back as it was: neither the one put back nor the one that failed is the other's earlier lead (#792).
        if let Some(sid) = back {
            crate::db::desk_unpast_lead(project, &sid, &failed);
        }
    }

    /// The lead `project`'s Desk had before session `sid` ran it (#792), for the lead chat's "Show earlier": { sid,
    /// name, at_ms (when it stopped), handoff (it handed off), count, first_ms, last_ms (its messages in Cue's log),
    /// note (its hand-off note's path, "" for none), cwd }. Null when `sid` was its first.
    pub(crate) fn desk_lead_before(&self, project: &str, sid: &str) -> serde_json::Value {
        let Some((old, name, at_ms, handoff)) = crate::db::desk_lead_before(project, sid) else { return serde_json::Value::Null };
        let (count, first_ms, last_ms) = crate::db::session_log_span(&old);
        let (known, cwd) = self.read(|st| st.sessions.all().into_iter().find(|s| s.origin.session_id == old).map(|s| (shown_name(&s), s.origin.cwd.clone())).unwrap_or_default());
        let name = if name.is_empty() { known } else { name };
        let note = if handoff { crate::db::handoff(&old).map(|k| k.note).unwrap_or_default() } else { String::new() };
        serde_json::json!({ "sid": old, "name": name, "at_ms": at_ms, "handoff": handoff, "count": count, "first_ms": first_ms, "last_ms": last_ms, "note": note, "cwd": cwd })
    }

    /// `project`'s lead from now (`lead`, or none), kept and saved.
    fn desk_lead_to(&self, project: &str, lead: Option<crate::desk::Lead>) {
        {
            let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
            // A lead is on its Desk, and stays on it once another runs it.
            let joined = self.write(|st| match lead.clone() {
                Some(l) => {
                    let joined = st.desk.join(project, &l.sid);
                    st.desk.lead.insert(project.to_string(), l);
                    joined
                }
                None => {
                    st.desk.lead.remove(project);
                    false
                }
            });
            match &lead {
                Some(l) => {
                    crate::db::desk_put_lead(project, &l.sid, &l.name, now_ms());
                    if joined {
                        crate::db::desk_put_member(project, &l.sid, now_ms());
                    }
                }
                None => crate::db::desk_drop_lead(project),
            }
        }
        self.changed();
    }

    /// Your words `text` sent from the Desk to its lead `sid`: kept (the last few) so a turn answering them is known as
    /// one you asked (UNPARK, #499).
    pub(crate) fn desk_yours(&self, sid: &str, text: &str) {
        self.write(|st| {
            let yours = st.desk.run.yours.entry(sid.to_string()).or_default();
            yours.push(text.trim().to_string());
            if yours.len() > 4 {
                yours.remove(0);
            }
        });
    }

    /// The lead has been told of these failed routes (item, name) with a message that reached it: each is cleared,
    /// unless a newer one failed since.
    pub(crate) fn desk_told(&self, failed: &[(String, String)]) {
        if failed.is_empty() {
            return;
        }
        let items = self.write(|st| {
            let mut items = vec![];
            // Each (item, what it was told of): a failed route's name, or a refused `after`.
            for (id, note) in failed {
                // Route answers held for the lead's next Desk block (#248): cleared once told, unless more came since.
                if let Some(project) = id.strip_prefix(crate::desk::hub::lead::run::ROUTES_HELD) {
                    if st.desk.run.routes_held.get(project) == Some(note) {
                        st.desk.run.routes_held.remove(project);
                    }
                    continue;
                }
                let Some(it) = st.desk.item_mut(id) else { continue };
                if &it.route_failed == note {
                    it.route_failed.clear();
                    items.push(id.clone());
                } else if &it.after_refused == note {
                    it.after_refused.clear();
                    items.push(id.clone());
                } else if &it.reopen_refused == note {
                    it.reopen_refused.clear();
                    items.push(id.clone());
                }
            }
            items
        });
        self.desk_done(Effects { items, ..Default::default() });
    }

    /// Cue asked session `sid` for a hand-off note (`why`: its context, and the reason: its cache ending, over the
    /// Desk's mark, its compacting failed), by the Desk's rules (cache_tick): a step by Cue, nothing to undo, on each
    /// item it holds (on its project's Desk when it holds none).
    pub(crate) fn desk_cue_asked(&self, sid: &str, why: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))) else { return fx };
            for project in st.desk.scopes(sid, &project) {
                let held: Vec<String> = st.desk.items.iter().filter(|i| i.project == project && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| i.id.clone()).collect();
                let on: Vec<Option<String>> = if held.is_empty() { vec![None] } else { held.into_iter().map(Some).collect() };
                for item in on {
                    decide(st, &project, item, "Cue", "", &format!("Asked {name} for a hand-off note"), why, now, &mut fx);
                    if let Some(d) = st.desk.decisions.last_mut() {
                        d.undo = "none".into();
                    }
                }
            }
            fx
        });
        self.desk_done(fx);
    }

    /// A lead asks you something: a question card of its session's, marked as its project's Desk's.
    /// Your answer goes back to it as a message (`desk_answered`).
    /// `using`: a route waiting on your yes (`UseAsk`), kept on the card for `desk_answered`.
    /// `item`: the item it's about (ASK: #id | …), which then waits on you with it.
    /// The card's id, None when the lead isn't a session here.
    pub(crate) fn desk_ask(&self, lsid: &str, project: &str, question: &str, choices: &[String], using: Option<&UseAsk>, item: Option<&str>) -> Option<String> {
        let origin = self.read(|st| st.sessions.origin(lsid))?;
        // Each choice with its line on what it does, the lead's suggestion marked (#450); no header: the card's head
        // names the lead.
        let options: Vec<Value> = choices.iter().map(|c| crate::desk::parse::ask_choice(c)).map(|(label, what)| json!({ "label": label, "description": what })).collect();
        let mut input = json!({ "questions": [{ "question": question, "header": "", "options": options, "multiSelect": false }] });
        if let Some(u) = using {
            input["desk_use"] = json!(u);
        }
        if let Some(id) = item {
            input["desk_item"] = json!(id);
        }
        let ask = NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() };
        // Nobody waits on the card: its answer is sent to the lead as a message (`desk_answered`).
        let (id, _) = self.add_ask(origin, ask);
        // The lead's older open ASK about the same item is moot now (#880): this one replaces it.
        if let Some(about) = item.filter(|_| using.is_none()) {
            self.desk_moot_older(lsid, &id, |t| t["desk_item"].as_str() == Some(about) && t.get("desk_use").is_none_or(Value::is_null));
        }
        self.write(|st| {
            let workspace = st.desk.project_of(project).to_string();
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.desk = true;
                it.project = workspace;
                it.tool_input["desk_id"] = json!(project);
                // Answered, the lead is at its prompt (not at work) until the answer reaches it.
                it.at_prompt = true;
            }
        });
        self.changed();
        Some(id)
    }

    /// The lead's ASKs about an item that has since finished (done, landed, or dropped: gone from the Desk) close by
    /// themselves (#613): the question no longer needs you, and nothing is sent to the lead (it finished the item).
    pub(crate) fn desk_close_finished_asks(&self) {
        let ids: Vec<String> = self.read(|st| {
            st.items
                .iter()
                .filter(|i| i.desk && i.status == "pending")
                .filter(|i| i.tool_input["desk_item"].as_str().is_some_and(|x| st.desk.items.iter().find(|d| d.id == x).is_none_or(|d| matches!(d.state.as_str(), "done" | "dropped"))))
                .map(|i| i.id.clone())
                .collect()
        });
        for id in &ids {
            self.finish(id, "gone", "its item is done");
        }
        if !ids.is_empty() {
            self.changed();
        }
    }

    /// Card `id` answered with `d`: a lead's ASK is filed and its answer sent to the lead. False for any other.
    pub(crate) fn desk_answered(&self, id: &str, d: &Decision) -> bool {
        let Some((sid, question, using, about, old, lands, push)) = self.read(|st| {
            st.items.iter().find(|i| i.id == id && i.desk).map(|i| {
                let old = i.tool_input["desk_old"].as_str().map(str::to_string);
                let push = i.tool_input.get("desk_push").filter(|v| v.is_object()).cloned();
                let lands = i.tool_input.get("desk_lands").filter(|v| v.is_object()).cloned();
                let using = serde_json::from_value::<UseAsk>(i.tool_input["desk_use"].clone()).ok();
                let about = i.tool_input["desk_item"].as_str().map(|x| format!(" about #{x}")).unwrap_or_default();
                (i.origin.session_id.clone(), i.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string(), using, about, old, lands, push)
            })
        }) else { return false };
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        let answer = answer.or_else(|| d.message.clone()).unwrap_or_else(|| if d.behavior == "deny" { "no".into() } else { "yes".into() });
        let answer = crate::desk::parse::plain_choice(&answer).to_string();
        // A plain no typed on Cue's Yes / No card is its No (#880).
        let answer = if (push.is_some() || old.is_some() || lands.is_some()) && plain_no(&answer) { "No".to_string() } else { answer };
        // Your own words on Cue's Yes / No card (a push, old copies, a change to how work lands) aren't a yes (#651):
        // they go to the lead, the card open still for a question of yours, else closed with nothing changed (#880).
        if d.behavior != "deny" && (push.is_some() || old.is_some() || lands.is_some()) && !["Yes", "No"].iter().any(|c| answer.eq_ignore_ascii_case(c)) {
            return self.desk_card_words(id, &answer);
        }
        // Cue's question when Desks' leads can't settle it (desk/escalate.rs): read before the card is filed.
        let to_user = self.read(|st| st.items.iter().find(|i| i.id == id).and_then(|i| serde_json::from_value::<crate::desk::escalate::ToUser>(i.tool_input["desk_to_user"].clone()).ok()));
        self.write(|st| st.waiters.remove(id));
        self.finish(id, "answered", &format!("answered: {answer}"));
        if let Some(q) = to_user {
            self.desk_to_user_answered(id, q, &answer);
            return true;
        }
        // The lead's proposed change to how work lands (desk/hub/lead/lands.rs): saved only on your yes, the lead told.
        if let Some(lands) = lands {
            self.desk_lands_answered(&sid, &lands, d.behavior != "deny" && answer == "Yes");
            return true;
        }
        // Cue's own question, put in the lead's chat (desk/hub/landing/copies.rs): answered here, nothing sent to the lead.
        if let Some(desk) = old {
            self.desk_old_copies_answered(desk, d.behavior != "deny" && answer == "Yes");
            return true;
        }
        // Cue's push of main (desk/hub/landing/push_main.rs): pushed only on your yes, a held start sent either way.
        if let Some(push) = push {
            self.desk_push_answered(push, d.behavior != "deny" && answer == "Yes");
            return true;
        }
        let mut words = format!("Your ASK{about}: {question}\nThe answer: {answer}");
        if let Some(u) = using {
            words = self.desk_use_answered(&u, d.behavior != "deny", &answer);
        }
        self.later(0, move |h| {
            // A sentence Cue wrote around your Yes or No: Cue's answer to its ASK (#837).
            if let Err(e) = h.tell(&sid, &words, crate::sessions::Kind::Answer) {
                eprintln!("cue: desk: couldn't send the lead its answer: {e}");
            }
        });
        true
    }

    /// Your words `text` typed on Cue's own Yes / No card `id` in a lead's chat, which aren't one of its choices
    /// (#651): never taken as a yes. A plain no ("no", "nope", "no thanks") is its No (#880). A question ("what does
    /// it mean?") keeps the card open, your words kept on it, for its Yes or No once the lead has answered you (#651).
    /// Anything else answers it in your words (#880): the card closes, nothing changes (as a no would), and the lead
    /// is told it can ask again. Either way the words go to the lead, who answers you in its chat. False when `id`
    /// isn't such a card (then the words go as they would).
    pub(crate) fn desk_card_words(&self, id: &str, text: &str) -> bool {
        let text = text.trim();
        let now = now_ms();
        let Some((lead, question, push)) = self.read(|st| {
            let it = st.items.iter().find(|i| i.id == id && i.desk && i.status == "pending" && ["desk_push", "desk_old", "desk_lands"].iter().any(|k| i.tool_input.get(*k).is_some_and(|v| !v.is_null())))?;
            Some((it.origin.session_id.clone(), it.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string(), it.tool_input.get("desk_push").cloned()))
        }) else { return false };
        if text.is_empty() {
            return true;
        }
        if plain_no(text) {
            let d = Decision { behavior: "allow".into(), answers: Some(json!({ question.as_str(): "No" })), ..Default::default() };
            return self.desk_answered(id, &d);
        }
        // A question anywhere in them ("what does it mean? what do i do"), pictures' paths left out.
        let open = crate::desk::your_words(text).0.contains('?');
        if open {
            self.write(|st| {
                if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                    if !it.tool_input["desk_said"].is_array() {
                        it.tool_input["desk_said"] = json!([]);
                    }
                    if let Some(all) = it.tool_input["desk_said"].as_array_mut() {
                        all.push(json!({ "text": text, "at": now }));
                    }
                }
            });
        } else {
            let shown: String = text.lines().next().unwrap_or("").chars().take(200).collect();
            self.finish(id, "answered", &format!("answered in your words: {shown}"));
            // A push card answered in words: no yes, so its held starts go as a no would.
            self.desk_push_card_closed(push);
        }
        self.changed();
        let words = card_words(&question, text, open);
        self.later(0, move |h| {
            // Your own typed words: yours, never Cue's (#837).
            if let Err(e) = h.send_to_session(&lead, &words, &[], false) {
                eprintln!("cue: desk: couldn't send the lead your words on Cue's question: {e}");
            }
        });
        true
    }

    /// Lead `lsid`'s older open questions that its newer one `id` replaces (#880): moot, closed without an answer and
    /// nothing sent. `same`: whether an older card asks the same thing (its `tool_input`).
    pub(crate) fn desk_moot_older(&self, lsid: &str, id: &str, same: impl Fn(&Value) -> bool) {
        let older: Vec<String> = self.read(|st| st.items.iter().filter(|i| i.id != id && i.desk && i.status == "pending" && i.origin.session_id == lsid && same(&i.tool_input)).map(|i| i.id.clone()).collect());
        for o in &older {
            self.finish(o, "gone", "a newer question replaced it");
        }
        if !older.is_empty() {
            self.changed();
        }
    }

    /// Ask `id` answered with `d`: when it's the question of an item's holder, and the answer is one of the options
    /// the question names, that's your pick (#281, #543): the item is picked and done, and what follows it filed, as a
    /// Pick on the page does. What's left to do (its saves, the done note to its holder) for once the answer, as
    /// given, has gone to the holder.
    pub(crate) fn desk_question_picked(&self, id: &str, d: &Decision) -> Option<Effects> {
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).and_then(Value::as_str).unwrap_or("").to_string();
        let option = letter_of(&answer)?;
        let mocks = self.read(|st| {
            let ask = st.items.iter().find(|i| i.id == id && i.kind == "question" && i.status == "pending")?;
            let qs = ask.tool_input["questions"].as_array().filter(|q| q.len() == 1)?;
            let question = qs[0]["question"].as_str().unwrap_or("");
            let it = st.desk.items.iter().find(|i| {
                i.picked.is_empty() && !matches!(i.state.as_str(), "done" | "dropped") && i.holder.as_ref().and_then(|h| ask_of(st, h)).is_some_and(|a| a.id == id)
            })?;
            // Only a question that asks you to pick, naming the lettered options in its words or its choices.
            let asks = question.split(|c: char| !c.is_alphanumeric()).any(|w| w.eq_ignore_ascii_case("pick") || w.eq_ignore_ascii_case("choose"));
            let mut letters = pick_letters(question);
            if letters.is_empty() {
                let labels = qs[0]["options"].as_array().map(|o| o.iter().filter_map(|x| x["label"].as_str().and_then(letter_of)).collect::<Vec<_>>()).unwrap_or_default();
                letters = if labels.len() > 1 { labels } else { vec![] };
            }
            (asks && letters.contains(&option)).then(|| it.id.clone())
        })?;
        let now = now_ms();
        match self.write(|st| pick(st, &mocks, &option, now)) {
            Ok(mut fx) => {
                // This very question is the one answered: by you, as you answered it.
                fx.answers.clear();
                Some(fx)
            }
            Err(e) => {
                eprintln!("cue: desk: an answer's pick on #{mocks} wasn't taken: {e}");
                None
            }
        }
    }

    /// `f` run while the Desk's saves wait for it (saves.rs: each writes what the store holds then, in order).
    pub(crate) fn desk_saving<T>(&self, f: impl FnOnce() -> T) -> T {
        let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
        f()
    }

    /// How the sends of steps `steps` went (#437), kept on each step: "sent", "kept" (Cue keeps it for when that
    /// session's turn ends) or "failed: <why>". Saved, and the window shown it.
    pub(crate) fn desk_went(&self, steps: &[String], sent: &Result<String, String>) {
        let went = match sent {
            Ok(said) if crate::hub::team::kept(said) => "kept".to_string(),
            Ok(_) => "sent".to_string(),
            Err(e) => format!("failed: {e}"),
        };
        let saved: Vec<DeskDecision> = self.write(|st| {
            st.desk.decisions.iter_mut().filter(|d| steps.contains(&d.id)).filter_map(|d| {
                let del = d.delivery.as_mut()?;
                del.went = went.clone();
                Some(d.clone())
            }).collect()
        });
        for d in &saved {
            crate::db::desk_put_decision(d);
        }
        if !saved.is_empty() {
            self.changed();
        }
    }

    /// What a change left to do, with the store let go: its saves, then its messages and questions, and new items
    /// for their lead (told a moment later, with any others that came meanwhile).
    pub(crate) fn desk_done(&self, mut fx: Effects) {
        // Items queued for a session start there once it's free, or in a fresh one once their wait is up (#576).
        self.desk_queue_sweep(&mut fx);
        if left_nothing(&fx) {
            return;
        }
        // Items going to work get their own copies (or wait for one), before their messages are saved and sent.
        // A paused Desk's sessions are sent nothing, and its items start no sessions, till it resumes (#366).
        self.desk_pause_hold(&mut fx);
        // One start of an item at a time (#656): one sent off while another for it is still on its way (from another
        // line, Start now, or the queue's wait running out) is dropped. Claimed before anything is sent, so a Desk
        // block made from here on shows it starting; desk_start_new lets the claim go.
        if !fx.starts.is_empty() {
            let starts = std::mem::take(&mut fx.starts);
            fx.starts = self.write(|st| starts.into_iter().filter(|s| st.desk.run.new_starts.insert(s.id.clone())).collect());
        }
        self.desk_trees(&mut fx);
        let (went_with, kept_back) = went_with(&mut fx);
        // Rows on a Desk's feed: its lead is sent them once it's free (desk/hub/lead/feed.rs).
        if self.desk_save(&mut fx) {
            self.later(0, |h| h.desk_feed_send());
        }
        self.changed();
        // A pick on an item whose holder waits on its question: answered with the option picked (#281).
        for (ask, question, answer) in fx.answers {
            let d = Decision { behavior: "allow".into(), message: None, answers: Some(json!({ question: answer })), permission: None };
            if !self.respond(&ask, d) {
                eprintln!("cue: desk: its holder's question was gone before the pick could answer it");
            }
        }
        if !kept_back.is_empty() {
            self.desk_went(&kept_back, &Ok("kept in Cue".into()));
        }
        let backs: Vec<String> = fx.backs.iter().map(|(_, m)| m.clone()).collect();
        self.desk_send_out(std::mem::take(&mut fx.sends), went_with, &fx.restarts, &fx.now, &backs);
        self.desk_send_bands(std::mem::take(&mut fx.to_lead), std::mem::take(&mut fx.tells));
        for q in std::mem::take(&mut fx.to_user) {
            self.desk_to_user_ask(&q);
        }
        for (lsid, project, q, choices, item) in fx.asks {
            self.desk_ask(&lsid, &project, &q, &choices, None, item.as_deref());
        }
        for p in fx.lands {
            self.desk_lands_ask(p);
        }
        self.desk_ask_first(fx.uses);
        self.desk_later(fx.starts, fx.checked);
        for id in fx.closes {
            self.later(0, move |h| h.desk_close_on_check(&id));
        }
        for o in fx.on_main {
            self.later(0, move |h| h.desk_close_on_main(o));
        }
        if fx.land {
            self.later(0, |h| h.desk_land_line());
        }
        for (desk, _lead) in fx.install {
            self.later(0, move |h| crate::desk::hub::landing::install_shown::lead_install(h, &desk));
        }
        self.desk_pause_and_cleanup(fx.pause, fx.cleanup);
    }

    /// `desk_done`'s words between sessions and leads, each sent a moment later: `to_lead`, a session's TO LEAD lines,
    /// and `tells`, a lead's bands to another Desk's lead.
    fn desk_send_bands(&self, to_lead: Vec<(String, String, String, String)>, tells: Vec<(String, String, String)>) {
        // A session's words for its lead (#474), to whoever leads that Desk as they go; when they can't, it's told.
        // Cue's, not yours (#831): never kept as words you sent, and never joined to yours (send.rs send_held).
        for (desk, sid, text, words) in to_lead {
            self.later(0, move |h| {
                if let Err(e) = crate::desk::hub::lead::desk_send_cue(h, &desk, &text, crate::sessions::Kind::Question) {
                    eprintln!("cue: desk: {sid}'s TO LEAD didn't reach {desk}'s lead: {e}");
                    let _ = h.tell(&sid, &format!("Your TO LEAD: line didn't reach the Desk's lead: {e}\nIt said: \"{words}\""), crate::sessions::Kind::Answer);
                }
            });
        }
        // A band to another Desk's lead (#540), as Cue's own words to it; when it can't go, the sending lead is told.
        for (desk, text, from) in tells {
            self.later(0, move |h| {
                if let Err(e) = crate::desk::hub::lead::desk_send_cue(h, &desk, &text, crate::sessions::Kind::Peer) {
                    eprintln!("cue: desk: a band didn't reach {desk}'s lead: {e}");
                    let _ = h.tell(&from, &format!("Your TELL didn't reach {}'s lead: {e}", h.desk_name(&desk)), crate::sessions::Kind::Answer);
                }
            });
        }
    }

    /// `desk_done`'s saves, under the Desk's save lock: features settled, sends and holders stamped, then the items,
    /// drops, steps and members written.
    fn desk_save(&self, fx: &mut Effects) -> bool {
        let fed;
        // Each save writes what the store holds now (saves.rs), in the order the saves run.
        let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
        let now = now_ms();
        // A piece changed: its feature as its pieces now have it (#282), saved with them.
        self.write(|st| {
            for f in crate::desk::hub::turn::features::features_of(st, &fx.items) {
                crate::desk::hub::turn::features::settle(st, &f, now, fx);
            }
        });
        let (items, decisions, rows) = self.write(|st| {
            // New items for their lead: rows on its Desk's feed (desk/feed.rs), saved with them.
            for (project, id) in std::mem::take(&mut fx.news) {
                crate::desk::hub::lead::feed::new_item(st, &project, &id, now);
            }
            let (items, decisions) = stamp_sends(st, fx, now);
            // A hold whose item is now done, dropped or parked ends with it (#540).
            let ended = crate::desk::holds::ended(st);
            // Items that waited at landing for it land now (desk/hold_check.rs).
            fx.land |= !ended.is_empty();
            fx.holds.extend(ended);
            (items, decisions, crate::desk::hub::lead::feed::unsaved(st))
        });
        if let Err(e) = crate::db::desk_save_holds(&std::mem::take(&mut fx.holds)) {
            eprintln!("cue: desk: couldn't save its holds: {e}");
        }
        if let Err(e) = crate::db::desk_save_bands(&std::mem::take(&mut fx.bands)) {
            eprintln!("cue: desk: couldn't save the bands between Desks: {e}");
        }
        if let Err(e) = crate::db::desk_save(&items, &fx.dropped, &rows) {
            eprintln!("cue: desk: couldn't save {} item(s) and {} change(s): {e}", items.len() + fx.dropped.len(), rows.len());
        }
        fed = !rows.is_empty();
        for d in &decisions {
            crate::db::desk_put_decision(d);
        }
        fx.joined.iter().filter(|(project, _)| !project.starts_with("desk-") || self.read(|st| st.desk.spaces.contains_key(project))).for_each(|(project, sid)| crate::db::desk_put_member(project, sid, now));
        fed
    }

    /// `desk_done`'s messages, each sent off this thread with the steps that go with it.
    /// Each is Cue's own message to the session (#836), unless it carries your words: a step of yours (your note, your
    /// question, your Send back, a Retry of one) or one of your Send backs (`backs`).
    fn desk_send_out(&self, sends: Vec<DeskSend>, went_with: Vec<Vec<String>>, restarts: &[(String, String, String)], now_ones: &[String], backs: &[String]) {
        for ((sid, text, kind), steps) in sends.into_iter().zip(went_with) {
            let yours = backs.contains(&text) || self.read(|st| st.desk.decisions.iter().any(|d| d.by == "You" && steps.contains(&d.id)));
            // A Resume's send says how it went, on its item (#291).
            let restart = restarts.iter().find(|(_, to, _)| *to == sid).map(|(id, _, name)| (id.clone(), name.clone()));
            let now = now_ones.contains(&sid);
            // Off this thread: typing into a terminal session waits for it to take the message.
            self.later(0, move |h| {
                // Your pictures from an item's page go as pictures, as a session's own chat sends them (#541).
                let (text, pictures) = crate::desk::as_sent(&text);
                let said = if yours { Said::You } else { Said::Cue(kind) };
                let sent = h.send_saved(&sid, &text, pictures, now, said);
                if let Err(e) = &sent {
                    eprintln!("cue: desk: couldn't send {sid} its item: {e}");
                }
                // Your note or send-back (#437): how it went, on its step.
                if !steps.is_empty() {
                    h.desk_went(&steps, &sent);
                }
                if let Some((id, name)) = restart {
                    h.desk_restart_went(&id, &name, sent);
                }
            });
        }
    }

    /// The lead's PAUSE and RESUME, and cleanup cards answered or brought to you, each a moment later.
    fn desk_pause_and_cleanup(&self, pause: Vec<(String, bool, String, String, String)>, cleanup: Vec<(String, Option<String>)>) {
        for (desk, on, name, sid, why) in pause {
            self.later(0, move |h| {
                if let Err(e) = h.desk_pause_set(&desk, on, (&name, &sid), &why) {
                    eprintln!("cue: desk: the lead's {} of {desk}: {e}", if on { "PAUSE" } else { "RESUME" });
                }
            });
        }
        for (card, answer) in cleanup {
            self.later(0, move |h| match answer {
                Some(text) => h.cleanup_answered(&card, &text),
                None => h.cleanup_to_you(&card),
            });
        }
    }

    /// How your Resume of item `id` went, sent to its holder `name`: on the item (shown while it needs resume) and
    /// on its Resume step, saved.
    fn desk_restart_went(&self, id: &str, name: &str, sent: Result<String, String>) {
        let note = match &sent {
            Ok(_) => format!("Resume went to {name}"),
            Err(e) => format!("Resume couldn't reach {name}: {e}"),
        };
        let saved = self.write(|st| {
            let it = st.desk.item_mut(id)?;
            it.restart = note.clone();
            let it = it.clone();
            let step = st.desk.decisions.iter_mut().rev().find(|d| d.item_id.as_deref() == Some(id) && d.by == "You" && d.text == "Resume").map(|d| {
                d.why = if sent.is_ok() { format!("to {name}") } else { format!("to {name}, couldn't reach it: {}", sent.as_ref().err().map_or("", |e| e.as_str())) };
                d.clone()
            });
            Some((it, step))
        });
        if let Some((it, step)) = saved {
            crate::db::desk_put_item(&it);
            if let Some(d) = step {
                crate::db::desk_put_decision(&d);
            }
        }
        self.changed();
    }
}

/// Whether a change left nothing for `desk_done` to do.
fn left_nothing(fx: &Effects) -> bool {
    fx.items.is_empty() && fx.said.is_empty() && fx.dropped.is_empty() && fx.decisions.is_empty() && fx.sends.is_empty() && fx.asks.is_empty() && fx.lands.is_empty() && fx.news.is_empty() && fx.starts.is_empty() && fx.joined.is_empty() && fx.uses.is_empty() && fx.stopped.is_empty() && fx.trees_gone.is_empty() && fx.closes.is_empty() && fx.on_main.is_empty() && fx.cleanup.is_empty() && !fx.land && fx.pause.is_empty() && fx.install.is_empty() && fx.to_lead.is_empty() && fx.holds.is_empty() && fx.tells.is_empty() && fx.bands.is_empty() && fx.to_user.is_empty()
}

/// Steps whose message goes now, by the send that carries it; the rest Cue keeps (a paused Desk, an item waiting
/// for a copy: it goes with the item's start), and their steps say so.
fn went_with(fx: &mut Effects) -> (Vec<Vec<String>>, Vec<String>) {
    let tracked = std::mem::take(&mut fx.went);
    let mut went_with: Vec<Vec<String>> = fx.sends.iter().map(|_| Vec::new()).collect();
    let mut kept_back = Vec::new();
    for (step, to, item) in tracked {
        match fx.sends.iter().position(|(s, text, _)| *s == to && text.contains(&format!("#{item} ("))) {
            Some(n) => went_with[n].push(step),
            None => kept_back.push(step),
        }
    }
    (went_with, kept_back)
}

/// In the store: each send kept as sent to its session, and items at work stamped for their holder; then the items
/// and steps to save.
/// A message the Desk sends a session: to whom, its words, and what kind of Cue's message it is (#837). Your notes
/// and send-backs among them go as yours (`desk_send_out`).
pub(crate) type DeskSend = (String, String, Kind);

/// `send` (to, words) as a Desk send of kind `kind`.
pub(crate) fn kinded((to, text): (String, String), kind: Kind) -> DeskSend {
    (to, text, kind)
}

fn stamp_sends(st: &mut Store, fx: &Effects, now: u64) -> (Vec<DeskItem>, Vec<DeskDecision>) {
    for (to, text, _) in &fx.sends {
        let sent = st.desk.run.sent.entry(to.clone()).or_default();
        // As the session gets it: pictures as their paths after the words (hub/send.rs deliver), so its prompt is
        // known as the Desk's (desk_prompt_taken).
        let (words, pictures) = crate::desk::as_sent(text);
        sent.push(if pictures.is_empty() { words } else { crate::uploads::with_paths(&words, &pictures) });
        if sent.len() > 8 {
            sent.remove(0);
        }
    }
    // An item at work with a holder it wasn't stamped for: its work (and files) start now.
    for id in &fx.items {
        let Some(it) = st.desk.item_mut(id).filter(|i| i.state == "working") else { continue };
        let sid = it.holder.as_ref().map(|h| h.sid.clone()).unwrap_or_default();
        if sid.is_empty() {
            continue;
        }
        if it.held_from.as_ref().is_none_or(|(by, _)| *by != sid) {
            it.held_by(&sid, now);
        }
        // Sent to its holder (again): a stall on it counts from here (desk/hub/stall/stall.rs).
        if fx.sends.iter().any(|(to, _, _)| *to == sid) {
            st.desk.run.sent_at.insert(id.clone(), (sid, now));
        }
    }
    let mut ids: Vec<&String> = fx.items.iter().collect();
    ids.extend(fx.said.iter().filter(|id| !fx.items.contains(id)));
    let items: Vec<DeskItem> = ids.into_iter().filter_map(|id| st.desk.item(id).cloned()).collect();
    let decisions: Vec<DeskDecision> = st.desk.decisions.iter().filter(|d| fx.decisions.contains(&d.id)).cloned().collect();
    (items, decisions)
}

/// What the lead is told when you write your own words on Cue's Yes / No card `question` (#651).
pub(crate) fn card_words(question: &str, text: &str) -> String {
    format!("On Cue's question in your chat (\"{question}\"), the user wrote: \"{text}\"\nThe question is still open and waits for their Yes or No: Cue took neither. Answer them here in plain words (what it means, what each choice does, what you'd pick); don't answer it for them.")
}
