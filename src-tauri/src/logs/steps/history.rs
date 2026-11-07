//! A session's background work over its whole transcript, for the Background tab's Show earlier.

use super::feed::Feed;
use super::queries::{helper_json, with_caught_up};
use super::FORGET;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Every piece of background work a session ran, read from its whole transcript (its feed reads only the
/// last START_BYTES and keeps MAX_HELPERS): read when asked for (the Background tab's Show earlier), kept
/// FORGET after the last ask. The table's lock is held only to find, file or forget one; each is read with
/// its own lock let go (`with_caught_up`).
static HISTORY: Mutex<Option<HashMap<String, (Arc<Mutex<Feed>>, Instant)>>> = Mutex::new(None);

/// The history feed for `path`, made (its log's first line read with no lock held) if there's none.
fn history_feed(path: &str) -> Arc<Mutex<Feed>> {
    let found = {
        let mut guard = crate::locking::lock(&HISTORY, "HISTORY");
        let all = guard.get_or_insert_with(HashMap::new);
        all.retain(|_, (_, used)| used.elapsed() < FORGET);
        all.get_mut(path).map(|(feed, used)| {
            *used = Instant::now();
            feed.clone()
        })
    };
    if let Some(feed) = found {
        return feed;
    }
    let new = Arc::new(Mutex::new(Feed { max_helpers: usize::MAX, ..Feed::start(path, 1, 0) }));
    let mut guard = crate::locking::lock(&HISTORY, "HISTORY");
    guard.get_or_insert_with(HashMap::new).entry(path.to_string()).or_insert((new, Instant::now())).0.clone()
}

fn with_history<T>(path: &str, f: impl FnOnce(&Feed) -> T) -> T {
    with_caught_up(&history_feed(path), f)
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
