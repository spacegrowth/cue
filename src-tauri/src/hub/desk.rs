//! The Desk in the hub (desk/ has its records and words): `Effects` and the helpers the hub/desk_*.rs files share,
//! restoring the Desk and setting its lead, Cue's asks to the lead and your answers, and `desk_done`. Which topic is
//! in which file: docs/dev/desk-files.md. The lead itself (picking it, starting it, what it's told before each
//! message) is desk_lead.rs.
//! A change is made under the store's lock and leaves `Effects`: its saves and messages, done once it's let go.

use super::*;
use crate::desk::{DeskDecision, DeskItem, Holder};
use crate::sessions::Session;
use super::desk_members::UseAsk;
pub(super) use super::desk_route::{By, decide, give_started, handoff_words, moved_from_of, release, route};
pub(super) use super::desk_view::{held_window, item_sources, shown};
use super::desk_act::{letter_of, pick, pick_letters};

/// What a change to the Desk leaves to do with the store let go.
#[derive(Default)]
pub(super) struct Effects {
    /// Items to save as they are now, and items dropped.
    pub(super) items: Vec<String>,
    /// Items whose holder's words were only kept on them (replied): saved, never counted as sent or started.
    pub(super) said: Vec<String>,
    pub(super) dropped: Vec<String>,
    pub(super) decisions: Vec<String>,
    /// Messages to sessions: (session, text).
    pub(super) sends: Vec<(String, String)>,
    /// Your pick on a mocks item whose holder waits on its question (#281): (that ask, its question, the answer),
    /// answered with the option picked. A pick always changes an item, so `items` is never empty with these.
    pub(super) answers: Vec<(String, String, String)>,
    /// A lead's questions for you: (its session, its project, question, choices, the item it's about).
    pub(super) asks: Vec<(String, String, String, Vec<String>, Option<String>)>,
    /// New items for their project's lead to route: (project, item), told together a moment later.
    pub(super) news: Vec<(String, String)>,
    /// Items a lead gave to a new session (`-> new`): each started a moment later (desk_lead.rs).
    pub(super) starts: Vec<NewStart>,
    /// Sessions that joined a Desk: (project, session), to save.
    pub(super) joined: Vec<(String, String)>,
    /// Routes to sessions not on the Desk: you're asked first, in the lead's name (`desk_ask`).
    pub(super) uses: Vec<UseAsk>,
    /// Items just said DONE: the files their check card shows, read a moment later (desk_checks.rs).
    pub(super) checked: Vec<String>,
    /// Items whose stop just went to the Desk lead: shown again once the lead's time to settle it is up (desk_stall.rs).
    pub(super) stopped: Vec<String>,
    /// Your Restarts (#291): (item, the session it goes to, its name), sent like `sends`, and how each went noted
    /// on the item and its step.
    pub(super) restarts: Vec<(String, String, String)>,
    /// Sessions whose `sends` go now, mid-turn (a Pause, #365): handed over as Send now does, so it reads it at once.
    pub(super) now: Vec<String>,
    /// Dropped items that had their own copy: kept or removed (desk_trees.rs).
    pub(super) trees_gone: Vec<DeskItem>,
    /// Items the lead said DONE on for you (`DeskItem::close_on_check`): closed as Looks right would once to check.
    pub(super) closes: Vec<String>,
    /// Items the lead said are on main already (CLOSE, #347): closed once git says their commit is on the main line.
    pub(super) on_main: Vec<super::desk_unstick::OnMain>,
    /// The lead's word on a landing clean-up question it had first (desk_cleanup.rs): (its card, the answer; None:
    /// passed on to you).
    pub(super) cleanup: Vec<(String, Option<String>)>,
    /// The lead moved an item up in the landing line (LAND FIRST): the line runs now.
    pub(super) land: bool,
    /// The lead's PAUSE (true) or RESUME (false) of its Desk (#366): (Desk, on, its name, its session, why).
    pub(super) pause: Vec<(String, bool, String, String, String)>,
}


/// Item `id` of `project` to a new session, as the lead (`by`, `by_sid`) asked, with its why.
pub(super) struct NewStart {
    pub(super) project: String,
    pub(super) id: String,
    pub(super) why: String,
    pub(super) by: String,
    pub(super) by_sid: String,
}

/// How long new items wait for others before their lead is told of them, in one message.
const NEWS_WAIT_MS: u64 = 1000;

pub(super) fn secs(ms: u64) -> u64 {
    ms / 1000
}

/// What a session goes by on the Desk: the name you gave it, Claude's, else its folder's.
pub(super) fn shown_name(s: &Session) -> String {
    if !s.name.is_empty() {
        return s.name.clone();
    }
    match s.claude_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => crate::model::folder_of(&s.origin.cwd),
    }
}

/// A crew holder's (lead session, agent id).
pub(super) fn crew_parts(h: &Holder) -> Option<(&str, &str)> {
    (h.kind == "crew" && !h.lead_sid.is_empty()).then_some((h.lead_sid.as_str(), h.sid.as_str()))
}

/// The sessions `project`'s items may go to: the project's own, live, oldest first.
pub(super) fn routable<'a>(st: &'a Store, project: &str) -> Vec<&'a Session> {
    let mut ours: Vec<&Session> = st.sessions.iter().filter(|s| s.project == st.desk.project_of(project) && st.desk.lead_of(&s.origin.session_id).is_none_or(|id| id == project)).collect();
    ours.sort_by_key(|s| s.since_ms);
    ours
}

/// Desk `scope`'s Ask me before line for the sessions it hands work to (one_way.rs; "" when nothing's set): its
/// project's, so two Desks on one project share it.
pub(super) fn rule_of(st: &Store, scope: &str) -> String {
    crate::one_way::ask_first(&crate::one_way::kinds(st.desk.project_of(scope)))
}

/// Live session `sid` as a holder: a lead when it has a crew.
pub(super) fn holder_of(st: &Store, sid: &str) -> Option<Holder> {
    st.sessions.get(sid).map(|s| Holder { kind: if s.team.is_empty() { "session".into() } else { "lead".into() }, sid: sid.to_string(), name: shown_name(s), lead_sid: String::new() })
}

/// The session of `project` a ROUTE names (`to`): by name (yours or Claude's), by "sid:<start of its id>" or the
/// start of its id (six characters or more), by folder, else a crew member by the name its lead gave it.
pub(super) fn resolve(st: &Store, project: &str, to: &str) -> Option<Holder> {
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
pub(super) fn at_work(st: &Store, h: &Holder) -> bool {
    match crew_parts(h) {
        Some((lead, agent)) => st.sessions.get(lead).is_some_and(|s| s.team.iter().any(|t| (t.agent_id == agent || t.call == agent) && t.working())),
        None => st.sessions.get(&h.sid).is_some_and(|s| matches!(s.state.as_str(), "working" | "deciding" | "agent")),
    }
}

/// An open ask (not a "your turn" card) of the holder's: for a crew member, its own.
pub(super) fn ask_of<'a>(st: &'a Store, h: &Holder) -> Option<&'a Item> {
    let agent = crew_parts(h).map(|(_, a)| a);
    st.items.iter().find(|i| {
        i.status == "pending" && i.kind != "waiting" && i.lead_has.is_none() && i.origin.session_id == h.to() && agent.is_none_or(|a| i.agent_id == a)
    })
}

/// An ask as an item shows it: { item_id, sid, text, choices }.
pub(super) fn ask_view(i: &Item) -> Value {
    let q = i.tool_input.pointer("/questions/0");
    let text = q.and_then(|q| q["question"].as_str()).map(str::to_string).unwrap_or_else(|| super::summary(i));
    let choices: Vec<String> = q.and_then(|q| q["options"].as_array()).map(|o| o.iter().filter_map(|x| x["label"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    // Its kind: a question (Answer, or Pick from its choices), or a tool to allow (Allow).
    json!({ "item_id": i.id, "sid": i.origin.session_id, "text": text, "choices": choices, "kind": i.kind })
}

/// The first line of a session's last words.
pub(super) fn last_words(s: &Session) -> String {
    s.thread.iter().rev().find(|e| e.role == "agent").and_then(|e| e.text.lines().map(str::trim).find(|l| !l.is_empty())).unwrap_or("").chars().take(160).collect()
}

impl Hub {
    /// The Desk as it was saved (with_ports).
    pub(super) fn desk_restore(st: &mut Store) {
        let (items, decisions, leads) = crate::db::desk_load(now_ms());
        st.desk.spaces = crate::db::desk_spaces().into_iter().map(|d| (d.id.clone(), d)).collect();
        st.desk.items = items;
        st.desk.decisions = decisions;
        st.desk.kept = crate::db::desk_kept();
        st.desk.moves_cleared = crate::db::desk_moves_cleared().into_iter().collect();
        // Items from before numbering get their numbers once, saved under them.
        let changed = st.desk.number_old();
        crate::db::desk_put_numbered(&st.desk, &changed);
        st.desk.lead = leads.into_iter().map(|(p, sid, name)| (p, crate::desk::Lead { sid, name })).collect();
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
        self.desk_lead_to(project, before);
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
                if let Some(project) = id.strip_prefix(super::desk_lead::ROUTES_HELD) {
                    if st.desk.routes_held.get(project) == Some(note) {
                        st.desk.routes_held.remove(project);
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
    pub(super) fn desk_cue_asked(&self, sid: &str, why: &str) {
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

    /// A lead asks you something: a question card of its session's in the Inbox, marked as its project's Desk's.
    /// Your answer goes back to it as a message (`desk_answered`).
    /// `using`: a route waiting on your yes (`UseAsk`), kept on the card for `desk_answered`.
    /// `item`: the item it's about (ASK: #id | …), which then waits on you with it.
    pub(super) fn desk_ask(&self, lsid: &str, project: &str, question: &str, choices: &[String], using: Option<&UseAsk>, item: Option<&str>) {
        let Some(origin) = self.read(|st| st.sessions.origin(lsid)) else { return };
        let options: Vec<Value> = choices.iter().map(|c| json!({ "label": c, "description": "" })).collect();
        let mut input = json!({ "questions": [{ "question": question, "header": "Desk", "options": options, "multiSelect": false }] });
        if let Some(u) = using {
            input["desk_use"] = json!(u);
        }
        if let Some(id) = item {
            input["desk_item"] = json!(id);
        }
        let ask = NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() };
        // Nobody waits on the card: its answer is sent to the lead as a message (`desk_answered`).
        let (id, _) = self.add_ask(origin, ask);
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
    }

    /// Card `id` answered with `d`: a lead's ASK is filed and its answer sent to the lead. False for any other.
    pub(super) fn desk_answered(&self, id: &str, d: &Decision) -> bool {
        let Some((sid, question, using, about)) = self.read(|st| {
            st.items.iter().find(|i| i.id == id && i.desk).map(|i| {
                let using = serde_json::from_value::<UseAsk>(i.tool_input["desk_use"].clone()).ok();
                let about = i.tool_input["desk_item"].as_str().map(|x| format!(" about #{x}")).unwrap_or_default();
                (i.origin.session_id.clone(), i.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string(), using, about)
            })
        }) else { return false };
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        let answer = answer.or_else(|| d.message.clone()).unwrap_or_else(|| if d.behavior == "deny" { "no".into() } else { "yes".into() });
        self.write(|st| st.waiters.remove(id));
        self.finish(id, "answered", &format!("answered: {answer}"));
        let mut words = format!("Your ASK{about}: {question}\nThe answer: {answer}");
        if let Some(u) = using {
            words = self.desk_use_answered(&u, d.behavior != "deny", &answer);
        }
        self.later(0, move |h| {
            if let Err(e) = h.send_to_session(&sid, &words, &[], false) {
                eprintln!("cue: desk: couldn't send the lead its answer: {e}");
            }
        });
        true
    }

    /// Ask `id` answered with `d`: when it's the question of a mocks item's holder, and the answer is one of the
    /// options the question names, that's your pick (#281): the item is picked and done, and its build item made, as
    /// a Pick on the page does. What's left to do (its saves, the done note to its holder) for once the answer, as
    /// given, has gone to the holder.
    pub(super) fn desk_question_picked(&self, id: &str, d: &Decision) -> Option<Effects> {
        let answer = d.answers.as_ref().and_then(|a| a.as_object()).and_then(|a| a.values().next()).and_then(Value::as_str).unwrap_or("").to_string();
        let option = letter_of(&answer)?;
        let mocks = self.read(|st| {
            let ask = st.items.iter().find(|i| i.id == id && i.kind == "question" && i.status == "pending")?;
            let qs = ask.tool_input["questions"].as_array().filter(|q| q.len() == 1)?;
            let question = qs[0]["question"].as_str().unwrap_or("");
            let it = st.desk.items.iter().find(|i| {
                !i.designs.trim().is_empty() && i.holder.as_ref().and_then(|h| ask_of(st, h)).is_some_and(|a| a.id == id)
                    && (i.built_as.is_empty() || st.desk.item(&i.built_as).is_none())
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
    pub(super) fn desk_saving<T>(&self, f: impl FnOnce() -> T) -> T {
        let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
        f()
    }

    /// What a change left to do, with the store let go: its saves, then its messages and questions, and new items
    /// for their lead (told a moment later, with any others that came meanwhile).
    pub(super) fn desk_done(&self, mut fx: Effects) {
        if fx.items.is_empty() && fx.said.is_empty() && fx.dropped.is_empty() && fx.decisions.is_empty() && fx.sends.is_empty() && fx.asks.is_empty() && fx.news.is_empty() && fx.starts.is_empty() && fx.joined.is_empty() && fx.uses.is_empty() && fx.stopped.is_empty() && fx.trees_gone.is_empty() && fx.closes.is_empty() && fx.on_main.is_empty() && fx.cleanup.is_empty() && !fx.land && fx.pause.is_empty() {
            return;
        }
        // Items going to work get their own copies (or wait for one), before their messages are saved and sent.
        // A paused Desk's sessions are sent nothing, and its items start no sessions, till it resumes (#366).
        self.desk_pause_hold(&mut fx);
        self.desk_trees(&mut fx);
        {
            // Each save writes what the store holds now (saves.rs), in the order the saves run.
            let _saving = crate::locking::lock(&self.saves.desk, "desk saves");
            let now = now_ms();
            // A piece changed: its feature as its pieces now have it (#282), saved with them.
            self.write(|st| {
                for f in super::desk_features::features_of(st, &fx.items) {
                    super::desk_features::settle(st, &f, now, &mut fx);
                }
            });
            let (items, decisions) = self.write(|st| {
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
                    // Sent to its holder (again): a stall on it counts from here (desk_stall.rs).
                    if fx.sends.iter().any(|(to, _)| *to == sid) {
                        st.desk.sent_at.insert(id.clone(), (sid, now));
                    }
                }
                let mut ids: Vec<&String> = fx.items.iter().collect();
                ids.extend(fx.said.iter().filter(|id| !fx.items.contains(id)));
                let items: Vec<DeskItem> = ids.into_iter().filter_map(|id| st.desk.item(id).cloned()).collect();
                let decisions: Vec<DeskDecision> = st.desk.decisions.iter().filter(|d| fx.decisions.contains(&d.id)).cloned().collect();
                (items, decisions)
            });
            for it in &items {
                crate::db::desk_put_item(it);
            }
            for id in &fx.dropped {
                crate::db::desk_drop_item(id);
            }
            for d in &decisions {
                crate::db::desk_put_decision(d);
            }
            fx.joined.iter().filter(|(project, _)| !project.starts_with("desk-") || self.read(|st| st.desk.spaces.contains_key(project))).for_each(|(project, sid)| crate::db::desk_put_member(project, sid, now));
        }
        self.changed();
        // A pick on mocks whose holder waits on its question: answered with the option picked (#281).
        for (ask, question, answer) in fx.answers {
            let d = Decision { behavior: "allow".into(), message: None, answers: Some(json!({ question: answer })), permission: None };
            if !self.respond(&ask, d) {
                eprintln!("cue: desk: its holder's question was gone before the pick could answer it");
            }
        }
        for (sid, text) in fx.sends {
            // A Resume's send says how it went, on its item (#291).
            let restart = fx.restarts.iter().find(|(_, to, _)| *to == sid).map(|(id, _, name)| (id.clone(), name.clone()));
            let now = fx.now.contains(&sid);
            // Off this thread: typing into a terminal session waits for it to take the message.
            self.later(0, move |h| {
                let sent = h.send_to_session(&sid, &text, &[], now);
                if let Err(e) = &sent {
                    eprintln!("cue: desk: couldn't send {sid} its item: {e}");
                }
                if let Some((id, name)) = restart {
                    h.desk_restart_went(&id, &name, sent);
                }
            });
        }
        for (lsid, project, q, choices, item) in fx.asks {
            self.desk_ask(&lsid, &project, &q, &choices, None, item.as_deref());
        }
        // A stop the lead hasn't settled turns red once its time is up: the Desk is shown again then.
        if !fx.stopped.is_empty() {
            self.later(super::desk_stall::STOP_WAIT_MS + 1000, |h| h.changed());
        }
        self.desk_ask_first(fx.uses);
        for (project, id) in fx.news {
            let first = self.write(|st| {
                if !st.desk.item(&id).is_some_and(|i| i.project == project) { return false; }
                let fresh = st.desk.fresh.entry(project.clone()).or_default();
                fresh.push(id);
                fresh.len() == 1
            });
            if first {
                self.later(NEWS_WAIT_MS, move |h| h.desk_tell_fresh(&project));
            }
        }
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
        for (desk, on, name, sid, why) in fx.pause {
            self.later(0, move |h| {
                if let Err(e) = h.desk_pause_set(&desk, on, (&name, &sid), &why) {
                    eprintln!("cue: desk: the lead's {} of {desk}: {e}", if on { "PAUSE" } else { "RESUME" });
                }
            });
        }
        for (card, answer) in fx.cleanup {
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
