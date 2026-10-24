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
    mark_in(size, [0, 0, 0], [0, 0, 0], 1.0, 1.0)
}
/// The mark in colors (not a template): the C in `ring`, the dot in `dot`, `glow` strong (0–1) and `grow` as big.
fn mark_in(size: u32, ring_rgb: [u8; 3], dot_rgb: [u8; 3], glow: f32, grow: f32) -> Image<'static> {
    let mut px = vec![0u8; (size * size * 4) as usize];
    let c = size as f32 / 2.0;
    let (r_out, r_in, r_dot) = (size as f32 * 0.42, size as f32 * 0.27, size as f32 * 0.12 * grow);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            // The C opens to the right: skip the ring where the angle is within ±35°.
            let angle = dy.atan2(dx).to_degrees().abs();
            let ring = (d - r_in).min(r_out - d).clamp(-0.5, 0.5) + 0.5;
            let ring = if angle < 35.0 { 0.0 } else { ring };
            let dot = ((r_dot - d).clamp(-0.5, 0.5) + 0.5) * glow;
            let i = ((y * size + x) * 4) as usize;
            let rgb = if dot > ring { dot_rgb } else { ring_rgb };
            px[i..i + 3].copy_from_slice(&rgb);
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

/// While `on` the mark's dot glows in Cue's lime, brighter and dimmer about once a second: an add-on in a
/// conversation with you (it says when it starts and ends). The C stays in the menu bar's own color.
static PULSING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Which pulse is the latest: an earlier one's thread, ending late, mustn't put the plain mark over a new pulse.
static PULSES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn pulse(app: &AppHandle, on: bool) {
    use std::sync::atomic::Ordering::SeqCst;
    if PULSING.swap(on, SeqCst) == on {
        return;
    }
    // `None`: back to the plain mark, a template macOS tints.
    let set = |app: &AppHandle, image: Option<Image<'static>>| {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                let template = image.is_none();
                let _ = tray.set_icon(Some(image.unwrap_or_else(|| mark(44))));
                let _ = tray.set_icon_as_template(template);
            }
        });
    };
    let this = PULSES.fetch_add(1, SeqCst) + 1;
    if !on {
        return set(app, None);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // In color the C can't follow the menu bar by itself: white on a dark one, black on a light one.
        let dark = std::process::Command::new("defaults").args(["read", "-g", "AppleInterfaceStyle"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"));
        let (ring, lime) = if dark { ([255, 255, 255], [0xb5, 0xc2, 0x7a]) } else { ([0, 0, 0], [0x8d, 0xa2, 0x3a]) };
        let start = std::time::Instant::now();
        while PULSING.load(SeqCst) && PULSES.load(SeqCst) == this {
            let s = (start.elapsed().as_secs_f32() * std::f32::consts::TAU).sin() * 0.5 + 0.5;   // once a second
            set(&app, Some(mark_in(44, ring, lime, 0.35 + 0.65 * s, 1.0 + 0.2 * s)));
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        // The plain mark back, unless a newer pulse is on (it draws its own): also when this one was ended just
        // after drawing a frame, which would otherwise stay.
        if PULSES.load(SeqCst) == this || !PULSING.load(SeqCst) {
            set(&app, None);
        }
    });
}

/// Keep the count beside the icon current, and the icon shown or hidden as Settings says.
/// On the main thread only: the icon's handle counts its owners without locks, so a copy made and
/// dropped on another thread can remove the icon there, and macOS stops Cue for it.
pub fn sync(app: &AppHandle, items: &[Item]) {
    let (handle, count) = (app.clone(), items.len());
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else { return };
        let _ = tray.set_visible(crate::config::tray_shown());
        let _ = tray.set_title(if count == 0 { None } else { Some(count.to_string()) });
    });
}
