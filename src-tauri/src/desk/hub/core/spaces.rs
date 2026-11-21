//! A Desk's identity and workspace. Existing project-keyed records remain valid Desk identities.

use crate::desk::hub::prelude::*;
use crate::desk::model::DeskSpace;

impl Hub {
    pub(crate) fn desk_name(&self, id: &str) -> String {
        self.read(|st| st.desk.name_of(id).to_string())
    }

    pub(crate) fn desk_claim_lead(&self, id: &str, sid: &str) -> Result<(), String> {
        self.write(|st| {
            let other = st.desk.lead_of(sid).or_else(|| st.desk.run.lead_claims.get(sid).cloned());
            if other.is_some_and(|other| other != id) {
                return Err("that session already runs or is joining another Desk as its lead".into());
            }
            st.desk.run.lead_claims.insert(sid.to_string(), id.to_string());
            Ok(())
        })
    }

    pub(crate) fn desk_put_space(&self, space: DeskSpace) -> Result<(), String> {
        self.desk_saving(|| -> Result<(), String> {
            crate::db::desk_put_space(&space)?;
            self.write(|st| { st.desk.spaces.insert(space.id.clone(), space); });
            Ok(())
        })?;
        self.changed();
        Ok(())
    }

    pub(crate) fn desk_rename(&self, id: &str, name: &str) -> Result<(), String> {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() || name.chars().count() > 80 {
            return Err("a Desk needs a name of 1–80 characters".into());
        }
        self.desk_saving(|| -> Result<(), String> {
            let mut space = self.read(|st| st.desk.spaces.get(id).cloned()).ok_or("no such Desk")?;
            space.name = name;
            crate::db::desk_put_space(&space)?;
            self.write(|st| { st.desk.spaces.insert(id.to_string(), space); });
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    /// Delete a Desk; its sessions and files remain untouched.
    pub(crate) fn desk_delete(&self, id: &str) -> Result<(), String> {
        self.desk_delete_selected(id, &[]).map(|_| ())
    }

    /// Atomically select only this Desk's unshared sessions while removing its records.
    pub(crate) fn desk_delete_selected(&self, id: &str, selected: &[String]) -> Result<Vec<String>, String> {
        let eligible = self.desk_saving(|| -> Result<Vec<String>, String> {
            let eligible = self.write(|st| {
                if !st.desk.spaces.contains_key(id) && !st.desk.lead.contains_key(id) && !st.desk.items.iter().any(|i| i.project == id) && !st.desk.members.contains_key(id) { return Err("no such Desk".to_string()); }
                if st.desk.run.starting.contains_key(id) || st.desk.items.iter().any(|i| i.project == id && st.desk.run.finish_claims.contains(&i.id)) {
                    return Err("The Desk is starting or finishing work. Try again when it finishes.".to_string());
                }
                let belongs = |desk: &str, sid: &str| st.desk.lead.get(desk).is_some_and(|l| l.sid == sid)
                    || st.desk.members.get(desk).is_some_and(|m| m.iter().any(|s| s == sid))
                    || st.desk.items.iter().any(|i| i.project == desk && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid)));
                Ok(selected.iter().filter(|sid| belongs(id, sid) && !st.desk.spaces.keys().chain(st.desk.lead.keys()).chain(st.desk.members.keys()).chain(st.desk.items.iter().map(|i| &i.project)).any(|other| other != id && belongs(other, sid))).cloned().collect::<Vec<String>>())
            })?;
            // The database is written off the store's lock; the Desk's saves stay held throughout, so no other
            // save lands in between.
            crate::db::desk_delete(id)?;
            self.write(|st| {
                let ids: Vec<String> = st.desk.items.iter().filter(|i| i.project == id).map(|i| i.id.clone()).collect();
                st.desk.spaces.remove(id);
                st.desk.items.retain(|i| i.project != id);
                st.desk.decisions.retain(|d| d.project != id);
                st.desk.lead.remove(id);
                st.desk.members.remove(id);
                st.desk.feed.remove(id);
                st.desk.holds.retain(|h| h.desk != id);
                st.desk.bands.retain(|b| b.from != id && b.to != id);
                st.desk.run.lead_claims.retain(|_, desk| desk != id);
                for item in ids {
                    st.desk.run.done_told.remove(&item);
                    st.desk.run.nudged.remove(&item);
                    st.desk.run.left_said.remove(&item);
                    st.desk.run.sent_at.remove(&item);
                }
            });
            Ok(eligible)
        })?;
        self.changed();
        Ok(eligible)
    }

    pub(crate) fn desk_session_shared(&self, sid: &str) -> bool {
        self.read(|st| st.desk.lead.values().any(|l| l.sid == sid) || st.desk.members.values().any(|m| m.iter().any(|s| s == sid)) || st.desk.items.iter().any(|i| i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))))
    }

}
