//! Whatever is playing (a video in the browser, Spotify, Music) paused while Listen reads a reply out, and played
//! again when it's done: as cue-voice pauses it while you talk (voice/Sources/cue-voice/Hush.swift), through the
//! Mac's Now Playing (the MediaRemote framework, as the media keys and Control Centre do), which needs no
//! permission. Only when Now Playing says it's playing, so it never starts something you'd paused. macOS no longer
//! tells an app like Cue what's playing, so that's asked through osascript, which it still tells.
//!
//! Holds and releases are done in order on a thread of their own, so a Stop right after Play can't play before the
//! pause has happened, and the window never waits on Now Playing.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

/// MediaRemote's commands.
const PLAY: i32 = 0;
const PAUSE: i32 = 1;

/// Listen started (true) or is done (false): handed to the thread that tells Now Playing.
static WORKER: OnceLock<Mutex<Sender<bool>>> = OnceLock::new();

/// Listen started: whatever is playing pauses.
pub fn hold() {
    send(true);
}

/// Listen is done: what it paused plays again.
pub fn release() {
    send(false);
}

fn send(listening: bool) {
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<bool>();
        std::thread::spawn(move || {
            // Paused by Cue, and not played again yet.
            let mut paused = false;
            for started in rx {
                if started && !paused {
                    paused = now_playing() == Some(true) && command(PAUSE);
                } else if !started && std::mem::take(&mut paused) {
                    command(PLAY);
                }
            }
        });
        Mutex::new(tx)
    });
    let _ = crate::locking::lock(worker, "music: Listen's holds").send(listening);
}

/// Whether Now Playing is playing, asked through osascript (JavaScript for Automation); None when it couldn't be asked.
fn now_playing() -> Option<bool> {
    let js = "ObjC.import('Foundation'); $.NSBundle.bundleWithPath('/System/Library/PrivateFrameworks/MediaRemote.framework/').load; $.NSClassFromString('MRNowPlayingRequest').localIsPlaying ? 'yes' : 'no'";
    let out = crate::focus::capped(std::process::Command::new("/usr/bin/osascript").args(["-l", "JavaScript", "-e", js]), std::time::Duration::from_secs(5)).ok()?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

type SendCommand = unsafe extern "C" fn(i32, *const std::ffi::c_void) -> bool;

/// Command `c` to Now Playing: whether it was taken.
fn command(c: i32) -> bool {
    static SEND: OnceLock<Option<SendCommand>> = OnceLock::new();
    let send = SEND.get_or_init(|| {
        // SAFETY: dlopen/dlsym with NUL-terminated names; the symbol is MediaRemote's
        // `Boolean MRMediaRemoteSendCommand(MRCommand, CFDictionaryRef)`, cast to that signature.
        unsafe {
            let lib = libc::dlopen(c"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                return None;
            }
            let f = libc::dlsym(lib, c"MRMediaRemoteSendCommand".as_ptr());
            (!f.is_null()).then(|| std::mem::transmute::<*mut libc::c_void, SendCommand>(f))
        }
    });
    // SAFETY: MediaRemote's function, with no options.
    send.is_some_and(|f| unsafe { f(c, std::ptr::null()) })
}
