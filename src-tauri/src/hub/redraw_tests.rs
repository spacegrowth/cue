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
