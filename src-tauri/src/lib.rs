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
    let origin = hub.session_origin(&session_id);
    let (cwd, transcript) = match origin {
        Some(o) if o.harness == "claude" => (o.cwd, o.transcript_path),
        Some(_) => return Some(vec![]),
        None => match live::claude().into_iter().find(|q| q.session_id == session_id) {
            Some(q) => (q.cwd.clone(), live::claude_transcript(&q.cwd, &session_id).unwrap_or_default()),
            None => return Some(vec![]),
        },
    };
    commands::for_session(&cwd, &transcript)
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
    let tab = focus::open_tab_with(&focus::resume_line(&cwd, session_id)?)?;
    let origin = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, ..Default::default() };
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
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err("not an image".into()),
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

/// "Go to tab" in the Sessions view: also for a session Cue hasn't heard from (quiet, or Pi that only connected).
#[tauri::command]
async fn focus_live(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || focus::focus(&h.live_origin(&session_id).ok_or("that session is gone")?)).await.map_err(|e| e.to_string())?
}

/// Close a session from the Sessions view: an executor through relay / pilead (they mark it closed and
/// close its tab); anything else by ending its agent (SIGTERM: the conversation is saved, `--resume`
/// brings it back) and closing its tab. Never while it works or asks you something.
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
        let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
        focus::end_agent(pid)?;
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
async fn new_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, agent: String, cwd: String, message: String, name: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || start_session(&h, &agent, &cwd, &message, &name))
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
    let dir = cwd.trim();
    let dir = if let Some(rest) = dir.strip_prefix("~") { format!("{}{rest}", std::env::var("HOME").unwrap_or_default()) } else { dir.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    let asks_trust = agent == "claude" && !focus::claude_trusts(&dir);
    let tab = focus::open_tab_with(&focus::start_line(agent, &dir, &sid, message, name)?)?;
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, ..Default::default() };
        h.started(origin, message, asks_trust);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} in {}", agent, tab.what) }))
}

/// Trust in Cue for a session Claude Code is asking "do you trust this folder?": presses Enter in its
/// terminal, which picks the question's first choice, "Yes, I trust this folder". Only from your click.
#[tauri::command]
async fn trust_folder(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || focus::press_enter(&origin)).await.map_err(|e| e.to_string())?
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

pub fn run() {
    let app = tauri::Builder::default()
        .manage(dictation::Dictation::default())
        .plugin(tauri_plugin_notification::init())
        // Cue reopens at the size and place you left it.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![adopt_session, session_command, session_commands, set_later, set_starred, btw, connect_agent, update_check, update_install, session_steps, step_detail, send_queued_now, ext_settings, crew_action, focus_live, close_session, new_session, trust_folder, agents_installed, search, session_log, session_log_page, transcript_page, resume_session, rename_session, dictate_start, dictate_stop, interrupt_session, open_link, image_data, get_state, respond, dismiss, focus_session, focus_session_id, send_to_session, reply, clipboard_image, set_setting, get_drafts, set_draft, test_notification])
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
                    // ...and any of those Cue doesn't know yet joins the Board (adopt says so itself).
                    let a = h.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || a.adopt_all()).await;
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
