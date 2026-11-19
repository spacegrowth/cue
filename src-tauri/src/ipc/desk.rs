//! The Desk's commands (desk/, hub/desk.rs, hub/desk_lead.rs): picking a project's lead, writing to it, adding an
//! item, giving one to a session, what you do to an item, and undoing a decision.

use super::{with_hub, StartOpts};
use crate::hub::{DeskClaim, Hub};
use std::sync::Arc;
use tauri::State;

/// Write to `project`'s lead: each message sent after the Desk's summary; resumed with it when parked. An error
/// ("no lead", "lead ended") when there's none to write to. Returns { sid }.
#[tauri::command]
pub(crate) async fn desk_send(hub: State<'_, Arc<Hub>>, project: String, text: String, images: Option<Vec<crate::model::Upload>>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| desk_send_images(h, &project, &text, &images.unwrap_or_default())).await?
}

/// Your words and the pictures you pasted or dropped to `project`'s lead. Returns { sid }.
pub(crate) fn desk_send_images(h: &Arc<Hub>, project: &str, text: &str, images: &[crate::model::Upload]) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() && images.is_empty() {
        return Err("nothing to send".into());
    }
    let saved = crate::uploads::save(images)?;
    let lost = |e: String| format!("{}'s lead didn't come up ({e}), so this wasn't sent: {}", h.desk_name(project), text.trim());
    let sid = deliver(h, project, text, saved, &lost)?;
    Ok(serde_json::json!({ "sid": sid }))
}

pub(crate) fn desk_send_with(h: &Arc<Hub>, project: &str, text: &str) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() {
        return Err("nothing to send".into());
    }
    let lost = |e: String| format!("{}'s lead didn't come up ({e}), so this wasn't sent: {}", h.desk_name(project), text.trim());
    let sid = deliver(h, project, text, vec![], &lost)?;
    Ok(serde_json::json!({ "sid": sid }))
}

/// Your words (`text`) and pictures (`saved`) to `project`'s lead, however it's reached (`desk_claim`), after the
/// Desk's summary as built for that lead: its id. Sent to it live, the pictures go as a session chat's do (shown in its
/// chat); held while it starts or resumed with it, the message names them by where they are, as a hosted session is
/// sent them. `lost`: what you're told when it was waited for or resumed and didn't come up.
fn deliver(h: &Arc<Hub>, project: &str, text: &str, saved: Vec<crate::uploads::Saved>, lost: &dyn Fn(String) -> String) -> Result<String, String> {
    let named = crate::uploads::with_paths(text, &saved);
    let (sid, failed) = match h.desk_claim(project, &named)? {
        DeskClaim::Send(sid) => {
            let (message, failed) = for_lead(h, project, &sid, text);
            h.send_saved(&sid, &message, saved, false)?;
            (sid, failed)
        }
        // Another message is resuming it (or a lead is starting or changing): this one is sent by that call once
        // it's done (with the summary for the lead it reaches), or not, and you're told so.
        DeskClaim::Wait(outcome) => return outcome.recv().unwrap_or_else(|_| Err("it gave up".into())).map_err(lost),
        DeskClaim::Resume(sid) => {
            let (message, failed) = for_lead(h, project, &sid, &named);
            (DeskStart::new(h, project).done(resume(h, &sid, &message)).map_err(lost)?, failed)
        }
    };
    // Told now: the failed routes it was told of are cleared.
    h.desk_told(&failed);
    Ok(sid)
}

/// Your words (`text`) as `project`'s lead `sid` is sent them: after the Desk's summary as it sees it (itself left
/// out of the sessions it can route to). With the failed routes it's told of.
fn for_lead(h: &Arc<Hub>, project: &str, sid: &str, text: &str) -> (String, Vec<(String, String)>) {
    let (summary, failed) = h.desk_summary(project, sid, false);
    (desk_message(&summary, text), failed)
}

/// Always exactly: the `<desk …>…</desk>` block, a blank line, then the words. The window strips that leading block
/// from your messages in the lead's chat, so keep this shape.
fn desk_message(summary: &str, text: &str) -> String {
    format!("{summary}\n\n{}", text.trim())
}

/// `project`'s lead handed off (hub/desk_handoff.rs): session `sid`, which carries on from its note, leads the Desk
/// from now, and is sent it with how a lead works (kept in Cue while it reads the note).
pub(crate) fn lead_carried_on(h: &Arc<Hub>, project: &str, sid: &str, name: &str) -> Result<(), String> {
    h.desk_put_lead(project, sid, name);
    let (summary, failed) = h.desk_summary(project, sid, true);
    let words = h.desk_handoff_words(project, sid, name);
    h.send_to_session(sid, &desk_message(&summary, &words), &[], false)?;
    h.desk_told(&failed);
    Ok(())
}

/// What a lead is told when it takes the Desk on.
fn hello(project: &str) -> String {
    format!("You run {project}'s Desk now. Say so in one line, then hand out anything not given out yet.")
}

/// Pick `project`'s lead: session `sid` (of the project: live, parked or ended), or, without one, a new hosted
/// Claude session in the project's folder named "<project>-lead". The lead is sent the Desk (with how a lead works,
/// for a session that didn't start as one); a parked or ended one is resumed with it. The lead it replaces, still
/// open, is told. `trust`: you said you trust the folder, when Cue asked (as + New does). Returns { sid }.
#[tauri::command]
pub(crate) async fn desk_set_lead(hub: State<'_, Arc<Hub>>, project: String, sid: Option<String>, trust: Option<bool>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| desk_set_lead_with(h, &project, sid.as_deref().filter(|s| !s.is_empty()), trust.unwrap_or(false))).await?
}

pub(crate) fn desk_set_lead_with(h: &Arc<Hub>, project: &str, sid: Option<&str>, trust: bool) -> Result<serde_json::Value, String> {
    set_lead(h, project, sid, trust, None, crate::hosted::Agent::Claude)
}

/// A new Desk in `folder` (Desk home's + New Desk, + New's Start a Desk): the folder's project, its lead a new
/// session Cue starts there. Every call creates an independent Desk. Returns { id, project: id, sid }.
#[tauri::command]
pub(crate) async fn desk_start(hub: State<'_, Arc<Hub>>, folder: String, trust: Option<bool>, name: Option<String>, agent: Option<String>) -> Result<serde_json::Value, String> {
    let agent = match agent.as_deref() {
        None => crate::hosted::Agent::Claude,
        Some(a) => crate::hosted::Agent::named(a).ok_or_else(|| format!("unknown agent: {a}"))?,
    };
    with_hub(&hub, move |h| desk_start_with_agent(h, &folder, trust.unwrap_or(false), name.as_deref(), agent)).await?
}

pub(crate) fn desk_start_named(h: &Arc<Hub>, folder: &str, trust: bool, name: Option<&str>) -> Result<serde_json::Value, String> {
    desk_start_with_agent(h, folder, trust, name, crate::hosted::Agent::Claude)
}

pub(crate) fn desk_start_with_agent(h: &Arc<Hub>, folder: &str, trust: bool, name: Option<&str>, agent: crate::hosted::Agent) -> Result<serde_json::Value, String> {
    let dir = expand_home(folder);
    if dir.is_empty() || !std::path::Path::new(&dir).is_dir() {
        return Err(format!("no such folder: {}", folder.trim()));
    }
    let dir = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    let workspace = crate::model::project_at(&dir, "");
    let name = name.unwrap_or(&workspace).split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 80 {
        return Err("a Desk needs a name of 1–80 characters".into());
    }
    let id = format!("desk-{}", crate::focus::new_session_id());
    let space = crate::desk::model::DeskSpace { id: id.clone(), name, project: workspace, folder: dir.clone() };
    h.desk_put_space(space)?;
    let result = set_lead(h, &id, None, trust, Some(dir), agent);
    let r = result?;
    Ok(serde_json::json!({ "id": id, "project": id, "sid": r["sid"] }))
}

/// Your answer to Ask me before for `project` (a workspace project): `kinds` ticked, or None for "Not now".
#[tauri::command]
pub(crate) async fn ask_before_set(hub: State<'_, Arc<Hub>>, project: String, kinds: Option<Vec<String>>) -> Result<Vec<&'static str>, String> {
    with_hub(&hub, move |h| h.ask_before_set(&project, kinds.as_deref())).await?
}

/// How work lands on Desk `desk`'s project, from its setting: each part's choice, and `at_once`.
#[tauri::command]
pub(crate) async fn desk_lands_set(hub: State<'_, Arc<Hub>>, desk: String, lands: serde_json::Value) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_lands_set(&desk, &lands)).await?
}

/// Away mode for Desk `desk` (`away`: { on, until, budget, rules }): saved, its lead told. Returns its view.
#[tauri::command]
pub(crate) async fn desk_away_set(hub: State<'_, Arc<Hub>>, desk: String, away: serde_json::Value) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_away_set(&desk, &away)).await?
}

/// Pause (`on`) or resume Desk `desk` (#366): its sessions stop at a safe point, or carry on. Returns its view.
#[tauri::command]
pub(crate) async fn desk_pause(hub: State<'_, Arc<Hub>>, desk: String, on: bool) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| {
        h.desk_pause_set(&desk, on, ("You", ""), "")?;
        Ok(crate::hub::desk_pause_view(&desk))
    })
    .await?
}

/// The owner pressed Away (before it's on): Desk `desk`'s lead (woken if parked) is asked to settle what it can under
/// `rules` and leave at most 5 questions; the Desk shows it's sorting things out until its next turn ends.
#[tauri::command]
pub(crate) async fn desk_away_sweep(hub: State<'_, Arc<Hub>>, desk: String, rules: serde_json::Value) -> Result<(), String> {
    with_hub(&hub, move |h| desk_away_sweep_with(h, &desk, &rules)).await?
}

pub(crate) fn desk_away_sweep_with(h: &Arc<Hub>, desk: &str, rules: &serde_json::Value) -> Result<(), String> {
    // Its lead stopped by a usage limit: nothing sent now (Cue carries it on after the reset).
    if !h.desk_away_sweeping(desk) {
        return Ok(());
    }
    let words = crate::hub::desk_away_sweep_words(rules);
    if let Err(e) = desk_send_with(h, desk, &words) {
        h.desk_away_sweep_failed(desk);
        return Err(e);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn desk_delete(hub: State<'_, Arc<Hub>>, id: String, close_sessions: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| {
        let requested = close_sessions.unwrap_or_default();
        let eligible = h.desk_delete_selected(&id, &requested)?;
        let mut kept = Vec::new();
        for sid in &requested {
            if !eligible.contains(sid) || h.desk_session_shared(sid) { kept.push(serde_json::json!({ "sid": sid, "reason": "shared with another Desk or no longer on this Desk" })); continue; }
            if let Err(e) = super::sessions::end_session(h, sid) { kept.push(serde_json::json!({ "sid": sid, "reason": e })); }
        }
        Ok(serde_json::json!({ "kept": kept }))
    }).await?
}

#[tauri::command]
pub(crate) async fn desk_rename(hub: State<'_, Arc<Hub>>, id: String, name: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_rename(&id, &name)).await?
}

fn expand_home(p: &str) -> String {
    let p = p.trim().trim_end_matches('/');
    match (p.strip_prefix('~'), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}{rest}"),
        _ => p.to_string(),
    }
}

/// `desk_set_lead_with`, a new lead started in `dir` when given (else in a folder of the project's sessions).
fn set_lead(h: &Arc<Hub>, project: &str, sid: Option<&str>, trust: bool, dir: Option<String>, agent: crate::hosted::Agent) -> Result<serde_json::Value, String> {
    if project.trim().is_empty() {
        return Err("no project".into());
    }
    let picked = match sid {
        None => None,
        Some(sid) => {
            let (name, state) = h.desk_lead_candidate(project, sid)?;
            if h.desk_lead_sid(project).as_deref() == Some(sid) && state == "live" {
                return Ok(serde_json::json!({ "sid": sid }));
            }
            Some((sid, name, state))
        }
    };
    // Any change of lead claims the project first, and only then reads the lead it replaces: one change (or start,
    // or resume) at a time, so a change that fails never puts back a lead another one has replaced since.
    h.desk_claim_start(project)?;
    let claim = DeskStart::new(h, project);
    if let Some((sid, _, _)) = &picked {
        h.desk_claim_lead(project, sid)?;
    }
    let before = h.desk_lead(project);
    let made = match &picked {
        None => {
            let name = h.desk_lead_name(project);
            start(h, project, &name, trust, dir.clone(), agent).map(|sid| (sid, name))
        }
        Some((sid, name, state)) => take_on(h, project, sid, name, state).map(|_| (sid.to_string(), name.clone())),
    };
    match &made {
        Ok((sid, _)) => drop(claim.done(Ok(sid.clone()))),
        // It didn't come up, or wasn't told: the Desk keeps the lead it had, and the messages held meanwhile go to it.
        Err(_) => {
            h.desk_put_back_lead(project, before.clone());
            claim.back();
        }
    }
    let (sid, name) = made?;
    // The lead it replaces keeps what it holds, and is told it no longer runs the Desk.
    if let Some(old) = before.map(|l| l.sid).filter(|old| *old != sid && h.session_origin(old).is_some()) {
        let words = format!("You no longer run {}'s Desk: {name} does now. Keep on with the items you hold.", h.desk_name(project));
        if let Err(e) = h.send_to_session(&old, &words, &[], false) {
            eprintln!("cue: desk: {project}'s last lead wasn't told it was replaced: {e}");
        }
    }
    Ok(serde_json::json!({ "sid": sid }))
}

/// Session `sid` (`name`, `state`: live, parked or ended) made `project`'s lead and sent the Desk with how a lead
/// works; a parked or ended one is resumed with it.
fn take_on(h: &Arc<Hub>, project: &str, sid: &str, name: &str, state: &str) -> Result<(), String> {
    h.desk_put_lead(project, sid, name);
    let (summary, failed) = h.desk_summary(project, sid, true);
    let message = desk_message(&summary, &hello(&h.desk_name(project)));
    match state {
        "live" => {
            h.send_to_session(sid, &message, &[], false)?;
        }
        "parked" => {
            resume(h, sid, &message)?;
        }
        _ => {
            let record = super::closed_record(h, sid)?;
            super::resume_record(h, record, &message)?;
        }
    }
    h.desk_told(&failed);
    Ok(())
}

/// Parked lead `sid` resumed with `message`; sent to it instead when it was running again already (Resume then only
/// takes it off Parked). Its id.
fn resume(h: &Arc<Hub>, sid: &str, message: &str) -> Result<String, String> {
    match super::resume_parked_session(h, sid, message) {
        Ok(r) if r == super::ALREADY_RUNNING => h.send_to_session(sid, message, &[], false).map(|_| sid.to_string()),
        Ok(_) => Ok(sid.to_string()),
        Err(e) => Err(e),
    }
}

/// A new hosted lead for `project` named `name`, started in the project's folder with the Desk and a hello
/// (its instructions come with its system prompt): its id.
fn start(h: &Arc<Hub>, project: &str, name: &str, trust: bool, dir: Option<String>, agent: crate::hosted::Agent) -> Result<String, String> {
    let dir = dir.or_else(|| h.desk_folder(project)).ok_or_else(|| format!("no folder known for {project}: start a session there first"))?;
    // Claude receives the Desk instructions as a system prompt; other agents need them in the first message.
    let (summary, _) = h.desk_summary(project, "", agent != crate::hosted::Agent::Claude);
    let message = desk_message(&summary, &hello(&h.desk_name(project)));
    let opts = StartOpts { trust, desk_lead: project.to_string(), ..Default::default() };
    let started = super::start_hosted(h, agent, &dir, &message, name, &opts, &[])?;
    started["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".into())
}

/// A start or resume of `project`'s lead that `desk_claim` (or `desk_claim_start`) allowed. The claim is let go once
/// it's over (`done`), or, if it never got there (a panic, an early return), when this is dropped: the messages held
/// meanwhile then hear they weren't sent.
pub(crate) struct DeskStart<'a> {
    h: &'a Arc<Hub>,
    project: &'a str,
    open: bool,
}

impl<'a> DeskStart<'a> {
    pub(crate) fn new(h: &'a Arc<Hub>, project: &'a str) -> Self {
        DeskStart { h, project, open: true }
    }

    /// It's over (`up`: the lead's id, or why it didn't come up): the messages that came meanwhile are sent to it
    /// now (kept in Cue while it was at work), each after the Desk's summary as it sees it, and each sender hears
    /// how that went. `up` handed back.
    pub(crate) fn done(mut self, up: Result<String, String>) -> Result<String, String> {
        self.open = false;
        for w in self.h.desk_claim_done(self.project) {
            let sent = up.clone().and_then(|sid| {
                let (message, failed) = for_lead(self.h, self.project, &sid, &w.text);
                self.h.send_to_session(&sid, &message, &[], false)?;
                self.h.desk_told(&failed);
                Ok(sid)
            });
            // Its sender may have gone (the window closed): nothing more to tell then.
            let _ = w.reply.send(sent);
        }
        up
    }

    /// A change of lead that failed is over, and the lead it had is back: the messages that came meanwhile go to that
    /// lead as any message would (resumed with it when parked; "no lead" when there was none), and each sender hears
    /// how that went.
    pub(crate) fn back(mut self) {
        self.open = false;
        for w in self.h.desk_claim_done(self.project) {
            let sent = deliver(self.h, self.project, &w.text, vec![], &|e| e);
            // Its sender may have gone (the window closed): nothing more to tell then.
            let _ = w.reply.send(sent);
        }
    }
}

impl Drop for DeskStart<'_> {
    fn drop(&mut self) {
        if self.open {
            // The held messages' replies go with them: each sender hears it wasn't sent.
            drop(self.h.desk_claim_done(self.project));
        }
    }
}

/// The "New item" button, or "Add to Desk" on a session (`holder`: that session, which holds it). Returns the item.
#[tauri::command]
pub(crate) async fn desk_add(hub: State<'_, Arc<Hub>>, project: String, kind: String, title: String, see: Option<String>, done_when: Option<String>, holder: Option<String>, images: Option<Vec<crate::model::Upload>>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_add_with(&project, &kind, &title, see.as_deref().unwrap_or(""), done_when.as_deref().unwrap_or(""), holder.as_deref().filter(|s| !s.is_empty()), &images.unwrap_or_default())).await?
}

/// "Give to…": item `id` to session `sid`, which is sent it. Returns the item as it is now.
#[tauri::command]
pub(crate) async fn desk_give(hub: State<'_, Arc<Hub>>, id: String, sid: String) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_give(&id, &sid)).await?
}

/// Session `sid` on `project`'s Desk (`on`) or off it: the sessions its lead routes to without asking you.
#[tauri::command]
pub(crate) async fn desk_member(hub: State<'_, Arc<Hub>>, project: String, sid: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_set_member(&project, &sid, on)).await?
}

/// What you did to an item: "looks_right" | "reopen" | "send_back" | "start_now" | "put_before" | "install_now" | "rerun"
/// | "cut" | "drop" | "remove_kept", with `text` for send_back (your words), cut (the item it no longer
/// waits on). Returns the item as it is now (state "dropped" once dropped).
#[tauri::command]
pub(crate) async fn desk_item_act(hub: State<'_, Arc<Hub>>, id: String, act: String, text: Option<String>, images: Option<Vec<crate::model::Upload>>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| {
        // Pictures sent back with it: saved, and their paths go with your words, as an item's own pictures do.
        let saved = crate::uploads::save(&images.unwrap_or_default())?;
        let mut text = text.unwrap_or_default();
        if !saved.is_empty() {
            let paths: Vec<String> = saved.iter().map(|s| format!("`{}`", s.path)).collect();
            text = format!("{}\nPictures with it (open each to see it): {}", text.trim(), paths.join(" "));
        }
        h.desk_item_act(&id, &act, &text)
    })
    .await?
}

/// Looks right on item `id`: its changed files committed at once (word check first, no push) and it's done; none, just
/// done; with files another session is editing, done and the commit waits. { commit, files, waiting } (hub/desk_commit.rs).
#[tauri::command]
pub(crate) async fn desk_looks_right(hub: State<'_, Arc<Hub>>, id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_looks_right(&id)).await?
}

/// Item `id`'s Files changed as one diff: to check, what Looks right would commit; done, its commit (or what waits
/// to be). { diff, machine } (hub/desk_diff.rs).
#[tauri::command]
pub(crate) async fn desk_item_diff(hub: State<'_, Arc<Hub>>, id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_item_diff(&id)).await?
}

/// Undo on Looks right's commit: taken back while it's the last one (its changes stay), the item ready to check again.
#[tauri::command]
pub(crate) async fn desk_commit_undo(hub: State<'_, Arc<Hub>>, id: String) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_commit_undo(&id)).await?
}

/// `project`'s done items, newest first, `take` of them after the one the list ends with (`before_ms`, `before_id`):
/// the Desk's list loads them as you scroll.
#[tauri::command]
pub(crate) async fn desk_done(hub: State<'_, Arc<Hub>>, project: String, before_ms: Option<u64>, before_id: Option<String>, take: u32) -> Result<Vec<crate::desk::DeskItem>, String> {
    with_hub(&hub, move |h| h.desk_done_page(&project, before_ms.zip(before_id.as_deref()), take)).await
}

/// `project`'s lead's own moves (#374), newest first, `take` of them after the one the list ends with (`before_ms`,
/// `before_id`): "What the lead moved" loads them as you scroll.
#[tauri::command]
pub(crate) async fn desk_moves(hub: State<'_, Arc<Hub>>, project: String, before_ms: Option<u64>, before_id: Option<String>, take: u32) -> Result<Vec<crate::desk::DeskDecision>, String> {
    with_hub(&hub, move |h| h.desk_moves_page(&project, before_ms.zip(before_id.as_deref()), take)).await
}

/// Clear on "What the lead moved": the list starts again from now.
#[tauri::command]
pub(crate) async fn desk_moves_clear(hub: State<'_, Arc<Hub>>, project: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_moves_clear(&project)).await?
}

/// Undo this: whoever made the decision is asked to undo it. What Cue did.
#[tauri::command]
pub(crate) async fn desk_undo(hub: State<'_, Arc<Hub>>, decision_id: String, answer: Option<String>) -> Result<String, String> {
    with_hub(&hub, move |h| h.desk_undo_with(&decision_id, answer.as_deref().unwrap_or(""))).await?
}
