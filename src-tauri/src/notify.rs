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

/// The session each mini window has its chat open on (by window): you're reading it, so nothing about it
/// bounces the Dock or is posted.
static READING: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
pub fn reading(window: &str, session_id: Option<String>) {
    let mut r = crate::locking::lock(&READING, "mini reading");
    r.retain(|(w, _)| w != window);
    if let Some(s) = session_id.filter(|s| !s.is_empty()) {
        r.push((window.to_string(), s));
    }
}
pub fn is_read(session_id: &str) -> bool {
    let r = crate::locking::lock(&READING, "mini reading");
    r.iter().any(|(_, s)| s == session_id)
}

/// A job for Notification Center (posting, taking back): each is an XPC round trip to macOS's notification
/// service, which can be slow to answer, and whoever changed Cue's state (a hook's connection, the window's
/// command, a tick) mustn't wait on it. They run on one thread of their own, in the order handed in, so a
/// notification taken back is never posted after it was.
pub fn later(job: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static JOBS: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let run = move || {
            for job in rx {
                crate::chaos::pause(crate::chaos::Knob::XpcDelayMs);
                // One that panics doesn't take the thread, and every later notification, with it.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
            }
        };
        std::thread::Builder::new().name("cue-notify".into()).spawn(run).expect("a thread for notifications");
        tx
    });
    let _ = jobs.send(Box::new(job));
}

/// A notification for Cue to post: `category` as `post` takes it; `watched`, a turn you asked to be told
/// about (shown even with Cue in front, with its own sound).
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub watched: bool,
}

/// How long Cue waits before something new goes to Notification Center: answered (or Cue brought forward)
/// within it, it never does. Meanwhile a session you wrote to that came back blinks in the mini window
/// (mini.js backToYou).
pub const DOCK_FIRST: std::time::Duration = std::time::Duration::from_secs(60);

/// The notifications waiting out `DOCK_FIRST` before they're posted. Taken back: gone from here, so it's never
/// posted.
static BOUNCING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Cue is the window in front of you: what's new is on screen already.
fn in_front(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false))
}

/// `note` for you: Cue waits first (the Dock never bounces), and only if it's still there a minute later (not answered, Cue not
/// in front) does it go to Notification Center, with its sound. Nothing while Cue is in front of you. A turn
/// you asked to be told about is posted at once, even with Cue in front. An unbundled dev binary gets the
/// plugin's plain notification instead, which Cue can't take back.
pub fn show(app: tauri::AppHandle, note: Note) {
    if note.watched {
        return later(move || post_now(&app, note));
    }
    // Noted here, not on the thread: one taken back before that thread gets to it is never posted.
    {
        let mut b = crate::locking::lock(&BOUNCING, "dock bounce");
        b.retain(|i| *i != note.id);
        b.push(note.id.clone());
    }
    later(move || {
        if in_front(&app) {
            crate::locking::lock(&BOUNCING, "dock bounce").retain(|i| *i != note.id);
            return;
        }
        if !crate::locking::lock(&BOUNCING, "dock bounce").iter().any(|i| *i == note.id) {
            return;   // taken back already
        }
        std::thread::spawn(move || {
            std::thread::sleep(DOCK_FIRST);
            later(move || {
                let still = {
                    let mut b = crate::locking::lock(&BOUNCING, "dock bounce");
                    let still = b.iter().any(|i| *i == note.id);
                    b.retain(|i| *i != note.id);
                    still
                };
                if still && !in_front(&app) {
                    post_now(&app, note);
                }
            });
        });
    });
}

fn post_now(app: &tauri::AppHandle, note: Note) {
    use tauri_plugin_notification::NotificationExt;
    let Note { id, title, body, category, watched } = note;
    let sound = if watched { crate::sounds::TADA.to_string() } else { crate::sounds::choice() };
    if !post_sound(&id, &title, &body, &category, &sound) {
        let _ = app.notification().builder().title(&title).body(&body).show();
    }
}

/// Notifications `ids` taken back: one still waiting is never posted; one already up comes out of Notification Center, on its thread (`later`), after every post handed in
/// before, so one posted a moment ago is never left up because its taking back went first.
pub fn take_back(ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    crate::locking::lock(&BOUNCING, "dock bounce").retain(|i| !ids.contains(i));
    later(move || remove(&ids));
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn notification_jobs_run_in_order_off_the_callers_thread_and_survive_a_panic() {
        let (stuck_tx, stuck) = channel::<()>();
        let (ran_tx, ran) = channel();
        let t0 = std::time::Instant::now();
        // Notification Center slow to answer: the one handing the job in doesn't wait for it.
        super::later(move || {
            let _ = stuck.recv();
        });
        super::later(|| panic!("a bad job"));
        for i in 0..3 {
            let tx = ran_tx.clone();
            super::later(move || tx.send(i).unwrap());
        }
        assert!(t0.elapsed() < Duration::from_millis(100), "handed in without waiting: {:?}", t0.elapsed());
        assert!(ran.recv_timeout(Duration::from_millis(200)).is_err(), "in order: nothing passes the slow one");
        stuck_tx.send(()).unwrap();
        let got: Vec<i32> = (0..3).map(|_| ran.recv_timeout(Duration::from_secs(2)).unwrap()).collect();
        assert_eq!(got, [0, 1, 2], "after it, in order, the panic notwithstanding");
    }

    /// Notification Center is reached through `later` only: a post or a taking back made on the caller's
    /// thread would wait on it there, and could pass one handed in before it.
    #[test]
    fn notification_center_is_reached_only_from_its_own_thread() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut inline = vec![];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().filter_map(Result::ok) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("notify.rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for (i, line) in text.lines().enumerate() {
                        if ["notify::remove(", "notify::post_sound("].iter().any(|c| line.contains(c)) {
                            inline.push(format!("{}:{}", p.display(), i + 1));
                        }
                    }
                }
            }
        }
        assert!(inline.is_empty(), "called outside notify::later: {inline:?}");
    }
}
