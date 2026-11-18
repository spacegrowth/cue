//! A Desk's own list of sessions: the ones its lead routes to without asking you (and new). You add and remove them
//! (a session's ⋯ menu, Add to Desk on the left); a session given an item joins, as does a lead. A ROUTE to another
//! session of the project asks you first, in the lead's name: yes puts it on the Desk and makes the route.

use super::desk::{route, shown_name, By, Effects};
use super::*;
use crate::desk::Holder;

/// A lead routed item `item` to session `sid` (`name`), which isn't on the Desk: you're asked to say yes first. A yes
/// puts it on the Desk and makes the route (`desk_answered`); the card keeps what to route, so it outlives a restart.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct UseAsk {
    pub(super) lead: String,
    pub(super) project: String,
    pub(super) sid: String,
    pub(super) name: String,
    pub(super) item: String,
    pub(super) to: String,
    #[serde(default)]
    pub(super) after: Option<String>,
    #[serde(default)]
    pub(super) why: String,
}

/// The answer that puts a session on the Desk, in a use ask's choices.
const USE_YES: &str = "Yes, add it to the Desk";

/// Whether `project`'s lead may route to holder `h` without asking you: a session on the Desk, the lead itself (or
/// its crew), or the one holding item `id` now.
pub(super) fn may_route(st: &Store, project: &str, h: &Holder, id: &str) -> bool {
    let to = h.to();
    st.desk.on_desk(project, to) || st.desk.lead_sid(project) == Some(to) || st.desk.item(id).and_then(|i| i.holder.as_ref()).is_some_and(|b| b.to() == to)
}

/// Session `sid` on `project`'s Desk from now (saved once the change is done).
pub(super) fn join(st: &mut Store, project: &str, sid: &str, fx: &mut Effects) {
    if st.desk.join(project, sid) {
        fx.joined.push((project.to_string(), sid.to_string()));
    }
}

/// Route `u` asked of you first: once for the same item and session, and only while the item is the lead's to route.
pub(super) fn ask_first(st: &Store, u: &UseAsk, fx: &mut Effects) {
    let open = st.items.iter().any(|c| c.status == "pending" && c.tool_input["desk_use"]["item"] == u.item.as_str() && c.tool_input["desk_use"]["sid"] == u.sid.as_str());
    if open || !st.desk.item(&u.item).is_some_and(|i| i.project == u.project && i.state != "done") {
        return;
    }
    let name = st.sessions.get(&u.sid).map(shown_name).unwrap_or_else(|| u.name.clone());
    fx.uses.push(UseAsk { name, ..u.clone() });
}

/// Session IDs with retained assignments on this Desk, including completed and reassigned items.
pub(super) fn work_history(st: &Store, project: &str) -> std::collections::BTreeSet<String> {
    st.desk.items.iter().filter(|i| i.project == project).flat_map(|i| {
        i.former_holders.iter().map(String::as_str).chain(i.holder.as_ref().map(Holder::to))
    }).filter(|sid| !sid.is_empty()).map(str::to_string).collect()
}

impl Hub {
    /// Session `sid` on `project`'s Desk (`on`) or off it, as you said (a session's menu, or Add to Desk). One added must
    /// be a live session of the project; one that has ended can still be taken off.
    pub(crate) fn desk_set_member(&self, project: &str, sid: &str, on: bool) -> Result<(), String> {
        let of = self.read(|st| st.sessions.get(sid).map(|s| s.project.clone()));
        match of {
            // One that has ended can still be taken off.
            None if !on => {}
            None => return Err("no such session".into()),
            Some(p) if p != self.read(|st| st.desk.project_of(project).to_string()) => return Err(format!("that session isn't of {project}")),
            Some(_) => {}
        }
        let removed_lead = self.desk_saving(|| -> Result<bool, String> {
            let (changed, removed_lead) = self.write(|st| {
                if project.starts_with("desk-") && !st.desk.spaces.contains_key(project) { return Err("no such Desk".to_string()); }
                if !on && work_history(st, project).contains(sid) {
                    return Err("This session has work history on this Desk.".into());
                }
                let removed_lead = !on && st.desk.lead_sid(project) == Some(sid);
                if removed_lead {
                    if st.desk.starting.contains_key(project) {
                        return Err("the Dispatcher is changing. Try again when it is ready.".into());
                    }
                    st.desk.lead.remove(project);
                }
                let changed = if on { st.desk.join(project, sid) } else { st.desk.leave(project, sid) };
                Ok((changed, removed_lead))
            })?;
            if removed_lead {
                crate::db::desk_drop_lead(project);
            }
            match (changed, on) {
                (false, _) => {}
                (true, true) => crate::db::desk_put_member(project, sid, now_ms()),
                (true, false) => crate::db::desk_drop_member(project, sid),
            }
            Ok(removed_lead)
        })?;
        if removed_lead && self.session_origin(sid).is_some() {
            let words = format!("You no longer run {}'s Desk. Keep on with the items you hold.", self.desk_name(project));
            if let Err(e) = self.send_to_session(sid, &words, &[], false) {
                eprintln!("cue: desk: {project}'s last lead wasn't told it was removed: {e}");
            }
        }
        self.changed();
        Ok(())
    }

    /// You answered route `u`'s card (`allowed`: not dismissed): yes ("Yes, add it to the Desk") puts the session on
    /// the Desk and makes the route; anything else leaves the item as it was. What the lead is told.
    pub(super) fn desk_use_answered(&self, u: &UseAsk, allowed: bool, answer: &str) -> String {
        if allowed && answer.trim().to_lowercase().starts_with("yes") {
            return self.desk_use(u);
        }
        format!("Cue asked the user before routing #{} to {}, which isn't on the Desk. The answer: {answer}. #{} stays as it was.", u.item, u.name, u.item)
    }

    /// You said yes to the lead's route of an item to a session not on the Desk: it joins, and the route is made as the
    /// lead asked. What the lead is told: the route's own answer when it couldn't be made (#248: the item done since,
    /// its finish under way, …), so it never hears a route worked that didn't.
    fn desk_use(&self, u: &UseAsk) -> String {
        let now = now_ms();
        let (fx, said) = self.write(|st| {
            let mut fx = Effects::default();
            if !st.desk.item(&u.item).is_some_and(|i| i.project == u.project && i.state != "done") {
                return (fx, Some(format!("Refused: #{} is done or gone. You can add a new ITEM for what's left.", u.item)));
            }
            join(st, &u.project, &u.sid, &mut fx);
            let name = st.desk.lead.get(&u.project).filter(|l| l.sid == u.lead).map(|l| l.name.clone()).or_else(|| st.sessions.get(&u.lead).map(shown_name)).unwrap_or_default();
            let said = route(st, &u.project, &By { name: &name, sid: &u.lead }, &u.item, &u.to, u.after.clone(), &u.why, now, &mut fx);
            (fx, said)
        });
        self.desk_done(fx);
        match said.filter(|s| !s.starts_with("Taken:")) {
            Some(no) => format!("The user said yes, but Cue couldn't route #{} to {}. {no}", u.item, u.name),
            None => format!("The user said yes: {} is on the Desk now, and Cue routed #{} to it as you asked.", u.name, u.item),
        }
    }

    /// Routes waiting on your yes, each asked as a card of its lead's (`desk_ask`, with the route kept on it).
    pub(super) fn desk_ask_first(&self, uses: Vec<UseAsk>) {
        for u in uses {
            let title = self.read(|st| st.desk.item(&u.item).map(|i| i.title.clone())).unwrap_or_default();
            let q = format!("Use {} for #{} {title}? It isn't on the Desk: yes adds it, and the item goes to it.", u.name, u.item);
            self.desk_ask(&u.lead, &u.project, q.trim_end(), &[USE_YES.to_string(), "No".to_string()], Some(&u), None);
        }
    }
}
