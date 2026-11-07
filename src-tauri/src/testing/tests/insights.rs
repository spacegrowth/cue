//! Insights (stability plan §4, Insights): what each turn cost, priced by kind from its transcript; the
//! heads-up before a waiting session's cache runs out, once per expiry; and transcripts that are huge or
//! carry lines that aren't JSON.

use crate::logs::prices;
use crate::logs::steps::{turn_rows, TurnRow};
use crate::testing::{Transcript, World};
use serde_json::{json, Value};

/// One reply as Claude Code logs it: its id (a reply split over several lines repeats it), its model, and its
/// tokens by kind (fresh input, cache read, written for 5 minutes, written for an hour, output).
fn reply(t: &Transcript, id: &str, model: &str, [fresh, read, w5, w1h, out]: [u64; 5]) {
    t.entry(json!({"type": "assistant", "message": {"id": id, "model": model, "content": [{"type": "text", "text": "ok"}],
        "usage": {"input_tokens": fresh, "cache_read_input_tokens": read, "cache_creation_input_tokens": w5 + w1h,
                  "cache_creation": {"ephemeral_5m_input_tokens": w5, "ephemeral_1h_input_tokens": w1h}, "output_tokens": out}}}));
}

/// A transcript of the test's own, outside any World.
fn transcript(name: &str) -> (Transcript, String) {
    let dir = std::env::temp_dir().join(format!("cue-insights-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("s.jsonl");
    (Transcript::at(&path), path.to_string_lossy().into_owned())
}

fn rows(path: &str) -> Vec<TurnRow> {
    turn_rows(path, 0, u64::MAX).0
}

fn usd(r: &TurnRow) -> Option<f64> {
    prices::cost(&r.model, &r.usage)
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

/// A turn's cost from its transcript: an hour's cache write at 2× input, reads at the model's own read rate,
/// a reply logged over two lines counted once.
#[test]
fn a_turn_is_priced_by_kind_and_a_split_reply_counted_once() {
    let (t, path) = transcript("kinds");
    t.prompt("look into the login bug");
    reply(&t, "m1", "claude-opus-5-5", [1_000, 0, 0, 100_000, 500]);
    reply(&t, "m1", "claude-opus-5-5", [1_000, 0, 0, 100_000, 500]);
    t.prompt("and fix it");
    reply(&t, "m2", "claude-opus-5-5", [0, 100_000, 2_000, 0, 200]);
    let r = rows(&path);
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].usage.fresh, r[0].usage.write_1h, r[0].usage.output), (1_000, 100_000, 500), "the split reply once");
    // 1k × $4 + 100k × $8 + 500 × $20, a million each.
    assert!(close(usd(&r[0]), 0.814), "{:?}", usd(&r[0]));
    // 100k × $0.20 + 2k × $5 + 200 × $20.
    assert!(close(usd(&r[1]), 0.034), "{:?}", usd(&r[1]));
}

/// A session that changes model mid-way: each turn at its own model's prices.
#[test]
fn each_turn_is_priced_at_its_own_models_rates() {
    let (t, path) = transcript("models");
    t.prompt("plan it");
    reply(&t, "m1", "claude-opus-5-5", [0, 0, 0, 0, 1_000_000]);
    t.prompt("now do it");
    reply(&t, "m2", "us.anthropic.claude-sonnet-5-5[1m]", [0, 0, 0, 0, 1_000_000]);
    let r = rows(&path);
    assert_eq!((r[0].model.as_str(), usd(&r[0])), ("claude-opus-5-5", Some(20.0)));
    assert!(close(usd(&r[1]), 10.0), "sonnet's output rate, provider and window aside: {:?}", usd(&r[1]));
}

/// A model Cue has no prices for costs nothing and says so (unpriced), rather than a made-up number.
#[test]
fn an_unpriced_model_is_unpriced_not_free() {
    let (t, path) = transcript("unpriced");
    t.prompt("hi");
    reply(&t, "m1", "gpt-5.2-codex", [5_000, 0, 0, 0, 100]);
    let r = rows(&path);
    assert_eq!(r[0].usage.fresh, 5_000, "its tokens are still counted");
    assert_eq!(usd(&r[0]), None);
}

/// Lines that aren't JSON (a crash mid-write, another program's output) are passed over: the turns around
/// them read as if they weren't there.
#[test]
fn lines_that_arent_json_are_passed_over() {
    let (clean, clean_path) = transcript("clean");
    let (noisy, noisy_path) = transcript("noisy");
    for (t, junk) in [(&clean, false), (&noisy, true)] {
        t.prompt("one");
        if junk {
            let mut f = std::fs::OpenOptions::new().append(true).open(&noisy_path).unwrap();
            use std::io::Write;
            writeln!(f, "{{\"type\": \"assistant\", \"message\": {{\"usage\"").unwrap();
            writeln!(f, "not json at all").unwrap();
            writeln!(f, "\u{0}\u{0}\u{0}").unwrap();
        }
        reply(t, "m1", "claude-opus-5-5", [10, 0, 0, 0, 10]);
        t.prompt("two");
        reply(t, "m2", "claude-opus-5-5", [20, 0, 0, 0, 20]);
    }
    let (a, b) = (rows(&clean_path), rows(&noisy_path));
    assert_eq!(b.len(), 2);
    assert_eq!(a.iter().map(|r| (r.usage.fresh, r.prompt.clone())).collect::<Vec<_>>(), b.iter().map(|r| (r.usage.fresh, r.prompt.clone())).collect::<Vec<_>>());
}

/// A line still being written (no newline yet) isn't read; the read after it's whole starts at its turn and
/// counts it.
#[test]
fn a_line_being_written_is_read_once_its_whole() {
    let (t, path) = transcript("partial");
    t.prompt("one");
    reply(&t, "m1", "claude-opus-5-5", [10, 0, 0, 0, 10]);
    let whole = std::fs::metadata(&path).unwrap().len();
    let line = json!({"type": "assistant", "message": {"id": "m2", "model": "claude-opus-5-5", "content": [], "usage": {"input_tokens": 5, "output_tokens": 5}}}).to_string();
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(line[..line.len() / 2].as_bytes()).unwrap();
    let (first, next) = turn_rows(&path, 0, u64::MAX);
    assert_eq!(first[0].usage.fresh, 10, "the half line not read");
    assert!(next < whole, "the next read starts at the open turn");
    writeln!(f, "{}", &line[line.len() / 2..]).unwrap();
    let (again, _) = turn_rows(&path, next, u64::MAX);
    assert_eq!(again[0].usage.fresh, 15, "the open turn read again, whole");
}

/// A transcript of `bytes` and more, written once: turns of a prompt and a reply carrying a long text. Its
/// last reply's context is `last_context`.
fn huge(path: &std::path::Path, bytes: u64, last_context: u64) {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let filler = "x".repeat(4_000);
    let (mut written, mut n) = (0u64, 0u64);
    while written < bytes {
        let prompt = json!({"type": "user", "timestamp": "2026-11-06T10:00:00.000Z", "message": {"content": format!("step {n}")}});
        let said = json!({"type": "assistant", "timestamp": "2026-11-06T10:00:01.000Z", "message": {"id": format!("m{n}"), "model": "claude-opus-5-5",
            "content": [{"type": "text", "text": filler}], "usage": {"input_tokens": 1, "cache_read_input_tokens": last_context - 1, "output_tokens": 1}}});
        let lines = format!("{prompt}\n{said}\n");
        f.write_all(lines.as_bytes()).unwrap();
        written += lines.len() as u64;
        n += 1;
    }
    f.flush().unwrap();
}

/// A 200 MB transcript (generated, never committed): the card's numbers come from its end at once, and a
/// read of its turns takes only as much as it's allowed.
#[test]
fn a_200_mb_transcript_is_read_from_its_end_and_a_bounded_slice_at_a_time() {
    let dir = std::env::temp_dir().join(format!("cue-insights-huge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.jsonl");
    huge(&path, 200 << 20, 150_000);
    let p = path.to_string_lossy().into_owned();
    let t0 = std::time::Instant::now();
    let state = crate::steps::insight_state(&p);
    let took = t0.elapsed();
    let (slice, next) = turn_rows(&p, 0, 8 << 20);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(took < std::time::Duration::from_secs(3), "the card's numbers took {took:?}");
    assert_eq!(state.map(|s| s.tokens), Some(150_000), "the latest reply's context");
    assert!(next <= 8 << 20 && !slice.is_empty(), "one slice: {} turns, up to byte {next}", slice.len());
}

/// Session `sid` waiting on you after a reply that kept `context` tokens cached for `ttl` ("1h" or "5m").
fn waiting_with_cache(w: &World, sid: &str, context: u64, ttl: &str) -> String {
    let a = w.session(sid);
    a.prompt("refactor the store");
    let t = a.transcript();
    t.prompt("refactor the store");
    let split = if ttl == "1h" { [1, 0, 0, context - 1, 10] } else { [1, 0, context - 1, 0, 10] };
    reply(&t, &format!("{sid}-r"), "claude-opus-5-5", split);
    a.stop("Done: the store is split.");
    w.snapshot().card(sid)["id"].as_str().unwrap().to_string()
}

/// How many times card `id`'s notification was posted.
fn posted(w: &World, id: &str) -> usize {
    w.notes.shown().iter().filter(|n| *n == id).count()
}

/// The heads-up that a waiting session's hour-long cache runs out comes once in its last ten minutes, however
/// many ticks follow; a new reply's cache is a new expiry, warned about again.
#[test]
fn a_cache_running_out_is_warned_about_once_per_expiry() {
    let w = World::new();
    let id = waiting_with_cache(&w, "a", 120_000, "1h");
    let before = posted(&w, &id);
    w.advance(45 * 60_000);
    w.tick(true);
    assert_eq!(posted(&w, &id), before, "not before its last ten minutes");
    w.advance(6 * 60_000);
    for _ in 0..5 {
        w.tick(true);
        w.advance(30_000);
    }
    assert_eq!(posted(&w, &id), before + 1, "once in its last ten minutes");
    let t = w.session("a").transcript();
    reply(&t, "a-r2", "claude-opus-5-5", [1, 119_999, 0, 1, 10]);
    w.advance(51 * 60_000);
    w.tick(true);
    w.tick(true);
    assert_eq!(posted(&w, &id), before + 2, "the renewed cache's own heads-up, once");
}

/// No heads-up where it's no use: a 5-minute cache (it runs out while you read), a small context (cheap to
/// write again), or with Settings → Notifications → cache off.
#[test]
fn no_cache_heads_up_for_a_short_cache_a_small_context_or_with_it_turned_off() {
    let w = World::new();
    let short = waiting_with_cache(&w, "short", 120_000, "5m");
    let small = waiting_with_cache(&w, "small", 20_000, "1h");
    std::fs::write(w.home().join("config.json"), r#"{"notify": {"cold": false}}"#).unwrap();
    crate::config::refresh();
    let off = waiting_with_cache(&w, "off", 120_000, "1h");
    let before: Vec<usize> = [&short, &small, &off].iter().map(|id| posted(&w, id)).collect();
    w.advance(3 * 60_000);
    w.tick(true);
    w.advance(50 * 60_000);
    w.tick(true);
    let after: Vec<usize> = [&short, &small, &off].iter().map(|id| posted(&w, id)).collect();
    assert_eq!(after, before);
    let card = w.snapshot().card("off").clone();
    assert!(card_insight(&w, "off").is_some_and(|i| i["tokens"] == 120_000), "its numbers are still read for the card: {card}");
}

/// The insight numbers the window gets for session `sid`'s card.
fn card_insight(w: &World, sid: &str) -> Option<Value> {
    w.snapshot().0["insight"].get(sid).cloned()
}
