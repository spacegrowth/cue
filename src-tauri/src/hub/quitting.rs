//! What quitting Cue would end. Sessions cue-host runs go on without Cue; any Cue runs as its own children (the host
//! couldn't be reached when they started) end with it, a turn under way cut off. So ⌘Q and an update ask first when
//! there are any, naming them, those mid-turn first (startup/window.rs, the window's quit dialog).

use super::*;

/// A session that ends if Cue quits.
#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct EndsWithCue {
    pub session_id: String,
    pub name: String,
    /// Working or asking you something: quitting cuts that turn off.
    pub mid_turn: bool,
}

impl Hub {
    /// The sessions that end if Cue quits now, those mid-turn first. None: quitting can't cut anything off.
    pub fn ends_with_cue(&self) -> Vec<EndsWithCue> {
        let ids = self.hosted().in_cue();
        let mut list: Vec<EndsWithCue> = ids
            .into_iter()
            .map(|sid| {
                let (state, cwd) = self.read(|st| st.sessions.all().into_iter().find(|s| s.origin.session_id == sid).map(|s| (s.state, s.origin.cwd))).unwrap_or_default();
                let name = Some(self.session_name(&sid)).filter(|n| !n.is_empty()).unwrap_or_else(|| crate::model::folder_of(&cwd));
                EndsWithCue { name, mid_turn: matches!(state.as_str(), "working" | "deciding"), session_id: sid }
            })
            .collect();
        list.sort_by(|a, b| b.mid_turn.cmp(&a.mid_turn).then_with(|| a.name.cmp(&b.name)));
        list
    }
}
