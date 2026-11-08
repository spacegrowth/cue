//! What Cue sets going when it opens: the Tauri app itself (its plugins, commands, menu and window events,
//! here), then, once the window exists, everything that runs alongside it, in this order: the window's own
//! setup (window.rs), Cue's folder and hooks, the hub, the watchers that bring sessions and machines back
//! (watchers.rs), the background tickers (tickers.rs), voice and the work done only outside a quiet test
//! instance.

mod tickers;
mod watchers;
pub(crate) mod window;

use crate::hub::Hub;
use crate::ipc::*;
use crate::terminal::{self, dictation};
use crate::{actions, config, db, extensions, hook, logs, notify, quit, server, talk};
use std::sync::Arc;
use tauri::Manager;

// E2E_DEBUG_ONLY: a release build never carries the end-to-end tests' WebDriver server.
#[cfg(all(feature = "e2e", not(debug_assertions)))]
compile_error!("the `e2e` feature puts a WebDriver server in the app: debug builds only");

/// The app, built: its plugins, the window's commands, what runs once the window exists (`setup`), the menu
/// bar and what closing the window does.
pub(crate) fn build() -> tauri::App {
    let builder = tauri::Builder::default();
    // The end-to-end tests (tests/e2e) drive this window through a WebDriver server inside the app, on
    // 127.0.0.1 at TAURI_WEBDRIVER_PORT. Only a debug build with the `e2e` feature has it (see E2E_DEBUG_ONLY).
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .manage(dictation::Dictation::default())
        .manage(terminal::speak::Speak::default())
        .plugin(tauri_plugin_notification::init())
        // Cue reopens at the size and place you left it.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // Extension pages: cueext://localhost/<name>/…, from <Cue's folder>/extensions (extensions.rs).
        .register_uri_scheme_protocol("cueext", |_ctx, req| extensions::serve(req))
        .invoke_handler(tauri::generate_handler![move_to_cue, move_to_terminal, park_session, resume_parked, unpark_session, closed_sessions, resume_closed, park_closed, machines_list, machine_add, machine_remove, machine_rename, machine_check, machine_login, machine_setup, run_line, unhold_message, send_held_now, term_open, hide_window, set_fullscreen, is_fullscreen, term_write, term_resume, term_resize, term_close, shell_open, shell_list, shell_name, shell_clear, session_screen, session_keys, adopt_session, session_command, session_commands, models_list, set_later, set_watch, set_starred, btw, catch_up, tasks_start, tasks_refresh, tasks_stop, status_session, background_session, connect_agent, update_check, update_install, session_steps, step_detail, session_bg_history, session_bg_helper, insight_state, insight_log, insight_counts, insights_summary, insight_delegate, insight_host_skills, handoff_sent, handoff_cue, handoff_continue, handoff_fresh, handoff_keep, ahead_watch, mcp_for, mcp_set, mcp_restart, restart_session, mcp_apply, mcp_apply_clear, mcp_for_folder, mcp_set_folder, send_queued_now, ext_settings, ext_set, crew_action, focus_live, close_session, delete_session, new_session, fork_session, trust_folder, continue_start, agents_installed, search, session_log, session_log_page, transcript_page, resume_session, rename_session, rename_dismiss, dictate_start, dictate_stop, speak_send, quit_check, quit_now, host_status, host_restart, music_hold, music_release, interrupt_session, open_link, read_doc, image_data, get_state, history_item, respond, dismiss, send_to_back, unqueue_review, forget_session, focus_session, focus_session_id, send_to_session, reply, answer_terminal, clipboard_image, copy_text, set_setting, get_drafts, set_draft, test_notification, extensions::ext_list, extensions::ext_log, extensions::ext_enable, extensions::ext_activity, extensions::ext_reveal, extensions::ext_git, extensions::ext_program, actions::actions_list, actions::actions_allow, actions::actions_run, actions::actions_restart, actions::actions_stop, actions::actions_log, actions::actions_attach_id, actions::actions_term_open, actions::actions_save, actions::actions_file, actions::actions_delete, actions::actions_running, actions::actions_prompt_set, actions::project_for_folder, crate::mini::mini_shape, crate::mini::mini_key, crate::mini::mini_drag, crate::mini::mini_open_in_cue, crate::mini::mini_reading])
        .setup(setup)
        .menu(window::app_menu)
        .on_menu_event(window::on_menu_event)
        .on_window_event(window::on_window_event)
        .build(tauri::generate_context!())
        .expect("error while building Cue")
}

/// Everything that runs once the window exists, in the order it starts.
fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    window::prepare(app);
    // Actions (actions/): the window hears when a project's change, and ones still running are watched.
    actions::start(app.handle().clone());
    settle_folder_and_hooks();
    // The soak's chaos knobs that run on their own (debug builds, when set).
    crate::chaos::start();
    let hub = Arc::new(Hub::new(Some(app.handle().clone())));
    app.manage(hub.clone());
    // Hosted sessions cue-host kept running while Cue was away: picked up before anything can be sent to them.
    hub.reattach_hosted();
    quit::on_signals(app.handle().clone());
    watchers::bring_back(&hub);
    // Events from the agents on your machines (+ New → Machine), over SSH, and their sessions' logs.
    // A test instance (CUE_HOME) watches its machines only when asked (CUE_TEST_MACHINES).
    if std::env::var_os("CUE_HOME").is_none() || std::env::var_os("CUE_TEST_MACHINES").is_some() {
        watchers::machines(&hub);
    }
    tickers::serve_socket(&hub, app.handle());
    tickers::local_crews(&hub);
    tickers::remote_crews(&hub);
    talk::start(app, hub.clone());
    #[cfg(feature = "ext")]
    crate::ext::start(app, hub.clone());
    tauri::async_runtime::spawn(server::watch(hub.clone()));
    if std::env::var_os("CUE_QUIET").is_none() {
        start_background_work(app.handle());
    }
    // What was waiting before the restart: the Dock badge shows it now.
    hub.show_waiting();
    tickers::usage_file(&hub);
    if std::env::var_os("CUE_QUIET").is_none() {
        tickers::updates(app.handle());
    }
    // Settings → Mini window: Cue at the edge of your screen (off unless you turned it on).
    crate::mini::apply(app.handle());
    // A quiet test instance keeps its windows out of your way.
    if std::env::var_os("CUE_QUIET").is_some() {
        window::hide(app);
    }
    Ok(())
}

/// Cue's folder where it belongs and the agents' hooks pointing at this copy of Cue.
fn settle_folder_and_hooks() {
    // One-time move from ~/.cue to Application Support (skipped when CUE_HOME is set).
    if std::env::var_os("CUE_HOME").is_none() {
        let old = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cue");
        if let Some(old) = server::move_from_old_folder(&old, &server::cue_dir()) {
            db::rewrite_paths(&format!("{old}/"), &format!("{}/", server::cue_dir().display()));
            // Its settings moved with it: read from where they are now.
            config::refresh();
        }
    }
    // Point the hook shim at this copy of Cue (wherever the app lives now).
    hook::write_shim();
    // Connected with an older Cue: add the hooks this one needs (a test instance leaves yours alone).
    if std::env::var_os("CUE_QUIET").is_none() && std::env::var_os("CUE_HOME").is_none() {
        config::refresh_hooks();
    }
}

/// What only a real (not quiet) Cue starts: notifications, the background indexes, Open at login.
fn start_background_work(app: &tauri::AppHandle) {
    notify::init(app);
    logs::archive::start(); // older Claude Code sessions, indexed by name in the background
    logs::turns::start(); // every session's turns, for the Insights tab, in the background
    // At launch: Cue turns on "Open at login" the first time (it's the default; Settings can undo it).
    config::login::first_run();
}
