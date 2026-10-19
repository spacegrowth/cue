//! What a model's tokens cost at API list prices, in dollars per million, matched by the start of the
//! model's id (the longest match wins, so claude-opus-5-5 beats claude-opus-5). Insights price usage
//! this way even on a subscription, so weeks and models compare; there it's what the usage is worth,
//! not a bill. Cache reads use the model's published rate where there is one, else a tenth of input;
//! cache writes cost 1.25× input when kept 5 minutes and 2× when kept an hour.
//! SHORTCUT: a hand-kept table (Anthropic's, 2026-10); other models price at nothing and count as
//! unpriced. Upgrade path: a prices file Cue updates with its releases.

use super::steps::Usage;

/// (model id start, input, output, cache read: None for a tenth of input)
const LIST: [(&str, f64, f64, Option<f64>); 14] = [
    ("claude-fable-5-1", 10.0, 50.0, Some(0.25)),
    ("claude-mythos-5-1", 10.0, 50.0, Some(0.25)),
    ("claude-fable-5", 10.0, 50.0, None),
    ("claude-mythos-5", 10.0, 50.0, None),
    ("claude-opus-5-5", 4.0, 20.0, Some(0.20)),
    ("claude-opus-5", 5.0, 25.0, None),
    ("claude-opus-4", 5.0, 25.0, None),
    ("claude-opus-4-0", 15.0, 75.0, None),
    ("claude-opus-4-1", 15.0, 75.0, None),
    ("claude-sonnet-5-5", 2.0, 10.0, Some(0.20)),
    ("claude-sonnet-5", 2.0, 10.0, None),
    ("claude-sonnet-4", 3.0, 15.0, None),
    ("claude-haiku-5-5", 0.10, 0.50, None),
    ("claude-haiku-4", 1.0, 5.0, None),
];

/// Dollars per million tokens, by kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub read: f64,
    pub write_5m: f64,
    pub write_1h: f64,
}

/// A model's rates, if Cue knows them. A provider's prefix ("anthropic/claude-…", "us.anthropic.claude-…")
/// and a "[1m]" suffix don't change the price.
pub fn rates(model: &str) -> Option<Rates> {
    let id = model.rsplit('/').next().unwrap_or(model);
    let id = id.find("claude-").map_or(id, |i| &id[i..]);
    LIST.iter().filter(|(start, ..)| id.starts_with(start)).max_by_key(|(start, ..)| start.len()).map(|&(_, input, output, read)| Rates {
        input,
        output,
        read: read.unwrap_or(input * 0.1),
        write_5m: input * 1.25,
        write_1h: input * 2.0,
    })
}

/// What a turn's tokens cost on `model`, if Cue knows its prices.
pub fn cost(model: &str, u: &Usage) -> Option<f64> {
    let r = rates(model)?;
    Some((u.fresh as f64 * r.input + u.output as f64 * r.output + u.read as f64 * r.read + u.write_5m as f64 * r.write_5m + u.write_1h as f64 * r.write_1h) / 1e6)
}

/// How many warm replies one cold reply costs on `model`, rounded: re-writing its context into a cache kept
/// `ttl_s` (an hour, else 5 minutes) against reading it back. 0 when the model isn't priced or nothing is
/// cached. Insights say this, not dollars: on a subscription nothing is billed, but the ratio holds.
pub fn cold_x(model: &str, ttl_s: u64) -> u64 {
    match rates(model) {
        // The nudge keeps an exact half (3.75 / 0.3) from rounding down on float error.
        Some(r) if ttl_s > 0 && r.read > 0.0 => ((if ttl_s >= 3600 { r.write_1h } else { r.write_5m }) / r.read + 1e-9).round() as u64,
        _ => 0,
    }
}

/// What re-writing `tokens` of context into a cache kept `ttl_s` costs on `model` at list prices, in cents
/// (0 when not priced or nothing is cached). Insights show it only to API users; on a subscription it's no bill.
pub fn cold_cents(model: &str, ttl_s: u64, tokens: u64) -> u64 {
    match rates(model) {
        Some(r) if ttl_s > 0 => (tokens as f64 * (if ttl_s >= 3600 { r.write_1h } else { r.write_5m }) / 1e4).round() as u64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cold_reply_is_counted_in_warm_ones() {
        assert_eq!(cold_x("claude-opus-5-5", 3600), 40, "$8 an hour's write against $0.20 a read");
        assert_eq!(cold_x("claude-opus-5-5", 300), 25);
        assert_eq!(cold_x("claude-sonnet-4-5", 300), 13, "a tenth of input: 1.25 / 0.1");
        assert_eq!(cold_cents("claude-opus-5-5", 3600, 300_000), 240, "300k at $8 a million");
        assert_eq!(cold_cents("claude-opus-5-5", 300, 300_000), 150, "300k at $5 a million");
        assert_eq!(cold_cents("gpt-5", 300, 300_000), 0);
        assert_eq!(cold_x("claude-opus-5-5", 0), 0, "nothing cached");
        assert_eq!(cold_x("gpt-5", 3600), 0, "unpriced");
    }

    #[test]
    fn the_longest_match_wins_and_providers_dont_matter() {
        assert_eq!(rates("claude-opus-5-5").unwrap().read, 0.20);
        assert_eq!(rates("claude-opus-5-20260101").unwrap().input, 5.0);
        assert_eq!(rates("us.anthropic.claude-opus-5-5[1m]").unwrap().input, 4.0);
        assert_eq!(rates("claude-opus-4-1-20250805").unwrap().input, 15.0);
        assert_eq!(rates("claude-opus-4-5").unwrap().input, 5.0);
        assert!(rates("gpt-5.2-codex").is_none());
    }

    #[test]
    fn a_turn_is_priced_by_kind() {
        let u = Usage { fresh: 1_000_000, output: 1_000_000, read: 1_000_000, write_5m: 1_000_000, write_1h: 1_000_000, ..Default::default() };
        // 4 + 20 + 0.20 + 5 + 8
        assert!((cost("claude-opus-5-5", &u).unwrap() - 37.2).abs() < 1e-9);
    }
}
