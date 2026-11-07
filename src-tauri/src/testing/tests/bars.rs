//! A session's activity bar in the window's state over a long run (stability plan §5, the nightly soak):
//! what the state carries of it levels off once the bar's 30 minutes have passed, however long the session
//! keeps working, so the state (sent on every change) stops growing with it.

use crate::testing::World;

/// A busy session's rhythm, as the soak drives it: a turn of 1.5–4.5 s, then a pause of 2–6 s, round after
/// round for `minutes`; its bar's stretches (in the state) and the session's size there, every 15 minutes.
fn busy(w: &World, minutes: u64) -> Vec<(u64, usize, usize)> {
    let a = w.session("a");
    let mut seen = vec![];
    let (mut t, mut round) = (0u64, 0u64);
    while t < minutes * 60_000 {
        a.prompt(&format!("task {round}"));
        let work = 1_500 + (round * 7_919) % 3_000;
        w.advance(work);
        a.stop("Done.");
        let pause = 2_000 + (round * 104_729) % 4_000;
        w.advance(pause);
        let before = t / 900_000;
        t += work + pause;
        round += 1;
        if t / 900_000 > before {
            let s = w.snapshot().session("a").clone();
            seen.push((t / 60_000, s["segments"].as_array().map_or(0, Vec::len), s.to_string().len()));
        }
    }
    seen
}

#[test]
fn a_busy_sessions_bar_in_the_state_levels_off_after_its_30_minutes() {
    let w = World::new();
    let seen = busy(&w, 120);
    let at = |m: u64| seen.iter().find(|(t, ..)| *t >= m).copied().unwrap();
    let (_, bar_30, size_30) = at(30);
    for (minute, bar, size) in seen.iter().filter(|(t, ..)| *t >= 45) {
        assert!(*bar <= bar_30 + bar_30 / 10, "minute {minute}: {bar} stretches, {bar_30} at 30 ({seen:?})");
        assert!(*size <= size_30 + size_30 / 10, "minute {minute}: {size} bytes, {size_30} at 30 ({seen:?})");
    }
}
