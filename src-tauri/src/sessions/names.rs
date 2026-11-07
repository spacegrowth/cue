//! The name you give a session in Cue, and keeping it in step with Claude Code's own list.

use super::Sessions;
use crate::model::Origin;
use std::collections::HashMap;

impl Sessions {
    /// Name a session in Cue. Returns what delivering it needs: (harness, state, where it is).
    /// You named it in Cue ("" clears the name). Shown at once; telling the agent is the caller's job,
    /// and any earlier rename still on its way is dropped.
    pub fn set_name(&mut self, session_id: &str, name: &str) -> Option<(String, String, Origin)> {
        let s = self.0.get_mut(session_id)?;
        s.name = name.to_string();
        s.rename_pending = false;
        s.rename_check = None;
        s.rename_error.clear();
        Some((s.origin.harness.clone(), s.state.clone(), s.origin.clone()))
    }

    /// Cue typed `/rename <name>` into Claude Code: its list must show `name` by `by_ms`.
    pub fn expect_rename(&mut self, session_id: &str, name: &str, by_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.rename_check = Some((name.to_string(), by_ms));
        }
    }

    /// The agent didn't get the name: Cue keeps it, and says why.
    pub fn rename_failed(&mut self, session_id: &str, why: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.rename_check = None;
            s.rename_error = why.to_string();
        }
    }

    /// × on the rename notice. True when there was one.
    pub fn dismiss_rename_error(&mut self, session_id: &str) -> bool {
        self.0.get_mut(session_id).is_some_and(|s| !std::mem::take(&mut s.rename_error).is_empty())
    }

    /// Claude Code's own list of its running sessions on this Mac (id → name), as of `now`. Settles each
    /// rename Cue typed (the name showed up: done; the time ran out: it failed, Cue keeps your name and
    /// says so), and follows a rename made in the agent itself (/rename in its terminal: the name there
    /// changed). A session Cue sees there for the first time takes the name it has there, if any (as the
    /// window always showed it).
    /// True when anything changed.
    pub fn sync_agent_names(&mut self, listed: &HashMap<String, String>, now: u64) -> bool {
        let mut changed = false;
        for s in self.0.values_mut() {
            if s.origin.harness != "claude" || !s.origin.machine.is_empty() {
                continue;
            }
            let Some(there) = listed.get(&s.origin.session_id).map(|n| n.trim().to_string()) else { continue };
            if let Some((want, by)) = s.rename_check.clone() {
                if there == want {
                    s.rename_check = None;
                    s.rename_error.clear();
                } else if now >= by {
                    s.rename_check = None;
                    s.rename_error = if there.is_empty() {
                        "Claude Code didn't take the new name".into()
                    } else {
                        format!("Claude Code didn't take the new name: it still calls it “{there}”")
                    };
                } else {
                    continue; // still on its way
                }
                s.claude_name = Some(there);
                changed = true;
                continue;
            }
            if s.rename_pending {
                continue; // typed when its turn ends; until then the old name there is expected
            }
            match &s.claude_name {
                None => {
                    if !there.is_empty() {
                        s.name = there.clone();
                    }
                    s.claude_name = Some(there);
                    changed = true;
                }
                Some(was) if *was != there => {
                    if !there.is_empty() {
                        s.name = there.clone();
                        s.rename_error.clear();
                    }
                    s.claude_name = Some(there);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    /// Remember that the agent still has to be told its new name (it was busy).
    pub fn defer_rename(&mut self, session_id: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.rename_pending = true;
        }
    }

    /// The deferred rename, if any (taken: it's delivered once).
    pub fn take_rename(&mut self, session_id: &str) -> Option<(String, Origin)> {
        let s = self.0.get_mut(session_id)?;
        std::mem::take(&mut s.rename_pending).then(|| (s.name.clone(), s.origin.clone()))
    }
}
