//! A project's Desk lead in the hub (desk/, desk/hub/core/desk.rs): which session it is, how a message reaches it, what it's
//! told before each one (`desk_summary`), and the new items it's told of to route.

use crate::desk::hub::core::desk::{routable, shown, shown_name};
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, Waiting};

/// How `desk_summary` names a project's held route answers (`routes_held`, #248) for `desk_told`: this, then the project.
pub(crate) const ROUTES_HELD: &str = "routes:";

/// How a failed start of a new session for an item begins its `route_failed` (the error follows in brackets).
const NEW_SESSION: &str = "a new session";

/// How a message reaches a project's lead (`desk_claim`).
pub(crate) enum DeskClaim {
    /// Live: sent to it.
    Send(String),
    /// Parked: resumed with it.
    Resume(String),
    /// Being started or resumed by another message: held, and sent once it's up. Its outcome: the lead's id, or
    /// why it wasn't sent (disconnected: the start gave up without a word).
    Wait(std::sync::mpsc::Receiver<Result<String, String>>),
}

impl Hub {
    /// What a session starting is told beyond your words: the lead's instructions, if it leads a Desk.
    pub(crate) fn desk_skill(&self, sid: &str) -> Option<&'static str> {
        self.read(|st| st.desk.lead_of(sid)).map(|_| crate::desk::LEAD_SKILL)
    }

    /// `project`'s lead as the window shows it: { sid, name, state: live | parked | ended }, or null.
    pub(crate) fn desk_lead_view(st: &Store, project: &str) -> Value {
        let Some(l) = st.desk.lead.get(project) else { return Value::Null };
        let parked = st.parked.iter().find(|p| p.session.origin.session_id == l.sid).map(|p| &p.session);
        let (state, name) = match (st.sessions.get(&l.sid), parked) {
            (Some(s), _) => ("live", shown_name(s)),
            (None, Some(s)) => ("parked", shown_name(s)),
            (None, None) => ("ended", l.name.clone()),
        };
        json!({ "sid": l.sid, "name": if name.is_empty() { l.name.clone() } else { name }, "state": state })
    }

    /// How your words (`text`) reach `project`'s lead, decided in one step so two at once never resume it
    /// twice: sent to it while it's live (paused is live: a message starts it again); resumed with it when it's
    /// parked. Resume claims the project until `desk_claim_done`; words that come meanwhile (or while a lead starts
    /// or changes) are held, and sent then (Wait). An error when there's no lead, or it has ended.
    pub(crate) fn desk_claim(&self, project: &str, text: &str) -> Result<DeskClaim, String> {
        self.write(|st| {
            if let Some(held) = st.desk.run.starting.get_mut(project) {
                let (reply, outcome) = std::sync::mpsc::channel();
                held.push(Waiting { text: text.to_string(), reply });
                return Ok(DeskClaim::Wait(outcome));
            }
            let sid = st.desk.lead_sid(project).map(str::to_string).ok_or("no lead")?;
            if st.sessions.get(&sid).is_some() {
                return Ok(DeskClaim::Send(sid));
            }
            if !st.parked.iter().any(|p| p.session.origin.session_id == sid) {
                return Err("lead ended".into());
            }
            st.desk.run.starting.insert(project.to_string(), vec![]);
            Ok(DeskClaim::Resume(sid))
        })
    }

    /// A new lead for `project` is starting: messages that come meanwhile wait for it (until `desk_claim_done`).
    /// An error when one is starting or resuming already.
    pub(crate) fn desk_claim_start(&self, project: &str) -> Result<(), String> {
        self.write(|st| {
            if project.starts_with("desk-") && !st.desk.spaces.contains_key(project) { return Err("no such Desk".into()); }
            if st.desk.run.starting.contains_key(project) {
                return Err(format!("{}'s lead is starting already", st.desk.name_of(project)));
            }
            st.desk.run.starting.insert(project.to_string(), vec![]);
            Ok(())
        })
    }

    /// The start or resume `desk_claim` allowed is over: the project is let go, and the messages held meanwhile
    /// handed back to be sent.
    pub(crate) fn desk_claim_done(&self, project: &str) -> Vec<Waiting> {
        self.write(|st| {
            st.desk.run.lead_claims.retain(|_, desk| desk != project);
            st.desk.run.starting.remove(project)
        }).unwrap_or_default()
    }

    /// How many messages wait on `project`'s lead starting.
    #[cfg(test)]
    pub(crate) fn desk_held(&self, project: &str) -> usize {
        self.read(|st| st.desk.run.starting.get(project).map_or(0, Vec::len))
    }

    /// Session `sid` is starting as `project`'s lead: kept before it starts, so it starts with its instructions.
    pub(crate) fn desk_lead_starting(&self, project: &str, sid: &str, name: &str) {
        self.desk_put_lead(project, sid, name);
    }

    /// `project`'s lead, if it has one.
    pub(crate) fn desk_lead(&self, project: &str) -> Option<crate::desk::Lead> {
        self.read(|st| st.desk.lead.get(project).cloned())
    }

    /// `project`'s lead's session, if it has one.
    pub(crate) fn desk_lead_sid(&self, project: &str) -> Option<String> {
        self.read(|st| st.desk.lead_sid(project).map(str::to_string))
    }

    /// Session `sid` as a lead for `project`: the name it goes by and where it is ("live", "parked", or "ended": in
    /// Cue's record of closed sessions). An error when it's none of those, another project's, or runs another
    /// project's Desk already (a session's lines count for one Desk).
    pub(crate) fn desk_lead_candidate(&self, project: &str, sid: &str) -> Result<(String, &'static str), String> {
        let found = self.read(|st| {
            let live = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s), "live"));
            live.or_else(|| st.parked.iter().find(|p| p.session.origin.session_id == sid).map(|p| (p.session.project.clone(), shown_name(&p.session), "parked")))
        });
        let (of, name, state) = match found {
            Some(f) => f,
            None => crate::db::ended_session(sid).map(|(_, s)| (s.project.clone(), shown_name(&s), "ended")).ok_or("no such session")?,
        };
        if of != self.read(|st| st.desk.project_of(project).to_string()) {
            return Err(format!("{name} isn't a session of {}", self.desk_name(project)));
        }
        if let Some(other) = self.read(|st| st.desk.lead_of(sid)).filter(|p| p != project) {
            return Err(format!("{name} already runs {}'s Desk", self.desk_name(&other)));
        }
        Ok((name, state))
    }

    /// A name for a new lead of `project` no open or parked session goes by: "<project>-lead", else "-lead-2"…
    pub(crate) fn desk_lead_name(&self, project: &str) -> String {
        self.free_name(&format!("{}-lead", self.read(|st| st.desk.name_of(project).to_string())))
    }

    /// `base`, else "<base>-2"…: the first no open or parked session goes by.
    fn free_name(&self, base: &str) -> String {
        let taken: Vec<String> = self.read(|st| st.sessions.iter().chain(st.parked.iter().map(|p| &p.session)).map(shown_name).collect());
        (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") }).find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n))).unwrap_or(base.to_string())
    }

    /// An item the lead gave to a new session (`ROUTE: #id -> new`): a hosted Claude started in the project's folder,
    /// named "<project>-<id>", that holds it and is sent it. If it can't start, the item says why (and the lead is told).
    pub(crate) fn desk_start_new(self: &Arc<Self>, s: crate::desk::hub::core::desk::NewStart) {
        let s = &s;
        let ready = |st: &Store| st.desk.item(&s.id).is_some_and(|i| i.project == s.project && i.state != "done" && i.after.is_none());
        // Claimed as starting since it was sent off (`DeskRun::new_starts`, #656): let go here when it won't start,
        // else once its session has it or couldn't start.
        if !self.write(|st| ready(st) || { st.desk.run.new_starts.remove(&s.id); false }) { return; }
        let name = self.free_name(&format!("{}-{}", self.read(|st| st.desk.name_of(&s.project).to_string()), s.id));
        // On a machine (the lead's `on:`, your choice on the item, or the Desk's setting: desk/hub/landing/remote_item.rs
        // `start_place`, #597): it starts in the Desk's clone there, from the latest main (`desk_remote_starts`).
        // Picked and claimed in one step (#661): two items starting at once on a Desk that spreads over several
        // machines count each other, so they go to different ones.
        // A machine with its number of the Desk's items at work already (#711, Desk settings): it waits for room there.
        let (place, full) = self.write(|st| {
            let place = st.desk.item(&s.id).map(|i| crate::desk::hub::landing::remote_item::start_place(st, i)).unwrap_or(Ok(None));
            if let Ok(p) = &place {
                let m = p.as_ref().map(|o| o.machine.clone()).unwrap_or_default();
                if !crate::desk::hub::landing::remote_item::has_room(st, &s.project, &m) {
                    let mut fx = crate::desk::hub::core::desk::Effects::default();
                    st.desk.run.new_starts.remove(&s.id);
                    if ready(st) {
                        machine_wait(st, s, &m, now_ms(), &mut fx);
                    }
                    return (place, Some(fx));
                }
                st.desk.run.start_machine.insert(s.id.clone(), m);
            }
            (place, None)
        });
        if let Some(fx) = full {
            return self.desk_done(fx);
        }
        let on = place.clone().ok().flatten();
        let dir = self.new_start_dir(s, &place, &on);
        // Its folder is one a session of the project already works in, or the one the lead named on a machine: trusted,
        // as you trust it there.
        // Its model (#620): the lead's on the route, else the Desk's default, else Claude Code's pick. Kept on the item,
        // so its card says what it started on.
        let asked = self.read(|st| st.desk.item(&s.id).map(|i| i.model.clone()).unwrap_or_default());
        let model = if asked.is_empty() { crate::desk::model_pick::default_of(&s.project) } else { asked };
        let opts = crate::hub::StartOpts { trust: true, machine: on.as_ref().map(|o| o.machine.clone()).unwrap_or_default(), extra: crate::desk::model_pick::flag(&model), ..Default::default() };
        let agent = self.session_origin(&s.by_sid).and_then(|o| crate::hosted::Agent::named(&o.harness)).unwrap_or(crate::hosted::Agent::Claude);
        let started = dir.and_then(|dir| crate::hub::start_hosted(self, agent, &dir, "", &name, &opts, &[]));
        // Moved off a holder (#364): its hand-off note goes with it, one written since it took the item.
        let moved = self.read(|st| st.desk.item(&s.id).and_then(|it| crate::desk::hub::core::desk::moved_from_of(st, it)));
        let note = moved.as_ref().and_then(|(sid, from, _)| sid.as_deref().and_then(crate::db::handoff).filter(|k| !k.note.is_empty() && k.note_ms >= *from)).map(|k| k.note).unwrap_or_default();
        let last = moved.map(|(_, _, last)| last).unwrap_or_default();
        // Started there: that machine's folder is remembered for the Desk's next item on it.
        if let (Some(o), Ok(_)) = (&on, &started) {
            crate::desk::machine_pref::remember(&s.project, &o.machine, &o.folder);
        }
        let sid = started.and_then(|v| v["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".to_string()));
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            // Held by its session from here (or not started): the claim goes in the same step, so the item never
            // reads as neither (#656).
            st.desk.run.new_starts.remove(&s.id);
            st.desk.run.start_machine.remove(&s.id);
            if !ready(st) { return fx; }
            let handoff = st.desk.item(&s.id).map(|it| crate::desk::hub::core::desk::handoff_words(it, &note, &last)).unwrap_or_default();
            if let (Ok(_), Some(it)) = (&sid, st.desk.item_mut(&s.id)) {
                it.model = if model.is_empty() { crate::desk::model_pick::DEFAULT.into() } else { model.clone() };
            }
            let given = sid.and_then(|sid| crate::desk::hub::core::desk::give_started(st, &s.id, &sid, (&s.by, &s.by_sid, &s.why), &handoff, now, &mut fx).map(|_| sid));
            match given {
                // Until its first turn ends, a route of the item to new finds it in this one (#656).
                Ok(sid) => _ = st.desk.run.started_for.insert(s.id.clone(), (sid, now)),
                Err(e) => start_failed(st, s, &e, now, &mut fx),
            }
            fx
        });
        self.desk_done(fx);
    }

    /// The folder new start `s` begins in: `place`'s error if it can't, its own copy in the Desk's clone on machine `on`,
    /// the repository it was routed to, else the Desk's folder.
    fn new_start_dir(&self, s: &crate::desk::hub::core::desk::NewStart, place: &Result<Option<crate::desk::copies::OnMachine>, String>, on: &Option<crate::desk::copies::OnMachine>) -> Result<String, String> {
        // Routed to another repository on this Mac (#635): it starts there.
        let in_repo = self.read(|st| st.desk.item(&s.id).and_then(|i| i.in_repo.as_ref()).map(|r| r.repo.clone()));
        match (place, on, in_repo) {
            (Err(e), _, _) => Err(e.clone()),
            // Its folder there not made yet (#664): the project downloaded into it first, from its online home.
            (_, Some(o), _) => self.desk_folder(&s.project).ok_or_else(|| format!("no folder known for {}", s.project)).and_then(|local| crate::desk::hub::landing::remote_setup::ensure(&o.machine, &o.folder, &local))
                // A copy of its own there (#679), so items on one machine never mix their work.
                .and_then(|_| self.remote_start_dir(&s.project, &s.id, &o.machine, &o.folder)),
            (_, _, Some(repo)) => Ok(repo),
            _ => self.desk_folder(&s.project).ok_or_else(|| format!("no folder known for {}", s.project)),
        }
    }

    /// The folder a new lead of `project` starts in: the repository of a session of the project on this Mac (live,
    /// then parked), the project's own repository first: a folder you put in the project is another repository,
    /// whose sessions go by its own name.
    pub(crate) fn desk_folder(&self, project: &str) -> Option<String> {
        if let Some(folder) = self.read(|st| st.desk.spaces.get(project).map(|d| d.folder.clone())) {
            return Some(folder);
        }
        let cwds: Vec<String> = self.read(|st| {
            let live = st.sessions.iter().map(|s| &s.origin);
            let parked = st.parked.iter().map(|p| &p.session.origin);
            live.chain(parked).filter(|o| o.machine.is_empty() && !o.cwd.is_empty()).cloned().collect::<Vec<_>>()
        })
        .into_iter()
        .filter(|o| crate::hub::crew_log::project_shown_for(o) == project)
        .map(|o| o.cwd)
        .collect();
        let cwd = folder_first(cwds, project)?;
        Some(crate::logs::repos::repo_root(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or(cwd))
    }

    /// The folder a loop the lead drafts for `project` runs in (#583), on this Mac: the one its line names (`folder`,
    /// "~/" for your home; its repository's main checkout when it's in one), else the project's Desk or sessions'
    /// (`desk_folder`), else a Desk by that name, else a repository by that name that a Desk or a session here works
    /// in: a folder you put into another project still goes by its own name.
    pub(crate) fn loop_folder(&self, project: &str, folder: &str) -> Option<String> {
        let at_root = |cwd: String| crate::logs::repos::repo_root(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or(cwd);
        if !folder.is_empty() {
            let home = std::env::var("HOME").unwrap_or_default();
            let f = match folder.strip_prefix("~/") {
                Some(rest) if !home.is_empty() => format!("{}/{rest}", home.trim_end_matches('/')),
                _ => folder.to_string(),
            };
            let f = f.trim_end_matches('/').to_string();
            return (f.starts_with('/') && std::path::Path::new(&f).is_dir()).then(|| at_root(f));
        }
        if let Some(f) = self.desk_folder(project) {
            return Some(f);
        }
        let (spaces, cwds) = self.read(|st| {
            let spaces: Vec<(String, String)> = st.desk.spaces.values().filter(|d| !d.folder.is_empty()).map(|d| (d.name.clone(), d.folder.clone())).collect();
            let live = st.sessions.iter().map(|s| &s.origin);
            let parked = st.parked.iter().map(|p| &p.session.origin);
            let cwds: Vec<String> = live.chain(parked).filter(|o| o.machine.is_empty() && !o.cwd.is_empty()).map(|o| o.cwd.clone()).collect();
            (spaces, cwds)
        });
        if let Some((_, f)) = spaces.iter().find(|(name, _)| name == project) {
            return Some(f.clone());
        }
        spaces.into_iter().map(|(_, f)| f).chain(cwds).find(|cwd| crate::model::own_project(cwd, "") == project).map(at_root)
    }

    /// What the lead is told before each message of yours: the project's items, who holds them, and the sessions it
    /// can hand work to, in one `<desk …>…</desk>` block (no `</desk>` inside it: the window strips the block by it).
    /// `lead`: the session told (left out of the sessions listed; "" for one not started yet). `with_skill`: how a
    /// lead works (for a session just made the lead: it has no such system prompt).
    /// With the routes it couldn't make that it's told of: `desk_told` once the message has reached it.
    pub(crate) fn desk_summary(&self, project: &str, lead: &str, with_skill: bool) -> (String, Vec<(String, String)>) {
        let now = now_ms();
        let lands = if lead.is_empty() { String::new() } else { format!("{}\n{}\n", self.desk_lands_line(project).trim_end(), crate::desk::model_pick::lead_line(project)) };
        let (text, failed) = self.read(|st| {
            let mut out = summary_head(st, project, lead, &lands, now);
            let mut failed = vec![];
            summary_items(st, project, &mut out, &mut failed);
            summary_routes(st, project, lead, &mut out, now);
            summary_tail(st, project, with_skill, &mut out, now);
            (out, failed)
        });
        // Titles and names are anyone's words: none of them may close the block early.
        (format!("<desk {}</desk>", crate::owner::in_prompt(&text).replace("</desk", "</ desk")), failed)
    }

    /// The short, visible hand-off for the successor. The full routing context travels separately in the Desk block.
    pub(crate) fn desk_handoff_words(&self, project: &str, sid: &str, name: &str) -> String {
        self.read(|st| {
            let desk = st.desk.name_of(project);
            let lead = st.sessions.get(sid).map(shown_name).filter(|s| !s.is_empty()).unwrap_or_else(|| name.to_string());
            let intro = format!("{lead} now runs {desk}'s Desk.");
            let mut needing_route = st.desk.items.iter()
                .filter(|i| i.project == project && i.state == "new" && i.holder.is_none());
            let titles: Vec<_> = needing_route.by_ref().take(3)
                .map(|i| i.title.split_whitespace().collect::<Vec<_>>().join(" "))
                .map(|title| {
                    let mut chars = title.chars();
                    let mut short: String = chars.by_ref().take(64).collect();
                    if chars.next().is_some() {
                        short.push('…');
                    }
                    short
                })
                .collect();
            if titles.is_empty() {
                return format!("{intro} Nothing needs routing.");
            }
            let more = needing_route.count();
            let mut words = format!("{intro} Please route {}.", titles.join(", "));
            if more > 0 {
                words.push_str(&format!(" {more} more to route."));
            }
            words
        })
    }
}

fn idle_route(st: &Store, s: &crate::sessions::Session) -> bool {
    matches!(s.state.as_str(), "" | "idle" | "waiting")
        && !crate::desk::hub::stall::stall::still_at_it(st, &s.origin.session_id)
        && s.compacting_ms == 0
        && s.running.is_empty()
        && s.held.is_none()
        && s.queued.is_none()
        && !s.thread.last().is_some_and(|e| e.role == "you" && e.pending)
}

/// `tokens` of context as the Desk says it: "142k".
pub(crate) fn context_k(tokens: u64) -> String {
    format!("{}k", (tokens + 500) / 1000)
}

/// A session whose cache ran out takes no new work from this size up (docs/desk-routing-merged.md, rule 17): waking it
/// writes its whole context to the cache again.
pub(crate) const COLD_NEW_WORK_MAX: u64 = 60_000;

/// Show cache details when a pending hand-off could affect a routing decision, or (`route`: a session the lead routes
/// to) when it is too big to wake cold for new work.
fn context_note(st: &Store, sid: &str, route: bool, now: u64) -> String {
    let Some(v) = st.insight.get(sid) else { return String::new() };
    let (tokens, until) = (v["tokens"].as_u64().unwrap_or(0), v["until_ms"].as_u64().unwrap_or(0));
    if !crate::hub::cache_tick::desk_will_hand_off(st, sid, v, now) {
        // Codex logs say no cache time (0): never called cold.
        if route && until > 0 && until <= now && tokens >= COLD_NEW_WORK_MAX {
            return format!(", {}, cold: no new work", context_k(tokens));
        }
        return String::new();
    }
    let minutes = until.saturating_sub(now).div_ceil(60_000);
    format!(", {}, cache {minutes}m → Cue will hand off at expiry", context_k(tokens))
}

/// Of the folders of a project's sessions (in order), the first in the repository named for `project`, else the first.
pub(crate) fn folder_first(cwds: Vec<String>, project: &str) -> Option<String> {
    let own = cwds.iter().position(|c| crate::model::own_project(c, "") == project).unwrap_or(0);
    cwds.into_iter().nth(own)
}

/// Cue couldn't start a session for item `s.id` (`e`, why): it shows "couldn't start" with a retry, and the Desk's
/// lead is told now, not with your next message (it may be a while).
/// New start `s` can't go to `machine` now: it has its number of the Desk's items at work (#711). The item waits
/// queued with no holder, with the why it was started with, and starts by itself once there's room
/// (desk/hub/turn/queue.rs `machine_sweep`).
fn machine_wait(st: &mut Store, s: &crate::desk::hub::core::desk::NewStart, machine: &str, now: u64, fx: &mut crate::desk::hub::core::desk::Effects) {
    let Some(it) = st.desk.item_mut(&s.id) else { return };
    if it.holder.is_some() {
        return;
    }
    let again = it.machine_wait == machine && it.state == "queued";
    (it.state, it.machine_wait, it.next_for, it.after) = ("queued".into(), machine.to_string(), None, None);
    if !s.why.trim().is_empty() {
        it.after_why = s.why.clone();
    }
    it.start_failed.clear();
    if !again {
        it.queued_at = now;
        it.updated = crate::desk::hub::core::desk::secs(now);
    }
    fx.items.push(s.id.clone());
}

pub(crate) fn start_failed(st: &mut Store, s: &crate::desk::hub::core::desk::NewStart, e: &str, now: u64, fx: &mut crate::desk::hub::core::desk::Effects) {
    let Some(it) = st.desk.item_mut(&s.id) else { return };
    it.start_failed = e.to_string();
    it.updated = crate::desk::hub::core::desk::secs(now);
    let words = format!("Cue couldn't start a new session for #{} ({}): {e}. It's still {}; ROUTE it to a session, or to new to try again.", it.id, it.title, it.state.replace('_', " "));
    let key = format!("start:{}", it.updated);
    fx.items.push(s.id.clone());
    // A row of its Desk's change feed, saved with the item.
    crate::desk::hub::lead::feed::raise(st, &s.project, &s.id, crate::desk::feed::ChangeKind::RouteFailed, words, &key, now);
}

/// The Desk summary's opening: which Desk, and the lead told (its name, its context, how its project lands).
fn summary_head(st: &Store, project: &str, lead: &str, lands: &str, now: u64) -> String {
    let mut out = match st.desk.spaces.get(project) {
        Some(d) => format!("id=\"{}\" name=\"{}\" project=\"{}\">\n", project, d.name.replace('"', "'"), d.project.replace('"', "'")),
        None => format!("project=\"{}\">\n", project.replace('"', "'")),
    };
    if !lead.is_empty() {
        let name = st.sessions.get(lead).map(shown_name)
            .or_else(|| st.desk.lead.get(project).filter(|l| l.sid == lead).map(|l| l.name.clone()))
            .unwrap_or_else(|| lead.chars().take(8).collect());
        out.push_str(&format!("Desk lead: {name} (you){}\n{lands}\n", context_note(st, lead, false, now)));
    }
    // Holds on this repository's code, every Desk's (#540): a lead that starts later, or after a compact, sees them.
    if let Some(holds) = crate::desk::holds::summary_line(st, project) {
        out.push_str(&holds);
    }
    out
}

/// The summary's items: to route, the others open, the parked ones; with the routes it couldn't make (`failed`).
fn summary_items(st: &Store, project: &str, out: &mut String, failed: &mut Vec<(String, String)>) {
    let mut told = vec![];
    let (mut ready, open): (Vec<_>, Vec<_>) = st.desk.items.iter()
        .filter(|i| i.project == project && i.state != "done")
        .map(|i| shown(st, i))
        .partition(|i| i.state == "new" && i.holder.is_none() && !st.desk.run.new_starts.contains(&i.id));
    // In your order (#387): the one you dragged to the top first.
    ready.sort_by_key(crate::desk::hub::turn::act::queue_key);
    out.push_str("Needs routing:\n");
    if ready.is_empty() {
        out.push_str("None.\n");
    }
    for it in &ready {
        item_lines(st, it, out, failed, &mut told);
    }
    // Parked (#385, #391): listed apart, so they never read as waiting for a route.
    let (held, open): (Vec<_>, Vec<_>) = open.into_iter().partition(|i| i.state == "held");
    if !open.is_empty() {
        out.push_str("\nOther open items:\n");
        for it in &open {
            item_lines(st, it, out, failed, &mut told);
        }
    }
    if !held.is_empty() {
        out.push_str("\nParked (the owner starts these themselves; don't ROUTE or START them):\n");
        for it in &held {
            item_lines(st, it, out, failed, &mut told);
        }
    }
    for line in told {
        out.push_str(&line);
    }
    // Its routes Cue couldn't make that it couldn't be told of then, as it was asking the user something (#248).
    if let Some(held) = st.desk.run.routes_held.get(project).filter(|h| !h.is_empty()) {
        out.push_str(held);
        failed.push((format!("{ROUTES_HELD}{project}"), held.clone()));
    }
}

/// The summary's routes: the Desk's sessions with work a line each, the rest in one line.
fn summary_routes(st: &Store, project: &str, lead: &str, out: &mut String, now: u64) {
    // Sessions with work (#363) get a line each; the rest of the Desk's, idle with no recent item, one line.
    let work = crate::desk::hub::core::members::with_work(st, project, now);
    let (on, older): (Vec<_>, Vec<_>) = routable(st, project).into_iter()
        .filter(|s| s.origin.session_id != lead && st.desk.on_desk(project, &s.origin.session_id))
        .partition(|s| work.contains(&s.origin.session_id));
    let mut on: Vec<_> = on.into_iter().map(|s| (idle_route(st, s), s)).collect();
    on.sort_by_key(|(idle, _)| !idle);
    if on.is_empty() && older.is_empty() {
        out.push_str("\nNo Desk sessions available. Use ROUTE: #<id> -> new for a new session.\n");
    } else {
        out.push_str("\nDesk routes (or use ROUTE: #<id> -> new):\n");
    }
    for (_, s) in on {
        out.push_str(&format!("- {}\n", route_of(st, s, now)));
    }
    if !older.is_empty() {
        let all: Vec<String> = older.iter().map(|s| route_of(st, s, now)).collect();
        out.push_str(&format!("Older, idle with no recent item: {}\n", all.join("; ")));
    }
}

/// One session as a route: its name, its sid's start, its state, its crew and its size and cache.
fn route_of(st: &Store, s: &crate::sessions::Session, now: u64) -> String {
    let sid = &s.origin.session_id;
    let id: String = sid.chars().take(8).collect();
    let mut r = format!("{} (sid:{id}", shown_name(s));
    if !s.origin.machine.is_empty() {
        r.push_str(&format!(", on {}", crate::machines::display_name(&s.origin.machine)));
    }
    if idle_route(st, s) {
        r.push_str(", idle");
    } else if matches!(s.state.as_str(), "waiting" | "stopped") && !s.running.is_empty() {
        // Its tests or build run on (#556): at work, not idle or waiting.
        r.push_str(&format!(", working: {}", s.running));
    } else if matches!(s.state.as_str(), "" | "idle" | "waiting") {
        r.push_str(", work pending");
    } else {
        r.push_str(&format!(", {}", s.state));
    }
    if !s.team.is_empty() {
        let crew: Vec<_> = s.team.iter().map(|t| t.name.as_str()).collect();
        r.push_str(&format!(", crew: {}", crew.join(", ")));
    }
    r.push_str(&context_note(st, sid, true, now));
    r.push(')');
    r
}

/// The summary's close: a pause or Away in force, how to route, and (for a new lead) how a lead works.
fn summary_tail(st: &Store, project: &str, with_skill: bool, out: &mut String, now: u64) {
    if let Some(paused) = crate::desk::hub::away::pause::pause_block(project, now) {
        out.push('\n');
        out.push_str(&paused);
    }
    if let Some(away) = crate::desk::hub::away::away::away_block(st, project, now) {
        out.push('\n');
        out.push_str(&away);
    }
    // Running leads get this too, even if their original system prompt is older.
    out.push('\n');
    out.push_str(crate::desk::ROUTING_GUIDANCE);
    out.push('\n');
    out.push_str("\nAnswer the owner's question or request directly in plain language before any Desk command lines.\n");
    // Your name (owner.rs): running leads get it too, and Cue's own words above use it.
    out.push_str(&crate::owner::told_line());
    if with_skill {
        out.push('\n');
        out.push_str(crate::desk::LEAD_SKILL);
        out.push('\n');
    }
}

/// Item `it`'s line in the Desk's summary (into `out`), with the routes it couldn't make (into `failed`) and what the
/// lead is told of them (into `told`).
fn item_lines(st: &Store, it: &DeskItem, out: &mut String, failed: &mut Vec<(String, String)>, told: &mut Vec<String>) {
    let kind = if it.kind.is_empty() { "no kind yet" } else { &it.kind };
    let status = match (it.stopped, it.loose.is_empty()) {
        (true, false) => "stopped, not committed".into(),
        (true, true) => "stopped".into(),
        _ if !it.waiting.is_empty() => format!("waiting: {}", it.waiting),
        _ if it.state == "held" => match it.held.as_ref().filter(|h| !h.2.is_empty()) {
            Some(h) => format!("parked: {}", h.2),
            None => "parked".into(),
        },
        _ => it.state.replace('_', " "),
    };
    out.push_str(&format!("- #{} {kind}: {} | {status}", it.id, it.title));
    if let Some(h) = &it.holder {
        out.push_str(&format!(" | held by {} ({})", h.name, h.kind));
        if !it.crew.is_empty() {
            out.push_str(&format!(" → {} (crew)", it.crew));
        }
    }
    if let Some(n) = it.next_for.as_ref().filter(|_| it.holder.is_none()) {
        out.push_str(&format!(" | next for {}", n.name));
    }
    // Cue is starting a new session for it (#656): taken, not waiting for a route.
    if it.holder.is_none() && st.desk.run.new_starts.contains(&it.id) {
        out.push_str(" | a new session is starting for it");
    }
    if let Some(a) = &it.after {
        out.push_str(&format!(" | after #{a}"));
    }
    out.push_str(&crate::desk::hub::turn::features::feature_words(st, it));
    if let Some((_, why)) = it.paused.as_ref().filter(|_| it.state == "paused") {
        out.push_str(&if why.is_empty() { " | RESUME it when it should go on".to_string() } else { format!(" | paused: {why}") });
    }
    // Its own wait, said (#340): with nothing else shown it read as free to start, and was nudged by hand.
    if it.state == "queued" && it.holder.is_none() && !it.machine_wait.is_empty() {
        let n = crate::desk::machine_pref::of(&it.project).at_once(&it.machine_wait);
        out.push_str(&format!(" | waiting for room on {} (the user's setting: {n} of this Desk's items at once there): it starts by itself once one there is done", crate::machines::display_name(&it.machine_wait)));
    } else if it.state == "queued" && it.tree_wait {
        out.push_str(&format!(" | {}", crate::desk::hub::landing::trees::copy_wait(st, &it.project, &it.tree_wait_repo)));
    } else if it.state == "working" && it.tree_wait && it.tree.is_none() {
        // At work without a copy (#566): in line for the next free one.
        out.push_str(&format!(" | at work where its holder is, in line for the next free copy of its repository ({}); COPY FIRST puts it first in that line", it.tree_wait_repo));
    }
    item_details(it, out);
    out.push('\n');
    item_refused(it, failed, told);
}

/// Item `it`'s line, on: when it's done and what you'll see, its files, a start that failed.
fn item_details(it: &DeskItem, out: &mut String) {
    if !it.done_when.is_empty() {
        out.push_str(&format!(" | done when: {}", it.done_when));
    }
    // Its plain line for the user (#382); an open one without it, or without its check, is said so, for the lead to SEE: it.
    if !it.see.is_empty() {
        out.push_str(&format!(" | you'll see: {}", it.see));
    } else if it.state != "done" {
        out.push_str(" | no you'll see yet");
    }
    if it.done_when.is_empty() && it.state != "done" {
        out.push_str(" | no done when yet");
    }
    if !it.files.is_empty() {
        // Its own files only (desk/hub/core/desk.rs), and a few of them: the rest as a count.
        let more = it.files.len().saturating_sub(5);
        let more = if more > 0 { format!(" +{more} more") } else { String::new() };
        out.push_str(&format!(" | files: {}{more}", it.files.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
    }
    if !it.start_failed.is_empty() {
        out.push_str(&format!(" | couldn't start a new session: {}", it.start_failed));
    }
}

/// The routes, queues and reopens Cue couldn't make for item `it` (into `failed`), and what the lead is told of them.
fn item_refused(it: &DeskItem, failed: &mut Vec<(String, String)>, told: &mut Vec<String>) {
    if !it.route_failed.is_empty() {
        failed.push((it.id.clone(), it.route_failed.clone()));
        // Still new only when nobody holds it; else it stays where it is.
        let stays = match &it.holder {
            Some(h) => format!("It stays with {}.", h.name),
            None => "It's still new.".to_string(),
        };
        let why = match it.route_failed.strip_prefix(NEW_SESSION) {
            Some(e) => format!("Cue couldn't start a new session for #{}: {}. {stays}\n", it.id, e.trim().trim_start_matches('(').trim_end_matches(')')),
            None => format!("Cue couldn't route #{} to \"{}\": no session of this project goes by that name. {stays}\n", it.id, it.route_failed),
        };
        told.push(why);
    }
    if !it.after_refused.is_empty() {
        failed.push((it.id.clone(), it.after_refused.clone()));
        told.push(format!("Cue couldn't queue #{} after {}: it started now. If the two would clash (the same files), say how to order them.\n", it.id, it.after_refused));
    }
    if !it.reopen_refused.is_empty() {
        failed.push((it.id.clone(), it.reopen_refused.clone()));
        told.push(format!("Cue didn't reopen #{}: {}.\n", it.id, it.reopen_refused));
    }
}

#[cfg(test)]
mod tests {
    /// The folder a new session of a project starts in: one in the project's own repository before one you put in it.
    #[test]
    fn a_new_session_starts_in_the_projects_own_folder_first() {
        let pick = |c: &[&str], p| super::folder_first(c.iter().map(|s| s.to_string()).collect(), p);
        assert_eq!(pick(&["/nowhere/elsewhere", "/nowhere/app"], "app").as_deref(), Some("/nowhere/app"));
        assert_eq!(pick(&["/nowhere/elsewhere", "/nowhere/more"], "app").as_deref(), Some("/nowhere/elsewhere"));
        assert_eq!(pick(&[], "app"), None);
    }
}
