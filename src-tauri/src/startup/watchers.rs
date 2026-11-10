//! At start, what brings the sessions back: settings watched, sessions a reboot killed opened again, waiting
//! cards and unsure messages rechecked, and the agents on your machines watched over SSH.

use crate::hub::Hub;
use crate::ipc::{bring_back_cards, restore_machine, restore_sessions};
use crate::{config, machines};
use std::sync::Arc;

/// Settings watched, then this Mac's sessions back as they were before Cue stopped.
pub(super) fn bring_back(hub: &Arc<Hub>) {
    // Settings kept current in memory (a hand edit of config.json, Login Items in System Settings): a
    // change redraws the window.
    let h = hub.clone();
    config::watch(move || h.redraw());
    // A reboot killed every tmux pane: open each live session again (before the watcher, so a
    // stale pid can't drop it before its new pane is recorded).
    restore_sessions(hub);
    // Waiting sessions always come back with their card (a rebuilt one gets it above; the
    // rest, whose card vanished another way, get it here).
    bring_back_cards(hub);
    // A message Cue was still unsure about when it last ran: its log may say by now, and what's
    // still open is watched again (a restart used to leave its ring on for good).
    hub.recheck_unsettled();
}

/// The agents on your machines: their events watched, their sessions back when a machine is reached, and
/// the logs of sessions already known there copied on again.
pub(super) fn machines(hub: &Arc<Hub>) {
    let h = hub.clone();
    machines::hooks::on_reached(move |m| restore_machine(&h, m));
    // A new session there: the machine read now (its setup screens), not at the next poll.
    let h = hub.clone();
    machines::hooks::on_new_session(move |m| {
        if let Some(mach) = machines::get(m) {
            if machines::listing::refresh(&mach.name, &mach.host).0 {
                h.redraw();
            }
        }
    });
    machines::hooks::watch_all();
    let known: Vec<(String, String)> = hub.snapshot()["sessions"].as_array().into_iter().flatten()
        .filter_map(|s| Some((s["machine"].as_str().filter(|m| !m.is_empty())?.to_string(), s["transcript_path"].as_str()?.to_string())))
        .collect();
    std::thread::spawn(move || machines::logs::resume(&known));
}
