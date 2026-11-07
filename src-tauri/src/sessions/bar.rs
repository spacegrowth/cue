//! A session's activity bar as the window gets it: the last 30 minutes of its stretches (working, waiting on
//! you, waiting on something else), with the ones too short for the bar to draw gathered up. The bar draws
//! nothing narrower than BAR_SLOT_MS (ui/core.js `bar`: 0.6 % of its width), so a run of shorter stretches
//! (a session switching between working and waiting every few seconds) goes as one stretch per slot, of the
//! kind that held most of it. Longer stretches go as they are. A session's bar is then at most a few hundred
//! stretches, whatever it did, instead of one per change: the state goes out on every change.

use super::{Segment, WINDOW_MS};
use std::collections::HashMap;

/// The narrowest stretch the bar draws: 0.6 % of its 30 minutes.
pub const BAR_SLOT_MS: u64 = WINDOW_MS * 6 / 1000;

/// `segments` clipped to start at `from`, short ones gathered (see the module's note). `now` ends the one
/// still going, for measuring it.
pub fn shown(segments: &[Segment], from: u64, now: u64) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut run = Run::default();
    for g in segments.iter().filter(|g| g.end_ms.is_none_or(|e| e > from)) {
        let start = g.start_ms.max(from);
        let end = g.end_ms.unwrap_or(now);
        if g.end_ms.is_none() || end.saturating_sub(start) >= BAR_SLOT_MS {
            run.close(&mut out);
            out.push(Segment { kind: g.kind.clone(), start_ms: start, end_ms: g.end_ms });
            continue;
        }
        run.add(&g.kind, start, end);
        if run.span() >= BAR_SLOT_MS {
            run.close(&mut out);
        }
    }
    run.close(&mut out);
    out
}

/// Short stretches gathered so far: where they start and end, and how long each kind held.
#[derive(Default)]
struct Run {
    start: u64,
    end: u64,
    held: HashMap<String, u64>,
}

impl Run {
    fn add(&mut self, kind: &str, start: u64, end: u64) {
        if self.held.is_empty() {
            self.start = start;
        }
        self.end = self.end.max(end);
        *self.held.entry(kind.to_string()).or_default() += end.saturating_sub(start);
    }

    fn span(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// The run as one stretch of the kind that held it longest (none, if nothing's gathered).
    fn close(&mut self, out: &mut Vec<Segment>) {
        let held = std::mem::take(&mut self.held);
        if let Some((kind, _)) = held.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))) {
            out.push(Segment { kind, start_ms: self.start, end_ms: Some(self.end) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: &str, start: u64, end: Option<u64>) -> Segment {
        Segment { kind: kind.into(), start_ms: start, end_ms: end }
    }

    #[test]
    fn long_stretches_go_as_they_are_clipped_to_the_window() {
        let from = 1_000_000;
        let segs = [seg("working", from - 60_000, Some(from + 30_000)), seg("waiting", from + 30_000, Some(from + 90_000)), seg("working", from + 90_000, None)];
        let shown = shown(&segs, from, from + 100_000);
        assert_eq!(shown.len(), 3);
        assert_eq!((shown[0].start_ms, shown[0].end_ms), (from, Some(from + 30_000)));
        assert_eq!(shown[2].end_ms, None, "the one still going stays open");
    }

    #[test]
    fn a_run_of_short_stretches_is_one_per_slot_of_the_kind_that_held_most() {
        let from = 0;
        // Five minutes of 3 s working, 4 s waiting.
        let mut segs = Vec::new();
        let mut t = 0;
        while t < 300_000 {
            segs.push(seg("working", t, Some(t + 3_000)));
            segs.push(seg("waiting", t + 3_000, Some(t + 7_000)));
            t += 7_000;
        }
        let shown = shown(&segs, from, t);
        assert!(shown.len() <= (300_000 / BAR_SLOT_MS + 1) as usize, "{}", shown.len());
        assert!(shown.iter().all(|g| g.kind == "waiting"), "waiting held most of each slot");
        assert_eq!(shown.first().unwrap().start_ms, 0);
        assert_eq!(shown.last().unwrap().end_ms, Some(t));
        // A long stretch after them ends the run there.
        segs.push(seg("working", t, Some(t + 60_000)));
        let after = super::shown(&segs, from, t + 60_000);
        assert_eq!(after.last().map(|g| (g.kind.as_str(), g.start_ms)), Some(("working", t)));
    }
}
