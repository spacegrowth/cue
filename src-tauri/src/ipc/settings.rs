//! Settings, updates, links, images and drafts: the window's commands that aren't about one session.

use super::{blocking, with_hub};
use crate::config;
use crate::db;
use crate::notify;
use crate::uploads;
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
    with_hub(&hub, move |h| {
        let done = config::connect(&harness, &pi_ext);
        h.redraw(); // Settings shows it connected
        done
    })
    .await?
}

/// Settings rows that talking from anywhere (talk.rs) and an add-on add ([{ group, tab?, rows: [{ title, sub, art?,
/// pick? }] }], `pick`: { id, value, options: [[value, label]] }).
/// Fetched each time Settings opens, so macOS is asked again then what Settings shows (Open at login).
#[tauri::command]
pub(crate) async fn ext_settings() -> Result<serde_json::Value, String> {
    crate::config::mac::refresh_soon();
    blocking(ext_settings_rows).await
}

fn ext_settings_rows() -> serde_json::Value {
    #[cfg(feature = "ext")]
    let mut rows = ext::settings();
    #[cfg(not(feature = "ext"))]
    let mut rows = serde_json::json!([]);
    // Talking from anywhere on the Mac (talk.rs) comes first, in the same shape.
    if let (Some(all), Some(voice)) = (rows.as_array_mut(), crate::talk::settings()) {
        all.insert(0, voice);
    }
    rows
}

/// One of those Settings picks changed (a row's `pick`: its `id`, the option picked).
#[tauri::command]
pub(crate) async fn ext_set(id: String, value: String) -> Result<(), String> {
    blocking(move || ext_set_now(&id, &value)).await?
}

fn ext_set_now(id: &str, value: &str) -> Result<(), String> {
    if crate::talk::set(id, value).is_ok() {
        return Ok(());
    }
    #[cfg(feature = "ext")]
    return ext::set(id, value);
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

/// A newer Cue was found (in the background): the window shows its card.
pub(crate) fn offer_update(app: &tauri::AppHandle, u: &tauri_plugin_updater::Update) {
    use tauri::Emitter;
    let _ = app.emit("update-ready", serde_json::json!({ "version": u.version, "notes": u.body }));
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
    blocking(move || {
        let target = link_target(&url)?;
        let mut open = std::process::Command::new("/usr/bin/open");
        if !is_web(&target) {
            match how_to_open(std::path::Path::new(&target)) {
                Opener::App => {}
                Opener::Text => { open.arg("-t"); }
                Opener::Finder => { open.arg("-R"); }
            }
        }
        open.arg(&target).spawn().map(|_| ()).map_err(|e| e.to_string())
    }).await?
}

pub(crate) fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Documents that open in an app without running anything (a web page opens in the browser, as a web link does).
pub(crate) const OPENABLE: &[&str] = &[
    "pdf", "md", "markdown", "txt", "log", "csv", "tsv", "json", "yaml", "yml", "toml", "rtf",
    "png", "jpg", "jpeg", "gif", "webp", "heic", "tiff", "bmp", "svg",
    "mp4", "mov", "m4v", "mp3", "m4a", "wav",
    "docx", "xlsx", "pptx", "pages", "numbers", "key",
    "html", "htm",
];

/// Code and config: opened as text (`open -t`, your text editor), because opening some of them the usual
/// way runs them (a .py in Python Launcher, a .sh or .command in Terminal).
pub(crate) const AS_TEXT: &[&str] = &[
    "rs", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rb", "go", "java", "kt", "kts", "swift", "c", "h", "cc",
    "cpp", "hpp", "m", "mm", "cs", "php", "pl", "lua", "sh", "bash", "zsh", "fish", "command", "sql", "css",
    "scss", "less", "xml", "vue", "svelte", "ini", "conf", "cfg", "env", "lock", "gradle", "tf", "proto",
    "graphql", "dart", "scala", "ex", "exs", "hs", "r", "jl", "diff", "patch", "plist",
];

/// How a path opens.
#[derive(Debug, PartialEq)]
pub(crate) enum Opener {
    /// In its own app: a document on OPENABLE, or a plain folder.
    App,
    /// As text in your editor: code and config (AS_TEXT), or a file with no extension (Makefile, .env, a binary
    /// shows as text and never runs).
    Text,
    /// Shown in Finder only: anything else (an app, an installer, a macro-enabled .docm, a disk image, …).
    Finder,
}

/// The real file decides, not a link's name: a "notes.pdf" link to a script opens as text, not in Preview.
pub(crate) fn how_to_open(p: &std::path::Path) -> Opener {
    let Ok(real) = std::fs::canonicalize(p) else { return Opener::Finder };
    let ext = real.extension().map(|e| e.to_string_lossy().to_lowercase());
    if real.is_dir() {
        // .app and other bundles are folders too: those only show.
        return if ext.is_none() { Opener::App } else { Opener::Finder };
    }
    match ext.as_deref() {
        None => Opener::Text,
        Some(e) if OPENABLE.contains(&e) => Opener::App,
        Some(e) if AS_TEXT.contains(&e) => Opener::Text,
        _ => Opener::Finder,
    }
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
    blocking(move || {
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
    }).await?
}

/// The agents "+ New session" can offer (installed on this Mac).
#[tauri::command]
pub(crate) async fn agents_installed() -> Vec<String> {
    blocking(focus::agents_installed).await.unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn set_setting(hub: State<'_, Arc<Hub>>, key: String, value: serde_json::Value) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_setting(&key, value)).await?
}

/// Text onto your clipboard, from outside a click (a terminal's copy: tmux hands it over as it happens, when
/// the window's own clipboard can refuse it).
#[tauri::command]
pub(crate) async fn copy_text(text: String) -> Result<(), String> {
    blocking(move || {
        use std::io::Write;
        let mut c = std::process::Command::new("/usr/bin/pbcopy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        c.stdin.take().ok_or("no pbcopy")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        c.wait().map_err(|e| e.to_string()).map(|_| ())
    })
    .await?
}

/// Paste fallback: the clipboard's image as a data: URL, if there is one.
#[tauri::command]
pub(crate) async fn clipboard_image() -> Option<String> {
    blocking(uploads::clipboard_image).await.ok().flatten()
}

/// Settings → "Send a test notification": the same path a real alert takes, minus the agent.
#[tauri::command]
pub(crate) async fn test_notification(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_notification::NotificationExt;
    blocking(move || {
        if notify::post("cue-test", "Cue test", "If you can read this, macOS notifications work.", "") {
            "sent: check the top right of your screen".into()
        } else {
            let _ = app.notification().builder().title("Cue test").body("Fallback notification (unbundled build).").show();
            "sent (fallback: this build isn't the installed app)".into()
        }
    })
    .await
}

/// Unsent drafts, per session (saved in cue.db so a restart keeps them).
#[tauri::command]
pub(crate) async fn get_drafts() -> Result<serde_json::Value, String> {
    blocking(db::drafts).await
}

#[tauri::command]
pub(crate) async fn set_draft(key: String, text: String, images: serde_json::Value) -> Result<(), String> {
    blocking(move || db::set_draft(&key, &text, &images)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_open_code_opens_as_text_and_apps_only_show() {
        let dir = std::env::temp_dir().join(format!("cue-open-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Some.app")).unwrap();
        for f in ["notes.md", "Report.PDF", "page.html", "run.sh", "go.command", "main.py", "lib.RS", "Makefile", "x.docm", "disk.dmg"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink(dir.join("run.sh"), dir.join("fake.pdf")).unwrap();
        let how = |f: &str| how_to_open(&dir.join(f));
        for f in ["notes.md", "Report.PDF", "page.html"] {
            assert_eq!(how(f), Opener::App, "{f}: opens in its app");
        }
        assert_eq!(how_to_open(&dir), Opener::App, "a plain folder opens");
        for f in ["run.sh", "go.command", "main.py", "lib.RS", "Makefile", "fake.pdf"] {
            assert_eq!(how(f), Opener::Text, "{f}: opens as text, never runs");
        }
        for f in ["x.docm", "disk.dmg", "Some.app", "missing.pdf"] {
            assert_eq!(how(f), Opener::Finder, "{f}: shown in Finder only");
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
