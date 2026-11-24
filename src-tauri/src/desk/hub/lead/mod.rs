//! A Desk's lead: picking one (a session of the project, or a new hosted one Cue starts), a new Desk with its lead,
//! taking the Desk on (sent the Desk with how a lead works), resuming a parked or ended one with it, and the lead
//! a hand-off carried on in. Writing to the lead: send.rs. The window's commands are in ipc/desk.rs.

mod send;
pub(crate) mod feed;
pub(crate) mod run;
pub(crate) mod block;
pub(crate) mod handoff;
pub(crate) mod cleanup;
pub(crate) mod checks;
pub(crate) mod lands;
pub(crate) mod stop_lead;

pub(crate) use send::{desk_send_cue, desk_send_images, desk_send_now, desk_send_with, DeskStart};
use send::desk_message;
use crate::hub::{Hub, StartOpts};
use std::sync::Arc;

/// `project`'s lead handed off (desk/hub/lead/handoff.rs): session `sid`, which carries on from its note, leads the Desk
/// from now, and is sent it with how a lead works (kept in Cue while it reads the note).
pub(crate) fn lead_carried_on(h: &Arc<Hub>, project: &str, sid: &str, name: &str) -> Result<(), String> {
    h.desk_put_lead(project, sid, name);
    let words = h.desk_handoff_words(project, sid, name);
    let (summary, failed) = h.desk_block(project, sid, &words, true, true);
    h.tell(sid, &desk_message(&summary, &words))?;
    h.desk_told(&failed);
    Ok(())
}

/// What a lead is told when it takes the Desk on.
fn hello(project: &str) -> String {
    format!("You run {project}'s Desk now. Say so in one line, then hand out anything not given out yet.")
}

pub(crate) fn desk_set_lead_with(h: &Arc<Hub>, project: &str, sid: Option<&str>, trust: bool) -> Result<serde_json::Value, String> {
    set_lead(h, project, sid, trust, None, crate::hosted::Agent::Claude)
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
    let space = crate::desk::model::DeskSpace { id: id.clone(), name, project: workspace, folder: dir.clone(), machine_folders: Default::default() };
    h.desk_put_space(space)?;
    let result = set_lead(h, &id, None, trust, Some(dir), agent);
    let r = result?;
    Ok(serde_json::json!({ "id": id, "project": id, "sid": r["sid"] }))
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
        if let Err(e) = h.tell(&old, &words) {
            eprintln!("cue: desk: {project}'s last lead wasn't told it was replaced: {e}");
        }
    }
    Ok(serde_json::json!({ "sid": sid }))
}

/// Session `sid` (`name`, `state`: live, parked or ended) made `project`'s lead and sent the Desk with how a lead
/// works; a parked or ended one is resumed with it.
fn take_on(h: &Arc<Hub>, project: &str, sid: &str, name: &str, state: &str) -> Result<(), String> {
    h.desk_put_lead(project, sid, name);
    let hi = hello(&h.desk_name(project));
    let (summary, failed) = h.desk_block(project, sid, &hi, true, true);
    let message = desk_message(&summary, &hi);
    match state {
        "live" => {
            h.tell(sid, &message)?;
        }
        "parked" => {
            resume(h, sid, &message)?;
        }
        _ => {
            let record = crate::hub::closed_record(h, sid)?;
            crate::hub::resume_record(h, record, &message)?;
        }
    }
    h.desk_told(&failed);
    Ok(())
}

/// Parked lead `sid` resumed with `message`; sent to it instead when it was running again already (Resume then only
/// takes it off Parked). Its id.
pub(super) fn resume(h: &Arc<Hub>, sid: &str, message: &str) -> Result<String, String> {
    match crate::hub::resume_parked_session(h, sid, message) {
        Ok(r) if r == crate::hub::ALREADY_RUNNING => h.tell(sid, message).map(|_| sid.to_string()),
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
    let started = crate::hub::start_hosted(h, agent, &dir, &message, name, &opts, &[])?;
    started["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".into())
}
