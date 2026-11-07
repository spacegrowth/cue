//! Cue answers agents that wait on it (a hook holds a session until Cue says allow, a `list` from the window's
//! peers) while it's in the background, its window hidden. macOS's App Nap throttles exactly such an app: its
//! timers coalesced, its threads put at the lowest priority, so when the Mac is busy a hook's answer or the
//! socket's next `accept` can wait tens of seconds. Cue holds a user-initiated activity for its whole life,
//! which keeps it out of App Nap; idle system sleep is still allowed (the Mac sleeps as it would anyway).

/// Out of App Nap until the process ends.
#[cfg(target_os = "macos")]
pub fn stay_responsive() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let reason = NSString::from_str("Answering the agents that wait on Cue");
    let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep, &reason);
    // Held for the process's life: the activity ends when its token is released.
    std::mem::forget(activity);
}

#[cfg(not(target_os = "macos"))]
pub fn stay_responsive() {}
