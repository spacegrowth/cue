//! A Desk's identity and workspace. Existing project-keyed records remain valid Desk identities.

use super::*;
use crate::desk::model::DeskSpace;

impl Hub {
    pub(crate) fn desk_name(&self, id: &str) -> String {
        self.read(|st| st.desk.name_of(id).to_string())
    }

    pub(crate) fn desk_claim_lead(&self, id: &str, sid: &str) -> Result<(), String> {
        self.write(|st| {
            let other = st.desk.lead_of(sid).or_else(|| st.desk.lead_claims.get(sid).cloned());
            if other.is_some_and(|other| other != id) {
                return Err("that session already runs or is joining another Desk as its lead".into());
            }
            st.desk.lead_claims.insert(sid.to_string(), id.to_string());
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
}
