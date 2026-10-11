//! One way to take a lock. In release builds it's `Mutex::lock`. In debug and test builds a lock that
//! isn't free within five seconds is a bug, not a wait (nothing in Cue holds one that long): the thread
//! panics naming the lock, so a lock taken twice on one thread (the guard of a lookup kept alive by a
//! longer expression, a `for` or `match` over a locked value) fails a test or a dev run at once, where
//! the standard mutex would hang the app with no word. Twice now that hang reached the installed app.
//! A lock another thread panicked while holding is taken anyway: what's in it is whole (each change is
//! made under one lock), and refusing it would stop every later lock too.

use std::sync::{Mutex, MutexGuard, TryLockError};

/// `m` locked. `what`: the lock's name, for the panic.
pub fn lock<'a, T>(m: &'a Mutex<T>, what: &str) -> MutexGuard<'a, T> {
    if cfg!(debug_assertions) {
        let t0 = std::time::Instant::now();
        loop {
            match m.try_lock() {
                Ok(g) => return g,
                Err(TryLockError::Poisoned(p)) => return p.into_inner(),
                Err(TryLockError::WouldBlock) => {}
            }
            if t0.elapsed() > std::time::Duration::from_secs(5) {
                panic!("cue: {what} has been held for 5s: taken twice on one thread, or a deadlock");
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    } else {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_taken_twice_on_one_thread_fails_by_name_instead_of_hanging() {
        static M: Mutex<u32> = Mutex::new(0);
        let _held = lock(&M, "M");
        // 5 s, on purpose: this is what a deadlock costs a test now, in place of forever.
        let again = std::panic::catch_unwind(|| drop(lock(&M, "M")));
        let msg = again.unwrap_err().downcast::<String>().map(|s| *s).unwrap_or_default();
        assert!(msg.contains("M has been held for 5s"), "{msg}");
    }

    /// The shape behind both hangs: a lock taken in the head of a `for`, `match`, `if let` or `while let`
    /// stays held through the body (Rust 2021), and a lock taken inside a closure of the same expression
    /// is taken with the first still held. Both are refused here, over every source file: bind the
    /// lookup with `let` and let it drop, then do the next thing.
    #[test]
    fn no_lock_is_taken_in_the_head_of_a_block_or_twice_in_one_expression() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let is_lock = |l: &str| l.contains("locking::lock(") || l.contains(".lock()") || l.contains(".st()");
        let mut bad = vec![];
        // Every `.rs` file under src/, folders included.
        fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    rs_files(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut files = vec![];
        rs_files(&dir, &mut files);
        files.sort();
        for path in files {
            // Every file but this one (its patterns name the shapes).
            if path.extension().is_none_or(|e| e != "rs") || path.file_name().is_some_and(|n| n == "locking.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                // Twice in one statement: a second lock after the first on the same line.
                if let Some(first) = ["locking::lock(", ".lock()", ".st()"].iter().filter_map(|p| t.find(p)).min() {
                    let rest = &t[first + 5..];
                    if rest.contains("locking::lock(") || rest.contains(".lock()") || rest.contains(".st()") {
                        bad.push(format!("{}:{}: two locks in one expression", path.display(), i + 1));
                    }
                }
                // In the head of a block whose body locks again.
                let head = ["for ", "match ", "if let ", "while let "].iter().any(|k| t.starts_with(k)) && is_lock(t) && t.ends_with('{');
                if head {
                    let mut depth = 0i32;
                    for body in &lines[i + 1..] {
                        depth += body.matches('{').count() as i32 - body.matches('}').count() as i32;
                        if depth < 0 {
                            break;
                        }
                        if is_lock(body) || body.contains(".changed()") {
                            bad.push(format!("{}:{}: a lock in the head of a block that locks again", path.display(), i + 1));
                            break;
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "lock taken while held (see locking.rs):\n{}", bad.join("\n"));
    }

    #[test]
    fn a_lock_a_panicking_thread_held_is_still_taken() {
        static P: Mutex<u32> = Mutex::new(1);
        let _ = std::thread::spawn(|| { let _g = P.lock().unwrap(); panic!("while holding it"); }).join();
        assert_eq!(*lock(&P, "P"), 1);
    }
}
