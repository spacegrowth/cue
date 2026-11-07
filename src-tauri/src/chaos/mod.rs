//! Chaos knobs: the soak harness (testing/soak, tests/e2e/soak.js) turns the outside world slow or broken
//! to see Cue keep answering. Each is an environment variable read once:
//! - CUE_CHAOS_TMUX_DELAY_MS: every tmux command this Mac runs answers that much later.
//! - CUE_CHAOS_SSH_DROP_EVERY: every that many seconds the line to every machine drops (`ssh`).
//! - CUE_CHAOS_XPC_DELAY_MS: every trip to Notification Center or the login-item service takes that much longer.
//! - CUE_CHAOS_DB_LOCK_SECS: another program holds cue.db for that many seconds, every 30 s (`db`).
//! - CUE_CHAOS_FS_DELAY_MS: every read of a transcript waits that much first.
//!
//! Debug builds only: in a release build `get` is always None, and the variables' names aren't in the
//! binary at all (tests/run.sh soak proves it with `strings`). Unset, zero or unreadable: off.

pub mod db;
pub mod ssh;

/// One knob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    TmuxDelayMs,
    SshDropEvery,
    XpcDelayMs,
    DbLockSecs,
    FsDelayMs,
}

/// The knob's setting, when it's on.
pub fn get(knob: Knob) -> Option<u64> {
    #[cfg(debug_assertions)]
    return set::read(knob);
    #[cfg(not(debug_assertions))]
    {
        let _ = knob;
        None
    }
}

/// A delay knob's wait (a real one: it stands for a slow program, which no clock of Cue's can hurry).
pub fn pause(knob: Knob) {
    if let Some(ms) = get(knob) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// What runs on its own for the knobs that are on: the line to the machines dropping, cue.db held.
pub fn start() {
    ssh::start();
    db::start();
}

/// The knobs as the environment sets them, read once. Only in a debug build.
#[cfg(debug_assertions)]
mod set {
    use super::Knob;
    use std::sync::OnceLock;

    const ALL: [(Knob, &str); 5] = [
        (Knob::TmuxDelayMs, "CUE_CHAOS_TMUX_DELAY_MS"),
        (Knob::SshDropEvery, "CUE_CHAOS_SSH_DROP_EVERY"),
        (Knob::XpcDelayMs, "CUE_CHAOS_XPC_DELAY_MS"),
        (Knob::DbLockSecs, "CUE_CHAOS_DB_LOCK_SECS"),
        (Knob::FsDelayMs, "CUE_CHAOS_FS_DELAY_MS"),
    ];

    pub(super) fn read(knob: Knob) -> Option<u64> {
        static SET: OnceLock<Vec<(Knob, u64)>> = OnceLock::new();
        let set = SET.get_or_init(|| ALL.iter().filter_map(|(k, name)| Some((*k, parse(&std::env::var(name).ok()?)?))).collect());
        set.iter().find(|(k, _)| *k == knob).map(|(_, v)| *v)
    }

    /// A setting: a whole number above zero, else off.
    pub(super) fn parse(raw: &str) -> Option<u64> {
        raw.trim().parse().ok().filter(|v| *v > 0)
    }

    /// Every knob's variable and setting, for the soak's report.
    pub(crate) fn named() -> Vec<(&'static str, u64)> {
        ALL.iter().filter_map(|(k, name)| Some((*name, read(*k)?))).collect()
    }
}

/// The knobs that are on, by their variables' names (none in a release build).
pub fn named() -> Vec<(&'static str, u64)> {
    #[cfg(debug_assertions)]
    return set::named();
    #[cfg(not(debug_assertions))]
    Vec::new()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_knob_is_a_whole_number_above_zero_or_off() {
        assert_eq!(super::set::parse(" 250 "), Some(250));
        assert_eq!(super::set::parse("0"), None);
        assert_eq!(super::set::parse("slow"), None);
        assert_eq!(super::set::parse("-5"), None);
    }
}
