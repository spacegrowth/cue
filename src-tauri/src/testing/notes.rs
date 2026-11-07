//! Notification Center as the World has it: every notification posted and every one taken back, by id.
//! Told to lag (`lags`, CUE_CHAOS_XPC_DELAY_MS in the soak), it's as slow as an XPC trip that answers late,
//! on a thread of its own as the real one has (notify::later): whoever posts never waits on it.

use crate::hub::ports::Notes;
use crate::notify::Note;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

type Trip = Box<dyn FnOnce(&Seen) + Send>;

#[derive(Default)]
pub struct FakeNotes {
    seen: Arc<Seen>,
    lag_ms: AtomicU64,
    /// Its own thread's queue, once it lags: the trips in order, each `lag_ms` late.
    queue: OnceLock<mpsc::Sender<(u64, Trip)>>,
}

#[derive(Default)]
struct Seen {
    shown: Mutex<Vec<String>>,
    taken_back: Mutex<Vec<String>>,
}

impl FakeNotes {
    /// The ids posted, in order.
    pub fn shown(&self) -> Vec<String> {
        crate::locking::lock(&self.seen.shown, "fake notes shown").clone()
    }

    /// The ids taken back, in order.
    pub fn taken_back(&self) -> Vec<String> {
        crate::locking::lock(&self.seen.taken_back, "fake notes taken back").clone()
    }

    /// Every trip answers `ms` late from now on.
    pub fn lags(&self, ms: u64) {
        self.lag_ms.store(ms, Ordering::SeqCst);
    }

    /// `f` on what's been seen: at once, or after the lag on Notification Center's thread.
    fn trip(&self, f: impl FnOnce(&Seen) + Send + 'static) {
        let ms = self.lag_ms.load(Ordering::SeqCst);
        if ms == 0 {
            return f(&self.seen);
        }
        let queue = self.queue.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<(u64, Trip)>();
            let seen = self.seen.clone();
            std::thread::spawn(move || {
                for (ms, trip) in rx {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    trip(&seen);
                }
            });
            tx
        });
        let _ = queue.send((ms, Box::new(f)));
    }
}

impl Notes for FakeNotes {
    fn show(&self, note: Note) {
        self.trip(move |s| crate::locking::lock(&s.shown, "fake notes shown").push(note.id));
    }

    fn take_back(&self, ids: Vec<String>) {
        self.trip(move |s| crate::locking::lock(&s.taken_back, "fake notes taken back").extend(ids));
    }
}
