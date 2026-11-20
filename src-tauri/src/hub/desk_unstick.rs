//! The Desk lead unsticking its Desk itself (#347): lines that close an item whose work is already on main (CLOSE),
//! take back a copy from a stuck or finished item (TAKE COPY), move an item up in the landing line (LAND FIRST) and
//! start a waiting item now (START), and pause an item at work and resume it later (PAUSE, RESUME, #365). Each is one of the item's Steps, shown on its Thread; what can't be done is told
//! to the lead with why.

use super::desk::{at_work, decide, release, rule_of, secs, By, Effects, NewStart};
use super::desk_tree_files::{remove, rev, snapshot, tree_repo};
use super::*;

/// An item the lead said is on main already (CLOSE): closed once its commit is found on the main line of one of
/// `folders` (its repository, its holder's folder, the lead's).
pub(super) struct OnMain {
    pub(super) project: String,
    pub(super) id: String,
    /// As the lead said it, or as the item has it; never empty.
    pub(super) commit: String,
    pub(super) folders: Vec<String>,
    pub(super) by: String,
    pub(super) by_sid: String,
    pub(super) why: String,
}

/// The lead's `what` ("close" | "take_copy" | "land_first" | "start") on item `id` of `project`. Err: what the lead
/// is told when it can't be done, and what it can do instead.
#[allow(clippy::too_many_arguments)]
pub(super) fn unstick(st: &mut Store, project: &str, by: &By, what: &str, id: &str, commit: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let line = match what {
        "close" => "not closed",
        "take_copy" => "copy not taken back",
        "land_first" => "not moved up",
        "pause" => "not paused",
        "resume" => "not resumed",
        "hold" => "not parked",
        _ => "not started",
    };
    let it = st.desk.item(id).filter(|i| i.project == project).ok_or_else(|| format!("#{id} {line}: this Desk has no item #{id}"))?;
    if it.state == "done" && what != "take_copy" {
        return Err(format!("#{id} {line}: it's done already"));
    }
    match what {
        "close" => close(st, project, by, id, commit, why, fx),
        "take_copy" => take_copy(st, project, by, id, why, now, fx),
        "land_first" => land_first(st, project, by, id, why, now, fx),
        "pause" => pause(st, project, by, id, why, now, fx),
        "resume" => resume(st, project, by, id, why, now, fx),
        "hold" => hold(st, by.name, id, why, now, fx).map(|text| step(st, project, by, "hold", id, &text, why, now, fx)),
        _ => start(st, project, by, id, why, now, fx),
    }
}

/// CLOSE: what git is asked is asked a moment later, off the store (`Hub::desk_close_on_main`).
fn close(st: &Store, project: &str, by: &By, id: &str, commit: &str, why: &str, fx: &mut Effects) -> Result<(), String> {
    let it = st.desk.item(id).ok_or_else(|| format!("#{id} not closed: this Desk has no item #{id}"))?;
    let fin = it.finish.as_ref();
    if st.desk.finish_claims.contains(id) || fin.is_some_and(|f| it.state == "finishing" && f.question.is_empty() && matches!(f.step.as_str(), "building" | "installing" | "starting")) {
        return Err(format!("#{id} not closed: it's being committed or installed right now; wait for that to end"));
    }
    let commit = [commit.to_string(), fin.map(|f| f.commit.clone()).unwrap_or_default(), it.commit.split_whitespace().next().unwrap_or("").to_string()]
        .into_iter()
        .chain(it.commit_in.iter().map(|(_, c)| c.clone()))
        .find(|c| !c.is_empty())
        .ok_or_else(|| format!("#{id} not closed: Cue doesn't know which commit holds its work. Say it: CLOSE: #{id} | commit: <sha> | why: …"))?;
    let session_folder = |sid: &str| st.sessions.get(sid).filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone());
    let mut folders: Vec<String> = [fin.map(|f| f.repo.clone()), it.tree.as_ref().map(|t| t.repo.clone()), Some(it.tree_wait_repo.clone())].into_iter().flatten().collect();
    folders.extend(it.commit_in.iter().map(|(f, _)| f.clone()));
    folders.extend(it.holder.as_ref().and_then(|h| session_folder(h.to())));
    folders.extend(session_folder(by.sid));
    folders.retain(|f| !f.is_empty());
    folders.dedup();
    fx.on_main.push(OnMain { project: project.into(), id: id.into(), commit, folders, by: by.name.into(), by_sid: by.sid.into(), why: why.into() });
    Ok(())
}

/// TAKE COPY: its copy goes (kept, with its branch, when it has work in it), so the item waiting longest for one starts.
/// One at work, or still finishing, keeps it. An item back from work (stuck, or stopped on a question) is new again,
/// for the lead to route.
fn take_copy(st: &mut Store, project: &str, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let busy = super::desk_finish::under_way(st, id);
    let it = st.desk.item(id).ok_or_else(|| format!("#{id} copy not taken back: this Desk has no item #{id}"))?;
    let Some(t) = it.tree.clone() else { return Err(format!("#{id} copy not taken back: it has no copy of its own")) };
    if let Some(busy) = busy.or_else(|| (it.state == "finishing" && it.finish.as_ref().is_some_and(|f| f.question.is_empty())).then(|| "is in the landing line".to_string())) {
        return Err(format!("#{id} copy not taken back: it {busy}. You can move it up (LAND FIRST: #{id} | why: …) or wait for it"));
    }
    if let Some(h) = it.holder.as_ref().filter(|h| it.state == "working" && at_work(st, h)) {
        return Err(format!("#{id} copy not taken back: {} is at work in it now. You can tell it to stop first (NOTE: #{id} | …), or wait", h.name));
    }
    fx.trees_gone.push(it.clone());
    let it = st.desk.item_mut(id).ok_or("gone")?;
    it.tree = None;
    let back = matches!(it.state.as_str(), "working" | "queued" | "finishing");
    if back {
        if let Some(h) = it.holder.take() {
            fx.sends.push((h.to().to_string(), format!("Desk item #{id}: the Desk lead took back its copy ({}). Its work, if any, is kept on branch {}. Stop work on it until it's routed to you again.", t.path, t.branch)));
        }
        (it.state, it.after, it.finish, it.tree_wait) = ("new".into(), None, None, false);
        it.paused = None;
        it.tree_wait_repo.clear();
    }
    it.updated = secs(now);
    fx.items.push(id.to_string());
    let text = if back { format!("Took back #{id}'s copy; it's new again, its work kept on {}", t.branch) } else { format!("Took back #{id}'s copy") };
    step(st, project, by, "take_copy", id, &text, why, now, fx);
    Ok(())
}

/// LAND FIRST: an item in the landing line lands next, before those the lead didn't move up.
fn land_first(st: &mut Store, project: &str, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let was = shown_state(st, id);
    let it = st.desk.item_mut(id).ok_or_else(|| format!("#{id} not moved up: this Desk has no item #{id}"))?;
    let in_line = it.state == "finishing" && it.tree.is_some();
    let Some(f) = it.finish.as_mut().filter(|f| in_line && matches!(f.step.as_str(), "commit" | "line")) else {
        return Err(format!("#{id} not moved up: it isn't waiting in the landing line (it's {was})"));
    };
    f.first = now;
    it.updated = secs(now);
    fx.items.push(id.to_string());
    fx.land = true;
    step(st, project, by, "land_first", id, &format!("Moved #{id} up in the landing line"), why, now, fx);
    Ok(())
}

/// START: a queued item starts now, not waiting for the item it was after, nor for a free copy (it works where its
/// holder is when none is free). Nobody holds it: a new session starts on it.
fn start(st: &mut Store, project: &str, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let rule = rule_of(st, project);
    let lead = st.desk.lead.get(project).cloned();
    let was = shown_state(st, id);
    let it = st.desk.item_mut(id).ok_or_else(|| format!("#{id} not started: this Desk has no item #{id}"))?;
    if it.state == "held" {
        return Err(format!("#{id} not started: the user parked it. Only they start it (its Start button); you can ask them (ASK: #{id} | …)"));
    }
    if it.state != "queued" {
        return Err(format!("#{id} not started: it isn't waiting (it's {was}). A new item starts with a ROUTE"));
    }
    let copy = it.tree_wait;
    (it.after, it.tree_wait, it.no_copy_wait, it.updated) = (None, false, copy || it.no_copy_wait, secs(now));
    it.tree_wait_repo.clear();
    it.after_why = why.to_string();
    if let Some(h) = it.holder.clone() {
        it.state = "working".into();
        fx.sends.push((h.to().to_string(), crate::desk::handed_words(it, why, &rule)));
    } else {
        it.state = "new".into();
        let (name, sid) = lead.as_ref().map_or(("Desk", ""), |l| (l.name.as_str(), l.sid.as_str()));
        fx.starts.push(NewStart { project: project.into(), id: id.into(), why: why.into(), by: name.into(), by_sid: sid.into() });
    }
    fx.items.push(id.to_string());
    let text = if copy { format!("Started #{id} now, without waiting for a copy") } else { format!("Started #{id} now") };
    step(st, project, by, "start", id, &text, why, now, fx);
    Ok(())
}

/// PAUSE (#365): an item at work stops at a clean point and keeps its place, e.g. to save tokens for an urgent one.
/// Its holder keeps it (and its copy), and is told to stop once the step it's in leaves the files sound, note where
/// it is, and end its turn; one not at work now isn't sent anything (a message would wake it). While it's paused its
/// holder isn't asked "Done?" and the lead isn't told it stopped.
fn pause(st: &mut Store, project: &str, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let was = shown_state(st, id);
    let it = st.desk.item(id).ok_or_else(|| format!("#{id} not paused: this Desk has no item #{id}"))?;
    if it.state != "working" || it.holder.is_none() {
        return Err(format!("#{id} not paused: only an item at work can be paused (it's {was})"));
    }
    if it.paused.is_some() {
        return Err(format!("#{id} not paused: it's paused already. RESUME: #{id} | why: … starts it again"));
    }
    let h = it.holder.clone().ok_or("no holder")?;
    let busy = at_work(st, &h);
    let it = st.desk.item_mut(id).ok_or("gone")?;
    it.paused = Some((secs(now), why.trim().to_string()));
    it.updated = secs(now);
    if busy {
        // Now, not after its turn: a direct holder reads it at once; a teammate's lead gets it when its turn ends.
        if h.kind != "crew" {
            fx.now.push(h.to().to_string());
        }
        fx.sends.push((h.to().to_string(), pause_words(id, &h, why)));
    }
    st.desk.nudged.remove(id);
    fx.items.push(id.to_string());
    step(st, project, by, "pause", id, &format!("Paused #{id}"), why, now, fx);
    Ok(())
}

/// What a holder at work on item `id` is told as the lead pauses it.
pub(crate) fn pause_words(id: &str, h: &crate::desk::Holder, why: &str) -> String {
    let why = if why.trim().is_empty() { String::new() } else { format!(" ({})", why.trim().trim_end_matches('.')) };
    let who = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    format!(
        "{who}Desk item #{id} is paused by the Desk lead{why}. Stop at the next clean point: finish or undo the edit you're in so the files make sense, don't start anything new on it, write in a few lines where you are and what's next, and end your turn. Keep it: Cue tells you when to resume."
    )
}

/// RESUME (#365): a paused item carries on. Its holder is told to pick up where it stopped.
fn resume(st: &mut Store, project: &str, by: &By, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let was = shown_state(st, id);
    let it = st.desk.item_mut(id).ok_or_else(|| format!("#{id} not resumed: this Desk has no item #{id}"))?;
    if it.paused.is_none() || it.state != "working" {
        return Err(format!("#{id} not resumed: it isn't paused (it's {was})"));
    }
    it.paused = None;
    it.updated = secs(now);
    if let Some(h) = it.holder.clone() {
        let why = if why.trim().is_empty() { String::new() } else { format!(" ({})", why.trim().trim_end_matches('.')) };
        let who = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
        fx.sends.push((h.to().to_string(), format!("{who}Desk item #{id} is resumed{why}: carry on from where you stopped. Say DONE: #{id} on a line of its own when it's done.")));
    }
    fx.items.push(id.to_string());
    step(st, project, by, "resume", id, &format!("Resumed #{id}"), why, now, fx);
    Ok(())
}

/// PARK (#385, #391), by you or by the lead as you asked: item `id`, not at work, is parked: nobody holds it,
/// it waits on nothing, and neither Cue nor the lead starts it till you press Start (`unhold`). A session it was
/// queued for is told it's off its list. Ok: its Step's words.
pub(super) fn hold(st: &mut Store, by: &str, id: &str, why: &str, now: u64, fx: &mut Effects) -> Result<String, String> {
    let was = shown_state(st, id);
    let it = st.desk.item_mut(id).ok_or_else(|| format!("#{id} not parked: there's no item #{id}"))?;
    match it.state.as_str() {
        "held" => return Err(format!("#{id} not parked: it's parked already")),
        "new" | "queued" => {}
        _ => return Err(format!("#{id} not parked: only an item nobody is at work on can be parked (it's {was}). PAUSE: #{id} stops one at work")),
    }
    if let Some(h) = it.holder.clone() {
        let who = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
        fx.sends.push((h.to().to_string(), format!("{who}Desk item #{id} ({}) is parked: it's off your list. Don't start it; Cue tells you if it comes back.", it.title)));
        it.remember_holder();
    }
    (it.state, it.holder, it.after, it.moved_from) = ("held".into(), None, None, None);
    (it.tree_wait, it.no_copy_wait) = (false, false);
    it.tree_wait_repo.clear();
    it.after_why.clear();
    it.crew.clear();
    it.start_failed.clear();
    it.route_failed.clear();
    it.held = Some((secs(now), by.to_string(), why.trim().to_string()));
    it.updated = secs(now);
    st.desk.nudged.remove(id);
    fx.items.push(id.to_string());
    Ok(format!("Parked #{id}"))
}

/// Start on a parked item (#385, yours only): no longer parked, new, its lead told of it to route as any new item.
pub(super) fn unhold(st: &mut Store, id: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let has_lead = st.desk.item(id).is_some_and(|i| st.desk.lead_sid(&i.project).is_some());
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    if it.state != "held" {
        return Err("it isn't parked".into());
    }
    (it.state, it.held, it.updated) = ("new".into(), None, secs(now));
    if has_lead {
        fx.news.push((it.project.clone(), id.to_string()));
    }
    fx.items.push(id.to_string());
    Ok(())
}

/// Item `id`'s state as the lead is told it.
fn shown_state(st: &Store, id: &str) -> String {
    st.desk.item(id).map_or("gone".into(), |i| match i.state.as_str() {
        "to_check" => "with the user to check".into(),
        "finishing" => i.finish.as_ref().map_or("finishing".into(), |f| format!("finishing: {}", f.step)),
        "working" if i.paused.is_some() => "paused".into(),
        "held" => "parked".into(),
        s => s.into(),
    })
}

/// The lead's unsticking (`what`, as `unstick` takes it), one of item `id`'s Steps: nothing to undo.
#[allow(clippy::too_many_arguments)]
fn step(st: &mut Store, project: &str, by: &By, what: &str, id: &str, text: &str, why: &str, now: u64, fx: &mut Effects) {
    decide(st, project, Some(id.to_string()), by.name, by.sid, text, why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
        d.act = what.into();
    }
}

impl Hub {
    /// CLOSE's item closed once its commit is on the main line, as Looks right closes one; its copy goes (kept when
    /// it holds work that isn't on main). Not found there: the lead is told where Cue looked.
    pub(super) fn desk_close_on_main(&self, o: OnMain) {
        let mut found: Option<(String, String, String, String)> = None;
        let mut not = Vec::new();
        let mut repos: Vec<String> = o.folders.iter().filter_map(|f| tree_repo(f)).collect();
        repos.dedup();
        for repo in &repos {
            let Some((_, branch)) = super::desk_main_line::main_checkout(repo) else { continue };
            let Some(tip) = rev(repo, &branch) else { continue };
            match rev(repo, &format!("{}^{{commit}}", o.commit)) {
                Some(full) if crate::self_install::is_ancestor(repo, &full, &tip) => {
                    found = Some((repo.clone(), branch, full, tip));
                    break;
                }
                Some(_) => not.push(format!("{} isn't on {branch} in {repo}", o.commit)),
                None => not.push(format!("{repo} has no commit {}", o.commit)),
            }
        }
        let Some((repo, branch, full, tip)) = found else {
            let where_ = if !not.is_empty() { not.join("; ") } else { "Cue found no repository to look in".to_string() };
            let fx = Effects { sends: vec![(o.by_sid.clone(), format!("Desk item #{} not closed: {where_}. Say the commit on main that holds its work: CLOSE: #{} | commit: <sha> | why: …", o.id, o.id))], ..Default::default() };
            self.desk_done(fx);
            return;
        };
        let short: String = full.chars().take(7).collect();
        // Its copy: removed when everything in it is on main (nothing kept would be lost), else kept and listed.
        let copy = self.read(|st| st.desk.item(&o.id).and_then(|i| i.tree.clone()));
        let spent = copy.as_ref().is_some_and(|t| {
            let head = rev(&t.path, "HEAD");
            let clean = snapshot(&t.path).is_some_and(|w| head.as_ref().and_then(|h| rev(&t.path, &format!("{h}^{{tree}}"))).as_deref() == Some(w.as_str()));
            clean && head.is_some_and(|h| crate::self_install::is_ancestor(&t.repo, &h, &tip))
        });
        // Removed before it's let go of: a free slot may go to another item at once.
        if let Some(t) = copy.as_ref().filter(|_| spent) {
            remove(t);
        }
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let it = st.desk.item_mut(&o.id).filter(|i| i.project == o.project && i.state != "done")?;
            let was = it.state.clone();
            if let Some(t) = it.tree.take() {
                if !spent {
                    let mut gone = it.clone();
                    gone.tree = Some(t);
                    fx.trees_gone.push(gone);
                }
            }
            it.state = "done".into();
            it.paused = None;
            (it.after, it.close_on_check, it.finish, it.tree_wait, it.no_copy_wait) = (None, false, None, false, false);
            it.tree_wait_repo.clear();
            it.done_at = Some(secs(now));
            it.updated = secs(now);
            if it.commit.is_empty() {
                it.commit = short.clone();
                it.commit_in = vec![(repo.clone(), short.clone())];
            }
            if let Some(h) = it.holder.as_ref().filter(|_| matches!(was.as_str(), "working" | "queued")) {
                fx.sends.push((h.to().to_string(), format!("Desk item #{} is closed: its work is already on {branch} ({short}). Stop work on it.", o.id)));
            }
            fx.items.push(o.id.clone());
            release(st, &o.id, now, &mut fx);
            let by = By { name: &o.by, sid: &o.by_sid };
            step(st, &o.project, &by, "close", &o.id, &format!("Closed #{}: its work is on {branch} ({short})", o.id), &o.why, now, &mut fx);
            Some(fx)
        });
        let Some(fx) = fx else { return };
        self.desk_done(fx);
        if let Some(t) = copy.filter(|_| spent) {
            let _ = self.desk_trees_free(&t.repo);
        }
    }
}
