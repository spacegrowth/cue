//! When a lead missed its wake-up: relay's timestamps read as times, and a report that has waited too long
//! for a lead that's free.

use super::files::s;
use super::Member;
use serde_json::Value;

/// Whether relay has woken the lead for the current packet's report (it marks that when it wakes the lead,
/// or when the lead opens the diff). Whether a review ran is a separate fact: Member::reviewed.
pub(super) fn report_woken(e: &Value) -> bool {
    let n = |k: &str| e.get(k).and_then(Value::as_u64);
    matches!((n("current_packet"), n("report_seen_packet")), (Some(cur), Some(seen)) if seen >= cur)
}

/// How long a report may sit before its lead is said to have missed it (relay's own wake-up is
/// normally within seconds).
pub(super) const MISSED_WAKE_SECS: u64 = 90;

/// relay's timestamp ("2026-10-04T09:50:00", the local time of the machine it ran on, maybe with a
/// fraction or a zone suffix, which is ignored) as calendar fields.
fn naive_tm(ts: &str) -> Option<libc::tm> {
    let (d, t) = ts.trim().split_once(['T', ' '])?;
    let mut dp = d.split('-').map(|n| n.parse::<i32>().ok());
    let (y, mo, da) = (dp.next()??, dp.next()??, dp.next()??);
    let mut tp = t.split(|c: char| !c.is_ascii_digit() && c != ':').next()?.split(':').map(|n| n.parse::<i32>().ok());
    let (h, mi) = (tp.next()??, tp.next()??);
    let sec = tp.next().flatten().unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    (tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, tm.tm_isdst) = (y - 1900, mo - 1, da, h, mi, sec, -1);
    Some(tm)
}

/// Seconds since the epoch for relay's timestamp. `zone`: the UTC offset (seconds) of the machine that
/// wrote it; None is this Mac, in its own zone (daylight saving included).
pub(super) fn epoch_secs(ts: &str, zone: Option<i64>) -> Option<u64> {
    let mut tm = naive_tm(ts)?;
    let t = match zone {
        None => unsafe { libc::mktime(&mut tm) as i64 },
        Some(off) => (unsafe { libc::timegm(&mut tm) } as i64) - off,
    };
    (t > 0).then_some(t as u64)
}

pub(super) fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether an executor is done (reported or idle) and has been for MISSED_WAKE_SECS (relay's
/// `updated`, in the zone of the machine it ran on: the last thing relay wrote to the record is the
/// report landing).
pub(super) fn ready_late(e: &Value, now: u64, zone: Option<i64>) -> bool {
    matches!(s(e, "status").as_str(), "reported" | "idle") && epoch_secs(&s(e, "updated"), zone).is_some_and(|at| now >= at + MISSED_WAKE_SECS)
}

/// The missed wake-up: the report is ready (a while) and its lead hasn't seen it, though the lead is
/// listening and free (`lead_free`: not working, compacting or asking you something). relay would
/// normally have woken it by now.
pub(super) fn missed_wake(m: &Member, lead_free: bool) -> bool {
    m.role == "executor" && matches!(m.status.as_str(), "reported" | "idle") && !m.woken && !m.reviewed && m.ready_late && m.lead_armed == Some(true) && lead_free
}
