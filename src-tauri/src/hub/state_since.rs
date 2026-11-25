//! What a window fetches when it's told the state changed: only what changed since the state it has (#877).
//! The whole state is tens of MB with hundreds of sessions (the Desk's items and threads, every session, every
//! waiting card), and one change sent it all, to each window, each time: Cue wrote it out, the web view read it
//! in, and both held it meanwhile. Now each state handed out is numbered, Cue keeps each window's last one, and a
//! window that says which it has gets a patch from it (`diff`), applied by ui/core/state_since.js.
//!
//! The patch, a tree of nodes:
//! - `{"=": v}`: the value is `v` now (it's new, changed kind, or isn't worth going into);
//! - `{"o": {key: node}, "d": [key]}`: an object, its keys that changed and those gone (either left out when none);
//! - `{"k": key, "ids": [id], "c": {id: node}}`: a list of objects each with its own `key` ("id", "session_id"),
//!   in the order `ids` (left out when it's the same as before), those that changed or are new under `c`;
//! - `{"n": len, "i": {index: node}}`: any other list, now `len` long, the places that changed under `i`.
//!
//! What a window's answer is (`Hub::state_since`): `{"state_v", "full"}` (the whole state: a window that has none,
//! or not the one Cue gave it last) or `{"state_v", "from", "patch", "now_ms"}`.

use super::*;
use std::time::{Duration, Instant};

/// A window that hasn't fetched for this long (closed, or a page that went away) has its state let go: Cue holds
/// each window's last one to patch from, and with hundreds of sessions one is a lot to keep for nobody. A window
/// still there gets the whole state at its next fetch.
const FORGET_AFTER: Duration = Duration::from_secs(120);

/// The states handed out: numbered, and each window's last.
#[derive(Default)]
pub(super) struct Given {
    next: u64,
    /// The state numbered last: fetched again (two windows, a second ask), it keeps its number.
    last: Option<(u64, Arc<Value>)>,
    /// Each window's (by label) state as Cue last gave it, and when.
    windows: HashMap<String, (u64, Arc<Value>, Instant)>,
}

impl Hub {
    /// The state `snap` for window `window`, which says it has state number `have`: a patch from that one when it's
    /// the one Cue gave it last, else the whole state (see the module's note).
    pub(crate) fn state_since(&self, window: &str, have: Option<u64>, snap: Arc<Value>) -> Value {
        let (v, base) = {
            let mut g = crate::locking::lock(&self.redraws.given, "state given");
            let v = match &g.last {
                Some((v, s)) if Arc::ptr_eq(s, &snap) => *v,
                _ => {
                    g.next += 1;
                    let v = g.next;
                    g.last = Some((v, snap.clone()));
                    v
                }
            };
            let now = Instant::now();
            g.windows.retain(|w, (_, _, at)| w == window || now.duration_since(*at) < FORGET_AFTER);
            let had = g.windows.insert(window.to_string(), (v, snap.clone(), now));
            (v, had.filter(|(hv, _, _)| Some(*hv) == have))
        };
        // Written with no lock held: comparing two states takes a while.
        match base {
            Some((from, old, _)) => {
                let patch = if Arc::ptr_eq(&old, &snap) { json!({}) } else { diff_object(old.as_object(), &snap).unwrap_or_else(|| json!({})) };
                json!({ "state_v": v, "from": from, "patch": patch, "now_ms": now_ms() })
            }
            None => json!({ "state_v": v, "full": Self::stamped(&snap) }),
        }
    }
}

/// The node that makes `old` into `new`; None when they're the same.
pub(super) fn diff(old: &Value, new: &Value) -> Option<Value> {
    if old == new {
        return None;
    }
    let node = match (old, new) {
        (Value::Object(o), Value::Object(_)) => diff_object(Some(o), new),
        (Value::Array(o), Value::Array(n)) => Some(diff_array(o, n)),
        _ => None,
    };
    Some(node.unwrap_or_else(|| json!({ "=": new })))
}

/// An object's node (`old` None: not an object before, so the whole of `new`). None when nothing changed.
fn diff_object(old: Option<&serde_json::Map<String, Value>>, new: &Value) -> Option<Value> {
    let (Some(o), Some(n)) = (old, new.as_object()) else { return Some(json!({ "=": new })) };
    let mut changed = serde_json::Map::new();
    for (k, nv) in n {
        match o.get(k) {
            Some(ov) => {
                if let Some(node) = diff(ov, nv) {
                    changed.insert(k.clone(), node);
                }
            }
            None => {
                changed.insert(k.clone(), json!({ "=": nv }));
            }
        }
    }
    let gone: Vec<&String> = o.keys().filter(|k| !n.contains_key(*k)).collect();
    if changed.is_empty() && gone.is_empty() {
        return None;
    }
    let mut node = serde_json::Map::new();
    if !changed.is_empty() {
        node.insert("o".into(), Value::Object(changed));
    }
    if !gone.is_empty() {
        node.insert("d".into(), json!(gone));
    }
    Some(Value::Object(node))
}

/// A list's node: by each object's own key when they all have one, else place by place.
fn diff_array(old: &[Value], new: &[Value]) -> Value {
    if let Some(key) = ["id", "session_id"].into_iter().find(|k| keyed(old, k).is_some() && keyed(new, k).is_some()) {
        let before: HashMap<String, &Value> = keyed(old, key).unwrap_or_default().into_iter().zip(old).collect();
        let ids = keyed(new, key).unwrap_or_default();
        let mut changed = serde_json::Map::new();
        for (id, nv) in ids.iter().zip(new) {
            let node = match before.get(id) {
                Some(ov) => diff(ov, nv),
                None => Some(json!({ "=": nv })),
            };
            if let Some(node) = node {
                changed.insert(id.clone(), node);
            }
        }
        let mut node = serde_json::Map::new();
        node.insert("k".into(), json!(key));
        if keyed(old, key).unwrap_or_default() != ids {
            node.insert("ids".into(), json!(ids));
        }
        node.insert("c".into(), Value::Object(changed));
        return Value::Object(node);
    }
    let mut changed = serde_json::Map::new();
    for (i, nv) in new.iter().enumerate() {
        let node = match old.get(i) {
            Some(ov) => diff(ov, nv),
            None => Some(json!({ "=": nv })),
        };
        if let Some(node) = node {
            changed.insert(i.to_string(), node);
        }
    }
    json!({ "n": new.len(), "i": changed })
}

/// Each object's `key` in `list` as text, when every one is an object with its own (a string or a whole number),
/// none the same.
fn keyed(list: &[Value], key: &str) -> Option<Vec<String>> {
    if list.is_empty() {
        return None;
    }
    let ids: Vec<String> = list
        .iter()
        .map(|v| match v.get(key)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let unique: HashSet<&String> = ids.iter().collect();
    (unique.len() == ids.len()).then_some(ids)
}
