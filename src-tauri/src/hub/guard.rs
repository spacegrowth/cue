//! Ask me before (desk/one_way.rs), checked before each tool call runs (from its command's words): a tool call that is one of the steps a project's Desk asks first
//! about (a push to main, a release, deleting files) waits on your yes. The agent's hook (hook.rs `guard`) or Pi's
//! extension sends it here before it runs; on any session of a project with a Desk lead it becomes a one-way card,
//! answered with a click. Elsewhere (no Desk on the project, or a step not ticked) it passes.
//!
//! And your answer to the question: saved, and every session on the project's Desks told the new rule.

use super::*;

impl Hub {
    /// A tool call checked before it runs. `hits`: the kinds its sender saw (empty: read here from `tool_input`, with
    /// `branch` for a plain push). Its card and the receiver its connection waits on, or None: it passes.
    /// `checked`: its sender read the kinds itself, so none (`hits` empty) means none. Before that, an item's holder
    /// leaving the copy Cue gave it is stopped, saying why (desk/hub/landing/copy_guard.rs, #483).
    pub fn guard(&self, origin: Origin, tool_name: String, tool_input: Value, hits: &[String], branch: &str, agent_id: String, checked: bool) -> Option<(String, oneshot::Receiver<Reply>)> {
        if let Some(why) = self.copy_guard(&origin, &tool_name, &tool_input) {
            return Some((format!("copy-{}", now_ms()), crate::desk::hub::away::away_holds::given("deny", Some(&why))));
        }
        if checked && hits.is_empty() {
            return None;
        }
        let (origin, kinds) = self.guarded(origin, &tool_input, hits, branch)?;
        if let Some(held) = self.held_guard(&origin, &tool_name, &tool_input, &kinds) {
            return Some(held);
        }
        let ask = NewAsk { kind: "permission".into(), tool_name, tool_input, suggestions: json!([]), context: vec![], agent_id };
        self.add_guard_ask(origin, None, ask)
    }

    /// The origin its card goes under (a session Cue hosts as it's kept here: its hook doesn't say it's hosted),
    /// and the kinds it is, when it's asked first about; None when it isn't.
    fn guarded(&self, origin: Origin, tool_input: &Value, hits: &[String], branch: &str) -> Option<(Origin, Vec<String>)> {
        let origin = self.read(|st| st.sessions.get(&origin.session_id).map(|s| s.origin.clone()).filter(|o| o.hosted)).unwrap_or(origin);
        let project = super::crew_log::project_shown_for(&origin);
        if !self.read(|st| has_desk(st, &project)) {
            return None;
        }
        let hits: Vec<&str> = if hits.is_empty() {
            let b = (!branch.is_empty()).then(|| branch.to_string());
            crate::desk::one_way::call_hits(tool_input, &|_| b.clone())
        } else {
            hits.iter().map(String::as_str).collect()
        };
        let kinds = crate::desk::one_way::kinds(&project);
        asked_about(&kinds, &hits).then(|| (origin, hits.iter().filter(|h| kinds.contains(h)).map(|h| h.to_string()).collect()))
    }

    /// The same for a hosted Pi's ask (its extension can't count on reaching Cue's socket), keyed by its request:
    /// None when it passes; Some(None) when its card is already open (the same lines read again).
    pub(super) fn hosted_guard(&self, origin: Origin, request_id: &str, tool_name: String, tool_input: Value, branch: &str) -> Option<Option<(String, oneshot::Receiver<Reply>)>> {
        let (origin, kinds) = self.guarded(origin, &tool_input, &[], branch)?;
        if let Some(held) = self.held_guard(&origin, &tool_name, &tool_input, &kinds) {
            return Some(Some(held));
        }
        let ask = NewAsk { kind: "permission".into(), tool_name, tool_input, suggestions: json!([]), context: vec![], agent_id: String::new() };
        Some(self.add_guard_ask(origin, Some(request_id), ask))
    }

    /// Your answer to Ask me before for `project`: saved (asked from then on), and when it changed, every session on
    /// the project's Desks told the new rule (it replaces what their instructions said). `kinds` None: not now (the
    /// setting kept as it is, never asked again). Returns the kinds as saved.
    pub(crate) fn ask_before_set(&self, project: &str, kinds: Option<&[String]>) -> Result<Vec<&'static str>, String> {
        let before = crate::desk::one_way::kinds(project);
        let Some(kinds) = kinds else {
            crate::desk::one_way::mark_asked(project)?;
            self.changed();
            return Ok(before);
        };
        let saved = crate::desk::one_way::answer(project, kinds)?;
        if saved != before {
            let words = crate::desk::one_way::changed_words(&saved);
            for sid in self.read(|st| desk_sessions(st, project)) {
                if let Err(e) = self.tell(&sid, &words) {
                    eprintln!("cue: couldn't tell {sid} the new Ask me before: {e}");
                }
            }
        }
        self.changed();
        Ok(saved)
    }
}

/// Whether `project` has a Desk run by a lead (any of its Desks: two on one project share the setting).
fn has_desk(st: &Store, project: &str) -> bool {
    !project.is_empty() && st.desk.lead.keys().any(|id| st.desk.project_of(id) == project)
}

/// Every live session on `project`'s Desks (each Desk's sessions, its lead among them), once each.
fn desk_sessions(st: &Store, project: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let desks: Vec<&String> = st.desk.lead.keys().filter(|id| st.desk.project_of(id) == project).collect();
    for id in desks {
        for s in crate::desk::hub::core::desk::routable(st, id) {
            if !out.contains(&s.origin.session_id) {
                out.push(s.origin.session_id.clone());
            }
        }
    }
    out
}

/// Whether any of `hits` is asked first about in a project with `kinds`: only a kind Cue checks (a push to main is a
/// push too, so a project asking before any push stops it).
fn asked_about(kinds: &[&str], hits: &[&str]) -> bool {
    crate::desk::one_way::KINDS.iter().any(|k| k.offered && kinds.contains(&k.key) && hits.contains(&k.key))
}

#[cfg(test)]
mod tests {
    use super::asked_about;

    #[test]
    fn only_what_is_ticked_is_asked_about() {
        assert!(asked_about(&["main", "release", "data"], &["main", "push"]));
        assert!(asked_about(&["push"], &["main", "push"]), "any push: main too");
        assert!(!asked_about(&["main"], &["push"]), "a crew branch pushed: not main");
        assert!(!asked_about(&["deploy", "outside"], &["data"]));
        assert!(!asked_about(&["main", "release"], &["data"]), "deleting not ticked");
    }
}
