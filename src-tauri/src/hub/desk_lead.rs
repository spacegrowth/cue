//! A project's Desk lead in the hub (desk/, hub/desk.rs): which session it is, how a message reaches it, what it's
//! told before each one (`desk_summary`), and the new items it's told of to route.

use super::desk::{routable, shown, shown_name};
use super::*;
use crate::desk::{DeskItem, Waiting};

/// How `desk_summary` names a project's held route answers (`routes_held`, #248) for `desk_told`: this, then the project.
pub(super) const ROUTES_HELD: &str = "routes:";

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
            if project.starts_with("desk-") && !st.desk.spaces.contains_key(project) { return Err("no such Desk".into()); }
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
        let ready = |st: &Store| st.desk.item(&s.id).is_some_and(|i| i.project == s.project && i.state != "done" && i.after.is_none());
        if !self.read(ready) { return; }
        let name = self.free_name(&format!("{}-{}", self.read(|st| st.desk.name_of(&s.project).to_string()), s.id));
        let dir = self.desk_folder(&s.project).ok_or_else(|| format!("no folder known for {}", s.project));
        // Its folder is one a session of the project already works in: trusted, as you trust it there.
        let opts = crate::ipc::StartOpts { trust: true, ..Default::default() };
        let agent = self.session_origin(&s.by_sid).and_then(|o| crate::hosted::Agent::named(&o.harness)).unwrap_or(crate::hosted::Agent::Claude);
        let started = dir.and_then(|dir| crate::ipc::start_hosted(self, agent, &dir, "", &name, &opts, &[]));
        // Moved off a holder (#364): its hand-off note goes with it, one written since it took the item.
        let moved = self.read(|st| st.desk.item(&s.id).and_then(|it| super::desk::moved_from_of(st, it)));
        let note = moved.as_ref().and_then(|(sid, from, _)| sid.as_deref().and_then(crate::db::handoff).filter(|k| !k.note.is_empty() && k.note_ms >= *from)).map(|k| k.note).unwrap_or_default();
        let last = moved.map(|(_, _, last)| last).unwrap_or_default();
        let sid = started.and_then(|v| v["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".to_string()));
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            if !ready(st) { return fx; }
            let handoff = st.desk.item(&s.id).map(|it| super::desk::handoff_words(it, &note, &last)).unwrap_or_default();
            let given = sid.and_then(|sid| super::desk::give_started(st, &s.id, &sid, (&s.by, &s.by_sid, &s.why), &handoff, now, &mut fx));
            if let Err(e) = given {
                start_failed(st, &s, &e, now, &mut fx);
            }
            fx
        });
        self.desk_done(fx);
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
        .filter(|o| super::crew_log::project_shown_for(o) == project)
        .map(|o| o.cwd)
        .collect();
        let cwd = folder_first(cwds, project)?;
        Some(crate::logs::repos::repo_root(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or(cwd))
    }

    /// What the lead is told before each message of yours: the project's items, who holds them, and the sessions it
    /// can hand work to, in one `<desk …>…</desk>` block (no `</desk>` inside it: the window strips the block by it).
    /// `lead`: the session told (left out of the sessions listed; "" for one not started yet). `with_skill`: how a
    /// lead works (for a session just made the lead: it has no such system prompt).
    /// With the routes it couldn't make that it's told of: `desk_told` once the message has reached it.
    pub(crate) fn desk_summary(&self, project: &str, lead: &str, with_skill: bool) -> (String, Vec<(String, String)>) {
        let now = now_ms();
        let (text, failed) = self.read(|st| {
            let mut out = match st.desk.spaces.get(project) {
                Some(d) => format!("id=\"{}\" name=\"{}\" project=\"{}\">\n", project, d.name.replace('"', "'"), d.project.replace('"', "'")),
                None => format!("project=\"{}\">\n", project.replace('"', "'")),
            };
            if !lead.is_empty() {
                let name = st.sessions.get(lead).map(shown_name)
                    .or_else(|| st.desk.lead.get(project).filter(|l| l.sid == lead).map(|l| l.name.clone()))
                    .unwrap_or_else(|| lead.chars().take(8).collect());
                out.push_str(&format!("Desk lead: {name} (you){}\n\n", context_note(st, lead, false, now)));
            }
            let (mut failed, mut told) = (vec![], vec![]);
            let (ready, open): (Vec<_>, Vec<_>) = st.desk.items.iter()
                .filter(|i| i.project == project && i.state != "done")
                .map(|i| shown(st, i))
                .partition(|i| i.state == "new" && i.holder.is_none());
            out.push_str("Needs routing:\n");
            if ready.is_empty() {
                out.push_str("None.\n");
            }
            for it in &ready {
                item_lines(st, it, &mut out, &mut failed, &mut told);
            }
            // On hold (#385): listed apart, so they never read as waiting for a route.
            let (held, open): (Vec<_>, Vec<_>) = open.into_iter().partition(|i| i.state == "held");
            if !open.is_empty() {
                out.push_str("\nOther open items:\n");
                for it in &open {
                    item_lines(st, it, &mut out, &mut failed, &mut told);
                }
            }
            if !held.is_empty() {
                out.push_str("\nOn hold (the user starts these themselves; don't ROUTE or START them):\n");
                for it in &held {
                    item_lines(st, it, &mut out, &mut failed, &mut told);
                }
            }
            for line in told {
                out.push_str(&line);
            }
            // Its routes Cue couldn't make that it couldn't be told of then, as it was asking the user something (#248).
            if let Some(held) = st.desk.routes_held.get(project).filter(|h| !h.is_empty()) {
                out.push_str(held);
                failed.push((format!("{ROUTES_HELD}{project}"), held.clone()));
            }

            // Sessions with work (#363) get a line each; the rest of the Desk's, idle with no recent item, one line.
            let work = super::desk_members::with_work(st, project, now);
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
            // One session as a route: its name, its sid's start, its state, its crew and its size and cache.
            let route_of = |s: &crate::sessions::Session| {
                let sid = &s.origin.session_id;
                let id: String = sid.chars().take(8).collect();
                let mut r = format!("{} (sid:{id}", shown_name(s));
                if idle_route(st, s) {
                    r.push_str(", idle");
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
            };
            for (_, s) in on {
                out.push_str(&format!("- {}\n", route_of(s)));
            }
            if !older.is_empty() {
                let all: Vec<String> = older.iter().map(|s| route_of(s)).collect();
                out.push_str(&format!("Older, idle with no recent item: {}\n", all.join("; ")));
            }
            if let Some(paused) = super::desk_pause::pause_block(project, now) {
                out.push('\n');
                out.push_str(&paused);
            }
            if let Some(away) = super::desk_away::away_block(st, project, now) {
                out.push('\n');
                out.push_str(&away);
            }
            // Running leads get this too, even if their original system prompt is older.
            out.push('\n');
            out.push_str(crate::desk::ROUTING_GUIDANCE);
            out.push('\n');
            out.push_str("\nAnswer the user's question or request directly in plain language before any Desk command lines.\n");
            if with_skill {
                out.push('\n');
                out.push_str(crate::desk::LEAD_SKILL);
                out.push('\n');
            }
            (out, failed)
        });
        // Titles and names are anyone's words: none of them may close the block early.
        (format!("<desk {}</desk>", text.replace("</desk", "</ desk")), failed)
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

fn idle_route(st: &Store, s: &crate::sessions::Session) -> bool {
    matches!(s.state.as_str(), "" | "idle" | "waiting")
        && !super::desk_stall::still_at_it(st, &s.origin.session_id)
        && s.compacting_ms == 0
        && s.held.is_none()
        && s.queued.is_none()
        && !s.thread.last().is_some_and(|e| e.role == "you" && e.pending)
}

/// `tokens` of context as the Desk says it: "142k".
pub(super) fn context_k(tokens: u64) -> String {
    format!("{}k", (tokens + 500) / 1000)
}

/// A session whose cache ran out takes no new work from this size up (docs/desk-routing-merged.md, rule 17): waking it
/// writes its whole context to the cache again.
pub(super) const COLD_NEW_WORK_MAX: u64 = 60_000;

/// Show cache details when a pending hand-off could affect a routing decision, or (`route`: a session the lead routes
/// to) when it is too big to wake cold for new work.
fn context_note(st: &Store, sid: &str, route: bool, now: u64) -> String {
    let Some(v) = st.insight.get(sid) else { return String::new() };
    let (tokens, until) = (v["tokens"].as_u64().unwrap_or(0), v["until_ms"].as_u64().unwrap_or(0));
    if !super::cache_tick::desk_will_hand_off(st, sid, v, now) {
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
pub(super) fn folder_first(cwds: Vec<String>, project: &str) -> Option<String> {
    let own = cwds.iter().position(|c| crate::model::own_project(c, "") == project).unwrap_or(0);
    cwds.into_iter().nth(own)
}

/// Cue couldn't start a session for item `s.id` (`e`, why): it shows "couldn't start" with a retry, and the Desk's
/// lead is told now, not with your next message (it may be a while).
pub(super) fn start_failed(st: &mut Store, s: &super::desk::NewStart, e: &str, now: u64, fx: &mut super::desk::Effects) {
    let Some(it) = st.desk.item_mut(&s.id) else { return };
    it.start_failed = e.to_string();
    it.updated = super::desk::secs(now);
    let words = format!("Cue couldn't start a new session for #{} ({}): {e}. It's still {}; ROUTE it to a session, or to new to try again.", it.id, it.title, it.state.replace('_', " "));
    fx.items.push(s.id.clone());
    let lead = if s.by_sid.is_empty() { st.desk.lead_sid(&s.project).unwrap_or_default().to_string() } else { s.by_sid.clone() };
    if st.sessions.get(&lead).is_some() {
        fx.sends.push((lead, words));
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
            Some(h) => format!("on hold: {}", h.2),
            None => "on hold".into(),
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
    if let Some(a) = &it.after {
        out.push_str(&format!(" | after #{a}"));
    }
    if let Some((_, why)) = it.paused.as_ref().filter(|_| it.state == "paused") {
        out.push_str(&if why.is_empty() { " | RESUME it when it should go on".to_string() } else { format!(" | paused: {why}") });
    }
    // Its own wait, said (#340): with nothing else shown it read as free to start, and was nudged by hand.
    if it.state == "queued" && it.tree_wait {
        out.push_str(&format!(" | {}", super::desk_trees::copy_wait(st, &it.project, &it.tree_wait_repo)));
    }
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
        // Its own files only (hub/desk.rs), and a few of them: the rest as a count.
        let more = it.files.len().saturating_sub(5);
        let more = if more > 0 { format!(" +{more} more") } else { String::new() };
        out.push_str(&format!(" | files: {}{more}", it.files.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
    }
    if !it.start_failed.is_empty() {
        out.push_str(&format!(" | couldn't start a new session: {}", it.start_failed));
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
