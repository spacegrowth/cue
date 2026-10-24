//! macOS notifications through UserNotifications, tagged with the card's id so Cue can take them
//! back out of Notification Center once the card is answered — in Cue, the menu
//! bar, or the agent's own terminal.
//!
//! A finished turn's notification has Reply and your quick phrases on it (hover it; more than one action
//! shows as Options), and Compact when its context is high: what you pick is typed into that session, as
//! replying in Cue would.
//!
//! UserNotifications only works from a bundled .app. A bare `cargo run` binary returns false here
//! and the caller falls back to the plain notification plugin (which can't remove anything).

/// Sessions whose finished turns aren't notified for now: something else is already telling you about them (an
/// add-on reading their answers out to you). What needs you still is. Empty: notify as usual.
static QUIET: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
pub fn quiet(session_ids: Vec<String>) {
    *crate::locking::lock(&QUIET, "quiet sessions") = session_ids;
}
pub fn is_quiet(session_id: &str) -> bool {
    let q = crate::locking::lock(&QUIET, "quiet sessions");
    q.iter().any(|s| s == session_id)
}

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread};
    use objc2_foundation::{NSArray, NSError, NSSet, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction, UNNotificationActionOptions,
        UNNotificationCategory, UNNotificationCategoryOptions, UNNotificationPresentationOptions, UNNotificationRequest,
        UNNotificationResponse, UNNotificationSound, UNTextInputNotificationAction, UNUserNotificationCenter,
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
                let action = response.actionIdentifier().to_string();
                let text = if action == "reply" {
                    // Only the text field's action sends "reply", and its response is a UNTextInputNotificationResponse.
                    let t: Retained<NSString> = unsafe { msg_send![response, userText] };
                    Some(t.to_string())
                } else {
                    action.strip_prefix("phrase.").and_then(|i| i.parse::<usize>().ok()).and_then(|i| crate::config::quick_phrases().get(i).cloned())
                };
                match (text, APP.get()) {
                    (Some(text), Some(app)) => send(app.clone(), id, text),
                    (None, Some(app)) if action == "compact" => compact(app.clone(), id),
                    (None, Some(app)) => crate::show_main(app, Some(id)),
                    _ => {}
                }
                handler.call(());
            }
        }
    );

    /// A notification's Reply or quick phrase: typed into its session (off the main thread: typing waits
    /// for the agent to take it). If it can't go, a notification says why.
    fn send(app: AppHandle, id: String, text: String) {
        use tauri::Manager;
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let hub = app.state::<std::sync::Arc<crate::hub::Hub>>().inner().clone();
            if let Err(e) = hub.reply(&id, &text, &[]) {
                log(&format!("reply from a notification didn't go: {e}"));
                post(&format!("unsent-{id}"), "Not sent", &format!("{e}: “{text}”"), "");
            }
        });
    }

    /// Compact from a notification (its context was high): typed into that session.
    fn compact(app: AppHandle, id: String) {
        use tauri::Manager;
        std::thread::spawn(move || {
            let hub = app.state::<std::sync::Arc<crate::hub::Hub>>().inner().clone();
            if let Err(e) = hub.compact_for_item(&id) {
                log(&format!("compact from a notification didn't go: {e}"));
                post(&format!("unsent-{id}"), "Didn't compact", &e, "");
            }
        });
    }

    /// The finished-turn notification's actions: Reply, and one per quick phrase (up to 4); "turn-full"
    /// (its context is high) adds Compact after Reply. Set when Cue starts and whenever the phrases change.
    pub fn set_phrases(phrases: &[String]) {
        if !bundled() {
            return;
        }
        let action = |id: &str, title: &str| UNNotificationAction::actionWithIdentifier_title_options(&NSString::from_str(id), &NSString::from_str(title), UNNotificationActionOptions::empty());
        let category = |id: &str, compact: bool| {
            let mut actions: Vec<Retained<UNNotificationAction>> = vec![Retained::into_super(
                UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
                    &NSString::from_str("reply"), &NSString::from_str("Reply"), UNNotificationActionOptions::empty(), &NSString::from_str("Send"), &NSString::from_str("Message"),
                ),
            )];
            if compact {
                actions.push(action("compact", "Compact"));
            }
            actions.extend(phrases.iter().take(4).enumerate().map(|(i, p)| action(&format!("phrase.{i}"), p)));
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str(id), &NSArray::from_retained_slice(&actions), &NSArray::from_retained_slice(&[]), UNNotificationCategoryOptions::empty(),
            )
        };
        UNUserNotificationCenter::currentNotificationCenter().setNotificationCategories(&NSSet::from_retained_slice(&[category("turn", false), category("turn-full", true)]));
    }

    fn bundled() -> bool {
        std::env::current_exe().map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/")).unwrap_or(false)
    }

    pub fn init(app: &AppHandle) {
        if !bundled() {
            return;
        }
        let _ = APP.set(app.clone());
        crate::sounds::install();
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The center holds its delegate weakly: keep ours alive for the life of the app.
        std::mem::forget(delegate);
        set_phrases(&crate::config::quick_phrases());
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

    /// `category`: "turn" for a finished turn (Reply and quick phrases on it), "turn-full" (and Compact), else "".
    pub fn post(id: &str, title: &str, body: &str, category: &str) -> bool {
        post_sound(id, title, body, category, &crate::sounds::choice())
    }

    /// `post` with a given sound (a choice in sounds.rs) instead of the one you picked.
    pub fn post_sound(id: &str, title: &str, body: &str, category: &str, sound: &str) -> bool {
        if !bundled() {
            return false;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        // The sound you picked (Settings → General), or none.
        if let Some(name) = crate::sounds::notification_name(sound) {
            content.setSound(Some(&UNNotificationSound::soundNamed(&NSString::from_str(&name))));
        }
        if !category.is_empty() {
            content.setCategoryIdentifier(&NSString::from_str(category));
        }
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
pub use mac::{init, post, post_sound, remove, set_phrases};

#[cfg(not(target_os = "macos"))]
pub fn init(_app: &tauri::AppHandle) {}
#[cfg(not(target_os = "macos"))]
pub fn post(_id: &str, _title: &str, _body: &str, _category: &str) -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn post_sound(_id: &str, _title: &str, _body: &str, _category: &str, _sound: &str) -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn set_phrases(_phrases: &[String]) {}
#[cfg(not(target_os = "macos"))]
pub fn remove(_ids: &[String]) {}
