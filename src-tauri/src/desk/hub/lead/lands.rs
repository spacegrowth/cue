//! The lead changing how work lands (#381), when you ask it in plain words: its `LANDS: <part> -> <choice>` line
//! (desk/parse.rs) is asked of you with the change in plain words ("Change how work lands on cue: check → none?"). Only
//! your yes saves it (desk/lands.rs save), as the settings screen would; a no leaves it. The lead never saves it itself,
//! and can't set a push to main to stop asking (desk/lands.rs propose).

use crate::hub::Hub;
use crate::desk::hub::core::view::desk_folder;
use serde_json::{json, Value};

/// A lead's proposed change: its session, its Desk, the part, the choice, your own steps (for "command") and why.
pub(crate) struct Proposal {
    pub(crate) lsid: String,
    pub(crate) desk: String,
    pub(crate) part: String,
    pub(crate) to: String,
    pub(crate) steps: Vec<String>,
    pub(crate) why: String,
}

impl Hub {
    /// Proposal `p` asked of you (a Yes / No card from the lead), or the lead told why it can't be.
    pub(crate) fn desk_lands_ask(&self, p: Proposal) {
        // Kept per Desk, as its items land by it (#460); named as the Desk is.
        let (project, name, folder) = self.read(|st| (p.desk.clone(), st.desk.name_of(&p.desk).to_string(), desk_folder(st, &p.desk)));
        let proposed = crate::desk::lands::propose(&crate::config::load(), &project, crate::desk::lands::place_of(&folder), &p.part, &p.to, &p.steps);
        let (set, words) = match proposed {
            Ok(x) => x,
            Err(why) => return self.desk_lands_tell(&p.lsid, format!("Not asked: LANDS: {} -> {}: {why}. Nothing changed.", p.part, p.to)),
        };
        let question = format!("Change how work lands on {name}: {words}?");
        let Some(id) = self.desk_ask(&p.lsid, &p.desk, &question, &["Yes".into(), "No".into()], None, None) else { return };
        let lands = json!({ "project": project, "name": name, "folder": folder, "set": set, "words": words, "why": p.why });
        self.write(|st| {
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.tool_input["desk_lands"] = lands;
            }
        });
    }

    /// Your answer to a proposed change (`lands`: its card's `desk_lands`), the lead's session `lsid`: `yes` saves it,
    /// the setting shown with it from now; no leaves it. Either way the lead is told.
    pub(crate) fn desk_lands_answered(&self, lsid: &str, lands: &Value, yes: bool) {
        let project = lands["project"].as_str().unwrap_or_default();
        let name = lands["name"].as_str().unwrap_or(project);
        let words = lands["words"].as_str().unwrap_or_default();
        let said = if !yes {
            format!("The user said no to changing how work lands on {name} ({words}). Nothing changed.")
        } else {
            let place = crate::desk::lands::place_of(lands["folder"].as_str().unwrap_or_default());
            match crate::desk::lands::save(project, place, &lands["set"]) {
                Ok(_) => {
                    self.changed();
                    format!("The user said yes: how work lands on {name} is changed ({words}). It shows on the settings screen.")
                }
                Err(e) => format!("The user said yes, but how work lands on {name} couldn't be changed ({words}): {e}"),
            }
        };
        self.desk_lands_tell(lsid, said);
    }

    /// How work lands on Desk `desk`, as its lead reads it (desk/hub/lead/run.rs desk_summary).
    pub(crate) fn desk_lands_line(&self, desk: &str) -> String {
        let folder = self.read(|st| desk_folder(st, desk));
        let mut line = crate::desk::lands::lead_line(&crate::desk::lands::lands_of(desk, &folder));
        // Where new sessions start, and the machines this Desk has used (#597).
        let p = crate::desk::machine_pref::of(desk);
        let used: Vec<String> = self.read(|st| {
            let mut all: Vec<String> = p.folders.keys().cloned().collect();
            all.extend(st.desk.spaces.get(desk).map(|d| d.machine_folders.keys().cloned().collect::<Vec<_>>()).unwrap_or_default());
            all.sort();
            all.dedup();
            all.iter().filter(|m| crate::machines::get(m).is_some()).map(|m| crate::desk::machine_pref::of(desk).folder(m).map_or_else(|| crate::machines::display_name(m), |f| format!("{} ({f})", crate::machines::display_name(m)))).collect()
        });
        let spread = p.spread();
        if spread.len() > 1 {
            // Several ticked (#661): Cue spreads new sessions over them.
            let names: Vec<String> = spread.iter().map(|m| if m.is_empty() { "this Mac".to_string() } else { crate::machines::display_name(m) }).collect();
            line.push_str(&format!("New sessions start on whichever of {} has the fewest of this Desk's items at work (the user's setting): a ROUTE to new goes there unless you say on: <machine>.\n", names.join(", ")));
        } else if !p.machine.is_empty() && crate::machines::get(&p.machine).is_some() {
            line.push_str(&format!("New sessions start on {} (the user's setting): a ROUTE to new goes there unless you say on: this Mac.\n", crate::desk::machine_pref::of(desk).folder(&p.machine).map_or_else(|| crate::machines::display_name(&p.machine), |f| format!("{} in {f}", crate::machines::display_name(&p.machine)))));
        }
        if !used.is_empty() {
            line.push_str(&format!("Machines this Desk has used: {}.\n", used.join(", ")));
        }
        // How many at once on each (#711): a new session for a machine that has that many of the Desk's items at work
        // waits for room there, or goes to another place ticked.
        let mut caps: Vec<String> = spread.iter().filter(|m| !m.is_empty() && crate::machines::get(m).is_some()).map(|m| format!("{} {}", crate::machines::display_name(m), p.at_once(m))).collect();
        // This Mac's is its Items at once (its folders for items of their own), when another machine is ticked too.
        let mac = crate::desk::lands::lands_known(desk, &folder);
        if !caps.is_empty() && spread.iter().any(String::is_empty) && mac.at == "copy" {
            caps.insert(0, format!("this Mac {}", mac.at_once));
        }
        caps.dedup();
        if !caps.is_empty() {
            line.push_str(&format!("Most of this Desk's items at work at once (the user's setting): {}. A new session for a machine at its number waits for room there, or starts on another place ticked; it starts by itself.\n", caps.join(", ")));
        }
        line
    }

    fn desk_lands_tell(&self, lsid: &str, words: String) {
        let sid = lsid.to_string();
        self.later(0, move |h| {
            if let Err(e) = h.tell(&sid, &words, crate::sessions::Kind::Answer) {
                eprintln!("cue: desk: couldn't tell the lead about how work lands: {e}");
            }
        });
    }
}
