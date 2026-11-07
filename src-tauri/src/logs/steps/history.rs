//! A session's background work over its whole transcript, for the Background tab's Show earlier.

use super::feed::Feed;
use super::queries::helper_json;
use super::FORGET;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Every piece of background work a session ran, read from its whole transcript (its feed reads only the
/// last START_BYTES and keeps MAX_HELPERS): read when asked for (the Background tab's Show earlier), kept
/// FORGET after the last ask.
static HISTORY: Mutex<Option<HashMap<String, Feed>>> = Mutex::new(None);

fn with_history<T>(path: &str, f: impl FnOnce(&Feed) -> T) -> T {
    let mut guard = crate::locking::lock(&HISTORY, "HISTORY");
    let all = guard.get_or_insert_with(HashMap::new);
    all.retain(|_, h| h.used.elapsed() < FORGET);
    let feed = all.entry(path.to_string()).or_insert_with(|| Feed { max_helpers: usize::MAX, ..Feed::start(path, 1, 0) });
    feed.used = Instant::now();
    feed.catch_up();
    f(feed)
}

/// A session's background work, all of it, oldest first: each one as `steps` gives it, but without its
/// steps or output (`latest`: the newest thing it did, in a few words). Opening one asks `history_helper`.
pub fn history(path: &str) -> Value {
    let now = crate::model::now_ms();
    with_history(path, |f| {
        Value::Array(f.helpers.iter().map(|h| {
            let mut v = helper_json(h, now);
            v["latest"] = json!(h.latest());
            v["items"] = json!([]);
            v["output"] = json!([]);
            v
        }).collect())
    })
}

/// One piece of a session's background work in full (its full view), from its history.
pub fn history_helper(path: &str, id: &str) -> Value {
    let now = crate::model::now_ms();
    with_history(path, |f| f.helpers.iter().find(|h| h.id == id).map(|h| helper_json(h, now)).unwrap_or(Value::Null))
}
