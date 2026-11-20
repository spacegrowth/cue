//! The lead changing how work lands (#381), when you ask it in plain words: its `LANDS: <part> -> <choice>` line
//! (desk/parse.rs) is asked of you with the change in plain words ("Change how work lands on cue: check → none?"). Only
//! your yes saves it (lands.rs save), as the settings screen would; a no leaves it. The lead never saves it itself,
//! and can't set a push to main to stop asking (lands.rs propose).

use super::Hub;
use super::desk_view::desk_folder;
use serde_json::{json, Value};

/// A lead's proposed change: its session, its Desk, the part, the choice, your own steps (for "command") and why.
pub(super) struct Proposal {
    pub(super) lsid: String,
    pub(super) desk: String,
    pub(super) part: String,
    pub(super) to: String,
    pub(super) steps: Vec<String>,
    pub(super) why: String,
}

impl Hub {
    /// Proposal `p` asked of you (a Yes / No card from the lead), or the lead told why it can't be.
    pub(super) fn desk_lands_ask(&self, p: Proposal) {
        let (project, folder) = self.read(|st| (st.desk.project_of(&p.desk).to_string(), desk_folder(st, &p.desk)));
        let proposed = crate::lands::propose(&crate::config::load(), &project, crate::lands::place_of(&folder), &p.part, &p.to, &p.steps);
        let (set, words) = match proposed {
            Ok(x) => x,
            Err(why) => return self.desk_lands_tell(&p.lsid, format!("Not asked: LANDS: {} -> {}: {why}. Nothing changed.", p.part, p.to)),
        };
        let question = format!("Change how work lands on {project}: {words}?");
        let Some(id) = self.desk_ask(&p.lsid, &p.desk, &question, &["Yes".into(), "No".into()], None, None) else { return };
        let lands = json!({ "project": project, "folder": folder, "set": set, "words": words, "why": p.why });
        self.write(|st| {
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.tool_input["desk_lands"] = lands;
            }
        });
    }

    /// Your answer to a proposed change (`lands`: its card's `desk_lands`), the lead's session `lsid`: `yes` saves it,
    /// the setting shown with it from now; no leaves it. Either way the lead is told.
    pub(super) fn desk_lands_answered(&self, lsid: &str, lands: &Value, yes: bool) {
        let project = lands["project"].as_str().unwrap_or_default();
        let words = lands["words"].as_str().unwrap_or_default();
        let said = if !yes {
            format!("The user said no to changing how work lands on {project} ({words}). Nothing changed.")
        } else {
            let place = crate::lands::place_of(lands["folder"].as_str().unwrap_or_default());
            match crate::lands::save(project, place, &lands["set"]) {
                Ok(_) => {
                    self.changed();
                    format!("The user said yes: how work lands on {project} is changed ({words}). It shows on the settings screen.")
                }
                Err(e) => format!("The user said yes, but how work lands on {project} couldn't be changed ({words}): {e}"),
            }
        };
        self.desk_lands_tell(lsid, said);
    }

    /// How work lands on Desk `desk`'s project, as its lead reads it (desk_lead.rs desk_summary).
    pub(super) fn desk_lands_line(&self, desk: &str) -> String {
        let (project, folder) = self.read(|st| (st.desk.project_of(desk).to_string(), desk_folder(st, desk)));
        crate::lands::lead_line(&crate::lands::lands_of(&project, &folder))
    }

    fn desk_lands_tell(&self, lsid: &str, words: String) {
        let sid = lsid.to_string();
        self.later(0, move |h| {
            if let Err(e) = h.send_to_session(&sid, &words, &[], false) {
                eprintln!("cue: desk: couldn't tell the lead about how work lands: {e}");
            }
        });
    }
}
