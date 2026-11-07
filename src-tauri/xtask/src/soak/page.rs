//! The one-page report: for each run, its verdict, how fast the window answered (p50/p99), events a
//! second, the waits for each busy lock, how memory and the window's state grew, the hooks' decisions,
//! and every failure in words.

use super::Run;
use serde_json::Value;
use std::fmt::Write;

/// The page for these runs.
pub fn page(runs: &[Run]) -> String {
    let mut out = String::new();
    for r in runs {
        section(&mut out, r);
    }
    let failed = runs.iter().any(|r| !r.failures().is_empty());
    let _ = writeln!(out, "Verdict: {}", if failed { "FAIL" } else { "PASS" });
    out
}

fn section(out: &mut String, r: &Run) {
    let failures = r.failures();
    let minutes = r.num("/seconds") / 60.0;
    let chaos = chaos(&r.0["chaos"]);
    let _ = writeln!(out, "== {} soak: {minutes:.1} min, {} sessions, chaos: {chaos} — {}", r.name(), r.num("/sessions"), if failures.is_empty() { "PASS" } else { "FAIL" });
    answers(out, r);
    rates(out, r);
    locks(out, r);
    curve(out, "memory (MB)", &r.curve("/memory"), 1e6);
    curve(out, "window state (KB)", &r.curve("/snapshot"), 1e3);
    decisions(out, r);
    let _ = writeln!(out, "  failures: {}", if failures.is_empty() { "none".to_string() } else { String::new() });
    for f in failures {
        let _ = writeln!(out, "    - {f}");
    }
    out.push('\n');
}

fn chaos(knobs: &Value) -> String {
    match knobs.as_object() {
        Some(k) if !k.is_empty() => k.iter().map(|(n, v)| format!("{}={v}", n.trim_start_matches("CUE_CHAOS_"))).collect::<Vec<_>>().join(" "),
        _ => "none".into(),
    }
}

/// p50/p99/max of `ms`, "n=0" for none.
fn spread(ms: &[f64]) -> String {
    if ms.is_empty() {
        return "n=0".into();
    }
    let mut s = ms.to_vec();
    s.sort_by(f64::total_cmp);
    let at = |q: f64| s[((s.len() - 1) as f64 * q).round() as usize];
    format!("n={} p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms", s.len(), at(0.5), at(0.99), at(1.0))
}

fn answers(out: &mut String, r: &Run) {
    let what = r.0["answer_what"].as_str().unwrap_or("window answer");
    let _ = writeln!(out, "  {what}: {}", spread(&r.samples("/answer_ms")));
    let pings = r.num("/probe/main_thread/pings");
    if pings > 0.0 {
        let p = |k: &str| r.num(&format!("/probe/main_thread/{k}"));
        let _ = writeln!(out, "  main-thread probe pings: n={pings} p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms", p("p50_ms"), p("p99_ms"), p("max_ms"));
    }
}

fn rates(out: &mut String, r: &Run) {
    let secs = r.num("/seconds").max(1.0);
    let per = |ptr: &str| r.num(ptr) / secs;
    let _ = writeln!(
        out,
        "  per second: hook events {:.1} ({} failed), typing {:.1} ({} refused), list polls {:.1}",
        per("/events/sent"),
        r.num("/events/failed"),
        per("/typing/sent"),
        r.num("/typing/failed"),
        per("/polls"),
    );
    // What only some runs do (the e2e run opens terminals, the Rust run's machine drops its line).
    let counted: Vec<String> = [("asks", "/asks"), ("terminals opened", "/terminals/opened"), ("terminals closed", "/terminals/closed"), ("machine drops", "/machine_drops")]
        .iter()
        .filter(|(_, ptr)| r.0.pointer(ptr).is_some())
        .map(|(what, ptr)| format!("{what} {}", r.num(ptr)))
        .collect();
    let _ = writeln!(out, "  in all: {}", counted.join(", "));
    if let Some(why) = r.0.pointer("/typing/refused").and_then(Value::as_object).filter(|w| !w.is_empty()) {
        let mut why: Vec<_> = why.iter().map(|(k, n)| (n.as_u64().unwrap_or(0), k)).collect();
        why.sort_by(|a, b| b.0.cmp(&a.0));
        let _ = writeln!(out, "  typing refused: {}", why.iter().take(3).map(|(n, k)| format!("{n}× \"{k}\"")).collect::<Vec<_>>().join("; "));
    }
}

fn locks(out: &mut String, r: &Run) {
    let locks = r.locks();
    if locks.is_empty() {
        let _ = writeln!(out, "  lock waits: no lock was ever busy");
        return;
    }
    let _ = writeln!(out, "  lock waits (busy locks only, longest first):");
    for (name, waits, max, spread) in locks.iter().take(15) {
        let buckets = spread.as_object().map(|b| b.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let _ = writeln!(out, "    {name:<28} {waits:>7} waits  max {max:>8.1} ms  {buckets}");
    }
    if locks.len() > 15 {
        let _ = writeln!(out, "    … and {} more, none longer", locks.len() - 15);
    }
}

/// A curve as a few points: its start, then every tenth of the run.
fn curve(out: &mut String, what: &str, c: &[(f64, f64)], unit: f64) {
    if c.is_empty() {
        return;
    }
    let step = (c.len() / 10).max(1);
    let points: Vec<String> = c.iter().step_by(step).chain(c.last()).map(|(t, v)| format!("{}:{:02} {:.1}", (*t as u64) / 60, (*t as u64) % 60, v / unit)).collect();
    let _ = writeln!(out, "  {what}: {}", points.join(" | "));
}

fn decisions(out: &mut String, r: &Run) {
    let _ = writeln!(out, "  decisions heard by their hooks: {}; never heard: {}", spread(&r.samples("/decisions_ms")), r.num("/undecided"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_page_shows_each_run_and_the_verdict() {
        let r = Run(json!({"run": "rust", "seconds": 600, "sessions": 40, "chaos": {"CUE_CHAOS_TMUX_DELAY_MS": 200}, "answer_what": "list over the socket",
            "answer_ms": [1, 2, 3, 900], "events": {"sent": 6000, "failed": 0}, "probe": {"locks": {"store": {"waits": 3, "max_ms": 12.5, "spread": {"<50ms": 3}}}},
            "memory": [[0, 40e6], [600, 45e6]]}));
        let p = page(&[r]);
        for part in ["== rust soak: 10.0 min, 40 sessions, chaos: TMUX_DELAY_MS=200 — FAIL", "list over the socket: n=4 p50 3.0 ms", "hook events 10.0", "store", "<50ms:3", "memory (MB): 0:00 40.0", "Verdict: FAIL"] {
            assert!(p.contains(part), "{part} in:\n{p}");
        }
    }
}
