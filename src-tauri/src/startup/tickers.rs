//! What runs in the background for as long as Cue does: the socket the agents' hooks talk to, the crews
//! read here every 3 s and on your machines on their own schedule, the usage file watched, and a newer
//! Cue looked for.

use crate::hub::Hub;
use crate::ipc::{find_update, offer_update};
use crate::logs::{live, usage_file};
use crate::machines::{self, Machine};
use crate::remote_poll::{self, Schedule};
use crate::{leads, logs, server};
use std::sync::{Arc, Mutex};

/// The socket the agents' hooks and `cue` commands talk to; the window hears if it stops.
pub(super) fn serve_socket(hub: &Arc<Hub>, app: &tauri::AppHandle) {
    let h = hub.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = server::serve(h).await {
            eprintln!("cue: socket server stopped: {e}");
            use tauri::Emitter;
            let _ = handle.emit("server-error", e.to_string());
        }
    });
}

/// Leads and executors (claude-relay, pi-lead): re-read their state folders every few seconds.
pub(super) fn local_crews(hub: &Arc<Hub>) {
    let h = hub.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            tick.tick().await;
            refresh_local(&h).await;
        }
    });
}

/// One local read: crews, live Claude sessions and folders, then the sessions whose agent is gone and the
/// ones Cue doesn't know yet.
async fn refresh_local(h: &Arc<Hub>) {
    // Also every live Claude session, for the Sessions view, and each folder's repository and branch
    // (non-short-circuit |: all run).
    let (others, folders) = tauri::async_runtime::spawn_blocking(|| (leads::refresh() | live::refresh(), logs::repos::refresh())).await.unwrap_or((false, false));
    // Folders' facts came in: the projects the hub keeps derived again (that redraws).
    if folders {
        let f = h.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || f.folders_found()).await;
    } else if others {
        h.redraw();
    }
    // Claude sessions here whose agent is gone (no process Cue can watch: Cue's own terminal, a lost tab).
    let s = h.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || s.sweep("", Some(&live::claude().into_iter().map(|q| q.session_id).collect()))).await;
    // ...and from that list: sessions Cue doesn't know yet, prompts in a terminal it has no card for.
    let a = h.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || a.sync_live()).await;
}

/// Leads and executors on your machines: relay's and pi-lead's listing there (one SSH call),
/// on every machine, whether or not Cue has anything open there: every 10 s where it has a
/// live session, every minute elsewhere, less and less often while one can't be reached
/// (remote_poll). Each machine is read on its own, so one that hangs holds no other up.
pub(super) fn remote_crews(hub: &Arc<Hub>) {
    let h = hub.clone();
    tauri::async_runtime::spawn(async move {
        let sched = Arc::new(Mutex::new(Schedule::default()));
        let mut tick = tokio::time::interval(remote_poll::NEAR);
        loop {
            tick.tick().await;
            let a = h.clone();
            let s = sched.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || read_due_machines(&a, &s)).await;
        }
    });
}

/// The machines whose turn it is, each read on its own thread; crews of machines that were removed dropped.
fn read_due_machines(a: &Arc<Hub>, s: &Arc<Mutex<Schedule>>) {
    let busy: std::collections::HashSet<String> = a.saved_sessions().into_iter().map(|s| s.origin.machine).filter(|m| !m.is_empty()).collect();
    let list = machines::list();
    let names: Vec<String> = list.iter().map(|m| m.name.clone()).collect();
    if leads::retain_remote(&names) {
        a.redraw();
    }
    let marks: Vec<(String, bool)> = list.iter().map(|m| (m.name.clone(), busy.contains(&m.name))).collect();
    let due = crate::locking::lock(s, "remote poll schedule").due(&marks, std::time::Instant::now());
    for m in list.into_iter().filter(|m| due.contains(&m.name)) {
        let (a, s, has_sessions) = (a.clone(), s.clone(), busy.contains(&m.name));
        std::thread::spawn(move || read_machine(&a, &s, &m, has_sessions));
    }
}

/// One machine's crews read, its schedule told whether it answered, and (where Cue has sessions there)
/// the ones that ended while Cue wasn't looking swept.
fn read_machine(a: &Arc<Hub>, s: &Mutex<Schedule>, m: &Machine, has_sessions: bool) {
    let (changed, live) = leads::refresh_remote(&m.name, &m.host);
    let change = crate::locking::lock(s, "remote poll schedule").done(&m.name, live.is_some());
    match change {
        Some(false) => eprintln!("cue: can't reach {} for its crews; trying less often until it answers", m.name),
        Some(true) => eprintln!("cue: reached {} again", m.name),
        None => {}
    }
    // ...and its sessions that ended there while Cue wasn't looking.
    if has_sessions {
        a.sweep(&m.name, live.as_ref());
    }
    if changed {
        a.redraw();
    }
}

/// Your usage file: when it changes, the usage pill shows the new numbers (or what's wrong).
pub(super) fn usage_file(hub: &Arc<Hub>) {
    let h = hub.clone();
    tauri::async_runtime::spawn(async move {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            every.tick().await;
            if usage_file::changed() {
                h.redraw();
            }
        }
    });
}

/// A newer Cue: looked for a minute after launch, then every 6 hours; the window offers it.
pub(super) fn updates(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let mut every = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        loop {
            every.tick().await;
            if let Ok(Some(u)) = find_update(&handle).await {
                offer_update(&handle, &u);
            }
        }
    });
}
