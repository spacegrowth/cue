//! A teammate or background agent gone quiet: no line from it for QUIET_MS. What its last line was says which
//! of three it is, so Cue acts once and calmly rather than only showing "quiet":
//! - its own words (a summary, an answer): it finished and lingers in the list; it's done.
//! - a tool call with no result yet: a long run (a test suite, a build); it's left to it ("running … for 12m").
//! - a tool's result, and nothing after: stuck; its lead is nudged once for that quiet stretch (hub/team.rs).

/// No line for this long: quiet, and judged by its last line. The one threshold, for teams, background agents and
/// the add-on alike. A debug build can set it shorter with CUE_TEST_QUIET_MS (the real-CLI tests: a minute, not 15).
pub const QUIET_MS: u64 = 15 * 60_000;

/// The threshold in force: QUIET_MS, or (debug builds only) CUE_TEST_QUIET_MS.
pub fn quiet_ms() -> u64 {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("CUE_TEST_QUIET_MS").ok().and_then(|v| v.parse::<u64>().ok()).filter(|ms| *ms > 0) {
        return ms;
    }
    QUIET_MS
}

/// What a teammate's or agent's last line was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Last {
    /// Its own words.
    Said,
    /// A tool call, its result not in yet.
    Call,
    /// A tool's result (or its start): it was to go on from there.
    Result,
}

impl Last {
    /// As a teammate keeps it ("said", "call", "result"; anything else: a result).
    pub fn from_word(w: &str) -> Last {
        match w {
            "said" => Last::Said,
            "call" => Last::Call,
            _ => Last::Result,
        }
    }
}

/// Where a quiet one stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quiet {
    /// Lines came within the threshold.
    Active,
    /// Its last line was its own words: it finished, and only lingers.
    Finished,
    /// A tool call is open: a long run, left to it.
    Tool,
    /// Nothing open and nothing said: stuck.
    Stuck,
}

impl Quiet {
    /// As the window reads it: "" (active), "done", "tool", "stuck".
    pub fn word(self) -> &'static str {
        match self {
            Quiet::Active => "",
            Quiet::Finished => "done",
            Quiet::Tool => "tool",
            Quiet::Stuck => "stuck",
        }
    }
}

/// One at work whose last line was `last`, at `last_ms`, as of `now`.
pub fn judge(last: Last, last_ms: u64, now: u64) -> Quiet {
    if now.saturating_sub(last_ms) < quiet_ms() {
        return Quiet::Active;
    }
    match last {
        Last::Said => Quiet::Finished,
        Last::Call => Quiet::Tool,
        Last::Result => Quiet::Stuck,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_is_judged_by_its_last_line_once_the_threshold_is_past() {
        let t0 = 1_000_000;
        for last in [Last::Said, Last::Call, Last::Result] {
            assert_eq!(judge(last, t0, t0 + QUIET_MS - 1), Quiet::Active, "{last:?}: not quiet yet");
        }
        assert_eq!(judge(Last::Said, t0, t0 + QUIET_MS), Quiet::Finished);
        assert_eq!(judge(Last::Call, t0, t0 + QUIET_MS), Quiet::Tool);
        assert_eq!(judge(Last::Result, t0, t0 + QUIET_MS), Quiet::Stuck);
        assert_eq!((Last::from_word("said"), Last::from_word("call"), Last::from_word("")), (Last::Said, Last::Call, Last::Result));
    }
}
