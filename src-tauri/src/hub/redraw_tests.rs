//! The draw worker of a hub with a window (hub/redraw.rs), started here without one.

use super::*;

/// Polls `done` for up to two seconds.
fn soon(done: impl Fn() -> bool) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

/// Between draws the worker waits holding nothing of the hub: once the app lets the hub go, it's gone.
#[test]
fn the_draw_worker_holds_the_hub_only_while_it_draws() {
    let _w = crate::testing::World::new();
    let hub = Arc::new(Hub::new(None));
    hub.draw_on_worker(Arc::downgrade(&hub));
    hub.redraws.mark();
    assert!(soon(|| hub.draws() == 1), "the change drawn");
    assert!(soon(|| hub.redraws.waits.load(Ordering::SeqCst) == 2), "the worker waiting for the next change");
    assert_eq!(Arc::strong_count(&hub), 1, "the waiting worker holds no hub");
    let gone = Arc::downgrade(&hub);
    drop(hub);
    assert!(gone.upgrade().is_none(), "dropped with the app's");
}

/// A state goes to the windows only once they drew the last (#587): a window slower to draw than DRAW_EVERY
/// would get states faster than it draws them, each waiting behind the last, your clicks behind all of them.
#[test]
fn the_next_state_waits_for_the_windows_to_draw_the_last() {
    let r = Arc::new(Redraws::default());
    // No window has said it draws states: nothing to wait for.
    r.sending();
    assert!(r.wait_drawn(std::time::Duration::from_millis(1)));
    // The main window drew one: the next waits for it to draw that too.
    r.drawn("main");
    r.sending();
    let drew = std::thread::spawn({
        let r = r.clone();
        move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            r.drawn("main");
        }
    });
    let t0 = std::time::Instant::now();
    assert!(r.wait_drawn(std::time::Duration::from_secs(5)), "back once it drew it");
    assert!(t0.elapsed() >= std::time::Duration::from_millis(40), "waited for it: {:?}", t0.elapsed());
    drew.join().unwrap();
    // A window that never says so (closed, hidden) is waited for once, then not until it draws one again.
    r.sending();
    assert!(!r.wait_drawn(std::time::Duration::from_millis(20)), "gave up on it");
    r.sending();
    let t0 = std::time::Instant::now();
    assert!(r.wait_drawn(std::time::Duration::from_secs(5)));
    assert!(t0.elapsed() < std::time::Duration::from_secs(1), "not waited for again");
}
