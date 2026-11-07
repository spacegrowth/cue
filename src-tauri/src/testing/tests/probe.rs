//! The probe (`cue-ctl probe`): what the soak judges Cue by, over the socket as a client asks for it.

use crate::testing::World;
use serde_json::json;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

#[test]
fn the_probe_says_how_cue_is_holding_up() {
    let w = World::new();
    let p = w.ask(json!({"type": "probe"}));
    assert_eq!(p["pid"], std::process::id());
    assert!(p["resident_bytes"].as_u64().unwrap() > 1 << 20, "{p}");
    assert!(p["main_thread"]["pings"].is_u64() && p["locks"].is_object() && p["reentries"].is_u64(), "{p}");
    assert_eq!(p["chaos"], json!({}), "no chaos knob is set in the tests");
}

#[test]
fn a_wait_for_a_busy_lock_is_counted_by_the_locks_name() {
    static BUSY: Mutex<()> = Mutex::new(());
    let w = World::new();
    let (held, release) = (mpsc::channel(), mpsc::channel::<()>());
    let holder = std::thread::spawn(move || {
        let _g = crate::locking::lock(&BUSY, "probe test busy lock");
        held.0.send(()).unwrap();
        let _ = release.1.recv_timeout(Duration::from_millis(40));
    });
    held.1.recv().unwrap();
    drop(crate::locking::lock(&BUSY, "probe test busy lock"));
    holder.join().unwrap();
    let busy = &w.ask(json!({"type": "probe"}))["locks"]["probe test busy lock"];
    assert!(busy["waits"].as_u64().unwrap() >= 1, "{busy}");
    assert!(busy["max_ms"].as_f64().unwrap() >= 20.0, "waited for the holder's 40 ms: {busy}");
    drop(release.0);
}
