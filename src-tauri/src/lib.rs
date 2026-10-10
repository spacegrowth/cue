mod config;
mod archive;
mod btw;
mod db;
mod dictation;
mod focus;
mod hook;
mod changes;
mod commands;
mod hub;
mod leads;
mod live;
mod model;
mod notify;
// An optional add-on, compiled in only with the `ext` feature: Rust in `src-tauri/ext/` (not part of
// this repo). Without it there's nothing here.
#[cfg(feature = "ext")]
#[path = "../ext/mod.rs"]
mod ext;
mod server;
mod sessions;
mod steps;
mod transcript;
mod usage_file;
mod uploads;
mod usage;
mod tray;
mod term;
mod machines;
mod machine_hooks;
mod machine_logs;
mod shell;

use hub::Hub;
use model::Decision;
use std::sync::Arc;
use tauri::{Manager, RunEvent, State, WindowEvent};

/// Settings → Connect: give an agent Cue's hooks (Claude Code, Codex) or extension (Pi), as
/// install.sh does, for an app downloaded on its own.
#[tauri::command]
async fn connect_agent(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, harness: String) -> Result<String, String> {
    let pi_ext = app.path().resource_dir().map_err(|e| e.to_string())?.join("pi/cue.ts");
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let done = config::connect(&harness, &pi_ext);
        h.redraw(); // Settings shows it connected
        done
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The chat's ⋯ menu: Compact, or a relay lead's Hand off (typed into the session; opens no window).
#[tauri::command]
async fn session_command(hub: State<'_, Arc<Hub>>, session_id: String, action: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.session_command(&session_id, &action)).await.map_err(|e| e.to_string())?
}

/// The "/" menu for a session's box: the commands Claude Code has enabled for it. None while they're
/// being read (ask again shortly); empty for agents without a list.
#[tauri::command]
fn session_commands(hub: State<Arc<Hub>>, session_id: String) -> Option<Vec<commands::Cmd>> {
    let cwd = match hub.session_origin(&session_id) {
        Some(o) if o.harness == "claude" => o.cwd,
        Some(_) => return Some(vec![]),
        None => match live::claude().into_iter().find(|q| q.session_id == session_id) {
            Some(q) => q.cwd,
            None => return Some(vec![]),
        },
    };
    commands::for_session(&cwd)
}

/// Settings rows an add-on adds ([{ group, rows: [{ title, sub, art? }] }]); none without one.
#[tauri::command]
fn ext_settings() -> serde_json::Value {
    #[cfg(feature = "ext")]
    return ext::settings();
    #[cfg(not(feature = "ext"))]
    serde_json::json!([])
}

#[tauri::command]
fn get_state(hub: State<Arc<Hub>>) -> serde_json::Value {
    hub.snapshot()
}

#[tauri::command]
fn respond(hub: State<Arc<Hub>>, id: String, decision: Decision) -> bool {
    hub.respond(&id, decision)
}

#[tauri::command]
fn dismiss(hub: State<Arc<Hub>>, id: String) {
    hub.dismiss(&id)
}

/// × on a session whose machine can't be reached: take it out of Cue.
#[tauri::command]
fn forget_session(hub: State<Arc<Hub>>, session_id: String) {
    hub.forget(&session_id)
}

/// × on a queued review in its lead's chat: take it back.
#[tauri::command]
fn unqueue_review(hub: State<Arc<Hub>>, exec: String) {
    hub.unqueue_review(&exec)
}

#[tauri::command]
fn send_to_back(hub: State<Arc<Hub>>, id: String) -> Result<(), String> {
    hub.send_to_back(&id)
}

#[tauri::command]
fn focus_session(hub: State<Arc<Hub>>, id: String) -> Result<String, String> {
    let it = hub.get(&id).ok_or("no such item")?;
    focus::focus(&it.origin)
}

#[tauri::command]
async fn send_to_session(hub: State<'_, Arc<Hub>>, session_id: String, text: String, images: Option<Vec<model::Upload>>, now: Option<bool>) -> Result<String, String> {
    let h = hub.inner().clone();
    let images = images.unwrap_or_default();
    let now = now.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || h.send_to_session(&session_id, &text, &images, now)).await.map_err(|e| e.to_string())?
}

/// "Send now" on a queued message: stop the current turn so the agent reads it straight away.
#[tauri::command]
async fn send_queued_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.send_queued_now(&session_id)).await.map_err(|e| e.to_string())?
}

/// Stop a working session mid-turn (the Stop button, Esc twice).
#[tauri::command]
async fn interrupt_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.interrupt(&session_id)).await.map_err(|e| e.to_string())?
}

/// Search everything Cue has logged: session names, your messages, the agents' replies (ended sessions too).
#[tauri::command]
fn search(q: String) -> Vec<serde_json::Value> {
    search_all(&q)
}

/// Resume an older Claude Code session: `claude --resume` in a terminal tab opened behind Cue, in its
/// folder. Cue knows it at once (name and last few messages from its transcript), so it opens in Active.
#[tauri::command]
async fn resume_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || resume_older(&h, &session_id)).await.map_err(|e| e.to_string())?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// The work behind Resume (the window's search). Blocking: it opens a terminal tab.
pub(crate) fn resume_older(h: &Arc<Hub>, session_id: &str) -> Result<serde_json::Value, String> {
    let (path, cwd, title) = archive::lookup(session_id).ok_or("can't tell which folder that session ran in")?;
    let line = focus::resume_line(&cwd, session_id)?;
    let name = model::project_of(&cwd);
    let tab = open_for_resume(&line, &cwd, &name)?;
    let origin = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
    h.resumed(origin, &title, &transcript::recent_context(&path, 6), false);
    Ok(serde_json::json!({ "session_id": session_id, "detail": format!("Resumed in {}", tab.what) }))
}

/// Where a resumed session runs on this Mac: its own tmux session (Settings), else a window of Cue's
/// tmux tabs, else a plain terminal tab.
fn open_for_resume(line: &str, cwd: &str, name: &str) -> Result<focus::NewTab, String> {
    if config::tmux_sessions() {
        focus::open_in_tmux(line, cwd, name)
    } else if focus::tmux_installed() {
        focus::open_in_tmux_tab(line, cwd, name)
    } else {
        focus::open_tab_with(line)
    }
}

/// Park: stop a session (its agent ended as Close ends it, its tab closed) and keep it under Parked,
/// to resume with its whole conversation. Claude Code, Codex and Pi; not while it works or asks you
/// something, and not one relay runs.
#[tauri::command]
async fn park_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || park(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn park(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let s = h.session_record(session_id).ok_or("Cue has no record of that session")?;
    if s.origin.cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in, so it couldn't resume it".into());
    }
    // Only one Cue can bring back: checked before anything is ended.
    focus::resume_line_for(&s.origin.harness, &s.origin.cwd, session_id)?;
    // An executor is relay's (or pilead's) to close; a lead with executors still open can't go yet.
    // A lead with none left is parked like any session: `claude --resume` brings the lead back.
    match leads::close_plan(session_id) {
        Some(Err(e)) => return Err(e),
        Some(Ok(_)) => return Err("relay runs it: close it through relay instead".into()),
        None => {}
    }
    h.closable(session_id)?;
    // Its finished turn leaves Waiting as parked: before its end, which would call it "session ended".
    // (Only a finished turn can be here: closable refuses one that asks you something.)
    for id in h.pending().iter().filter(|i| i.origin.session_id == session_id).map(|i| i.id.clone()) {
        h.finish(&id, "gone", "parked");
    }
    end_session(h, session_id)?;
    // Its end (SessionEnd, or its agent's exit) drops Cue's record of it: let that land first, so
    // nothing late from it lands on the parked one.
    for _ in 0..20 {
        if h.session_origin(session_id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    h.add_parked(s);
    Ok("Parked: its agent stopped and its terminal closed".into())
}

/// Resume a parked session: its agent's resume command, in tmux on this Mac or on its machine; its
/// record back as it was (name, star, recent thread), at its prompt. With a message (Resume & send),
/// the message goes on that command line, so the agent starts on it once the conversation is loaded.
#[tauri::command]
async fn resume_parked(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let h = hub.inner().clone();
    let message = message.unwrap_or_default();
    let res = tauri::async_runtime::spawn_blocking(move || resume_parked_session(&h, &session_id, &message)).await.map_err(|e| e.to_string())?;
    // Cue stays in front: a terminal tab may have opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

pub(crate) fn resume_parked_session(h: &Arc<Hub>, session_id: &str, message: &str) -> Result<String, String> {
    let p = h.parked(session_id).ok_or("it isn't parked any more")?;
    // Running again already (you resumed it in a terminal yourself): a second agent would share its
    // conversation. It's live, so it just leaves Parked.
    if p.session.origin.machine.is_empty() && live::claude().iter().any(|q| q.session_id == session_id) {
        h.take_parked(session_id);
        return Ok("It's already running".into());
    }
    resume_record(h, p, message)
}

/// Closed, in the Parked drawer: sessions that ended in the last 30 days (at most 30), newest first,
/// not parked and not running again. To find the ones that got lost (an agent that exited, a tab closed).
#[tauri::command]
fn closed_sessions(hub: State<Arc<Hub>>) -> Vec<serde_json::Value> {
    hub.closed(CLOSED_DAYS * 24 * 3600 * 1000, CLOSED_MAX)
}
const CLOSED_DAYS: u64 = 30;
const CLOSED_MAX: usize = 30;

/// Resume a closed session, as a parked one resumes: its agent's resume command, its record back.
#[tauri::command]
async fn resume_closed(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let h = hub.inner().clone();
    let message = message.unwrap_or_default();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let p = closed_record(&h, &session_id)?;
        resume_record(&h, p, &message)
    }).await.map_err(|e| e.to_string())?;
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// Park a closed session: kept under Parked (nothing to stop, it has ended already).
#[tauri::command]
fn park_closed(hub: State<Arc<Hub>>, session_id: String) -> Result<(), String> {
    let p = closed_record(hub.inner(), &session_id)?;
    hub.add_parked(p.session);
    Ok(())
}

/// A closed session's saved record, if it can be brought back: ended, not live again, an agent Cue resumes.
fn closed_record(h: &Arc<Hub>, session_id: &str) -> Result<crate::sessions::Parked, String> {
    if h.session_origin(session_id).is_some() || live::claude().iter().any(|q| q.session_id == session_id) {
        return Err("it's running again".into());
    }
    let (ended_ms, session) = db::ended_session(session_id).ok_or("Cue has no record of that session")?;
    focus::resume_line_for(&session.origin.harness, &session.origin.cwd, session_id)?;
    Ok(crate::sessions::Parked { parked_ms: ended_ms, session })
}

/// The work behind Resume, for a parked or a closed session: in tmux on this Mac or on its machine.
fn resume_record(h: &Arc<Hub>, p: crate::sessions::Parked, message: &str) -> Result<String, String> {
    let session_id = p.session.origin.session_id.clone();
    let session_id = session_id.as_str();
    let o = p.session.origin.clone();
    let mut line = focus::resume_line_for(&o.harness, &o.cwd, session_id)?;
    if !message.trim().is_empty() {
        line += &format!(" {}", focus::shq(message.trim()));
    }
    let label = if p.session.name.is_empty() { model::project_of(&o.cwd) } else { p.session.name.clone() };
    let base = model::Origin { session_id: session_id.to_string(), harness: o.harness.clone(), transcript_path: o.transcript_path.clone(), ..Default::default() };
    let (fresh, at) = if o.machine.is_empty() {
        let tab = open_for_resume(&line, &o.cwd, &label)?;
        let at = tab.what.clone();
        (model::Origin { cwd: o.cwd.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..base }, at)
    } else {
        let m = machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
        let (pane, tty, dir) = machines::open_in_tmux(&m, &line, &o.cwd, &label)?;
        (model::Origin { cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..base }, format!("tmux on {}", m.name))
    };
    h.unparked(p, fresh, message.trim());
    Ok(format!("Resumed in {at}"))
}

/// Unpark: take it off Parked without resuming it. Its conversation stays in History and search.
#[tauri::command]
fn unpark_session(hub: State<Arc<Hub>>, session_id: String) {
    hub.take_parked(&session_id);
}

/// Would a reboot have left this session without its pane, so Cue rebuilds it? Claude only:
/// it's the only harness with `--resume`. A session without a pane (a plain Terminal one) is left alone.
fn rebuildable(o: &model::Origin) -> bool {
    o.harness == "claude" && !o.tmux_pane.is_empty() && !o.cwd.is_empty()
}

/// Run `f` (blocking) at most `secs`: None when it hasn't finished (its thread keeps going).
fn with_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, secs: u64) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(f()); });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// One session again in tmux, as Resume does: `claude --resume` typed into its new pane,
/// the new pane recorded (its old one died with the reboot). Failures skip it, quietly.
fn rebuild(hub: &Arc<Hub>, o: &model::Origin, label: &str) {
    let Ok(line) = focus::resume_line(&o.cwd, &o.session_id) else { return };
    let Ok(tab) = focus::open_in_tmux(&line, &o.cwd, label) else { return };
    let fresh = model::Origin {
        session_id: o.session_id.clone(),
        harness: "claude".into(),
        cwd: o.cwd.clone(),
        transcript_path: o.transcript_path.clone(),
        term_program: tab.term_program,
        tty: tab.tty,
        tmux_pane: tab.tmux_pane,
        ..Default::default()
    };
    hub.mark_state(&fresh, "waiting");
    // Its card comes back too: the turn it finished is still the turn to answer.
    hub.waiting_card(&fresh);
}

/// Machine sessions, each call time-boxed at 8s. A failure goes in machines.log; a host that can't be
/// reached is skipped (Cue can't tell whether its pane survived) until it's reached again.
fn remote_restores(hub: &Arc<Hub>, sessions: Vec<(String, model::Origin, String)>) {
    for (machine, o, name) in sessions {
        let sid = o.session_id.clone();
        let mn = machine.clone();
        let h = hub.clone();
        let done = with_timeout(move || {
            let Some(m) = machines::get(&machine) else { return };
            // Its agent still runs there (Claude Code's registry: <pid>.json naming the session, that
            // pid alive), in Cue's tmux or your own: leave it, a second one would share its conversation.
            match machines::run(&m.host, &agent_running_script(&o.session_id)) {
                Ok(s) if s.trim() == "GONE" => {}
                Ok(_) => return,
                Err(e) => { machine_hooks::log_raw(&machine, &format!("rebuild {} skipped: {e}", o.session_id)); return; }
            }
            let Ok(line) = focus::resume_line(&o.cwd, &o.session_id) else { return };
            let label = if !name.is_empty() { name.clone() } else { model::project_of(&o.cwd) };
            let (pane, tty, dir) = match machines::open_in_tmux(&m, &line, &o.cwd, &label) {
                Ok(t) => t,
                Err(e) => { machine_hooks::log_raw(&machine, &format!("rebuild {} failed: {e}", o.session_id)); return; }
            };
            let fresh = model::Origin {
                session_id: o.session_id.clone(),
                harness: "claude".into(),
                cwd: dir,
                transcript_path: o.transcript_path.clone(),
                term_program: "tmux".into(),
                tty,
                tmux_pane: pane,
                machine: m.name.clone(),
                ..Default::default()
            };
            h.mark_state(&fresh, "waiting");
            // Its card comes back too: the turn it finished is still the turn to answer.
            h.waiting_card(&fresh);
        }, 8);
        if done.is_none() {
            machine_hooks::log_raw(&mn, &format!("rebuild {sid} timed out"));
        }
    }
}

/// The script that says whether a Claude session's agent runs on a machine: LIVE or GONE.
fn agent_running_script(session_id: &str) -> String {
    format!(
        r#"for f in "${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}"/sessions/*.json; do
  [ -f "$f" ] && grep -qF {sid} "$f" && kill -0 "$(basename "$f" .json)" 2>/dev/null && {{ echo LIVE; exit 0; }}
done
echo GONE"#,
        sid = focus::shq(session_id)
    )
}

/// At launch: a reboot killed every tmux pane, so live sessions' panes are gone. Open each
/// again (its own tmux session, `claude --resume` typed in), like Resume does for one.
/// Skipped in a quiet test instance, or when Settings says sessions don't run in tmux.
fn restore_sessions(hub: &Arc<Hub>) {
    if std::env::var_os("CUE_QUIET").is_some() || !config::tmux_sessions() {
        return;
    }
    // Claude Code's own registry of the sessions running now, read fresh (the watcher hasn't yet).
    live::refresh();
    let running: std::collections::HashSet<String> = live::claude().into_iter().map(|q| q.session_id).collect();
    for s in hub.saved_sessions() {
        // A machine's sessions come back when Cue reaches the machine (restore_machine), not here.
        if !rebuildable(&s.origin) || !s.origin.machine.is_empty() {
            continue;
        }
        let label = if !s.name.is_empty() { s.name.clone() } else { model::project_of(&s.origin.cwd) };
        // Its agent still runs (Cue restarted, the Mac didn't): wherever it runs, in Cue's tmux, your
        // own, or a renamed session, a second `claude --resume` would be two agents on one conversation.
        if running.contains(&s.origin.session_id) || focus::pane_in_its_session(&s.origin.tmux_pane, &label) {
            continue;
        }
        rebuild(hub, &s.origin, &label);
    }
}

/// A machine reached (Cue started, or the machine came back from a reboot or a dropped line): its live
/// sessions whose agent no longer runs there are opened again. One pass per machine at a time.
fn restore_machine(hub: &Arc<Hub>, machine: &str) {
    if std::env::var_os("CUE_QUIET").is_some() || !config::tmux_sessions() {
        return;
    }
    static BUSY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    if BUSY.lock().unwrap().iter().any(|m| m == machine) {
        return;
    }
    let sessions: Vec<_> = hub.saved_sessions().into_iter().filter(|s| s.origin.machine == machine && rebuildable(&s.origin)).map(|s| (s.origin.machine.clone(), s.origin, s.name)).collect();
    if sessions.is_empty() {
        return;
    }
    BUSY.lock().unwrap().push(machine.to_string());
    remote_restores(hub, sessions);
    BUSY.lock().unwrap().retain(|m| m != machine);
}

/// Launch: every session saved as waiting comes back with its card, however it lost it. The card
/// is the turn it finished, and its saved thread still has it. (A rebuild makes one right away;
/// anything else that's waiting with no card gets it here.)
fn bring_back_cards(hub: &Arc<Hub>) {
    if std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    let pending = hub.pending();
    for s in hub.saved_sessions() {
        if s.state != "waiting" {
            continue;
        }
        if pending.iter().any(|i| i.kind == "waiting" && i.origin.session_id == s.origin.session_id) {
            continue;
        }
        // The saved pid is stale at best (it died with whatever ended its agent); the card
        // must not carry it, or the watcher would take it for the session's end.
        let mut o = s.origin.clone();
        o.agent_pid = None;
        hub.waiting_card(&o);
    }
}

/// Search, as the window's: Cue's own sessions and messages, then Claude Code's older sessions by name.
pub(crate) fn search_all(q: &str) -> Vec<serde_json::Value> {
    let mut out = db::search(q, 60);
    out.extend(archive::find(q, &db::known_session_ids(), 20));
    out
}

/// Updates: is a newer Cue out (the latest release on GitHub, signed with Cue's update key)? Always
/// says which version this is.
#[tauri::command]
async fn update_check(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let current = app.package_info().version.to_string();
    Ok(match find_update(&app).await? {
        Some(u) => serde_json::json!({ "current": current, "version": u.version, "notes": u.body }),
        None => serde_json::json!({ "current": current }),
    })
}
/// Download the newer Cue, check its signature, put it in place of this one, and restart into it.
#[tauri::command]
async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    install_update(app).await
}
pub(crate) async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let Some(u) = find_update(&app).await? else { return Err("Cue is already up to date".into()) };
    u.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("Couldn't install the update: {e}"))?;
    app.restart()
}
/// A newer Cue was found (in the background, or from a menu): the menu bar icon's menu offers it and
/// the window shows its card.
fn offer_update(app: &tauri::AppHandle, u: &tauri_plugin_updater::Update) {
    use tauri::Emitter;
    tray::update_item(&format!("Update to Cue {} and Restart", u.version), true);
    let _ = app.emit("update-ready", serde_json::json!({ "version": u.version, "notes": u.body }));
}
/// The menu bar icon's "Check for Updates…": check, and once one is found, install it and restart.
pub(crate) async fn update_from_menu(app: tauri::AppHandle) {
    if tray::update_offered() {
        tray::update_item("Installing the update…", false);
        if let Err(e) = install_update(app).await {
            eprintln!("cue: update: {e}");
            tray::update_item("Couldn't update. Try again", true);
        }
        return;
    }
    tray::update_item("Checking for updates…", false);
    match find_update(&app).await {
        Ok(Some(u)) => offer_update(&app, &u),
        Ok(None) => tray::update_item("Cue is up to date", true),
        Err(_) => tray::update_item("Couldn't check for updates", true),
    }
}
async fn find_update(app: &tauri::AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    use tauri_plugin_updater::UpdaterExt;
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| format!("Couldn't check for updates: {e}"))
}

/// What a live session did, step by step (the chat's step lines), from its transcript. `version`: what
/// the caller has; unchanged, only the version comes back. Claude Code sessions only for now.
#[tauri::command]
async fn session_steps(hub: State<'_, Arc<Hub>>, session_id: String, version: Option<u64>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || steps_for(&h, &session_id, version.unwrap_or(0))).await.map_err(|e| e.to_string())
}
/// Opened steps: each one's full command or path and its output or diff.
#[tauri::command]
async fn step_detail(hub: State<'_, Arc<Hub>>, session_id: String, ids: Vec<String>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || step_detail_for(&h, &session_id, &ids)).await.map_err(|e| e.to_string())
}
/// The log a session's steps come from: a Claude Code session's transcript (a quiet one's found through
/// Claude Code's own list of running sessions), a Codex session's rollout log, or a Pi session's log.
fn steps_path(h: &Hub, session_id: &str) -> Option<String> {
    let origin = h.session_origin(session_id);
    if origin.as_ref().is_some_and(|o| o.harness == "pi") {
        return pi_log(session_id);
    }
    origin
        .filter(|o| (o.harness == "claude" || o.harness == "codex") && !o.transcript_path.is_empty())
        .map(|o| o.transcript_path)
        .or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).and_then(|q| live::claude_transcript(&q.cwd, session_id)))
        .or_else(|| pi_log(session_id))
        .or_else(|| codex_log(session_id))
}

/// A Codex session's rollout log, `~/.codex/sessions/<year>/<month>/<day>/rollout-<time>-<session id>.jsonl`
/// (CODEX_HOME moves ~/.codex), newest days first. Looked up once per session.
fn codex_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.len() < 8 {
        return None;
    }
    if let Some(p) = FOUND.lock().unwrap().get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let root = std::env::var("CODEX_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".codex")).join("sessions");
    let tail = format!("-{session_id}.jsonl");
    let sorted = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
        v.sort();
        v.reverse();
        v
    };
    for year in sorted(&root) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                if let Some(f) = sorted(&day).into_iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail))) {
                    let found = f.to_string_lossy().into_owned();
                    FOUND.lock().unwrap().get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
                    return Some(found);
                }
            }
        }
    }
    None
}

/// A Pi session's log: `<Pi's folder>/sessions/<its folder>/<time>_<session id>.jsonl` (Pi's folder is
/// ~/.pi/agent, or PI_CODING_AGENT_DIR). Looked up once per session.
fn pi_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.is_empty() {
        return None;
    }
    if let Some(p) = FOUND.lock().unwrap().get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let root = std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent")).join("sessions");
    let tail = format!("_{session_id}.jsonl");
    let found = std::fs::read_dir(root).ok()?.filter_map(Result::ok).filter_map(|d| std::fs::read_dir(d.path()).ok()).flatten().filter_map(Result::ok)
        .map(|f| f.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail)))?
        .to_string_lossy().into_owned();
    FOUND.lock().unwrap().get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
    Some(found)
}
pub(crate) fn steps_for(h: &Hub, session_id: &str, version: u64) -> serde_json::Value {
    match steps_path(h, session_id) {
        Some(path) => {
            let mut v = steps::steps(&path, version);
            // Claude Code's cost isn't in its transcript: it's what its status line last said.
            if let (Some(meta), Some(cost)) = (v.get_mut("meta").filter(|m| m.is_object() && m["cost"].is_null()), h.session_cost(session_id)) {
                meta["cost"] = serde_json::json!(cost);
            }
            v
        }
        None => serde_json::json!({ "version": 0, "turns": [] }),
    }
}
pub(crate) fn step_detail_for(h: &Hub, session_id: &str, ids: &[String]) -> serde_json::Value {
    let ids: Vec<String> = ids.iter().take(60).cloned().collect();
    steps_path(h, session_id).map(|path| steps::detail(&path, &ids)).unwrap_or_else(|| serde_json::json!({}))
}

/// A session's whole conversation as Cue logged it (search opens it, scrolled to the message you found).
#[tauri::command]
fn session_log(session_id: String) -> Vec<serde_json::Value> {
    db::session_log(&session_id)
}

/// Earlier than Cue's log goes (a session from before Cue, or resumed): the conversation from the
/// session's Claude Code transcript, the `limit` messages before `before_ms`. Only when you ask for it.
#[tauri::command]
async fn transcript_page(session_id: String, before_ms: u64, limit: Option<usize>) -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || transcript::find_claude(&session_id).map(|p| transcript::conversation_before(&p, before_ms, limit.unwrap_or(30).min(200))).unwrap_or_default())
        .await
        .unwrap_or_default()
}

/// Earlier messages for the chat, a page at a time as you scroll up: the `limit` before `before_ms`.
#[tauri::command]
fn session_log_page(session_id: String, before_ms: u64, limit: Option<usize>) -> Vec<serde_json::Value> {
    db::session_log_page(&session_id, before_ms, limit.unwrap_or(30).min(200))
}

/// Rename a session (Cue, and the agent where it can be told). Typing into a terminal blocks: off the UI thread.
#[tauri::command]
async fn rename_session(hub: State<'_, Arc<Hub>>, session_id: String, name: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.rename(&session_id, &name)).await.map_err(|e| e.to_string())?
}

/// A relay / pi-lead button (Review, Diff, Auto): type the command into the lead, open the page, or run relay.
#[tauri::command]
async fn crew_action(hub: State<'_, Arc<Hub>>, session_id: String, action: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_crew_action(&h, &session_id, &action)).await.map_err(|e| e.to_string())?
}

/// What a relay / pi-lead button does.
pub(crate) fn run_crew_action(h: &Hub, session_id: &str, action: &str) -> Result<String, String> {
    match leads::act(session_id, action)? {
        // One review at a time per lead: refused while it's busy (see Hub::review).
        leads::Act::Send { to, text } if action == "review" => h.review(&to, session_id, &text),
        leads::Act::Send { to, text } => h.send_to_session(&to, &text, &[], false).map(|_| format!("Sent {text}")),
        leads::Act::Run(cmd, done) => cmd.run().map(|_| done),
        // relay writes the page there and prints its path: bring it here and open it.
        leads::Act::RemoteDiff(cmd) => {
            let said = cmd.run()?;
            let path = said.lines().rev().map(str::trim).find(|l| l.ends_with(".html")).ok_or("relay didn't say where it wrote the page")?.to_string();
            let page = leads::Cmd { bin: "cat".into(), args: vec![path.clone()], machine: cmd.machine.clone() }.run()?;
            let dir = server::cue_dir().join("diffs");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "diff.html".into());
            let here = dir.join(format!("{}-{name}", cmd.args.get(1).cloned().unwrap_or_default()));
            std::fs::write(&here, page).map_err(|e| e.to_string())?;
            std::process::Command::new("/usr/bin/open").arg(&here).status().map(|_| "Opened in your browser".into()).map_err(|e| e.to_string())
        }
        leads::Act::Open(page) => std::process::Command::new("open").arg(&page).status().map(|_| "Opened in your browser".into()).map_err(|e| e.to_string()),
    }
}

/// The mic button: start or stop dictating (words arrive as "dictation" events).
#[tauri::command]
fn dictate_start(app: tauri::AppHandle, d: State<dictation::Dictation>) -> Result<(), String> {
    dictation::start(app, &d)
}
#[tauri::command]
fn dictate_stop(d: State<dictation::Dictation>) {
    dictation::stop(&d)
}

/// A link in a message: open it in your browser (web links); a document (PDF, Markdown, text, an
/// image…) or a plain folder opens in its usual app; anything else is only shown in Finder (`open -R`):
/// agent text can name an app or a script, and a click mustn't run it. (Paths: file:// links, and bare
/// absolute paths on ⌘-click in the window.)
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    let target = link_target(&url)?;
    let mut open = std::process::Command::new("/usr/bin/open");
    if !is_web(&target) && !safe_to_open(std::path::Path::new(&target)) {
        open.arg("-R");
    }
    open.arg(&target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Documents that open in an app without running anything. Not on the list (a script, an app, an
/// installer, a .command, a macro-enabled .docm, a web page, …): shown in Finder only.
const OPENABLE: &[&str] = &[
    "pdf", "md", "markdown", "txt", "log", "csv", "tsv", "json", "yaml", "yml", "toml", "rtf",
    "png", "jpg", "jpeg", "gif", "webp", "heic", "tiff", "bmp",
    "mp4", "mov", "m4v", "mp3", "m4a", "wav",
    "docx", "xlsx", "pptx", "pages", "numbers", "key",
];

/// Whether a path opens rather than only showing in Finder: a document on OPENABLE, or a plain folder
/// (one with no extension: .app and other bundles are folders too). The real file decides, not a
/// link's name (a "notes.pdf" link to a script is shown, not opened).
fn safe_to_open(p: &std::path::Path) -> bool {
    let Ok(real) = std::fs::canonicalize(p) else { return false };
    let ext = real.extension().map(|e| e.to_string_lossy().to_lowercase());
    if real.is_dir() {
        return ext.is_none();
    }
    ext.is_some_and(|e| OPENABLE.contains(&e.as_str()))
}

/// What `open` gets: web links as-is; everything else is a path — file:// stripped, ~ expanded,
/// and it must exist (so a dead path says so in Cue instead of Finder beeping).
fn link_target(url: &str) -> Result<String, String> {
    if is_web(url) {
        return Ok(url.to_string());
    }
    let mut p = match url.strip_prefix("file://") {
        Some(rest) => percent_decoded(rest),
        None => url.to_string(),
    };
    if let Some(rest) = p.strip_prefix('~') {
        let home = std::env::var("HOME").map_err(|_| "no home folder".to_string())?;
        p = format!("{home}{rest}");
    }
    if !p.starts_with('/') {
        return Err("not a link".into());
    }
    if !std::path::Path::new(&p).exists() {
        return Err("nothing at that path".into());
    }
    Ok(p)
}

/// A file:// link's path as the disk has it: "%20" a space, and so on.
fn percent_decoded(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An image a message mentions by path (a screenshot the agent made), as a data: URL. The window
/// can only load files from uploads/ itself, so this hands it image files, and nothing else, by path.
#[tauri::command]
fn image_data(path: String) -> Result<String, String> {
    use base64::Engine;
    let p = std::path::Path::new(&path);
    let mime = match uploads::mime_of(&path) {
        "" => return Err("not an image".into()),
        m => m,
    };
    if !p.is_absolute() || std::fs::metadata(p).map_err(|e| e.to_string())?.len() > 20 * 1024 * 1024 {
        return Err("not a readable image".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Open a session Cue hasn't heard from yet in Cue: its chat and message box, not only its tab.
#[tauri::command]
async fn adopt_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.adopt(&session_id)).await.map_err(|e| e.to_string())?
}

/// The message Cue kept for a session while it worked, back into your box (Esc, Edit): its text and images.
#[tauri::command]
fn unhold_message(hub: State<Arc<Hub>>, session_id: String) -> Option<model::Exchange> {
    hub.unhold(&session_id)
}

/// Send the kept message now: stops the turn first (as Send now on a queued one).
#[tauri::command]
async fn send_held_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.send_held(&session_id, true)).await.map_err(|e| e.to_string())?
}

/// "Go to tab" in the Sessions view: also for a session Cue hasn't heard from (quiet, or Pi that only connected).
#[tauri::command]
async fn focus_live(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || focus::focus(&h.live_origin(&session_id).ok_or("that session is gone")?)).await.map_err(|e| e.to_string())?
}

/// Close a session from the Sessions view: an executor through relay / pilead (they mark it closed and
/// close its tab); anything else by ending its agent (SIGTERM: the conversation is saved, `--resume`
/// brings it back; on a machine, one SIGTERM to everything in its tmux pane there) and closing its tab.
/// Never while it works or asks you something.
#[tauri::command]
async fn close_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || end_session(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// The work behind Close. Blocking: it ends a process and closes a tab.
pub(crate) fn end_session(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let session_id = session_id.to_string();
    {
        h.closable(&session_id)?;
        // Its machine can't be reached: nothing to end there now; it's taken out of Cue.
        if h.unreachable(&session_id) {
            h.forget(&session_id);
            return Ok("Removed from Cue (its machine can't be reached)".into());
        }
        // An executor: relay / pi-lead close it, where it runs (on its machine, over SSH).
        if let Some(plan) = leads::close_plan(&session_id) {
            return plan?.run().map(|_| "Closed".into());
        }
        let origin = h.live_origin(&session_id).ok_or("that session is gone")?;
        // On this Mac, the agent's process. On a machine, there is no pid here (it runs there): its
        // pane is ended on the machine instead.
        if origin.machine.is_empty() {
            let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
            focus::end_agent(pid)?;
        } else {
            focus::end_machine_agent(&origin)?;
        }
        Ok(match focus::close_tab(&origin) {
            Ok(t) => format!("Closed its {t}"),
            // Many tabs close by themselves when the agent they run exits (relay launches its tabs so).
            Err(e) if e.contains("is gone") || e.contains("couldn't find") => "Ended it; its tab closed with it".into(),
            Err(e) => format!("Ended it ({e})"),
        })
    }
}

/// "+ New session": open a terminal tab in `cwd`, in the background, and start `agent` there with an
/// optional first message and name. Cue picks the session id, so it knows the session at once (it
/// opens in Active, ready to type into). Returns the id.
#[tauri::command]
async fn new_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, agent: String, cwd: String, message: String, name: String, tmux: Option<bool>, perm: Option<String>, extra: Option<String>, machine: Option<String>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let opts = StartOpts { tmux, perm: perm.unwrap_or_default(), extra: extra.unwrap_or_default(), machine: machine.unwrap_or_default() };
    let res = tauri::async_runtime::spawn_blocking(move || start_session_with(&h, &agent, &cwd, &message, &name, &opts))
        .await
        .map_err(|e| e.to_string())?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// The work behind "+ New session": open a terminal tab in `cwd`
/// and start `agent` there. Blocking: it types into a terminal.
pub(crate) fn start_session(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str) -> Result<serde_json::Value, String> {
    start_session_with(h, agent, cwd, message, name, &StartOpts::default())
}

/// What + New session can choose beyond the folder: where it runs (None: Settings' default), how much it
/// may do without asking, and any extra flags.
#[derive(Default)]
pub(crate) struct StartOpts {
    pub tmux: Option<bool>,
    pub perm: String,
    pub extra: String,
    /// A machine from + New → Machine to run it on (empty: this Mac). Always in tmux there.
    pub machine: String,
}

pub(crate) fn start_session_with(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    if !opts.machine.is_empty() {
        return start_on_machine(h, agent, cwd, message, name, opts);
    }
    let dir = cwd.trim();
    let dir = if let Some(rest) = dir.strip_prefix("~") { format!("{}{rest}", std::env::var("HOME").unwrap_or_default()) } else { dir.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    let asks_trust = agent == "claude" && !focus::claude_trusts(&dir);
    let line = focus::start_line_with(agent, &dir, &sid, message, name, &opts.perm, &opts.extra)?;
    let in_tmux = opts.tmux.map_or_else(config::tmux_sessions, |t| t && focus::tmux_installed());
    let label = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| model::project_of(&dir));
    // In tmux either way when it's installed: "Runs in iTerm" only adds a terminal tab showing it.
    let tab = if in_tmux {
        focus::open_in_tmux(&line, &dir, &label)?
    } else if focus::tmux_installed() {
        focus::open_in_tmux_tab(&line, &dir, &label)?
    } else {
        focus::open_tab_with(&line)?
    };
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
        h.started(origin, message, asks_trust);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} in {}", agent, tab.what) }))
}

/// + New session on a machine: in tmux there (Cue's terminal shows it, over SSH), in `cwd` there.
fn start_on_machine(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    let m = machines::get(&opts.machine).ok_or(format!("{} isn't one of your machines (+ New → Machine)", opts.machine))?;
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    // It starts where tmux opens the window (`cwd`): the line itself stays in that folder.
    let line = focus::start_line_with(agent, ".", &sid, message, name, &opts.perm, &opts.extra)?;
    let label = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| model::project_of(cwd.trim()));
    let (pane, tty, dir) = machines::open_in_tmux(&m, &line, cwd, &label)?;
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..Default::default() };
        h.started(origin, message, false);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} on {}", agent, m.name) }))
}

/// Trust in Cue for a session Claude Code is asking "do you trust this folder?": presses Enter in its
/// terminal, which picks the question's first choice, "Yes, I trust this folder". Only from your click.
#[tauri::command]
async fn trust_folder(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || focus::press_enter(&origin)).await.map_err(|e| e.to_string())?
}

/// "!" in a box: run the rest of the line in the session's folder (or `cwd`, for the + New session box),
/// in your shell — on this Mac, or on the machine the session runs on (the folder is there). What it
/// printed, its exit code and how long it took: shown as a card, never sent to the agent unless you
/// choose to.
#[tauri::command]
async fn run_line(hub: State<'_, Arc<Hub>>, session_id: Option<String>, cwd: Option<String>, machine: Option<String>, line: String) -> Result<shell::Ran, String> {
    let (dir, machine) = match session_id.filter(|s| !s.is_empty()) {
        Some(sid) => {
            let o = hub.live_origin(&sid).ok_or("Cue doesn't know that session's folder")?;
            let dir = o.cwd.trim().to_string();
            if dir.is_empty() {
                return Err("Cue doesn't know that session's folder".into());
            }
            (dir, o.machine)
        }
        None => {
            let c = cwd.unwrap_or_default().trim().to_string();
            if c.is_empty() {
                return Err("pick a folder first".into());
            }
            let m = machine.unwrap_or_default();
            // On this Mac a `~` becomes your home here; on a machine it stays, for the shell there.
            let c = if m.is_empty() {
                match c.strip_prefix('~') { Some(rest) => format!("{}{rest}", std::env::var("HOME").unwrap_or_default()), None => c }
            } else {
                c
            };
            (c, m)
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        if machine.is_empty() {
            shell::run(&dir, &line)
        } else {
            let m = machines::get(&machine).ok_or(format!("{machine} isn't one of your machines any more (+ New → Machine)"))?;
            shell::run_on_machine(&m, &dir, &line)
        }
    }).await.map_err(|e| e.to_string())?
}

/// "Move to Cue": a Claude Code session in a terminal tab, ended (its tab closed with it) and resumed in
/// Cue's tmux session, where its terminal shows in Cue. Idle sessions only: the conversation comes back
/// whole (claude --resume), but anything mid-turn would be cut off. Shell state in the old tab is gone.
#[tauri::command]
async fn move_to_cue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || move_session_to_cue(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn move_session_to_cue(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    if !focus::tmux_installed() {
        return Err("tmux isn't installed".into());
    }
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    if origin.harness != "claude" {
        return Err("only a Claude Code session can be resumed in Cue".into());
    }
    if !origin.tmux_pane.is_empty() {
        return Err("it already runs in Cue".into());
    }
    if leads::close_plan(session_id).is_some() {
        return Err("a relay lead: close it through relay, then resume it in Cue".into());
    }
    let cwd = origin.cwd.clone();
    if cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in".into());
    }
    let name = h.session_name(session_id);
    let star = h.session_star(session_id);   // its record goes with the old process: the star comes back on the new one
    let path = if origin.transcript_path.is_empty() { archive::lookup(session_id).map(|(p, _, _)| p).unwrap_or_default() } else { origin.transcript_path.clone() };
    let label = if name.is_empty() { model::project_of(&cwd) } else { name };
    let line = focus::resume_line(&cwd, session_id)?;
    // Its window in Cue first, empty: only once that's there does the old one end, so a tmux that won't
    // cooperate never leaves the session ended and nowhere.
    let mut tab = focus::open_in_tmux("", &cwd, &label)?;
    // Ended as Close does (SIGTERM, its tab closed). One that doesn't take the signal within 3 s is asked
    // to leave the way you would, with /exit typed into it, and given a few seconds more.
    if let Err(e) = end_session(h, session_id) {
        if !e.contains("didn't quit") {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err(e);
        }
        let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
        if let Err(e) = focus::type_into(&origin, "/exit") {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err(e);
        }
        let alive = || pid > 0 && unsafe { libc::kill(pid, 0) == 0 };
        for _ in 0..60 {
            if !alive() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if alive() {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err("it didn't quit, even to /exit: close it in its tab, then Resume it here".into());
        }
        let _ = focus::close_tab(&origin);
    }
    // Its SessionEnd hook drops Cue's record of it as it exits: let that land before the new one is made.
    for _ in 0..20 {
        if h.session_origin(session_id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // It has ended: from here it always comes back somewhere. In Cue's window if it can be typed into,
    // else in a terminal tab, as Resume would.
    let mut fell_back = None;
    if let Err(e) = focus::tmux_type_line(&tab.tmux_pane, &line) {
        focus::tmux_kill_pane(&tab.tmux_pane);
        tab = focus::open_tab_with(&line)?;
        fell_back = Some(e);
    }
    let recent = if path.is_empty() { Vec::new() } else { transcript::recent_context(&path, 6) };
    let moved = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path, term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane.clone(), ..Default::default() };
    // In the window at once, as working (loading), so its screen can be watched while it comes up.
    h.resumed(moved.clone(), &label, &recent, true);
    if star > 0 {
        h.restore_star(session_id, star);
    }
    if let Some(e) = fell_back {
        return Ok(format!("Couldn't open it in Cue ({e}), so it was resumed in {} instead", tab.what));
    }
    // Until Claude Code is up in it with the conversation loaded: its prompt (or a question of its own)
    // on the screen. Up to 60 s (a long conversation takes a while); then it's left to finish on its own.
    for _ in 0..300 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if focus::tmux_screen(&tab.tmux_pane).map(|sc| sc.contains('❯')).unwrap_or(false) {
            break;
        }
    }
    h.mark_state(&moved, "waiting");
    Ok("Moved to Cue: resumed in its terminal here".into())
}

/// "By the way": a side question about a Claude Code session, answered in a panel in Cue. The session
/// never sees it (Cue asks a copy of its conversation).
#[tauri::command]
async fn btw(hub: State<'_, Arc<Hub>>, session_id: String, question: String) -> Result<String, String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || btw::ask_about(&origin, &question)).await.map_err(|e| e.to_string())?
}

/// "Later" on a session: put it off (its finished turns wait under Need to decide), or back.
#[tauri::command]
fn set_later(hub: State<Arc<Hub>>, session_id: String, on: bool) {
    hub.set_later(&session_id, on);
}

/// What a session's terminal shows, for a session in tmux: Cue's view of a prompt it has no buttons for.
#[tauri::command]
async fn session_screen(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || focus::session_screen(&o)).await.map_err(|e| e.to_string())?
}

/// Keys pressed in that view (↑ ↓ Enter Esc 1 2 3…). Only from your click.
#[tauri::command]
async fn session_keys(hub: State<'_, Arc<Hub>>, session_id: String, keys: Vec<String>) -> Result<(), String> {
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || focus::tmux_keys(&o, &keys)).await.map_err(|e| e.to_string())?
}

/// Where a session is, for the commands that only work in tmux (on this Mac or a machine).
fn tmux_origin_of(hub: &Hub, session_id: &str) -> Result<model::Origin, String> {
    let o = hub.session_origin(session_id).ok_or("that session is gone")?;
    if o.tmux_pane.is_empty() {
        return Err("that session isn't in tmux".into());
    }
    Ok(o)
}

/// The built-in terminal (sessions in tmux): open it at its size; its output comes as "term" events.
#[tauri::command]
async fn term_open(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, cols: u16, rows: u16) -> Result<u64, String> {
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || term::open(app, &session_id, &o, cols, rows)).await.map_err(|e| e.to_string())?
}

/// What you type in the built-in terminal.
#[tauri::command]
fn term_write(session_id: String, data: String) -> Result<(), String> {
    term::write(&session_id, &data)
}

#[tauri::command]
fn term_resize(session_id: String, cols: u16, rows: u16) -> Result<(), String> {
    term::resize(&session_id, cols, rows)
}

/// Close it (the session keeps running).
#[tauri::command]
fn term_close(session_id: String) {
    term::close(&session_id);
}

/// + New → Machine: the machines you added, to run sessions on over SSH.
#[tauri::command]
fn machines_list() -> Vec<machines::Machine> {
    machines::list()
}

#[tauri::command]
fn machine_add(host: String, name: String) -> Result<machines::Machine, String> {
    let m = machines::add(&host, &name)?;
    machine_hooks::watch(&m);
    Ok(m)
}

#[tauri::command]
fn machine_rename(name: String, label: String) -> Result<machines::Machine, String> {
    machines::rename(&name, &label)
}

#[tauri::command]
fn machine_remove(name: String) -> Result<(), String> {
    machine_hooks::unwatch(&name);
    machines::remove(&name)
}

/// Set up Cue's hooks there, for the agents its check found (never installing any): what it did.
#[tauri::command]
async fn machine_setup(name: String, tools: serde_json::Value) -> Result<Vec<String>, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machine_hooks::setup(&m, &tools)).await.map_err(|e| e.to_string())?
}

/// Can Cue reach it, and what's installed there (it connects, so it takes a moment).
#[tauri::command]
async fn machine_check(name: String) -> Result<serde_json::Value, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machines::check(&m)).await.map_err(|e| e.to_string())
}

/// Log in yourself, in a terminal tab; Cue uses the connection you open.
#[tauri::command]
async fn machine_login(name: String) -> Result<String, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machines::login(&m)).await.map_err(|e| e.to_string())?
}

/// Star a session (you're following it), or unstar it.
#[tauri::command]
fn set_starred(hub: State<Arc<Hub>>, session_id: String, on: bool) {
    hub.set_starred(&session_id, on);
}

/// The agents "+ New session" can offer (installed on this Mac).
#[tauri::command]
async fn agents_installed() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(focus::agents_installed).await.unwrap_or_default()
}

/// "Go to tab" for a session that has no card (a working one).
#[tauri::command]
fn focus_session_id(hub: State<Arc<Hub>>, session_id: String) -> Result<String, String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    focus::focus(&origin)
}

#[tauri::command]
async fn reply(hub: State<'_, Arc<Hub>>, id: String, text: String, images: Option<Vec<model::Upload>>) -> Result<String, String> {
    let h = hub.inner().clone();
    let images = images.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || h.reply(&id, &text, &images)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_setting(hub: State<Arc<Hub>>, key: String, value: serde_json::Value) -> Result<(), String> {
    hub.set_setting(&key, value)
}

/// Paste fallback: the clipboard's image as a data: URL, if there is one.
#[tauri::command]
async fn clipboard_image() -> Option<String> {
    tauri::async_runtime::spawn_blocking(uploads::clipboard_image).await.ok().flatten()
}

/// Settings → "Send a test notification": the same path a real alert takes, minus the agent.
#[tauri::command]
fn test_notification(app: tauri::AppHandle) -> String {
    use tauri_plugin_notification::NotificationExt;
    if notify::post("cue-test", "Cue test", "If you can read this, macOS notifications work.", "") {
        "sent: check the top right of your screen".into()
    } else {
        let _ = app.notification().builder().title("Cue test").body("Fallback notification (unbundled build).").show();
        "sent (fallback: this build isn't the installed app)".into()
    }
}

/// Unsent drafts, per session (saved in cue.db so a restart keeps them).
#[tauri::command]
fn get_drafts() -> serde_json::Value {
    db::drafts()
}

#[tauri::command]
fn set_draft(key: String, text: String, images: serde_json::Value) {
    db::set_draft(&key, &text, &images)
}

/// Bring up the full window, optionally on one item (the menu bar, a notification).
pub(crate) fn show_main(app: &tauri::AppHandle, id: Option<String>) {
    use tauri::Emitter;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    activate_app();
    if let Some(id) = id {
        let _ = app.emit_to("main", "select", id);
    }
}

/// Bring Cue in front of the app you're in. macOS refuses in-process activation for a click on a
/// menu bar item ("cooperative activation"); opening our own bundle via LaunchServices is honoured
/// (measured in a Tauri menu bar app). A bare dev binary has no bundle: it falls back
/// to the in-process call, which works only once Cue is already active.
fn activate_app() {
    let bundle = std::env::current_exe().ok().and_then(|exe| exe.ancestors().nth(3).map(|p| p.to_path_buf())).filter(|p| p.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(b) => {
            let _ = std::process::Command::new("open").arg(&b).status();
        }
        None => {}
    }
}

/// The agents' hook (see hook.rs): runs and returns; the caller exits.
pub fn hook_main(args: &[String]) {
    hook::main(args)
}

/// `cue connect <claude|codex|pi>…` from a terminal: the same as Settings → Connect. Prints what it
/// did; exit code 1 if any failed. Pi's extension comes from the app bundle, else this checkout.
pub fn connect_main(agents: &[String]) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let bundled = exe.parent().map(|d| d.join("../Resources/pi/cue.ts")).filter(|p| p.exists());
    // A development build run from this checkout uses the checkout's copy; a release build only its bundle
    // (so the build machine's paths never end up in a released app).
    #[cfg(debug_assertions)]
    let pi_ext = bundled.unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi/cue.ts")));
    #[cfg(not(debug_assertions))]
    let pi_ext = bundled.unwrap_or_else(|| exe.with_file_name("pi/cue.ts"));
    if agents.is_empty() {
        eprintln!("usage: cue connect <claude|codex|pi>…");
        return 1;
    }
    let mut code = 0;
    for a in agents {
        match config::connect(a, &pi_ext) {
            Ok(m) => println!("{m}"),
            Err(e) => {
                eprintln!("{a}: {e}");
                code = 1;
            }
        }
    }
    code
}

/// Whether these locale settings (LC_ALL, LC_CTYPE, LANG, in the order that wins) say UTF-8.
fn utf8_locale(get: impl Fn(&str) -> Option<String>) -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|k| get(k).filter(|v| !v.is_empty())).is_some_and(|v| {
        let v = v.to_ascii_uppercase();
        v.contains("UTF-8") || v.contains("UTF8")
    })
}

/// A setting Cue drops from its own environment at start, so tmux and the sessions it opens don't inherit
/// it: the marks of the Claude Code session Cue may have been opened from (`CLAUDE_CODE_*`, `CLAUDECODE`),
/// and, when that shell pointed Claude Code at another provider (`ANTHROPIC_BASE_URL` set), all of that
/// setup (`ANTHROPIC_*`: its key must not go to Anthropic either). Your own `ANTHROPIC_API_KEY` alone stays.
fn inherited_agent_var(k: &str, other_provider: bool) -> bool {
    (other_provider && k.starts_with("ANTHROPIC_")) || k.starts_with("CLAUDE_CODE_") || k == "CLAUDECODE"
}

pub fn run() {
    // Opened from the Finder or the Dock, an app gets no locale, and tmux without UTF-8 rewrites what it
    // prints (a tab in its answers as "_", "❯" as "_" in the terminal it draws). Cue and everything it
    // starts (tmux, agents, shells) get UTF-8, as they would from a terminal. Set before any thread starts.
    if !utf8_locale(|k| std::env::var(k).ok()) {
        std::env::set_var("LC_CTYPE", "UTF-8");
    }
    // Opened from a shell set up for another provider, or from inside a Claude Code session, Cue would hand
    // that setup to tmux, and tmux to every session it opens. Cue starts them clean, as a new terminal would.
    let other_provider = std::env::var_os("ANTHROPIC_BASE_URL").is_some_and(|v| !v.is_empty());
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(|k| inherited_agent_var(k, other_provider)) {
            std::env::remove_var(&k);
        }
    }
    let app = tauri::Builder::default()
        .manage(dictation::Dictation::default())
        .plugin(tauri_plugin_notification::init())
        // Cue reopens at the size and place you left it.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![move_to_cue, park_session, resume_parked, unpark_session, closed_sessions, resume_closed, park_closed, machines_list, machine_add, machine_remove, machine_rename, machine_check, machine_login, machine_setup, run_line, unhold_message, send_held_now, term_open, term_write, term_resize, term_close, session_screen, session_keys, adopt_session, session_command, session_commands, set_later, set_starred, btw, connect_agent, update_check, update_install, session_steps, step_detail, send_queued_now, ext_settings, crew_action, focus_live, close_session, new_session, trust_folder, agents_installed, search, session_log, session_log_page, transcript_page, resume_session, rename_session, dictate_start, dictate_stop, interrupt_session, open_link, image_data, get_state, respond, dismiss, send_to_back, unqueue_review, forget_session, focus_session, focus_session_id, send_to_session, reply, clipboard_image, set_setting, get_drafts, set_draft, test_notification])
        .setup(|app| {
            // One-time move from ~/.cue to Application Support (skipped when CUE_HOME is set).
            if std::env::var_os("CUE_HOME").is_none() {
                let old = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cue");
                if let Some(old) = server::move_from_old_folder(&old, &server::cue_dir()) {
                    db::rewrite_paths(&format!("{old}/"), &format!("{}/", server::cue_dir().display()));
                }
            }
            // Point the hook shim at this copy of Cue (wherever the app lives now).
            hook::write_shim();
            // Connected with an older Cue: add the hooks this one needs (a test instance leaves yours alone).
            if std::env::var_os("CUE_QUIET").is_none() && std::env::var_os("CUE_HOME").is_none() {
                config::refresh_hooks();
            }
            let hub = Arc::new(Hub::new(Some(app.handle().clone())));
            app.manage(hub.clone());
            // A reboot killed every tmux pane: open each live session again (before the watcher, so a
            // stale pid can't drop it before its new pane is recorded).
            restore_sessions(&hub);
            // Waiting sessions always come back with their card (a rebuilt one gets it above; the
            // rest, whose card vanished another way, get it here).
            bring_back_cards(&hub);
            // Events from the agents on your machines (+ New → Machine), over SSH, and their sessions' logs.
            if std::env::var_os("CUE_HOME").is_none() {
                let h = hub.clone();
                machine_hooks::on_reached(move |m| restore_machine(&h, m));
                machine_hooks::watch_all();
                let known: Vec<(String, String)> = hub.snapshot()["sessions"].as_array().into_iter().flatten()
                    .filter_map(|s| Some((s["machine"].as_str().filter(|m| !m.is_empty())?.to_string(), s["transcript_path"].as_str()?.to_string())))
                    .collect();
                std::thread::spawn(move || machine_logs::resume(&known));
            }
            let h = hub.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server::serve(h).await {
                    eprintln!("cue: socket server stopped: {e}");
                    use tauri::Emitter;
                    let _ = handle.emit("server-error", e.to_string());
                }
            });
            // Leads and executors (claude-relay, pi-lead): re-read their state folders every few seconds.
            let h = hub.clone();
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(3));
                loop {
                    tick.tick().await;
                    // Also every live Claude session, for the Sessions view (non-short-circuit |: both run).
                    if tauri::async_runtime::spawn_blocking(|| leads::refresh() | live::refresh()).await.unwrap_or(false) {
                        h.redraw();
                    }
                    // Claude sessions here whose agent is gone (no process Cue can watch: Cue's own terminal, a lost tab).
                    let s = h.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || s.sweep("", Some(&live::claude().into_iter().map(|q| q.session_id).collect()))).await;
                    // ...and from that list: sessions Cue doesn't know yet, prompts in a terminal it has no card for.
                    let a = h.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || a.sync_live()).await;
                }
            });
            // Leads and executors on your machines: relay's listing there, every 10 s, for each machine
            // Cue has sessions on (a machine with none isn't woken; its crews are forgotten).
            let h = hub.clone();
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tick.tick().await;
                    let a = h.clone();
                    let changed = tauri::async_runtime::spawn_blocking(move || {
                        let busy: std::collections::HashSet<String> = a.saved_sessions().into_iter().map(|s| s.origin.machine).filter(|m| !m.is_empty()).collect();
                        let mut changed = false;
                        for m in machines::list() {
                            if busy.contains(&m.name) {
                                let (c, live) = leads::refresh_remote(&m.name, &m.host);
                                changed |= c;
                                // ...and its sessions that ended there while Cue wasn't looking.
                                a.sweep(&m.name, live.as_ref());
                            } else {
                                changed |= leads::forget_remote(&m.name);
                            }
                        }
                        changed
                    })
                    .await
                    .unwrap_or(false);
                    if changed {
                        h.redraw();
                    }
                }
            });
            #[cfg(feature = "ext")]
            ext::start(app, hub.clone());
            let waiting = hub.clone();
            tauri::async_runtime::spawn(server::watch(hub));
            if std::env::var_os("CUE_QUIET").is_none() {
                notify::init(app.handle());
                archive::start(); // older Claude Code sessions, indexed by name in the background
                tray::init(app.handle())?;
            }
            // What was waiting before the restart: the icon and the badge show it now.
            waiting.show_waiting();
            // Your usage file: when it changes, the usage pill shows the new numbers (or what's wrong).
            let h = waiting.clone();
            tauri::async_runtime::spawn(async move {
                let mut every = tokio::time::interval(std::time::Duration::from_secs(3));
                loop {
                    every.tick().await;
                    if usage_file::changed() {
                        h.redraw();
                    }
                }
            });
            // A newer Cue: looked for a minute after launch, then every 6 hours; the window offers it.
            if std::env::var_os("CUE_QUIET").is_none() {
                let handle = app.handle().clone();
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
            // A quiet test instance keeps its windows out of your way.
            if std::env::var_os("CUE_QUIET").is_some() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Cue running (it must stay up to receive requests).
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Cue");

    app.run(|handle, event| {
        if let RunEvent::Reopen { .. } = event {
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_providers_setup_and_a_parent_claude_session_are_dropped() {
        for k in ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL", "CLAUDE_CODE_SESSION_ID", "CLAUDECODE"] {
            assert!(inherited_agent_var(k, true), "{k}");
        }
        for k in ["PATH", "HOME", "LC_CTYPE", "CUE_DRIVEN_BY", "CLAUDE_CONFIG_DIR", "ANTHROPIC"] {
            assert!(!inherited_agent_var(k, true), "{k} stays");
        }
        // No other provider: your own key and settings stay; a parent session's marks still go.
        assert!(!inherited_agent_var("ANTHROPIC_API_KEY", false) && !inherited_agent_var("ANTHROPIC_MODEL", false));
        assert!(inherited_agent_var("CLAUDECODE", false) && inherited_agent_var("CLAUDE_CODE_SESSION_ID", false));
    }

    #[test]
    fn a_utf8_locale_is_recognised_whichever_setting_says_so() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert!(!utf8_locale(env(&[])), "none at all (opened from the Finder)");
        assert!(utf8_locale(env(&[("LANG", "en_US.UTF-8")])));
        assert!(utf8_locale(env(&[("LC_CTYPE", "C.UTF-8"), ("LANG", "C")])), "LC_CTYPE wins over LANG");
        assert!(!utf8_locale(env(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")])), "LC_ALL wins over both");
        assert!(utf8_locale(env(&[("LC_ALL", ""), ("LANG", "de_DE.utf8")])), "an empty one doesn't count");
    }

    #[test]
    fn a_machine_session_whose_agent_runs_is_left_alone() {
        let home = std::env::temp_dir().join(format!("cue-running-{}", std::process::id()));
        let dir = home.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let run = |sid: &str| {
            let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).args(["-c", &agent_running_script(sid)]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(run("abc-1"), "GONE", "no registry entry");
        std::fs::write(dir.join(format!("{}.json", std::process::id())), r#"{"pid":1,"sessionId":"abc-1"}"#).unwrap();
        assert_eq!(run("abc-1"), "LIVE", "its pid is alive");
        assert_eq!(run("other"), "GONE", "another session's entry");
        std::fs::write(dir.join("999999.json"), r#"{"sessionId":"dead-1"}"#).unwrap();
        assert_eq!(run("dead-1"), "GONE", "its pid is gone");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn rebuildable_takes_claude_sessions_with_a_pane_and_a_folder() {
        assert!(rebuildable(&model::Origin { harness: "claude".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }));
        assert!(!rebuildable(&model::Origin { harness: "codex".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }), "only claude has --resume");
        assert!(!rebuildable(&model::Origin { harness: "claude".into(), cwd: "/tmp/x".into(), ..Default::default() }), "the Apple Terminal one: no pane");
    }

    #[test]
    fn documents_and_folders_open_but_scripts_and_apps_only_show() {
        let dir = std::env::temp_dir().join(format!("cue-open-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Some.app")).unwrap();
        for f in ["notes.md", "Report.PDF", "run.sh", "go.command", "x.docm", "page.html", "noext"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink(dir.join("run.sh"), dir.join("fake.pdf")).unwrap();
        let ok = |f: &str| safe_to_open(&dir.join(f));
        assert!(ok("notes.md") && ok("Report.PDF"), "documents open in their app");
        assert!(safe_to_open(&dir), "a plain folder opens");
        for f in ["run.sh", "go.command", "x.docm", "page.html", "noext", "Some.app", "fake.pdf", "missing.pdf"] {
            assert!(!ok(f), "{f}: shown in Finder only");
        }
        assert_eq!(link_target(&format!("file://{}/My%20Notes.md", "/tmp")).unwrap_err(), "nothing at that path");
        std::fs::write(dir.join("My Notes.md"), "x").unwrap();
        assert_eq!(link_target(&format!("file://{}/My%20Notes.md", dir.display())).unwrap(), format!("{}/My Notes.md", dir.display()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn link_target_passes_web_links_through_and_checks_paths_exist() {
        assert_eq!(link_target("https://x.dev").unwrap(), "https://x.dev");
        let dir = std::env::temp_dir();
        let there = dir.join("cue-link-target");
        std::fs::create_dir_all(&there).unwrap();
        assert_eq!(link_target(&format!("file://{}", there.display())).unwrap(), there.display().to_string());
        assert_eq!(link_target(&there.display().to_string()).unwrap(), there.display().to_string());
        assert!(link_target("/definitely/not/here").is_err(), "a dead path says so in Cue");
        assert!(link_target("not a link").is_err());
        std::fs::remove_dir_all(&there).unwrap();
    }

    #[test]
    fn a_waiting_session_comes_back_with_its_card_at_launch() {
        let dir = std::env::temp_dir().join(format!("cue-back-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = model::Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        let hub = Arc::new(Hub::new(None));
        hub.event(origin.clone(), "stopped", "Here's the fix.".into(), vec![], "");
        let id = hub.pending()[0].id.clone();
        hub.remove_waiting(&id); // its card is gone (however it went)
        bring_back_cards(&hub);
        let items = hub.pending();
        assert_eq!(items.len(), 1, "the waiting session's card is back, from its saved thread");
        assert_eq!(items[0].message, "Here's the fix.");
        assert_eq!(items[0].origin.agent_pid, None, "no stale pid for the watcher to take for its end");
        bring_back_cards(&hub);
        assert_eq!(hub.pending().len(), 1, "twice doesn't stack a second card");
    }
}
