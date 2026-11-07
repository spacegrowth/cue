//! What a function's body does that the lock and I/O analysis cares about, in the order it does it,
//! each with the locks the body itself holds at that moment (`held`, by lock name).

use crate::parse::index::FnId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    /// `std::fs`/`tokio::fs`, or a `Path` method that stats the disk (`exists`, `metadata`, …).
    Fs,
    /// A call into `crate::db` (SQLite).
    Db,
    /// `Command::new`: a child process (osascript and ssh among them).
    Process,
    /// Notification Center over XPC (`UNUserNotificationCenter`).
    Notify,
    /// Login items over XPC (`SMAppService`).
    Login,
    /// A Unix socket (`UnixStream`, `UnixListener`).
    Socket,
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Fs => "fs",
            Effect::Db => "db",
            Effect::Process => "process",
            Effect::Notify => "notify",
            Effect::Login => "login",
            Effect::Socket => "socket",
        }
    }

    pub const ALL: [Effect; 6] = [Effect::Fs, Effect::Db, Effect::Process, Effect::Notify, Effect::Login, Effect::Socket];

    pub fn parse(s: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|e| e.name() == s)
    }
}

#[derive(Clone, Debug)]
pub enum Ev {
    /// A lock taken: `locking::lock/read/write(&x, "name")`, or a lock taken another way on purpose.
    Lock { name: String, line: usize, held: Vec<String> },
    /// A crate function called (a function that hands out a guard is a call too: its lock is in what it reaches).
    Call { target: FnId, line: usize, held: Vec<String> },
    /// The function's own parameter `index`, a closure, called.
    ParamCall { index: usize, line: usize, held: Vec<String> },
    /// The function's own callable parameter `index` passed on to `target`'s parameter `target_index`.
    ParamPass { index: usize, target: FnId, target_index: usize, line: usize, held: Vec<String> },
    Effect { effect: Effect, detail: String, line: usize, held: Vec<String> },
    /// Code that runs somewhere else than in line: a closure handed to a thread, or to a crate function
    /// (which may call it at once, under its own locks, or keep it for later). `held`: the locks held
    /// where it's handed over; its events' `held` start empty.
    Frame { frame: Frame, line: usize, held: Vec<String>, events: Vec<Ev> },
}

#[derive(Clone, Debug)]
pub enum Frame {
    /// Handed to a thread or task (`thread::spawn`, `spawn_blocking`, …), named by the spawning call.
    Spawn(String),
    /// Passed as argument `index` of the crate function `target`.
    ClosureArg { target: FnId, index: usize },
    /// A closure bound to a name that the body never calls nor passes on in a way this follows.
    Unfollowed,
}

/// What one function's body says.
#[derive(Default)]
pub struct Facts {
    pub events: Vec<Ev>,
    /// Lock acquisitions, for the declaration ↔ name table: (line, lock name, identifiers in its first argument).
    pub lock_sites: Vec<LockSite>,
    /// Locks taken another way than `locking::lock` (line, the receiver as written).
    pub bare_locks: Vec<(usize, String)>,
    /// Calls that may be crate calls but couldn't be resolved: a path that leads nowhere known, a
    /// closure called through an expression, or a method of unknown receiver whose name some crate type
    /// has (calls of standard-library-only method names aren't counted).
    pub unresolved: usize,
    /// Calls of crate functions, followed.
    pub followed: usize,
    /// Calls of a path outside the crate (or a constructor named like one): classified, not followed.
    pub outside: usize,
}

pub struct LockSite {
    pub line: usize,
    pub name: String,
    pub idents: Vec<String>,
}

/// Every event in `events`, frames' events included, in order.
pub fn each(events: &[Ev], f: &mut impl FnMut(&Ev)) {
    for ev in events {
        f(ev);
        if let Ev::Frame { events, .. } = ev {
            each(events, f);
        }
    }
}
