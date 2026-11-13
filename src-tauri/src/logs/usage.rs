//! Claude Code usage limits: when a session is out of usage, which limit it hit, and when it resets.
//!
//! The signal is typed: Claude Code's StopFailure hook says `error_type: "rate_limit"`, and the
//! transcript gets a synthetic reply marked `isApiErrorMessage` with `error: "rate_limit"`. Which
//! limit (the plan's session or weekly limit, a model's own limit like Fable's, usage credits) and
//! the reset time are only in the message text, so those are read from it. The percentages come
//! from the status line's `rate_limits`, which Cue gets when the status line script forwards them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};

/// One limit you're up against right now.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Limit {
    /// "all" (nothing in Claude Code can run) or a model's name ("Fable": other models still work).
    pub scope: String,
    /// "session" | "weekly" | "model" | "credits" | "other"
    pub kind: String,
    /// Claude Code's own words, e.g. "You've hit your session limit · resets 4:10am (America/Los_Angeles)".
    pub text: String,
    /// When it lifts, if the message (or the status line) says.
    pub resets_ms: Option<u64>,
    pub since_ms: u64,
}

/// What a limit message means. `None` for an error that isn't about usage.
pub fn parse(error_type: &str, text: &str, now: u64) -> Option<Limit> {
    let t = text.to_lowercase();
    let usage_words = t.contains("limit") || t.contains("usage credits") || t.contains("out of usage");
    if error_type != "rate_limit" && !(error_type == "billing_error" && usage_words) {
        return None;
    }
    let (scope, kind) = if let Some(model) = model_limit(text) {
        (model, "model")
    } else if t.contains("session limit") {
        ("all".to_string(), "session")
    } else if t.contains("weekly limit") {
        ("all".to_string(), "weekly")
    } else if t.contains("usage credits") {
        // Extra usage ran out while you were past the plan's limit: nothing runs until either refills.
        ("all".to_string(), "credits")
    } else {
        ("all".to_string(), "other")
    };
    Some(Limit { scope, kind: kind.into(), text: text.trim().to_string(), resets_ms: reset_time(text, now), since_ms: now })
}

/// "You've reached your Fable limit." → "Fable".
fn model_limit(text: &str) -> Option<String> {
    let rest = text.split("reached your ").nth(1)?;
    let name = rest.split(" limit").next()?.trim();
    let generic = ["session", "weekly", "usage", "daily", "monthly", "spend"];
    (!name.is_empty() && name.len() < 30 && !generic.contains(&name.to_lowercase().as_str())).then(|| name.to_string())
}

/// "resets 4:10am (America/Los_Angeles)" → the next 4:10am; "resets Oct 3 at 8am (…)" → that day.
/// The zone in brackets is the one Claude Code runs in, i.e. this Mac's, so local time is used.
pub fn reset_time(text: &str, now: u64) -> Option<u64> {
    let after = text.split("resets ").nth(1)?;
    let after = after.split('(').next()?.trim();
    let (date, clock) = match after.split_once(" at ") {
        Some((d, c)) => (Some(d.trim()), c.trim()),
        None => (None, after),
    };
    let (hour, min) = clock_of(clock)?;
    let mut tm = local_tm(now / 1000);
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = 0;
    if let Some(d) = date {
        let mut parts = d.split_whitespace();
        let month = month_of(parts.next()?)?;
        let day: i32 = parts.next()?.trim_end_matches(',').parse().ok()?;
        let this_month = tm.tm_mon;
        tm.tm_mon = month;
        tm.tm_mday = day;
        // "Jan 2" seen in late December is next year.
        if month < this_month && this_month - month > 6 {
            tm.tm_year += 1;
        }
    }
    let mut at = to_epoch(&mut tm)? * 1000;
    // A bare time already past today means that time tomorrow: the next day on the calendar, not 24 hours on
    // (a clock change overnight makes that day 23 or 25 hours long).
    if date.is_none() && at + 60_000 < now {
        tm.tm_mday += 1;
        at = to_epoch(&mut tm)? * 1000;
    }
    Some(at)
}

/// "4:10am" → (4, 10); "8am" → (8, 0); "12:30pm" → (12, 30).
fn clock_of(s: &str) -> Option<(i32, i32)> {
    let s = s.trim().to_lowercase();
    let (num, pm) = if let Some(n) = s.strip_suffix("am") {
        (n, false)
    } else if let Some(n) = s.strip_suffix("pm") {
        (n, true)
    } else {
        return None;
    };
    let (h, m) = match num.trim().split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None => (num.trim().parse::<i32>().ok()?, 0),
    };
    if !(1..=12).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    Some(((h % 12) + if pm { 12 } else { 0 }, m))
}

fn month_of(s: &str) -> Option<i32> {
    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let p = s.to_lowercase();
    months.iter().position(|m| p.starts_with(m)).map(|i| i as i32)
}

pub(crate) fn local_tm(secs: u64) -> libc::tm {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

pub(crate) fn to_epoch(tm: &mut libc::tm) -> Option<u64> {
    tm.tm_isdst = -1;
    let t = unsafe { libc::mktime(tm) };
    (t > 0).then_some(t as u64)
}

/// One usage window from the status line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Window {
    pub pct: f64,
    pub resets_ms: Option<u64>,
}

/// Where the last usage Cue had is kept, so the meter is back at once after a restart (an install, say) instead of
/// waiting for a session to report it again.
fn rates_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("rates.json")
}

/// The usage Cue last had, as kept at `rates_file`: a window that has reset since is dropped, its number being over.
pub fn saved_rates(now: u64) -> Option<Rates> {
    let mut r: Rates = serde_json::from_slice(&std::fs::read(rates_file()).ok()?).ok()?;
    let live = |w: &Option<Window>| w.as_ref().is_some_and(|w| w.resets_ms.is_some_and(|t| t > now));
    if !live(&r.five_hour) {
        r.five_hour = None;
    }
    if !live(&r.seven_day) {
        r.seven_day = None;
    }
    (r.five_hour.is_some() || r.seven_day.is_some()).then_some(r)
}

/// Keeps `r` for the next start (`saved_rates`).
pub fn save_rates(r: &Rates) {
    if let Ok(b) = serde_json::to_vec(r) {
        let _ = std::fs::write(rates_file(), b);
    }
}

/// The plan's usage as of the last status line Cue was sent.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Rates {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// Model-specific weekly limits (Fable). Claude Code doesn't hand these to the status line;
    /// they're read from the copy its /usage panel saves, so they can be old: see `fetched_ms`.
    pub models: Vec<ModelWindow>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelWindow {
    pub name: String,
    pub pct: f64,
    pub resets_ms: Option<u64>,
    /// When Claude Code last fetched it (it only does while /usage is open).
    pub fetched_ms: u64,
}

/// The status line's `rate_limits` object → Rates. Missing windows stay None.
pub fn rates_from_statusline(rl: &Value, now: u64) -> Rates {
    let window = |k: &str| -> Option<Window> {
        let w = rl.get(k)?;
        let pct = w.get("used_percentage")?.as_f64()?;
        let resets_ms = w.get("resets_at").and_then(Value::as_u64).map(|s| s * 1000);
        Some(Window { pct, resets_ms })
    };
    Rates { five_hour: window("five_hour"), seven_day: window("seven_day"), models: model_windows(), at_ms: now }
}

/// Per-model limits from Claude Code's saved /usage data in ~/.claude.json. Not a documented
/// format: anything unexpected just means no rows.
fn model_windows() -> Vec<ModelWindow> {
    let Ok(home) = std::env::var("HOME") else { return vec![] };
    let Ok(text) = std::fs::read_to_string(format!("{home}/.claude.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return vec![] };
    let Some(c) = v.get("cachedUsageUtilization").filter(|c| !c.is_null()) else { return vec![] };
    let fetched_ms = c.get("fetchedAtMs").and_then(Value::as_f64).unwrap_or(0.0) as u64;
    c.pointer("/utilization/limits")
        .and_then(Value::as_array)
        .map(|limits| {
            limits
                .iter()
                .filter_map(|l| {
                    let name = l.pointer("/scope/model/display_name")?.as_str()?.to_string();
                    let pct = l.get("percent")?.as_f64()?;
                    let resets_ms = l.get("resets_at").and_then(Value::as_str).and_then(iso_ms);
                    Some(ModelWindow { name, pct, resets_ms, fetched_ms })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// "2026-10-10T14:59:59.000Z" (UTC, or with an offset) → epoch ms.
fn iso_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    // Days from the civil date (Howard Hinnant's algorithm), so no time zone is involved.
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let mut secs = days * 86400 + h * 3600 + mi * 60 + se;
    // A trailing "+hh:mm" / "-hh:mm" offset.
    let tail = &s[19..];
    if let Some(i) = tail.find(['+', '-']) {
        let off = &tail[i..];
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let oh = off.get(1..3).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        let om = off.get(4..6).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        secs -= sign * (oh * 3600 + om * 60);
    }
    (secs > 0).then(|| secs as u64 * 1000)
}

/// The API error a Claude Code turn ended on, if its transcript's last reply is one:
/// (error type, message). How Cue notices a turn that died without any hook reaching it.
/// SHORTCUT: re-reads the last 256KB of each Claude session that has said "working" for 20s+, every
/// 10s: fine for a dozen sessions; remember each file's size and skip unchanged ones if that grows.
pub fn last_api_error(transcript: &str) -> Option<(String, String)> {
    let mut f = std::fs::File::open(transcript).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(256 * 1024);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    for line in text.lines().rev() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        match e.get("type").and_then(Value::as_str) {
            // Bookkeeping lines that come after a reply.
            Some("assistant") => {}
            Some("user") => {
                // Claude Code adds a hidden note after a limit; anything you wrote means a new turn.
                if e.get("isMeta").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                return None;
            }
            _ => continue,
        }
        if e.get("isApiErrorMessage").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let kind = e.get("error").and_then(Value::as_str).unwrap_or("unknown").to_string();
        let msg = e
            .pointer("/message/content")
            .and_then(Value::as_array)
            .and_then(|c| c.iter().find_map(|b| b.get("text").and_then(Value::as_str)))
            .unwrap_or("")
            .to_string();
        return Some((kind, msg));
    }
    None
}

/// Claude Code failing to reach the API and trying again: it writes a `system` / `api_error` line per
/// try (the error, try n of max, the wait before the next), for minutes before it gives up with an
/// API-error reply (last_api_error). Meanwhile its terminal only shows a spinner, so Cue says it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Retry {
    /// What went wrong, in Claude Code's words ("Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)").
    pub error: String,
    pub attempt: u64,
    pub max: u64,
    /// When it tries again (ms; from the line's time and its wait).
    pub next_ms: u64,
    /// It couldn't connect at all (no network, or nothing at that address), not an error the API sent.
    pub network: bool,
}

/// The retry a Claude Code turn is in now: the last `api_error` line, when nothing since says the API
/// answered (a reply) or a new turn started (something you wrote). The last 256KB, as last_api_error.
pub fn retrying(transcript: &str) -> Option<Retry> {
    let mut f = std::fs::File::open(transcript).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(256 * 1024))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    for line in text.lines().rev() {
        // Most lines are neither: skip them without parsing.
        if !line.contains("\"api_error\"") && !line.contains("\"type\":\"assistant\"") && !line.contains("\"type\":\"user\"") {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("assistant") => return None,
            Some("user") if e.get("isMeta").and_then(Value::as_bool) != Some(true) => return None,
            Some("system") if e.get("subtype").and_then(Value::as_str) == Some("api_error") => {
                let n = |k: &str| e.get(k).and_then(Value::as_u64).unwrap_or(0);
                let err = e.get("error");
                let error = err.and_then(|x| x.get("formatted").or_else(|| x.get("message"))).and_then(Value::as_str).unwrap_or("API error").to_string();
                let at = e.get("timestamp").and_then(Value::as_str).and_then(super::transcript::iso_ms).unwrap_or(0);
                return Some(Retry {
                    error,
                    attempt: n("retryAttempt"),
                    max: n("maxRetries"),
                    next_ms: at + n("retryInMs"),
                    network: err.and_then(|x| x.get("isNetworkDown")).and_then(Value::as_bool) == Some(true) || err.and_then(|x| x.get("connection")).is_some_and(|c| !c.is_null()),
                });
            }
            _ => {}
        }
    }
    None
}

/// "4:10am" for a time today, "Sat 8am" otherwise.
pub fn when(ms: u64, now: u64) -> String {
    let tm = local_tm(ms / 1000);
    let today = local_tm(now / 1000);
    let h12 = if tm.tm_hour % 12 == 0 { 12 } else { tm.tm_hour % 12 };
    let ampm = if tm.tm_hour < 12 { "am" } else { "pm" };
    let clock = if tm.tm_min == 0 { format!("{h12}{ampm}") } else { format!("{h12}:{:02}{ampm}", tm.tm_min) };
    let soon = ms.saturating_sub(now) < 20 * 3600 * 1000;
    if (tm.tm_yday == today.tm_yday && tm.tm_year == today.tm_year) || soon {
        clock
    } else {
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        format!("{} {clock}", days[tm.tm_wday.clamp(0, 6) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now_ms;

    #[test]
    fn a_retry_is_read_until_the_api_answers_or_you_write() {
        let p = std::env::temp_dir().join(format!("cue-retry-{}.jsonl", std::process::id()));
        // As Claude Code 2.1.316 writes them, trimmed.
        let user = r#"{"type":"user","isSidechain":false,"timestamp":"2026-11-01T22:57:28.000Z","message":{"role":"user","content":"hi"}}"#;
        let try1 = r#"{"type":"system","subtype":"api_error","level":"error","isSidechain":false,"timestamp":"2026-11-01T22:57:29.199Z","retryAttempt":1,"maxRetries":10,"retryInMs":502,"error":{"message":"Connection error.","formatted":"Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)","connection":{"code":"ECONNREFUSED"},"isNetworkDown":true,"rateLimits":null,"noResponse":null}}"#;
        let try5 = r#"{"type":"system","subtype":"api_error","level":"error","isSidechain":false,"timestamp":"2026-11-01T22:57:40.000Z","retryAttempt":5,"maxRetries":10,"retryInMs":9620,"error":{"message":"Overloaded","formatted":"Overloaded","connection":null,"isNetworkDown":false}}"#;
        let reply = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-01T22:58:00.000Z","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let bookkeeping = r#"{"type":"api-request","timestamp":"2026-11-01T22:57:41.000Z"}"#;
        let write = |lines: &[&str]| std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let path = p.to_str().unwrap();
        write(&[user, try1, bookkeeping]);
        let r = retrying(path).unwrap();
        assert_eq!((r.attempt, r.max, r.network), (1, 10, true));
        assert!(r.error.contains("ECONNREFUSED"), "{}", r.error);
        assert_eq!(r.next_ms, crate::logs::transcript::iso_ms("2026-11-01T22:57:29.199Z").unwrap() + 502);
        write(&[user, try1, try5]);
        let r = retrying(path).unwrap();
        assert_eq!((r.attempt, r.network, r.error.as_str()), (5, false, "Overloaded"));
        write(&[user, try1, reply]);
        assert!(retrying(path).is_none(), "the API answered");
        write(&[try1, user]);
        assert!(retrying(path).is_none(), "a new turn");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn limit_messages_say_which_limit() {
        let now = now_ms();
        let s = parse("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)", now).unwrap();
        assert_eq!((s.scope.as_str(), s.kind.as_str()), ("all", "session"));
        let r = s.resets_ms.unwrap();
        assert!(r > now - 60_000 && r <= now + 24 * 3600 * 1000 + 60_000);
        let tm = local_tm(r / 1000);
        assert_eq!((tm.tm_hour, tm.tm_min), (4, 10));

        let w = parse("rate_limit", "You've hit your weekly limit · resets Oct 3 at 8am (America/Los_Angeles)", now).unwrap();
        assert_eq!(w.kind, "weekly");
        let tm = local_tm(w.resets_ms.unwrap() / 1000);
        assert_eq!((tm.tm_mon, tm.tm_mday, tm.tm_hour), (9, 3, 8));

        let f = parse("rate_limit", "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model.", now).unwrap();
        assert_eq!((f.scope.as_str(), f.kind.as_str(), f.resets_ms), ("Fable", "model", None));

        let c = parse("rate_limit", "You're out of usage credits. Switch to another model, or manage usage credits at claude.ai/settings/usage, to continue.", now).unwrap();
        assert_eq!(c.kind, "credits");

        assert!(parse("overloaded", "Overloaded", now).is_none());
    }

    /// "resets 4:10am" seen in the evening is 4:10am the next day by the clock, also across the night the
    /// clocks change (America/Los_Angeles went back an hour on the night of 31 Oct 2026, a 25-hour day).
    #[test]
    fn a_bare_reset_time_is_that_clock_time_tomorrow_across_a_clock_change() {
        let mut evening = local_tm(0);
        (evening.tm_year, evening.tm_mon, evening.tm_mday, evening.tm_hour, evening.tm_min, evening.tm_sec) = (126, 9, 31, 20, 0, 0);
        let now = to_epoch(&mut evening).unwrap() * 1000;
        let r = reset_time("resets 4:10am (America/Los_Angeles)", now).unwrap();
        let tm = local_tm(r / 1000);
        assert_eq!((tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_min), (10, 1, 4, 10));
    }

    #[test]
    fn clock_times() {
        assert_eq!(clock_of("4:10am"), Some((4, 10)));
        assert_eq!(clock_of("12am"), Some((0, 0)));
        assert_eq!(clock_of("12:30pm"), Some((12, 30)));
        assert_eq!(clock_of("8pm"), Some((20, 0)));
        assert_eq!(clock_of("noon"), None);
    }

    #[test]
    fn iso_times() {
        assert_eq!(iso_ms("1970-01-02T00:00:00Z"), Some(86_400_000));
        assert_eq!(iso_ms("2026-10-10T14:59:59.000+00:00"), Some(1_791_644_399_000));
        assert_eq!(iso_ms("2026-10-10T07:59:59-07:00"), Some(1_791_644_399_000));
    }

    #[test]
    fn a_turn_that_died_on_a_limit_is_found_in_the_transcript() {
        let dir = std::env::temp_dir().join(format!("cue-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.jsonl");
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"ok do all"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"note"}}"#,
            r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 4:10am (America/Los_Angeles)"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration"}"#,
        ];
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let (kind, text) = last_api_error(p.to_str().unwrap()).unwrap();
        assert_eq!(kind, "rate_limit");
        assert!(text.contains("session limit"));
        // You typed again: the error is behind you.
        std::fs::write(&p, lines.join("\n") + "\n" + r#"{"type":"user","message":{"role":"user","content":"continue"}}"# + "\n").unwrap();
        assert!(last_api_error(p.to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
