//! Settings, updates, links, images and drafts: the window's commands that aren't about one session.

use crate::config;
use crate::db;
use crate::notify;
use crate::uploads;
use crate::tray;
use crate::terminal::focus;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::{Manager, State};
#[cfg(feature = "ext")]
use crate::ext;

/// Settings → Connect: give an agent Cue's hooks (Claude Code, Codex) or extension (Pi), as
/// install.sh does, for an app downloaded on its own.
#[tauri::command]
pub(crate) async fn connect_agent(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, harness: String) -> Result<String, String> {
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

/// Settings rows an add-on adds ([{ group, rows: [{ title, sub, art?, pick? }] }], `pick`: { id, value,
/// options: [[value, label]] }); none without one.
#[tauri::command]
pub(crate) fn ext_settings() -> serde_json::Value {
    #[cfg(feature = "ext")]
    return ext::settings();
    #[cfg(not(feature = "ext"))]
    serde_json::json!([])
}

/// One of an add-on's Settings picks changed (a row's `pick`: its `id`, the option picked).
#[tauri::command]
pub(crate) fn ext_set(id: String, value: String) -> Result<(), String> {
    #[cfg(feature = "ext")]
    return ext::set(&id, &value);
    #[cfg(not(feature = "ext"))]
    Err(format!("nothing to set {id} to {value}"))
}

/// Updates: is a newer Cue out (the latest release on GitHub, signed with Cue's update key)? Always
/// says which version this is.
#[tauri::command]
pub(crate) async fn update_check(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let current = app.package_info().version.to_string();
    Ok(match find_update(&app).await? {
        Some(u) => serde_json::json!({ "current": current, "version": u.version, "notes": u.body }),
        None => serde_json::json!({ "current": current }),
    })
}

/// Download the newer Cue, check its signature, put it in place of this one, and restart into it.
#[tauri::command]
pub(crate) async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    install_update(app).await
}

pub(crate) async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let Some(u) = find_update(&app).await? else { return Err("Cue is already up to date".into()) };
    u.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("Couldn't install the update: {e}"))?;
    app.restart()
}

/// A newer Cue was found (in the background, or from a menu): the menu bar icon's menu offers it and
/// the window shows its card.
pub(crate) fn offer_update(app: &tauri::AppHandle, u: &tauri_plugin_updater::Update) {
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

/// A Cue built with the add-on never offers a release: releases are built without it, so installing
/// one would drop the add-on. That Cue is updated by building it again.
pub(crate) async fn find_update(app: &tauri::AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    if cfg!(feature = "ext") {
        return Ok(None);
    }
    use tauri_plugin_updater::UpdaterExt;
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| format!("Couldn't check for updates: {e}"))
}

/// A link in a message: open it in your browser (web links); a document (PDF, Markdown, text, an
/// image…) or a plain folder opens in its usual app; anything else is only shown in Finder (`open -R`):
/// agent text can name an app or a script, and a click mustn't run it. (Paths: file:// links, and bare
/// absolute paths on ⌘-click in the window.)
#[tauri::command]
pub(crate) async fn open_link(url: String) -> Result<(), String> {
    // Off the window's thread: a path on a volume that's gone can take a while to say so.
    tauri::async_runtime::spawn_blocking(move || {
        let target = link_target(&url)?;
        let mut open = std::process::Command::new("/usr/bin/open");
        if !is_web(&target) && !safe_to_open(std::path::Path::new(&target)) {
            open.arg("-R");
        }
        open.arg(&target).spawn().map(|_| ()).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

pub(crate) fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Documents that open in an app without running anything. Not on the list (a script, an app, an
/// installer, a .command, a macro-enabled .docm, a web page, …): shown in Finder only.
pub(crate) const OPENABLE: &[&str] = &[
    "pdf", "md", "markdown", "txt", "log", "csv", "tsv", "json", "yaml", "yml", "toml", "rtf",
    "png", "jpg", "jpeg", "gif", "webp", "heic", "tiff", "bmp",
    "mp4", "mov", "m4v", "mp3", "m4a", "wav",
    "docx", "xlsx", "pptx", "pages", "numbers", "key",
];

/// Whether a path opens rather than only showing in Finder: a document on OPENABLE, or a plain folder
/// (one with no extension: .app and other bundles are folders too). The real file decides, not a
/// link's name (a "notes.pdf" link to a script is shown, not opened).
pub(crate) fn safe_to_open(p: &std::path::Path) -> bool {
    let Ok(real) = std::fs::canonicalize(p) else { return false };
    let ext = real.extension().map(|e| e.to_string_lossy().to_lowercase());
    if real.is_dir() {
        return ext.is_none();
    }
    ext.is_some_and(|e| OPENABLE.contains(&e.as_str()))
}

/// What `open` gets: web links as-is; everything else is a path — file:// stripped, ~ expanded,
/// and it must exist (so a dead path says so in Cue instead of Finder beeping).
pub(crate) fn link_target(url: &str) -> Result<String, String> {
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
pub(crate) fn percent_decoded(s: &str) -> String {
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
pub(crate) async fn image_data(path: String) -> Result<String, String> {
    // Off the window's thread (a 20 MB read and its base64), and judged by what the path really is: a
    // link named .png to something else is not an image.
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let p = std::path::Path::new(&path);
        if !p.is_absolute() {
            return Err("not a readable image".into());
        }
        let real = std::fs::canonicalize(p).map_err(|e| e.to_string())?;
        let mime = match uploads::mime_of(&real.to_string_lossy()) {
            "" => return Err("not an image".into()),
            m => m,
        };
        if std::fs::metadata(&real).map_err(|e| e.to_string())?.len() > 20 * 1024 * 1024 {
            return Err("not a readable image".into());
        }
        let bytes = std::fs::read(&real).map_err(|e| e.to_string())?;
        Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    }).await.map_err(|e| e.to_string())?
}

/// The agents "+ New session" can offer (installed on this Mac).
#[tauri::command]
pub(crate) async fn agents_installed() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(focus::agents_installed).await.unwrap_or_default()
}

#[tauri::command]
pub(crate) fn set_setting(hub: State<Arc<Hub>>, key: String, value: serde_json::Value) -> Result<(), String> {
    hub.set_setting(&key, value)
}

/// Text onto your clipboard, from outside a click (a terminal's copy: tmux hands it over as it happens, when
/// the window's own clipboard can refuse it).
#[tauri::command]
pub(crate) async fn copy_text(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        let mut c = std::process::Command::new("/usr/bin/pbcopy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        c.stdin.take().ok_or("no pbcopy")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        c.wait().map_err(|e| e.to_string()).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Paste fallback: the clipboard's image as a data: URL, if there is one.
#[tauri::command]
pub(crate) async fn clipboard_image() -> Option<String> {
    tauri::async_runtime::spawn_blocking(uploads::clipboard_image).await.ok().flatten()
}

/// Settings → "Send a test notification": the same path a real alert takes, minus the agent.
#[tauri::command]
pub(crate) fn test_notification(app: tauri::AppHandle) -> String {
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
pub(crate) fn get_drafts() -> serde_json::Value {
    db::drafts()
}

#[tauri::command]
pub(crate) fn set_draft(key: String, text: String, images: serde_json::Value) {
    db::set_draft(&key, &text, &images)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
