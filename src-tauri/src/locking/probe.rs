//! The main-thread probe: a thread of its own asks the main thread to answer every second; one that
//! hasn't in two seconds is logged as stalled ("main thread stalled <ms>") with every lock held at
//! that moment, then again when it answers. A stalled main thread is the beachball: this names what
//! it was waiting for; a sample of the window's page process (`sample`) names what the page was busy
//! with. Every answer's time is counted (`stats`).

use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Runs a closure on the main thread; false once there's no main thread to run it (the app quit).
pub type Post = Box<dyn Fn(Box<dyn FnOnce() + Send>) -> bool + Send>;

#[derive(Clone, Copy)]
struct Timing {
    every: Duration,
    stalled_after: Duration,
}

const TIMING: Timing = Timing { every: Duration::from_secs(1), stalled_after: Duration::from_secs(2) };

/// Start watching the main thread, reached through `post` (Tauri's `run_on_main_thread`).
pub fn watch_main(post: Post) {
    let _ = std::thread::Builder::new().name("cue main-thread probe".into()).spawn(move || watch(&post, TIMING));
}

fn watch(post: &Post, timing: Timing) {
    loop {
        std::thread::sleep(timing.every);
        let (tx, rx) = mpsc::channel();
        let t0 = Instant::now();
        if !post(Box::new(move || {
            let _ = tx.send(());
        })) {
            return;
        }
        if rx.recv_timeout(timing.stalled_after).is_ok() {
            super::stats::answered(t0.elapsed());
            continue;
        }
        let sample = super::sample::on_stall(&super::report::dir(), crate::model::now_ms());
        super::report::stall(t0.elapsed(), &super::held::all(), sample.as_deref());
        // Until it answers (or the ping is dropped unanswered: the app is going).
        if rx.recv().is_ok() {
            super::stats::answered(t0.elapsed());
            super::report::stall_over(t0.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The two tests here share the page process `sample` would sample: one at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// A stand-in main thread that takes `slow` to get to each ping, and a probe that stops after `pings`.
    fn stand_in(slow: Duration, pings: usize) -> Post {
        let (to_main, main_queue) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        std::thread::Builder::new()
            .name("probe-test stand-in main".into())
            .spawn(move || {
                for f in main_queue {
                    std::thread::sleep(slow);
                    f();
                }
            })
            .unwrap();
        let sent = std::sync::atomic::AtomicUsize::new(0);
        Box::new(move |f| sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < pings && to_main.send(f).is_ok())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_stall_leaves_a_sample_of_the_page_process_next_to_the_log() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = crate::locking::report::dir();
        let _ = std::fs::remove_dir_all(&dir);
        // This test process stands in for the page's: it's this app's own, as the window's page process is.
        super::super::sample::page_process(std::process::id() as i32);
        watch(&stand_in(Duration::from_millis(400), 1), Timing { every: Duration::from_millis(10), stalled_after: Duration::from_millis(100) });
        super::super::sample::wait_idle();
        super::super::sample::page_process(0);
        let samples: Vec<_> = std::fs::read_dir(&dir).expect("the samples' folder").map(|e| e.unwrap().path()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(samples.len(), 1, "{samples:?}");
        let name = samples[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("stall-") && name.ends_with(".sample.txt"), "{name}");
        let said = crate::locking::report::said();
        assert!(said.iter().any(|e| e.contains("main thread stalled") && e.contains(&name)), "the log names the sample: {said:?}");
    }

    #[test]
    fn a_main_thread_that_doesnt_answer_is_logged_with_the_locks_held() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        static HELD_DURING_STALL: Mutex<()> = Mutex::new(());
        // A stand-in main thread that takes 300 ms to get to each ping, while a lock is held elsewhere.
        let (got_it, held_until) = (mpsc::channel(), mpsc::channel::<()>());
        let held = std::thread::Builder::new().name("probe-test holder".into()).spawn(move || {
            let _g = crate::locking::lock(&HELD_DURING_STALL, "probe test lock");
            got_it.0.send(()).unwrap();
            let _ = held_until.1.recv();
        }).unwrap();
        got_it.1.recv().unwrap();
        // Two pings, then the probe stops (as when the app quits).
        watch(&stand_in(Duration::from_millis(300), 2), Timing { every: Duration::from_millis(10), stalled_after: Duration::from_millis(100) });
        held_until.0.send(()).unwrap();
        held.join().unwrap();
        let said = crate::locking::report::said();
        let stall = said.iter().find(|e| e.contains("main thread stalled") && e.contains("probe test lock")).expect("the stall, with the lock held");
        assert!(stall.contains("held by probe-test holder"), "{stall}");
        assert!(said.iter().any(|e| e.contains("main thread answered again after")), "{said:?}");
    }
}
