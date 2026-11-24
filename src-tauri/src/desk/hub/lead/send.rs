//! Writing to a Desk's lead: your words after the Desk's summary, sent live, held while it starts or changes,
//! or resumed with it when parked. `DeskStart` is a start or resume of a lead in progress: what came meanwhile
//! goes to it once it's up. Starting, resuming and taking on a lead: lead/mod.rs.

use super::resume;
use crate::hub::{DeskClaim, Hub};
use std::sync::Arc;

/// Your words and the pictures you pasted or dropped to `project`'s lead. Returns { sid }.
#[cfg_attr(not(feature = "ext"), allow(dead_code))]
pub(crate) fn desk_send_images(h: &Arc<Hub>, project: &str, text: &str, images: &[crate::model::Upload]) -> Result<serde_json::Value, String> {
    desk_send_now(h, project, text, images, false)
}

/// The same; `now` (⌘↵ in the lead's chat, as in a session's, #630): sent to a lead at work at once rather than kept
/// in Cue until its turn ends.
pub(crate) fn desk_send_now(h: &Arc<Hub>, project: &str, text: &str, images: &[crate::model::Upload], now: bool) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() && images.is_empty() {
        return Err("nothing to send".into());
    }
    let saved = crate::uploads::save(images)?;
    let lost = |e: String| format!("{}'s lead didn't come up ({e}), so this wasn't sent: {}", h.desk_name(project), text.trim());
    let sid = deliver(h, project, text, saved, now, &lost)?;
    h.desk_yours(&sid, text);
    Ok(serde_json::json!({ "sid": sid }))
}

pub(crate) fn desk_send_with(h: &Arc<Hub>, project: &str, text: &str) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() {
        return Err("nothing to send".into());
    }
    let lost = |e: String| format!("{}'s lead didn't come up ({e}), so this wasn't sent: {}", h.desk_name(project), text.trim());
    let sid = deliver(h, project, text, vec![], false, &lost)?;
    h.desk_yours(&sid, text);
    Ok(serde_json::json!({ "sid": sid }))
}

/// Cue's own words (its Desk changes, desk/hub/lead/feed.rs) to `project`'s lead as `deliver` sends them (resumed
/// with the Desk's summary when parked), not kept as yours: its id.
pub(crate) fn desk_send_cue(h: &Arc<Hub>, project: &str, text: &str) -> Result<String, String> {
    let lost = |e: String| format!("{}'s lead didn't come up ({e})", h.desk_name(project));
    deliver(h, project, text, vec![], false, &lost)
}

/// Your words (`text`) and pictures (`saved`) to `project`'s lead, however it's reached (`desk_claim`), after the
/// Desk's summary as built for that lead: its id. Sent to it live, the pictures go as a session chat's do (shown in its
/// chat); held while it starts or resumed with it, the message names them by where they are, as a hosted session is
/// sent them. `now`: sent live to a lead at work at once, not kept until its turn ends. `lost`: what you're told when it was waited for or resumed and didn't come up.
fn deliver(h: &Arc<Hub>, project: &str, text: &str, saved: Vec<crate::uploads::Saved>, now: bool, lost: &dyn Fn(String) -> String) -> Result<String, String> {
    let named = crate::uploads::with_paths(text, &saved);
    let (sid, failed) = match h.desk_claim(project, &named)? {
        DeskClaim::Send(sid) => {
            let (message, failed) = for_lead(h, project, &sid, text, now);
            h.send_saved(&sid, &message, saved, now)?;
            (sid, failed)
        }
        // Another message is resuming it (or a lead is starting or changing): this one is sent by that call once
        // it's done (with the summary for the lead it reaches), or not, and you're told so.
        DeskClaim::Wait(outcome) => return outcome.recv().unwrap_or_else(|_| Err("it gave up".into())).map_err(lost),
        DeskClaim::Resume(sid) => {
            let (message, failed) = for_lead(h, project, &sid, &named, true);
            (DeskStart::new(h, project).done(resume(h, &sid, &message)).map_err(lost)?, failed)
        }
    };
    // Told now: the failed routes it was told of are cleared.
    h.desk_told(&failed);
    Ok(sid)
}

/// Your words (`text`) as `project`'s lead `sid` is sent them: after the Desk's summary as it sees it (itself left
/// out of the sessions it can route to), whole or the changes since its last (`desk_block`; `going`: it goes now even
/// if the lead is at work). With the failed routes it's told of.
fn for_lead(h: &Arc<Hub>, project: &str, sid: &str, text: &str, going: bool) -> (String, Vec<(String, String)>) {
    let (summary, failed) = h.desk_block(project, sid, text, false, going);
    (desk_message(&summary, text), failed)
}

/// Always exactly: the `<desk …>…</desk>` block, a blank line, then the words. The window strips that leading block
/// from your messages in the lead's chat, so keep this shape.
pub(super) fn desk_message(summary: &str, text: &str) -> String {
    // No block when nothing changed (#820): the words alone.
    if summary.is_empty() {
        return text.trim().to_string();
    }
    format!("{summary}\n\n{}", text.trim())
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
                let (message, failed) = for_lead(self.h, self.project, &sid, &w.text, false);
                self.h.tell(&sid, &message)?;
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
            let sent = deliver(self.h, self.project, &w.text, vec![], false, &|e| e);
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

/// The kept message for session `sid` to send now. For a Desk lead, your words never go joined to a session's words
/// for it (#831): kept together, yours (with the Desk's block) go now, and the sessions' stay kept, after them, for its
/// next turn, a message of their own. Under the session's typing gate (hub/send.rs send_held), so nothing is kept
/// meanwhile.
pub(crate) fn held_yours_first(st: &mut crate::hub::Store, sid: &str) -> Option<crate::model::Exchange> {
    let mut h = st.sessions.held(sid)?;
    if st.desk.lead_of(sid).is_some() {
        if let Some((mine, relays)) = crate::desk::relays_apart(&h.text) {
            st.sessions.held_text_to(sid, format!("{mine}\n\n{relays}"));
            h.text = mine;
        }
    }
    Some(h)
}

/// The kept message for session `sid`, taken back (Esc, Edit). A Desk lead's: the sessions' words for it kept with
/// yours aren't yours to take back (#831): they stay kept; with none of yours, nothing comes back.
pub(crate) fn take_back_yours(st: &mut crate::hub::Store, sid: &str) -> Option<crate::model::Exchange> {
    let mut h = st.sessions.take_held(sid)?;
    if st.desk.lead_of(sid).is_some() {
        if let Some((mine, relays)) = crate::desk::relays_apart(&h.text) {
            st.sessions.hold(sid, &relays, vec![]);
            h.text = mine;
        } else if crate::desk::only_relays(&h.text) {
            st.sessions.hold(sid, &h.text, h.images);
            return None;
        }
    }
    Some(h)
}
