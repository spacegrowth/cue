//! The menu bar icon: the number waiting on you beside Cue's mark. A click shows a small menu:
//! which Cue this is, updates, Open Cue, Quit. What's waiting lives in the window.

use crate::model::Item;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

const TRAY_ID: &str = "cue";
/// The "C around a dot" mark as a template image (black + alpha; macOS tints it for light/dark).
fn mark(size: u32) -> Image<'static> {
    let mut px = vec![0u8; (size * size * 4) as usize];
    let c = size as f32 / 2.0;
    let (r_out, r_in, r_dot) = (size as f32 * 0.42, size as f32 * 0.27, size as f32 * 0.12);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            // The C opens to the right: skip the ring where the angle is within ±35°.
            let angle = dy.atan2(dx).to_degrees().abs();
            let ring = (d - r_in).min(r_out - d).clamp(-0.5, 0.5) + 0.5;
            let ring = if angle < 35.0 { 0.0 } else { ring };
            let dot = (r_dot - d).clamp(-0.5, 0.5) + 0.5;
            let i = ((y * size + x) * 4) as usize;
            px[i + 3] = (ring.max(dot) * 255.0) as u8;
        }
    }
    Image::new_owned(px, size, size)
}

/// The menu's updates item: "Check for Updates…", then what it found ("Update to Cue 0.2.0 and Restart").
static UPDATE_ITEM: std::sync::OnceLock<MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static UPDATE_OFFERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set the updates item's words (`enabled`: clickable). An "Update to…" text means clicking installs.
pub fn update_item(text: &str, enabled: bool) {
    UPDATE_OFFERED.store(text.starts_with("Update to"), std::sync::atomic::Ordering::Relaxed);
    if let Some(item) = UPDATE_ITEM.get() {
        let _ = item.set_text(text);
        let _ = item.set_enabled(enabled);
    }
}
pub fn update_offered() -> bool {
    UPDATE_OFFERED.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // At launch: Cue turns on "Open at login" the first time (it's the default; Settings can undo it).
    crate::config::login::first_run();
    // Right click: which Cue this is, updates, open, quit.
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "version", format!("Cue {}", app.package_info().version), false, None::<&str>)?)?;
    let update = MenuItem::with_id(app, "update", "Check for Updates…", true, None::<&str>)?;
    menu.append(&update)?;
    let _ = UPDATE_ITEM.set(update);
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Cue", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Cue", true, None::<&str>)?)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(mark(44))
        .icon_as_template(true)
        .tooltip("Cue")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => crate::show_main(app, None),
            "update" => {
                tauri::async_runtime::spawn(crate::update_from_menu(app.clone()));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?
        .set_visible(crate::config::tray_shown())?; // hidden from the start if you turned it off
    Ok(())
}

/// Keep the count beside the icon current, and the icon shown or hidden as Settings says.
pub fn sync(app: &AppHandle, items: &[Item]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let _ = tray.set_visible(crate::config::tray_shown());
    let _ = tray.set_title(if items.is_empty() { None } else { Some(items.len().to_string()) });
}
