//! The Desk's one list of waits (#250): who waits on whom, on what, since when. Today it holds an item queued after
//! another and a finishing item whose commit waits on another session's file. Each time it changes Cue walks it for
//! a circle (items each waiting on the next, none able to move): one is kept in `st.desk.circles`, shown on its
//! items, and its project's lead is told once, with the links it may cut. A cut link: a queued item starts, or a
//! commit lands without the files it waited on (they stay in the folder, uncommitted, for their session).

use super::*;
use crate::desk::DeskItem;
use serde_json::{json, Value};

/// One wait in the list: item `item` waits on `on` (an item; "" when no item of the Desk is behind it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Wait {
    pub(crate) item: String,
    /// "queued" (after item `on`) | "commit" (its commit waits on a file `by` is editing or changed too)
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
    /// What the link is, as a circle's row says it: "queued after it", "needs its file committed".
    pub(crate) fn why(&self) -> String {
        match self.kind {
            "queued" => "queued after it".into(),
            _ => format!("needs {} committed", if self.name.is_empty() { "its file".to_string() } else { self.name.clone() }),
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
        let Some(f) = it.finish.as_ref().filter(|f| it.state == "finishing" && f.step == "commit_wait") else { continue };
        for w in &f.files_wait {
            let on = if w.sid.is_empty() { String::new() } else { held_by(st, &w.sid, &it.id) };
            out.push(Wait { item: it.id.clone(), kind: "commit", on, by: w.by.clone(), sid: w.sid.clone(), path: w.path.clone(), name: w.name.clone(), how: w.how.clone(), since: f.wait_from });
        }
    }
    out
}

/// The item session `sid` has at work (not `not`, nor one done): the one it's working on first.
fn held_by(st: &Store, sid: &str, not: &str) -> String {
    let held: Vec<&DeskItem> = st.desk.items.iter().filter(|i| i.id != not && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))).collect();
    held.iter().find(|i| i.state == "working").or(held.first()).map(|i| i.id.clone()).unwrap_or_default()
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
         Cut the link that costs least: CUT: #{a} -> #{b} | why: <reason> (a queued item then starts now; a commit \
         lands without the file it waited on, left in the folder for its session). Only if the user must choose, ASK.",
        circle_words(c),
        links.join("\n"),
    )
}

/// The circles in item `id`'s project now, with when each was found and whether its lead was told.
pub(crate) fn circles_of(st: &Store, project: &str) -> Vec<(Vec<Wait>, u64, bool)> {
    circles(&waits(st, Some(project))).into_iter().map(|c| {
        let (at, told) = st.desk.circles.get(&circle_key(&c)).copied().unwrap_or((0, false));
        (c, at, told)
    }).collect()
}

/// `project`'s waits, for the window: who waits on whom, on what, since when.
pub(super) fn waits_view(st: &Store, project: &str) -> Value {
    Value::Array(waits(st, Some(project)).iter().map(wait_json).collect())
}

/// `project`'s circles, for the window: each with its links in order, when it was found and whether its lead was told.
pub(super) fn circles_view(st: &Store, project: &str) -> Value {
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
    pub(super) fn circle_check(&self) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            let all = circles(&waits(st, None));
            let keys: Vec<String> = all.iter().map(|c| circle_key(c)).collect();
            st.desk.circles.retain(|k, _| keys.contains(k));
            for c in &all {
                let key = circle_key(c);
                let told = st.desk.circles.get(&key).is_some_and(|(_, t)| *t);
                if told {
                    continue;
                }
                let project = c.first().and_then(|w| st.desk.item(&w.item)).map(|i| i.project.clone()).unwrap_or_default();
                let lead = st.desk.lead_sid(&project).filter(|l| st.sessions.get(*l).is_some_and(|s| s.state != "stopped")).map(str::to_string);
                if let Some(lead) = &lead {
                    fx.sends.push((lead.clone(), circle_note(c)));
                }
                let at = st.desk.circles.get(&key).map_or(now, |(at, _)| *at);
                st.desk.circles.insert(key, (at, lead.is_some()));
            }
            fx
        });
        self.desk_done(fx);
    }

    /// Item `id` no longer waits on item `on`, as you or its lead (`by`) cut that link: queued after it, it starts
    /// now; its commit waiting on `on`'s session's files, it lands without them. Err when it doesn't wait on `on`.
    pub(crate) fn desk_cut(&self, id: &str, on: &str) -> Result<(), String> {
        let now = now_ms();
        let (fx, commit) = self.write(|st| cut_in(st, id, on, now))?;
        self.desk_done(fx);
        if commit {
            self.desk_finish_commit(id);
        }
        self.circle_check();
        Ok(())
    }

    /// Item `id`'s commit lands without file `path` (absolute): left in its folder, uncommitted, for whoever is
    /// changing it too.
    pub(crate) fn desk_land_without(&self, id: &str, path: &str) -> Result<(), String> {
        let ok = self.write(|st| {
            let Some(f) = st.desk.item_mut(id).filter(|i| i.state == "finishing").and_then(|i| i.finish.as_mut()).filter(|f| f.step == "commit_wait") else { return false };
            if !f.files_wait.iter().any(|w| w.path == path) {
                return false;
            }
            without(f, &[path.to_string()]);
            true
        });
        if !ok {
            return Err("Its commit doesn't wait on that file".into());
        }
        self.desk_saved(id);
        self.desk_finish_commit(id);
        self.circle_check();
        Ok(())
    }

    /// The session behind item `id`'s commit wait on `path` asked to commit its own change to it (once).
    pub(crate) fn desk_ask_commit(&self, id: &str, path: &str) -> Result<(), String> {
        let asked = self.write(|st| {
            let it = st.desk.item_mut(id).filter(|i| i.state == "finishing")?;
            let (iid, title) = (it.id.clone(), it.title.clone());
            let f = it.finish.as_mut().filter(|f| f.step == "commit_wait")?;
            let w = f.files_wait.iter().find(|w| w.path == path && !w.sid.is_empty())?.clone();
            if !f.asked.contains(&w.sid) {
                f.asked.push(w.sid.clone());
            }
            Some((w.sid, format!(
                "Please commit your own change to {name} when you can (only your part of it, nothing else). Desk item #{iid} \
                 ({title}) is done and waits to land, and its commit waits on your change to that file.",
                name = w.name,
            )))
        });
        let Some((sid, words)) = asked else { return Err("Cue doesn't know whose change that is".into()) };
        self.desk_saved(id);
        let fx = super::desk::Effects { sends: vec![(sid, words)], ..Default::default() };
        self.desk_done(fx);
        Ok(())
    }

    /// Item `id` saved as it is now, and the window told.
    fn desk_saved(&self, id: &str) {
        let it = self.desk_saving(|| self.read(|st| st.desk.item(id).cloned()).inspect(crate::db::desk_put_item));
        if it.is_some() {
            self.changed();
        }
    }
}

/// `paths` left out of `f`'s commit: no longer waited on.
fn without(f: &mut crate::desk::Finish, paths: &[String]) {
    for p in paths {
        if !f.without.contains(p) {
            f.without.push(p.clone());
        }
    }
    f.files_wait.retain(|w| !paths.contains(&w.path));
}

/// `cut` under the store's lock: what to do with it let go, and whether the item's commit is to be tried again.
pub(super) fn cut_in(st: &mut Store, id: &str, on: &str, now: u64) -> Result<(super::desk::Effects, bool), String> {
    let mut fx = super::desk::Effects::default();
    let id = st.desk.canon(id);
    let on = st.desk.canon(on);
    let links: Vec<Wait> = waits(st, None).into_iter().filter(|w| w.item == id && w.on == on).collect();
    if links.is_empty() {
        return Err(format!("#{id} doesn't wait on #{on}"));
    }
    let rule = st.desk.item(&id).map(|i| super::desk::rule_of(st, &i.project)).unwrap_or_default();
    let mut commit = false;
    if let Some(it) = st.desk.item_mut(&id) {
        if links.iter().any(|w| w.kind == "queued") {
            it.after = None;
            it.updated = super::desk::secs(now);
            if let Some(h) = &it.holder {
                it.state = "working".into();
                fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, "", &rule)));
            } else {
                it.state = "new".into();
                fx.news.push((it.project.clone(), id.clone()));
            }
        }
        let paths: Vec<String> = links.iter().filter(|w| w.kind == "commit").map(|w| w.path.clone()).collect();
        if let Some(f) = it.finish.as_mut().filter(|_| !paths.is_empty()) {
            without(f, &paths);
            commit = true;
        }
        fx.items.push(id.clone());
    }
    Ok((fx, commit))
}
