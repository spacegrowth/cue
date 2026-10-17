mod config;
mod btw;
mod db;
mod hook;
mod changes;
mod safe_git;
mod commands;
mod hub;
mod ipc;
mod leads;
mod locking;
mod logs;
mod model;
mod notify;
mod sounds;
// An optional add-on, compiled in only with the `ext` feature: Rust in `src-tauri/ext/` (not part of
// this repo). Without it there's nothing here.
#[cfg(feature = "ext")]
#[path = "../ext/mod.rs"]
mod ext;
mod extensions;
mod server;
mod sessions;
mod uploads;
mod tray;
mod terminal;
mod machines;
mod remote_poll;

// The old crate-root paths of the moved modules, kept for what still names them there: the hub (its own
// packet) and the optional add-on.
pub(crate) use logs::{live, steps, transcript, usage, usage_file};
pub(crate) use terminal::{dialog, focus};
#[cfg(feature = "ext")]
pub(crate) use terminal::shell;

use hub::Hub;
use ipc::*;
use logs::archive;
use terminal::dictation;
use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

/// The menu bar's menus: Tauri's default ones, but Close (⌘W) is Cue's own: it goes to the window
/// ("close-key"), where inside Cue's terminal it closes the tab you're in, elsewhere it hides the window
/// as before. (Cue's terminal's shells run in tmux: quitting never asks, they carry on.)
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let pkg = app.package_info();
    let about = AboutMetadata { name: Some(pkg.name.clone()), version: Some(pkg.version.to_string()), ..Default::default() };
    Menu::with_items(app, &[
        &Submenu::with_items(app, pkg.name.clone(), true, &[
            &P::about(app, None, Some(about))?,
            &P::separator(app)?,
            &P::services(app, None)?,
            &P::separator(app)?,
            &P::hide(app, None)?,
            &P::hide_others(app, None)?,
            &P::separator(app)?,
            &MenuItem::with_id(app, "app-quit", "Quit Cue", true, Some("CmdOrCtrl+Q"))?,
        ])?,
        &Submenu::with_items(app, "File", true, &[&MenuItem::with_id(app, "app-close", "Close", true, Some("CmdOrCtrl+W"))?])?,
        &Submenu::with_items(app, "Edit", true, &[&P::undo(app, None)?, &P::redo(app, None)?, &P::separator(app)?, &P::cut(app, None)?, &P::copy(app, None)?, &P::paste(app, None)?, &P::select_all(app, None)?])?,
        &Submenu::with_items(app, "View", true, &[&P::fullscreen(app, None)?])?,
        &Submenu::with_items(app, "Window", true, &[&P::minimize(app, None)?, &P::maximize(app, None)?])?,
    ])
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

/// Text size: the whole window zooms (text, cards and buttons together), as a browser's ⌘+ does.
pub(crate) fn apply_zoom(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_zoom(config::zoom());
    }
}

/// Switch WebRTC off for this web view (WebKit's peer-connection preference), where WebKit has it.
#[cfg(target_os = "macos")]
fn no_webrtc(w: &tauri::WebviewWindow) {
    let _ = w.with_webview(|wv| unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{msg_send, sel};
        let wk = wv.inner() as *mut AnyObject;
        if wk.is_null() {
            return;
        }
        let config: *mut AnyObject = msg_send![&*wk, configuration];
        let prefs: *mut AnyObject = if config.is_null() { std::ptr::null_mut() } else { msg_send![&*config, preferences] };
        if prefs.is_null() {
            return;
        }
        let has: bool = msg_send![&*prefs, respondsToSelector: sel!(_setPeerConnectionEnabled:)];
        if has {
            let _: () = msg_send![&*prefs, _setPeerConnectionEnabled: false];
        }
    });
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
        // Extension pages: cueext://localhost/<name>/…, from <Cue's folder>/extensions (extensions.rs).
        .register_uri_scheme_protocol("cueext", |_ctx, req| extensions::serve(req))
        .invoke_handler(tauri::generate_handler![move_to_cue, park_session, resume_parked, unpark_session, closed_sessions, resume_closed, park_closed, machines_list, machine_add, machine_remove, machine_rename, machine_check, machine_login, machine_setup, run_line, unhold_message, send_held_now, term_open, hide_window, term_write, term_resize, term_close, shell_open, shell_list, shell_name, shell_clear, session_screen, session_keys, adopt_session, session_command, session_commands, models_list, set_later, set_starred, btw, connect_agent, update_check, update_install, session_steps, step_detail, mcp_for, mcp_all, mcp_set, mcp_restart, mcp_apply, mcp_apply_clear, mcp_for_folder, mcp_set_folder, send_queued_now, ext_settings, crew_action, focus_live, close_session, delete_session, new_session, trust_folder, continue_start, agents_installed, search, session_log, session_log_page, transcript_page, resume_session, rename_session, rename_dismiss, dictate_start, dictate_stop, interrupt_session, open_link, image_data, get_state, respond, dismiss, send_to_back, unqueue_review, forget_session, focus_session, focus_session_id, send_to_session, reply, answer_terminal, clipboard_image, copy_text, set_setting, get_drafts, set_draft, test_notification, extensions::ext_list, extensions::ext_log, extensions::ext_enable, extensions::ext_activity, extensions::ext_reveal, extensions::ext_git, extensions::ext_program])
        .setup(|app| {
            // No WebRTC in Cue's web view: Cue doesn't use it, and it's the one way out an extension's page
            // would otherwise have (a STUN or TURN request to a server of its choosing, which no page policy
            // stops). Extensions can read your code (git.read); nothing they read may leave this Mac.
            #[cfg(target_os = "macos")]
            if let Some(w) = app.get_webview_window("main") {
                no_webrtc(&w);
            }
            apply_zoom(app.handle());
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
            // A test instance (CUE_HOME) watches its machines only when asked (CUE_TEST_MACHINES).
            if std::env::var_os("CUE_HOME").is_none() || std::env::var_os("CUE_TEST_MACHINES").is_some() {
                let h = hub.clone();
                machines::hooks::on_reached(move |m| restore_machine(&h, m));
                // A new session there (a relay executor just started): its crew read now, not at the next poll.
                let h = hub.clone();
                machines::hooks::on_new_session(move |m| {
                    if let Some(mach) = machines::get(m) {
                        if leads::refresh_remote(&mach.name, &mach.host).0 {
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
            // Leads and executors on your machines: relay's and pi-lead's listing there (one SSH call),
            // on every machine, whether or not Cue has anything open there: every 10 s where it has a
            // live session, every minute elsewhere, less and less often while one can't be reached
            // (remote_poll). Each machine is read on its own, so one that hangs holds no other up.
            let h = hub.clone();
            tauri::async_runtime::spawn(async move {
                let sched = Arc::new(std::sync::Mutex::new(remote_poll::Schedule::default()));
                let mut tick = tokio::time::interval(remote_poll::NEAR);
                loop {
                    tick.tick().await;
                    let a = h.clone();
                    let s = sched.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let busy: std::collections::HashSet<String> = a.saved_sessions().into_iter().map(|s| s.origin.machine).filter(|m| !m.is_empty()).collect();
                        let list = machines::list();
                        let names: Vec<String> = list.iter().map(|m| m.name.clone()).collect();
                        if leads::retain_remote(&names) {
                            a.redraw();
                        }
                        let marks: Vec<(String, bool)> = list.iter().map(|m| (m.name.clone(), busy.contains(&m.name))).collect();
                        let due = crate::locking::lock(&s, "remote poll schedule").due(&marks, std::time::Instant::now());
                        for m in list.into_iter().filter(|m| due.contains(&m.name)) {
                            let (a, s, has_sessions) = (a.clone(), s.clone(), busy.contains(&m.name));
                            std::thread::spawn(move || {
                                let (changed, live) = leads::refresh_remote(&m.name, &m.host);
                                let change = crate::locking::lock(&s, "remote poll schedule").done(&m.name, live.is_some());
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
                            });
                        }
                    })
                    .await;
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
        .menu(app_menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "app-quit" => app.exit(0),
            "app-close" => {
                use tauri::Emitter;
                let _ = app.emit_to("main", "close-key", ());
            }
            _ => {}
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

    app.run(|handle, event| match event {
        RunEvent::Reopen { .. } => {
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    /// The window's scripts (ui/index.html, in order) share one global scope: a top-level name declared
    /// in two of them is a SyntaxError that stops the second script before it runs, and the window
    /// shows its background and nothing else (that reached the installed app once). Each name once.
    #[test]
    fn the_windows_scripts_declare_each_top_level_name_once() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let index = std::fs::read_to_string(ui.join("index.html")).unwrap();
        let scripts: Vec<&str> = index.split("<script src=\"").skip(1).filter_map(|r| r.split('"').next()).filter(|s| !s.starts_with("vendor/")).collect();
        assert!(scripts.len() >= 2, "{scripts:?}");
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut twice = vec![];
        for script in &scripts {
            let text = std::fs::read_to_string(ui.join(script)).unwrap();
            for (i, line) in text.lines().enumerate() {
                let mut words = line.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'));
                let (Some(kw), Some(name)) = (words.next(), words.next()) else { continue };
                if !matches!(kw, "const" | "let" | "var" | "function" | "class") || name.is_empty() || line.starts_with(' ') || line.starts_with('\t') {
                    continue;
                }
                let at = format!("{script}:{}", i + 1);
                match seen.insert(name.to_string(), at.clone()) {
                    Some(first) if !first.starts_with(&format!("{script}:")) => twice.push(format!("{name}: {first} and {at}")),
                    _ => {}
                }
            }
        }
        assert!(twice.is_empty(), "declared in two of the window's scripts:\n{}", twice.join("\n"));
    }

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
}
