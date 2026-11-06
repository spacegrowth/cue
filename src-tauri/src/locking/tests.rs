use super::*;
use std::sync::mpsc;

/// A thread named `name` that holds `m` until told to let go; returns once it holds it.
fn hold(m: &'static Mutex<u32>, what: &'static str, name: &str) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (holding, let_go) = (mpsc::channel(), mpsc::channel::<()>());
    let t = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _g = lock(m, what);
            holding.0.send(()).unwrap();
            let _ = let_go.1.recv();
        })
        .unwrap();
    holding.1.recv().unwrap();
    (let_go.0, t)
}

fn panic_message(r: std::thread::Result<()>) -> String {
    let e = r.unwrap_err();
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

#[test]
fn a_lock_taken_twice_on_one_thread_fails_by_name_at_once_instead_of_hanging() {
    static M: Mutex<u32> = Mutex::new(0);
    let held = lock(&M, "reentry test lock");
    let t0 = Instant::now();
    let again = std::panic::catch_unwind(|| drop(lock(&M, "reentry test lock")));
    assert!(panic_message(again).contains("LOCK REENTRY reentry test lock"));
    assert!(t0.elapsed() < Duration::from_secs(1), "caught before waiting, not after five seconds");
    assert!(report::said().iter().any(|e| e.contains("LOCK REENTRY reentry test lock") && e.lines().count() > 1), "logged, with a backtrace");
    // The lock held before is still held, and let go as usual.
    drop(held);
    assert_eq!(*lock(&M, "reentry test lock"), 0);
}

#[test]
fn two_different_locks_on_one_thread_are_not_a_reentry() {
    static A: Mutex<u32> = Mutex::new(1);
    static B: Mutex<u32> = Mutex::new(2);
    let a = lock(&A, "nesting test A");
    let b = lock(&B, "nesting test B");
    assert_eq!(*a + *b, 3);
}

#[test]
fn a_long_wait_is_logged_naming_the_holder() {
    static W: Mutex<u32> = Mutex::new(0);
    let (let_go, holder) = hold(&W, "watchdog test lock", "watchdog-test holder");
    let patience = Patience { report_after: Duration::from_millis(50), panic_after: None };
    let waiter = std::thread::spawn(move || *lock_with(&W, "watchdog test lock", patience) + 1);
    std::thread::sleep(Duration::from_millis(200));
    let_go.send(()).unwrap();
    holder.join().unwrap();
    assert_eq!(waiter.join().unwrap(), 1, "it gets the lock once let go");
    let said = report::said();
    let wait = said.iter().find(|e| e.contains("LOCK WAIT watchdog test lock")).expect("the long wait is logged");
    assert!(wait.contains("held by watchdog-test holder"), "{wait}");
}

#[test]
fn a_wait_past_the_limit_in_debug_fails_naming_the_holder() {
    static D: Mutex<u32> = Mutex::new(0);
    let (let_go, holder) = hold(&D, "deadlock test lock", "deadlock-test holder");
    let patience = Patience { report_after: Duration::from_millis(20), panic_after: Some(Duration::from_millis(100)) };
    let waited = std::thread::spawn(move || drop(lock_with(&D, "deadlock test lock", patience))).join();
    let_go.send(()).unwrap();
    holder.join().unwrap();
    let msg = panic_message(waited);
    assert!(msg.contains("deadlock test lock has been held") && msg.contains("deadlock-test holder"), "{msg}");
}

#[test]
fn a_lock_let_go_is_no_longer_held_by_anyone() {
    static F: Mutex<u32> = Mutex::new(0);
    drop(lock(&F, "let go test lock"));
    assert!(!held::all().iter().any(|h| h.starts_with("let go test lock")));
    let g = lock(&F, "let go test lock");
    assert!(held::all().iter().any(|h| h.starts_with("let go test lock held by")));
    drop(g);
}

#[test]
fn a_condvar_wait_lets_the_lock_go_and_takes_it_back() {
    static C: Mutex<bool> = Mutex::new(false);
    static READY: Condvar = Condvar::new();
    let setter = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        *lock(&C, "condvar test lock") = true;
        READY.notify_all();
    });
    let mut g = lock(&C, "condvar test lock");
    while !*g {
        g = Guard::wait(g, &READY);
    }
    // Held again, by this thread, as a reentry check sees it.
    assert!(held::by_this_thread(&C as *const Mutex<bool> as usize));
    drop(g);
    setter.join().unwrap();
}

#[test]
fn a_lock_a_panicking_thread_held_is_still_taken() {
    static P: Mutex<u32> = Mutex::new(1);
    let _ = std::thread::spawn(|| {
        let _g = lock(&P, "poisoned test lock");
        panic!("while holding it");
    })
    .join();
    assert_eq!(*lock(&P, "poisoned test lock"), 1);
}
