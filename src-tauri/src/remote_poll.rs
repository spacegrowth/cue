//! When each of your machines has its crews read (relay's and pi-lead's listing there, one SSH call).
//! Every machine is read, whether or not Cue has anything open there: every 10 s where it has a live
//! session, every minute elsewhere. A machine that can't be reached is tried less and less often (up to
//! every 5 minutes) until it answers again, and it never holds the others up: each read runs on its
//! own, and a machine still being read isn't started again.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Where Cue has a live session.
pub const NEAR: Duration = Duration::from_secs(10);
/// Where it has none.
pub const FAR: Duration = Duration::from_secs(60);
/// The longest wait for a machine that keeps failing.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// A tick that comes a moment early still counts (the clock that ticks isn't the one that stamps).
const SLACK: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Slot {
    /// When its last read started (None: never read).
    last: Option<Instant>,
    /// Reads failed in a row.
    fails: u32,
    running: bool,
}

#[derive(Default)]
pub struct Schedule {
    slots: HashMap<String, Slot>,
}

/// How long after a read started the next one is due.
fn wait(live_here: bool, fails: u32) -> Duration {
    let normal = if live_here { NEAR } else { FAR };
    if fails == 0 {
        return normal;
    }
    // 20 s, 40 s, 80 s … up to MAX_BACKOFF, never sooner than the machine's normal pace.
    let backoff = NEAR.saturating_mul(1u32 << fails.min(8)).min(MAX_BACKOFF);
    backoff.max(normal)
}

impl Schedule {
    /// The machines to read now, from (name, whether Cue has a live session there): each is marked as
    /// being read until `done`. Machines no longer listed are dropped.
    pub fn due(&mut self, machines: &[(String, bool)], now: Instant) -> Vec<String> {
        self.slots.retain(|name, _| machines.iter().any(|(n, _)| n == name));
        let mut out = vec![];
        for (name, live_here) in machines {
            let slot = self.slots.entry(name.clone()).or_default();
            if slot.running {
                continue;
            }
            if slot.last.is_some_and(|t| now + SLACK < t + wait(*live_here, slot.fails)) {
                continue;
            }
            slot.running = true;
            slot.last = Some(now);
            out.push(name.clone());
        }
        out
    }

    /// A read finished (`ok`: the machine answered). Some(false) when the machine just became
    /// unreachable, Some(true) when it answered again after failing, else None: one log line per
    /// change, not one per try.
    pub fn done(&mut self, name: &str, ok: bool) -> Option<bool> {
        let slot = self.slots.get_mut(name)?;
        slot.running = false;
        let was = slot.fails;
        slot.fails = if ok { 0 } else { slot.fails.saturating_add(1) };
        match (was, ok) {
            (0, false) => Some(false),
            (n, true) if n > 0 => Some(true),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, s: u64) -> Instant {
        t0 + Duration::from_secs(s)
    }

    #[test]
    fn a_machine_with_a_live_session_is_read_every_10s_and_one_without_every_minute() {
        let mut s = Schedule::default();
        let t0 = Instant::now();
        let ms = vec![("busy".to_string(), true), ("quiet".to_string(), false)];
        // At launch: both (Cue has nothing open on "quiet", it's read all the same).
        assert_eq!(s.due(&ms, t0), vec!["busy", "quiet"]);
        s.done("busy", true);
        s.done("quiet", true);
        let mut reads: HashMap<&str, u32> = HashMap::new();
        // Ten-second ticks for two minutes.
        for k in 1..=12 {
            for m in s.due(&ms, at(t0, 10 * k)) {
                *reads.entry(if m == "busy" { "busy" } else { "quiet" }).or_default() += 1;
                s.done(&m, true);
            }
        }
        assert_eq!(reads.get("busy"), Some(&12), "every tick where Cue has a live session");
        assert_eq!(reads.get("quiet"), Some(&2), "once a minute elsewhere");
    }

    #[test]
    fn a_session_opened_on_a_quiet_machine_brings_it_back_to_10s() {
        let mut s = Schedule::default();
        let t0 = Instant::now();
        assert_eq!(s.due(&[("m".into(), false)], t0).len(), 1);
        s.done("m", true);
        assert!(s.due(&[("m".into(), false)], at(t0, 10)).is_empty(), "a minute's pace");
        assert_eq!(s.due(&[("m".into(), true)], at(t0, 10)), vec!["m"], "something live there now: 10 s");
    }

    #[test]
    fn a_machine_still_being_read_isnt_started_again_and_others_go_on() {
        let mut s = Schedule::default();
        let t0 = Instant::now();
        let ms = vec![("down".to_string(), true), ("up".to_string(), true)];
        assert_eq!(s.due(&ms, t0).len(), 2);
        s.done("up", true);
        // "down" hangs (its SSH waits out its connect timeout): only "up" is read on the next ticks.
        assert_eq!(s.due(&ms, at(t0, 10)), vec!["up"]);
        s.done("up", true);
        assert_eq!(s.due(&ms, at(t0, 20)), vec!["up"]);
    }

    #[test]
    fn an_unreachable_machine_is_tried_less_often_and_logged_once() {
        let mut s = Schedule::default();
        let t0 = Instant::now();
        let ms = vec![("down".to_string(), true)];
        let mut tries = vec![];
        let mut changes = vec![];
        for k in 0..=120 {
            for m in s.due(&ms, at(t0, 10 * k)) {
                tries.push(10 * k);
                if let Some(c) = s.done(&m, false) {
                    changes.push(c);
                }
            }
        }
        // 0, then 20 s, 40 s, 80 s, 160 s later, then every 5 minutes.
        assert_eq!(&tries[..6], &[0, 20, 60, 140, 300, 600], "{tries:?}");
        assert!(tries.windows(2).skip(4).all(|w| w[1] - w[0] == 300), "never more than every 5 minutes: {tries:?}");
        assert_eq!(changes, vec![false], "one line when it went, none per try");
        // It answers again: back to 10 s, and one line saying so.
        let next = s.due(&ms, at(t0, 10_000));
        assert_eq!(next.len(), 1);
        assert_eq!(s.done("down", true), Some(true));
        assert_eq!(s.due(&ms, at(t0, 10_010)).len(), 1, "10 s pace again");
    }

    #[test]
    fn a_removed_machine_is_dropped() {
        let mut s = Schedule::default();
        let t0 = Instant::now();
        s.due(&[("a".into(), true), ("b".into(), true)], t0);
        s.due(&[("a".into(), true)], t0);
        assert_eq!(s.done("b", true), None);
        assert!(!s.slots.contains_key("b"));
    }
}
