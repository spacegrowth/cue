//! A hosted session's life (docs/dev/cue-hosted-sessions.md): + New → Cue-hosted, closed and parked, resumed,
//! Move to Cue into one (Settings → Cue-hosted sessions on), and Move to terminal out of one. What it reports,
//! and what's sent to it: hub/hosted.rs.

use super::with_hub;
// Kept here for the add-on.
#[allow(unused_imports)]
pub(crate) use crate::hub::{start_hosted, UNTRUSTED};
use crate::hub::Hub;
use crate::logs::transcript;
use crate::model::{self, Origin};
use std::sync::Arc;
use tauri::State;

/// Move to Cue with Settings → Cue-hosted sessions on: the terminal session ended (its tab closed with it),
/// then resumed as a hosted one, its record (name, star, recent thread) carried over. `end`: ending it as
/// Close does. Its history needs nothing done: both write the same transcript.
pub(crate) fn move_into_hosted(h: &Arc<Hub>, origin: Origin, end: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let sid = origin.session_id.clone();
    let name = h.session_name(&sid);
    let star = h.session_star(&sid);
    let path = if origin.transcript_path.is_empty() { crate::logs::archive::lookup(&sid).map(|(p, _, _)| p).unwrap_or_default() } else { origin.transcript_path.clone() };
    end()?;
    // Its SessionEnd drops Cue's record of it as it exits: that lands before the hosted one is made.
    for _ in 0..20 {
        if h.session_origin(&sid).is_none() {
            break;
        }
        crate::clock::sleep_ms(100);
    }
    // On a machine it stays there, hosted by cue-host there (its conversation is there; `path` is Cue's copy of it).
    let fresh = Origin { session_id: sid.clone(), harness: "claude".into(), cwd: origin.cwd.clone(), transcript_path: path.clone(), hosted: true, machine: origin.machine.clone(), ..Default::default() };
    let moved = h.resume_hosted_as(&fresh)?;
    let recent = if path.is_empty() { Vec::new() } else { transcript::recent_context(&path, 6) };
    let label = if name.is_empty() { model::folder_of(&origin.cwd) } else { name };
    h.resumed(moved, &label, &recent, false);
    if star > 0 {
        h.restore_star(&sid, star);
    }
    Ok(if origin.machine.is_empty() { "Moved to Cue: hosted here now".into() } else { format!("Moved: hosted on {} now", crate::machines::display_name(&origin.machine)) })
}


/// What quitting Cue would end now (hub/quitting.rs): asked before ⌘Q goes through and before an update restarts Cue.
#[tauri::command]
pub(crate) async fn quit_check(hub: State<'_, Arc<Hub>>) -> Result<Vec<crate::hub::EndsWithCue>, String> {
    with_hub(&hub, |h| h.ends_with_cue()).await
}

/// Quit, having been told what it ends (the window's quit dialog: Quit anyway).
#[tauri::command]
pub(crate) async fn quit_now(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

/// cue-host as Settings → Agents shows it: reachable, its build, the sessions it runs, older hosts still finishing.
#[tauri::command]
pub(crate) async fn host_status(hub: State<'_, Arc<Hub>>) -> Result<serde_json::Value, String> {
    with_hub(&hub, |h| h.hosted().host_status()).await
}

/// Restart cue-host (Settings → Agents): every hosted agent ended (each session comes back paused and resumes with
/// your next message), and every host, for one that misbehaves.
#[tauri::command]
pub(crate) async fn host_restart(hub: State<'_, Arc<Hub>>) -> Result<String, String> {
    with_hub(&hub, |h| h.hosted().restart_host()).await?
}
