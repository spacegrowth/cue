//! Today at a glance (the line atop Insights, ui/history.js `histStats`), worked out here from every answer of
//! the last day: the window's state carries these few numbers instead of the answers themselves, which grow
//! all day. Days and hours are this Mac's local ones, as the window shows them.

use crate::model::Answer;
use serde_json::{json, Value};

const HOUR_MS: u64 = 3_600_000;
/// The chart's hours: the current one and the eleven before it.
const HOURS: u64 = 12;

/// Today's numbers as of `now`: how many answered since midnight, their median and longest wait (and whose the
/// longest was), how many were replies, allowed or answered, and denied, and the answers in each of the last
/// HOURS hours (each with its start).
pub(super) fn today(answers: &[Answer], now: u64) -> Value {
    let midnight = local_start(now, false);
    let today: Vec<&Answer> = answers.iter().filter(|a| a.resolved_ms >= midnight).collect();
    let mut waits: Vec<(u64, &Answer)> = today.iter().filter(|a| a.resolved_ms >= a.created_ms).map(|a| (a.resolved_ms - a.created_ms, *a)).collect();
    waits.sort_by_key(|(w, _)| *w);
    let longest = waits.last().map(|(w, a)| json!({ "wait_ms": w, "session_id": a.session_id, "project": a.project }));
    let count = |starts: &[&str]| today.iter().filter(|a| starts.iter().any(|s| a.outcome.starts_with(s))).count();
    let hour0 = local_start(now, true);
    let hours: Vec<Value> = (0..HOURS)
        .map(|k| {
            let from = hour0.saturating_sub((HOURS - 1 - k) * HOUR_MS);
            json!({ "from_ms": from, "n": answers.iter().filter(|a| a.resolved_ms >= from && a.resolved_ms < from + HOUR_MS).count() })
        })
        .collect();
    json!({
        "answered": today.len(),
        "median_wait_ms": waits.get(waits.len() / 2).map(|(w, _)| *w),
        "longest": longest,
        "replied": count(&["replied"]), "allowed": count(&["allowed", "answered"]), "denied": count(&["denied"]),
        "hours": hours,
    })
}

/// When today's numbers next change with time alone: the next hour starts (a new bar; at midnight, a new day).
pub(super) fn next_change(now: u64) -> u64 {
    local_start(now, true) + HOUR_MS
}

/// The local midnight (or, `hour`, the start of the local hour) at or before `ms`.
fn local_start(ms: u64, hour: bool) -> u64 {
    let t = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_sec = 0;
    tm.tm_min = 0;
    if !hour {
        tm.tm_hour = 0;
    }
    tm.tm_isdst = -1;
    let start = unsafe { libc::mktime(&mut tm) };
    if start < 0 { ms } else { start as u64 * 1000 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(sid: &str, created: u64, resolved: u64, outcome: &str) -> Answer {
        Answer { session_id: sid.into(), project: "p".into(), status: "answered".into(), created_ms: created, resolved_ms: resolved, outcome: outcome.into() }
    }

    #[test]
    fn todays_numbers_count_since_midnight_and_chart_the_last_twelve_hours() {
        let now = crate::model::now_ms();
        let midnight = local_start(now, false);
        let hour0 = local_start(now, true);
        let answers = [
            answer("a", now - 60_000, now - 1_000, "replied: “ok”"),
            answer("b", now - 600_000, now - 2_000, "allowed"),
            answer("c", now - 30_000, now - 3_000, "denied"),
            answer("old", midnight - 10 * HOUR_MS, midnight - 2 * HOUR_MS, "allowed"),
        ];
        let t = today(&answers, now);
        let since_midnight = answers.iter().filter(|a| a.resolved_ms >= midnight).count();
        assert_eq!(t["answered"], since_midnight);
        assert_eq!((t["replied"].as_u64(), t["denied"].as_u64()), (Some(1), Some(1)));
        assert_eq!(t["longest"]["session_id"], "b");
        assert_eq!(t["longest"]["wait_ms"], 598_000);
        let hours = t["hours"].as_array().unwrap();
        assert_eq!(hours.len(), HOURS as usize);
        assert_eq!(hours[11]["from_ms"], hour0);
        assert_eq!(hours.iter().map(|h| h["n"].as_u64().unwrap()).sum::<u64>(), answers.iter().filter(|a| a.resolved_ms >= hour0 - 11 * HOUR_MS).count() as u64);
        assert_eq!(next_change(now), hour0 + HOUR_MS);
    }

    #[test]
    fn nothing_answered_says_so() {
        let t = today(&[], crate::model::now_ms());
        assert_eq!(t["answered"], 0);
        assert_eq!(t["median_wait_ms"], Value::Null);
        assert_eq!(t["longest"], Value::Null);
    }
}
