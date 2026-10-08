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
    let tab = if config::tmux_sessions() { focus::open_in_tmux(&line, &cwd, &name)? } else if focus::tmux_installed() { focus::open_in_tmux_tab(&line, &cwd, &name)? } else { focus::open_tab_with(&line)? };
    let origin = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
    h.resumed(origin, &title, &transcript::recent_context(&path, 6), false);
    Ok(serde_json::json!({ "session_id": session_id, "detail": format!("Resumed in {}", tab.what) }))
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
        leads::Act::Run(bin, args, done) => {
            let out = std::process::Command::new(&bin).args(&args).output().map_err(|e| format!("couldn't run relay: {e}"))?;
            if out.status.success() {
                Ok(done)
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("relay failed").trim().to_string())
            }
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

/// A link in a message: open it in your browser. Web links only.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("not a web link".into());
    }
    std::process::Command::new("open").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
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
        if let Some(plan) = leads::close_plan(&session_id) {
            let (bin, args) = plan?;
            let out = std::process::Command::new(&bin).args(&args).output().map_err(|e| e.to_string())?;
            return if out.status.success() {
                Ok("Closed".into())
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("close failed").trim().to_string())
            };
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

pub fn run() {
    // Opened from the Finder or the Dock, an app gets no locale, and tmux without UTF-8 rewrites what it
    // prints (a tab in its answers as "_", "❯" as "_" in the terminal it draws). Cue and everything it
    // starts (tmux, agents, shells) get UTF-8, as they would from a terminal. Set before any thread starts.
    if !utf8_locale(|k| std::env::var(k).ok()) {
        std::env::set_var("LC_CTYPE", "UTF-8");
    }
    let app = tauri::Builder::default()
        .manage(dictation::Dictation::default())
        .plugin(tauri_plugin_notification::init())
        // Cue reopens at the size and place you left it.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![move_to_cue, machines_list, machine_add, machine_remove, machine_rename, machine_check, machine_login, machine_setup, run_line, unhold_message, send_held_now, term_open, term_write, term_resize, term_close, session_screen, session_keys, adopt_session, session_command, session_commands, set_later, set_starred, btw, connect_agent, update_check, update_install, session_steps, step_detail, send_queued_now, ext_settings, crew_action, focus_live, close_session, new_session, trust_folder, agents_installed, search, session_log, session_log_page, transcript_page, resume_session, rename_session, dictate_start, dictate_stop, interrupt_session, open_link, image_data, get_state, respond, dismiss, focus_session, focus_session_id, send_to_session, reply, clipboard_image, set_setting, get_drafts, set_draft, test_notification])
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
            // Events from the agents on your machines (+ New → Machine), over SSH, and their sessions' logs.
            if std::env::var_os("CUE_HOME").is_none() {
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
                    // ...and from that list: sessions Cue doesn't know yet, prompts in a terminal it has no card for.
                    let a = h.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || a.sync_live()).await;
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
    fn a_utf8_locale_is_recognised_whichever_setting_says_so() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert!(!utf8_locale(env(&[])), "none at all (opened from the Finder)");
        assert!(utf8_locale(env(&[("LANG", "en_US.UTF-8")])));
        assert!(utf8_locale(env(&[("LC_CTYPE", "C.UTF-8"), ("LANG", "C")])), "LC_CTYPE wins over LANG");
        assert!(!utf8_locale(env(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")])), "LC_ALL wins over both");
        assert!(utf8_locale(env(&[("LC_ALL", ""), ("LANG", "de_DE.utf8")])), "an empty one doesn't count");
    }
}
