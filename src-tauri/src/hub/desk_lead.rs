//! A project's Desk lead in the hub (desk/, hub/desk.rs): which session it is, how a message reaches it, what it's
//! told before each one (`desk_summary`), and the new items it's told of to route.

use super::desk::{routable, shown, shown_name};
use super::*;
use crate::desk::{DeskItem, Waiting};

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
    pub(super) fn desk_skill(&self, sid: &str) -> Option<&'static str> {
        self.read(|st| st.desk.lead_of(sid)).map(|_| crate::desk::LEAD_SKILL)
    }

    /// `project`'s lead as the window shows it: { sid, name, state: live | parked | ended }, or null.
    pub(super) fn desk_lead_view(st: &Store, project: &str) -> Value {
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
            if let Some(held) = st.desk.starting.get_mut(project) {
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
            st.desk.starting.insert(project.to_string(), vec![]);
            Ok(DeskClaim::Resume(sid))
        })
    }

    /// A new lead for `project` is starting: messages that come meanwhile wait for it (until `desk_claim_done`).
    /// An error when one is starting or resuming already.
    pub(crate) fn desk_claim_start(&self, project: &str) -> Result<(), String> {
        self.write(|st| {
            if st.desk.starting.contains_key(project) {
                return Err(format!("{}'s lead is starting already", st.desk.name_of(project)));
            }
            st.desk.starting.insert(project.to_string(), vec![]);
            Ok(())
        })
    }

    /// The start or resume `desk_claim` allowed is over: the project is let go, and the messages held meanwhile
    /// handed back to be sent.
    pub(crate) fn desk_claim_done(&self, project: &str) -> Vec<Waiting> {
        self.write(|st| {
            st.desk.lead_claims.retain(|_, desk| desk != project);
            st.desk.starting.remove(project)
        }).unwrap_or_default()
    }

    /// How many messages wait on `project`'s lead starting.
    #[cfg(test)]
    pub(crate) fn desk_held(&self, project: &str) -> usize {
        self.read(|st| st.desk.starting.get(project).map_or(0, Vec::len))
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
    pub(super) fn desk_start_new(self: &Arc<Self>, s: super::desk::NewStart) {
        let name = self.free_name(&format!("{}-{}", self.read(|st| st.desk.name_of(&s.project).to_string()), s.id));
        let dir = self.desk_folder(&s.project).ok_or_else(|| format!("no folder known for {}", s.project));
        // Its folder is one a session of the project already works in: trusted, as you trust it there.
        let opts = crate::ipc::StartOpts { trust: true, ..Default::default() };
        let started = dir.and_then(|dir| crate::ipc::start_hosted(self, crate::hosted::Agent::Claude, &dir, "", &name, &opts, &[]));
        let sid = started.and_then(|v| v["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".to_string()));
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            let given = sid.and_then(|sid| super::desk::give_by(st, &s.id, &sid, (&s.by, &s.by_sid, &s.why), now, &mut fx));
            if let (Err(e), Some(it)) = (given, st.desk.item_mut(&s.id)) {
                it.route_failed = format!("{NEW_SESSION} ({e})");
                it.updated = super::desk::secs(now);
                fx.items.push(s.id.clone());
            }
            fx
        });
        self.desk_done(fx);
    }

    /// The folder a new lead of `project` starts in: the repository of a session of the project on this Mac (live,
    /// then parked).
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
        .filter(|o| super::crew_log::project_shown_for(o) == project)
        .map(|o| o.cwd)
        .collect();
        let cwd = cwds.into_iter().next()?;
        Some(crate::logs::repos::repo_root(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or(cwd))
    }

    /// What the lead is told before each message of yours: the project's items, who holds them, and the sessions it
    /// can hand work to, in one `<desk …>…</desk>` block (no `</desk>` inside it: the window strips the block by it).
    /// `lead`: the session told (left out of the sessions listed; "" for one not started yet). `with_skill`: how a
    /// lead works, first (for a session just made the lead: it has no such system prompt).
    /// With the routes it couldn't make that it's told of: `desk_told` once the message has reached it.
    pub(crate) fn desk_summary(&self, project: &str, lead: &str, with_skill: bool) -> (String, Vec<(String, String)>) {
        let now = now_ms();
        let (text, failed) = self.read(|st| {
            let mut out = match st.desk.spaces.get(project) {
                Some(d) => format!("id=\"{}\" name=\"{}\" project=\"{}\">\n", project, d.name.replace('"', "'"), d.project.replace('"', "'")),
                None => format!("project=\"{}\">\n", project.replace('"', "'")),
            };
            if with_skill {
                out.push_str(crate::desk::LEAD_SKILL);
                out.push_str("\n\n");
            }
            out.push_str("Items:\n");
            let (mut failed, mut told) = (vec![], vec![]);
            let items: Vec<DeskItem> = st.desk.items.iter().filter(|i| i.project == project && i.state != "done").map(|i| shown(st, i)).collect();
            if items.is_empty() {
                out.push_str("(none yet)\n");
            }
            for it in &items {
                item_lines(it, &mut out, &mut failed, &mut told);
            }
            for line in told {
                out.push_str(&line);
            }
            let (on, off): (Vec<&crate::sessions::Session>, Vec<&crate::sessions::Session>) = routable(st, project).into_iter().filter(|s| s.origin.session_id != lead).partition(|s| st.desk.on_desk(project, &s.origin.session_id));
            let line = |s: &crate::sessions::Session| {
                    let crew: Vec<&str> = s.team.iter().map(|t| t.name.as_str()).collect();
                    let what = if crew.is_empty() { "session".to_string() } else { format!("lead, crew: {}", crew.join(", ")) };
                    let id: String = s.origin.session_id.chars().take(8).collect();
                    let state = if s.state.is_empty() { "idle" } else { &s.state };
                    format!("{} (sid:{id}, {what}, {state}{})", shown_name(s), context_note(st, &s.origin.session_id, now))
            };
            let names: Vec<String> = on.into_iter().map(line).collect();
            out.push_str(&format!("Sessions on the Desk, you can route to: {}\n", if names.is_empty() { "none open but you: ROUTE: #<id> -> new".to_string() } else { names.join("; ") }));
            if !off.is_empty() {
                let names: Vec<String> = off.into_iter().map(|s| { let id: String = s.origin.session_id.chars().take(8).collect(); format!("{} (sid:{id})", shown_name(s)) }).collect();
                out.push_str(&format!("Other sessions of this project, not on the Desk (a ROUTE to one asks the user first; a yes adds it): {}\n", names.join("; ")));
            }
            if let Some(me) = st.sessions.get(lead) {
                let context = context_note(st, lead, now);
                out.push_str(&format!("You: {}{context}. You only hand work out: ROUTE: #<id> -> new when no session fits.\n", shown_name(me)));
            }
            (out, failed)
        });
        // Titles and names are anyone's words: none of them may close the block early.
        (format!("<desk {}</desk>", text.replace("</desk", "</ desk")), failed)
    }

    /// `project`'s new items, waited on for a moment (`desk_done`): its lead is told of those still new and held by
    /// nobody, in one message, to route them. Not sent (no lead, or it has ended): they stay new on the Desk.
    pub(super) fn desk_tell_fresh(self: &Arc<Self>, project: &str) {
        let items: Vec<DeskItem> = self.write(|st| {
            let ids = st.desk.fresh.remove(project).unwrap_or_default();
            ids.iter().filter_map(|id| st.desk.item(id)).filter(|i| i.state == "new" && i.holder.is_none()).cloned().collect()
        });
        if items.is_empty() {
            return;
        }
        if let Err(e) = crate::ipc::desk_send_with(self, project, &crate::desk::new_items_words(&items)) {
            eprintln!("cue: desk: {project}'s lead wasn't told of its new items: {e}");
        }
    }
}

/// `tokens` of context as the Desk says it: "142k".
pub(super) fn context_k(tokens: u64) -> String {
    format!("{}k", (tokens + 500) / 1000)
}

/// Session `sid`'s context and cache as the lead's summary says them, from its card's numbers (cache_tick): ", 142k,
/// cache 12m" or ", 142k, cold", and " → Cue will hand off at expiry" when the Desk's rule will ask it for a note.
/// Nothing for a session with no card waiting (at work, or not on this Mac).
fn context_note(st: &Store, sid: &str, now: u64) -> String {
    let Some(v) = st.insight.get(sid) else { return String::new() };
    let tokens = v["tokens"].as_u64().unwrap_or(0);
    if tokens == 0 {
        return String::new();
    }
    let until = v["until_ms"].as_u64().unwrap_or(0);
    let mut out = format!(", {}", context_k(tokens));
    if until > now {
        out.push_str(&format!(", cache {}m", (until - now).div_ceil(60_000)));
    } else if until > 0 {
        out.push_str(", cold");
    }
    if super::cache_tick::desk_will_hand_off(st, sid, v, now) {
        out.push_str(" → Cue will hand off at expiry");
    }
    out
}

/// Item `it`'s line in the Desk's summary (into `out`), with the routes it couldn't make (into `failed`) and what the
/// lead is told of them (into `told`).
fn item_lines(it: &DeskItem, out: &mut String, failed: &mut Vec<(String, String)>, told: &mut Vec<String>) {
    let kind = if it.kind.is_empty() { "no kind yet" } else { &it.kind };
    out.push_str(&format!("#{} {kind}: {} | {}", it.id, it.title, it.state.replace('_', " ")));
    if let Some(h) = &it.holder {
        out.push_str(&format!(" | held by {} ({})", h.name, h.kind));
        if !it.crew.is_empty() {
            out.push_str(&format!(" → {} (crew)", it.crew));
        }
    }
    if let Some(a) = &it.after {
        out.push_str(&format!(" | after #{a}"));
    }
    if !it.done_when.is_empty() {
        out.push_str(&format!(" | done when: {}", it.done_when));
    }
    if !it.files.is_empty() {
        // Its own files only (hub/desk.rs), and a few of them: the rest as a count.
        let more = it.files.len().saturating_sub(5);
        let more = if more > 0 { format!(" +{more} more") } else { String::new() };
        out.push_str(&format!(" | files: {}{more}", it.files.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
    }
    out.push('\n');
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
