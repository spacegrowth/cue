//! The Desk's one list of waits (#250): who waits on whom, on what, since when. Today it holds an item queued after
//! another. Each time it changes Cue walks it for
//! a circle (items each waiting on the next, none able to move): one is kept in `st.desk.run.circles`, shown on its
//! items, and its project's lead is told once, with the links it may cut. A cut link: a queued item starts.

use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;
use serde_json::{json, Value};

/// One wait in the list: item `item` waits on `on` (an item; "" when no item of the Desk is behind it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Wait {
    pub(crate) item: String,
    /// "queued" (after item `on`)
    pub(crate) kind: &'static str,
    pub(crate) on: String,
    /// The session behind it ("": someone else, Cue doesn't know who), its id, the file (absolute) and its git path.
    pub(crate) by: String,
    pub(crate) sid: String,
    pub(crate) path: String,
    pub(crate) name: String,
    /// "editing" | "changed" for a commit wait.
    pub(crate) how: String,
    /// Since when (ms).
    pub(crate) since: u64,
}

impl Wait {
    /// What the link is, as a circle's row says it: "queued after it".
    pub(crate) fn why(&self) -> String {
        match self.kind {
            "queued" => "queued after it".into(),
            _ => "waits on it".into(),
        }
    }
}

/// Every wait on the Desk now, `project`'s alone when it's given.
pub(crate) fn waits(st: &Store, project: Option<&str>) -> Vec<Wait> {
    let mut out: Vec<Wait> = Vec::new();
    let open = |i: &&DeskItem| i.state != "done" && project.is_none_or(|p| i.project == p);
    for it in st.desk.items.iter().filter(open) {
        if let Some(a) = it.after.as_ref().filter(|_| it.state == "queued") {
            out.push(Wait { item: it.id.clone(), kind: "queued", on: a.clone(), by: String::new(), sid: String::new(), path: String::new(), name: String::new(), how: String::new(), since: it.updated * 1000 });
        }
    }
    out
}

/// The circles in `waits`: each as its links in order (each link's item waits on the next's), starting from its
/// smallest item, so a circle is found once and its key (`circle_key`) stays the same.
pub(crate) fn circles(waits: &[Wait]) -> Vec<Vec<Wait>> {
    let mut nodes: Vec<&str> = waits.iter().filter(|w| !w.on.is_empty()).map(|w| w.item.as_str()).collect();
    nodes.sort();
    nodes.dedup();
    let mut out: Vec<Vec<Wait>> = Vec::new();
    for start in &nodes {
        let mut path: Vec<&Wait> = Vec::new();
        walk(waits, start, start, &mut path, &mut out);
    }
    out
}

/// Links on from the end of `path` (from `start`, only through items after it, so each circle is found from its
/// smallest item); a link back to `start` closes a circle.
fn walk<'a>(waits: &'a [Wait], start: &str, at: &str, path: &mut Vec<&'a Wait>, out: &mut Vec<Vec<Wait>>) {
    let mut seen: Vec<&str> = Vec::new();
    for w in waits.iter().filter(|w| w.item == at && !w.on.is_empty()) {
        // Two files on the same session: one link.
        if seen.contains(&w.on.as_str()) {
            continue;
        }
        seen.push(&w.on);
        if w.on == start {
            let mut c: Vec<Wait> = path.iter().map(|w| (*w).clone()).collect();
            c.push(w.clone());
            if !out.iter().any(|o| circle_key(o) == circle_key(&c)) {
                out.push(c);
            }
        } else if w.on.as_str() > start && !path.iter().any(|p| p.item == w.on) {
            path.push(w);
            walk(waits, start, &w.on, path, out);
            path.pop();
        }
    }
}

/// A circle as one key: its items in order ("231>228>233").
pub(crate) fn circle_key(c: &[Wait]) -> String {
    c.iter().map(|w| w.item.as_str()).collect::<Vec<_>>().join(">")
}

/// "#231 waits on #228 waits on #233 waits on #231".
pub(crate) fn circle_words(c: &[Wait]) -> String {
    let mut s = c.iter().map(|w| format!("#{}", w.item)).collect::<Vec<_>>().join(" waits on ");
    if let Some(first) = c.first() {
        s.push_str(&format!(" waits on #{}", first.item));
    }
    s
}

/// What `project`'s lead is told of circle `c`, found just now.
fn circle_note(c: &[Wait]) -> String {
    let links: Vec<String> = c.iter().map(|w| format!("- #{} waits on #{}: {}", w.item, w.on, w.why())).collect();
    let (a, b) = c.first().map(|w| (w.item.clone(), w.on.clone())).unwrap_or_default();
    format!(
        "Circle on the Desk: {}. None of them can move until one link is cut.\n{}\n\
         Cut the link that costs least: CUT: #{a} -> #{b} | why: <reason> (a queued item then starts now). \
         Only if the user must choose, ASK.",
        circle_words(c),
        links.join("\n"),
    )
}

/// The circles in item `id`'s project now, with when each was found and whether its lead was told.
pub(crate) fn circles_of(st: &Store, project: &str) -> Vec<(Vec<Wait>, u64, bool)> {
    // Every circle with an item of `project`'s in it: one may run through another Desk's items on the same code (#540).
    let ours = |w: &Wait| st.desk.item(&w.item).is_some_and(|i| i.project == project);
    circles(&waits(st, None)).into_iter().filter(|c| c.iter().any(ours)).map(|c| {
        let (at, told) = st.desk.run.circles.get(&circle_key(&c)).copied().unwrap_or((0, false));
        (c, at, told)
    }).collect()
}

/// `project`'s waits, for the window: who waits on whom, on what, since when.
pub(crate) fn waits_view(st: &Store, project: &str) -> Value {
    // Its own items' waits, and other Desks' items waiting on one of its own (#540); each names the other Desk.
    let of = |id: &str| st.desk.item(id).map(|i| i.project.clone()).unwrap_or_default();
    Value::Array(waits(st, None).iter().filter(|w| of(&w.item) == project || of(&w.on) == project).map(|w| {
        let mut j = wait_json(w);
        let other = |id: &str| Some(of(id)).filter(|p| !p.is_empty() && p != project).map(|p| st.desk.name_of(&p).to_string()).unwrap_or_default();
        j["item_desk"] = json!(other(&w.item));
        j["on_desk"] = json!(other(&w.on));
        j
    }).collect())
}

/// `project`'s circles, for the window: each with its links in order, when it was found and whether its lead was told.
pub(crate) fn circles_view(st: &Store, project: &str) -> Value {
    Value::Array(circles_of(st, project).into_iter().map(|(c, at, told)| {
        json!({ "key": circle_key(&c), "items": c.iter().map(|w| w.item.clone()).collect::<Vec<_>>(), "words": circle_words(&c), "links": c.iter().map(wait_json).collect::<Vec<_>>(), "at": at, "told": told })
    }).collect())
}

fn wait_json(w: &Wait) -> Value {
    json!({ "item": w.item, "kind": w.kind, "on": w.on, "by": w.by, "sid": w.sid, "path": w.path, "name": w.name, "how": w.how, "since": w.since, "why": w.why() })
}

impl Hub {
    /// The waits list walked for circles: one new is kept and its lead told at once (when it has one at hand), one
    /// gone is let go. Run as a commit wait is set and on each finishing tick.
    pub(crate) fn circle_check(&self) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            let all = circles(&waits(st, None));
            let keys: Vec<String> = all.iter().map(|c| circle_key(c)).collect();
            st.desk.run.circles.retain(|k, _| keys.contains(k));
            for c in &all {
                let key = circle_key(c);
                let told = st.desk.run.circles.get(&key).is_some_and(|(_, t)| *t);
                if told {
                    continue;
                }
                // Each Desk with an item in it (#540: a circle may run across Desks on the same code): its lead told.
                let mut projects: Vec<String> = c.iter().filter_map(|w| st.desk.item(&w.item)).map(|i| i.project.clone()).collect();
                projects.sort();
                projects.dedup();
                let mut told = false;
                for project in &projects {
                    let lead = st.desk.lead_sid(project).filter(|l| st.sessions.get(*l).is_some_and(|s| s.state != "stopped")).map(str::to_string);
                    if let Some(lead) = lead {
                        fx.sends.push(crate::desk::hub::core::desk::kinded((lead, circle_note(c)), crate::sessions::Kind::Question));
                        told = true;
                    }
                }
                let at = st.desk.run.circles.get(&key).map_or(now, |(at, _)| *at);
                st.desk.run.circles.insert(key, (at, told));
            }
            fx
        });
        self.desk_done(fx);
    }

    /// Item `id` no longer waits on item `on`, as you or its lead (`by`) cut that link: queued after it, it starts
    /// now; its commit waiting on `on`'s session's files, it lands without them. Err when it doesn't wait on `on`.
    pub(crate) fn desk_cut(&self, id: &str, on: &str) -> Result<(), String> {
        let now = now_ms();
        let fx = self.write(|st| cut_in(st, id, on, now))?;
        self.desk_done(fx);
        self.circle_check();
        Ok(())
    }
}

/// `cut` under the store's lock: what to do with it let go.
pub(crate) fn cut_in(st: &mut Store, id: &str, on: &str, now: u64) -> Result<crate::desk::hub::core::desk::Effects, String> {
    let mut fx = crate::desk::hub::core::desk::Effects::default();
    let id = st.desk.canon(id);
    let on = st.desk.canon(on);
    let links: Vec<Wait> = waits(st, None).into_iter().filter(|w| w.item == id && w.on == on).collect();
    if links.is_empty() {
        return Err(format!("#{id} doesn't wait on #{on}"));
    }
    let rule = st.desk.item(&id).map(|i| crate::desk::hub::core::desk::rule_of(st, &i.project)).unwrap_or_default();
    if let Some(it) = st.desk.item_mut(&id) {
        if links.iter().any(|w| w.kind == "queued") {
            it.after = None;
            it.updated = crate::desk::hub::core::desk::secs(now);
            if let Some(h) = &it.holder {
                it.state = "working".into();
                fx.sends.push(crate::desk::hub::core::desk::kinded((h.to().to_string(), crate::desk::handed_words(it, "", &rule)), crate::sessions::Kind::Brief));
            } else if it.next_for.is_some() {
                // Next for a session (#576): it waits for that one from now.
                it.queued_at = now;
            } else {
                it.state = "new".into();
                fx.news.push((it.project.clone(), id.clone()));
            }
        }
        fx.items.push(id.clone());
    }
    Ok(fx)
}

#[cfg(test)]
impl Hub {
    /// Item `id` queued after item `on` with no check that `on` doesn't already wait on it, as a Desk saved before
    /// that check could load: the one way two items' queue links make a circle now.
    pub(crate) fn desk_queue_after_unchecked(&self, id: &str, on: &str) {
        self.write(|st| {
            if let Some(it) = st.desk.item_mut(id) {
                (it.after, it.state) = (Some(on.to_string()), "queued".into());
            }
        });
    }

    /// Item `id` put in `state` as it is, nothing else changed: to set up an item in any state at once.
    pub(crate) fn desk_put_in_state(&self, id: &str, state: &str) {
        self.write(|st| {
            if let Some(it) = st.desk.item_mut(id) {
                it.state = state.into();
            }
        });
    }
}
