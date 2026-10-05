//! macOS notifications through UserNotifications, tagged with the card's id so Cue can take them
//! back out of Notification Center once the card is answered — in Cue, the side panel, the menu
//! bar, or the agent's own terminal.
//!
//! UserNotifications only works from a bundled .app. A bare `cargo run` binary returns false here
//! and the caller falls back to the plain notification plugin (which can't remove anything).

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread};
    use objc2_foundation::{NSArray, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
        UNNotificationRequest, UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
        UNUserNotificationCenterDelegate,
    };
    use std::sync::OnceLock;
    use tauri::AppHandle;

    static APP: OnceLock<AppHandle> = OnceLock::new();

    define_class!(
        // Told by macOS when a notification is clicked (open Cue on that card) or arrives while
        // Cue is frontmost (show it anyway).
        #[unsafe(super(NSObject))]
        #[name = "CueNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                handler.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                handler: &block2::DynBlock<dyn Fn()>,
            ) {
                let id = response.notification().request().identifier().to_string();
                if let Some(app) = APP.get() {
                    crate::show_main(app, Some(id));
                }
                handler.call(());
            }
        }
    );

    fn bundled() -> bool {
        std::env::current_exe().map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/")).unwrap_or(false)
    }

    pub fn init(app: &AppHandle) {
        if !bundled() {
            return;
        }
        let _ = APP.set(app.clone());
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The center holds its delegate weakly: keep ours alive for the life of the app.
        std::mem::forget(delegate);
        // Record macOS's answer: a refusal or error used to vanish silently.
        let done = RcBlock::new(|granted: Bool, err: *mut NSError| {
            let why = if err.is_null() { String::new() } else { unsafe { (*err).localizedDescription().to_string() } };
            log(&format!("permission: {}{}", if granted.as_bool() { "granted" } else { "not granted" }, if why.is_empty() { String::new() } else { format!(" ({why})") }));
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &done,
        );
    }

    pub fn post(id: &str, title: &str, body: &str) -> bool {
        if !bundled() {
            return false;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(id), &content, None);
        let done = RcBlock::new(|err: *mut NSError| {
            if !err.is_null() {
                log(&format!("couldn't post: {}", unsafe { (*err).localizedDescription() }));
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        true
    }

    /// One line per event in notifications.log in Cue's data folder (permission answers, failures).
    fn log(line: &str) {
        use std::io::Write;
        let path = crate::server::cue_dir().join("notifications.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{} {line}", crate::model::now_ms());
        }
    }

    pub fn remove(ids: &[String]) {
        if !bundled() || ids.is_empty() {
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let ids: Vec<_> = ids.iter().map(|s| NSString::from_str(s)).collect();
        center.removeDeliveredNotificationsWithIdentifiers(&NSArray::from_retained_slice(&ids));
    }
}

#[cfg(target_os = "macos")]
pub use mac::{init, post, remove};

#[cfg(not(target_os = "macos"))]
pub fn init(_app: &tauri::AppHandle) {}
#[cfg(not(target_os = "macos"))]
pub fn post(_id: &str, _title: &str, _body: &str) -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn remove(_ids: &[String]) {}
