//! "Open at login": Cue as a macOS login item (System Settings → General → Login Items), through
//! SMAppService, the same API other menu bar apps use. The switch in Settings reads the real status,
//! so it stays in step if you change it in System Settings instead.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSError;

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// SMAppServiceStatus: notRegistered 0, enabled 1, requiresApproval 2, notFound 3.
const ENABLED: isize = 1;
const NEEDS_APPROVAL: isize = 2;

/// Cue.app itself, as a login item (macOS 13+).
fn service() -> Option<Retained<AnyObject>> {
    let cls = AnyClass::get(c"SMAppService")?;
    unsafe { msg_send![cls, mainAppService] }
}

/// On, or waiting for you to allow it in System Settings (macOS counts both as "will open").
pub fn enabled() -> bool {
    service().is_some_and(|s| matches!(unsafe { msg_send![&s, status] }, ENABLED | NEEDS_APPROVAL))
}

pub fn set(on: bool) -> Result<(), String> {
    if on == enabled() {
        return Ok(());
    }
    let s = service().ok_or("opening at login needs macOS 13 or later")?;
    let mut err: *mut NSError = std::ptr::null_mut();
    let ok: bool = unsafe {
        if on {
            msg_send![&s, registerAndReturnError: &mut err]
        } else {
            msg_send![&s, unregisterAndReturnError: &mut err]
        }
    };
    if ok {
        return Ok(());
    }
    Err(unsafe { err.as_ref() }.map(|e| e.localizedDescription().to_string()).unwrap_or_else(|| "macOS didn't allow it".into()))
}

/// The first time Cue runs with this, it turns itself on (the default); after that it's your switch.
pub fn first_run() {
    if super::load().pointer("/login/open").is_none() {
        let on = set(true).is_ok();
        let _ = super::remember("login.open", serde_json::json!(on));
    }
}
