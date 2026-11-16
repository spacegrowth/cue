//! The Desk's commands (desk/, hub/desk.rs): writing to a project's Dispatch, adding an item, what you do to an
//! item, and undoing a decision.

use super::{with_hub, StartOpts};
use crate::hub::{DeskClaim, Hub};
use std::sync::Arc;
use tauri::State;

/// Write to `project`'s Dispatch: started (hosted Claude, in the project's folder) the first time, and each message
/// sent after the Desk's summary. `trust`: you said you trust the folder, when Cue asked (as + New does).
/// Returns { sid }.
#[tauri::command]
pub(crate) async fn desk_send(hub: State<'_, Arc<Hub>>, project: String, text: String, trust: Option<bool>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| desk_send_with(h, &project, &text, trust.unwrap_or(false))).await?
}

pub(crate) fn desk_send_with(h: &Arc<Hub>, project: &str, text: &str, trust: bool) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() {
        return Err("nothing to send".into());
    }
    let (summary, failed) = h.desk_summary(project);
    // Always exactly: the `<desk …>…</desk>` block, a blank line, then your words. The window strips that leading
    // block from your messages in Dispatch's chat, so keep this shape.
    let message = format!("{summary}\n\n{}", text.trim());
    let lost = |e: String| format!("{project}'s Dispatch didn't start ({e}), so this wasn't sent: {}", text.trim());
    let sid = match h.desk_claim(project, &message, &failed) {
        DeskClaim::Send(sid) => {
            h.send_to_session(&sid, &message, &[], false)?;
            sid
        }
        // Another message is starting or resuming it: this one is sent by that call once it's up (which told
        // Dispatch of its failed routes), or not, and you're told so.
        DeskClaim::Wait(outcome) => {
            let sid = outcome.recv().unwrap_or_else(|_| Err("it gave up".into())).map_err(lost)?;
            return Ok(serde_json::json!({ "sid": sid }));
        }
        DeskClaim::Resume(sid) => {
            let claim = DeskStart::new(h, project);
            claim.done(resume(h, project, &sid, &message, trust)).map_err(lost)?
        }
        DeskClaim::Start => {
            let claim = DeskStart::new(h, project);
            claim.done(start(h, project, &message, trust, None)).map_err(lost)?
        }
    };
    h.desk_dispatch_started(project, &sid);
    // Told now: the failed routes it was told of are cleared.
    h.desk_told(&failed);
    Ok(serde_json::json!({ "sid": sid }))
}

/// Parked Dispatch `sid` resumed with `message` (still the project's Dispatch, with its instructions); sent to it
/// instead when it was running again already (Resume then only takes it off Parked). One that won't resume is left
/// parked, and a new one started with `message`. Dispatch's id.
fn resume(h: &Arc<Hub>, project: &str, sid: &str, message: &str, trust: bool) -> Result<String, String> {
    // Its folder, for a new one when it's the project's only session (desk_folder passes over Dispatch's own).
    let folder = h.parked(sid).map(|p| p.session.origin).filter(|o| o.machine.is_empty() && !o.cwd.is_empty()).map(|o| o.cwd);
    match super::resume_parked_session(h, sid, message) {
        Ok(r) if r == super::ALREADY_RUNNING => h.send_to_session(sid, message, &[], false).map(|_| sid.to_string()),
        Ok(_) => Ok(sid.to_string()),
        Err(e) => {
            eprintln!("cue: desk: {project}'s Dispatch didn't resume ({e}): starting a new one");
            start(h, project, message, trust, folder)
        }
    }
}

/// A new Dispatch for `project`, hosted Claude in the project's folder (else in `folder`), started with `message`:
/// its id.
fn start(h: &Arc<Hub>, project: &str, message: &str, trust: bool, folder: Option<String>) -> Result<String, String> {
    let dir = h.desk_folder(project).or(folder).ok_or_else(|| format!("no folder known for {project}: start a session there first"))?;
    let opts = StartOpts { trust, dispatch: project.to_string(), ..Default::default() };
    let started = super::start_hosted(h, crate::hosted::Agent::Claude, &dir, message, &format!("{project} Dispatch"), &opts, &[])?;
    started["session_id"].as_str().filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "it started without a session id".into())
}

/// A start or resume of `project`'s Dispatch that `desk_claim` allowed. The claim is let go once it's over
/// (`done`), or, if it never got there (a panic, an early return), when this is dropped: the messages held
/// meanwhile then hear they weren't sent, and later ones start it again.
pub(crate) struct DeskStart<'a> {
    h: &'a Arc<Hub>,
    project: &'a str,
    open: bool,
}

impl<'a> DeskStart<'a> {
    pub(crate) fn new(h: &'a Arc<Hub>, project: &'a str) -> Self {
        DeskStart { h, project, open: true }
    }

    /// It's over (`up`: Dispatch's id, or why it didn't come up): the messages that came meanwhile are sent to it
    /// now (kept in Cue while it was at work), and each sender hears how that went. `up` handed back.
    pub(crate) fn done(mut self, up: Result<String, String>) -> Result<String, String> {
        self.open = false;
        for w in self.h.desk_claim_done(self.project) {
            let sent = up.clone().and_then(|sid| self.h.send_to_session(&sid, &w.message, &[], false).map(|_| sid));
            if sent.is_ok() {
                self.h.desk_told(&w.failed);
            }
            // Its sender may have gone (the window closed): nothing more to tell then.
            let _ = w.reply.send(sent);
        }
        up
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

/// The "New item" button. Returns the item.
#[tauri::command]
pub(crate) async fn desk_add(hub: State<'_, Arc<Hub>>, project: String, kind: String, title: String, done_when: Option<String>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_add(&project, &kind, &title, done_when.as_deref().unwrap_or(""))).await?
}

/// What you did to an item: "looks_right" | "send_back" | "start_now" | "put_before" | "drop", with `text` for
/// send_back. Returns the item as it is now (state "dropped" once dropped).
#[tauri::command]
pub(crate) async fn desk_item_act(hub: State<'_, Arc<Hub>>, id: String, act: String, text: Option<String>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_item_act(&id, &act, text.as_deref().unwrap_or(""))).await?
}

/// Undo this: whoever made the decision is asked to undo it. What Cue did.
#[tauri::command]
pub(crate) async fn desk_undo(hub: State<'_, Arc<Hub>>, decision_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.desk_undo(&decision_id)).await?
}
