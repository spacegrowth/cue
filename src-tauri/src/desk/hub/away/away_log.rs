//! What happened during each past Away stretch (#416), for the dispatcher's Away tab. Kept in config.json:
//! - `desk.away_log`: one record a stretch, written as Away turns off: { id, desk, since, end, budget, spent,
//!   ended ("" when you turned it off, "budget" when it ran out first; "until" in stretches from before #790), items: [{ id, kind, title, from, to }]
//!   (each item that moved: its state as Away went on, "" for one added while away, and as it went off), calls: the
//!   lead's DECIDED lines made while away, as they read then }.
//! - `desk.held_log`: every step held while away, from the moment it's held, with your answer once given ("allow" |
//!   "no"), so the stretch shows it after the held list has let it go.
//! Each Desk keeps its last `KEEP` stretches; the held log its last `KEEP_HELD` steps. A call's Undo is read live from
//! the Desk's decisions, so it shows only while the call can still be undone.

use crate::desk::hub::prelude::*;

const KEEP: usize = 50;
const KEEP_HELD: usize = 400;

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Desk `desk`'s items as they stand: { id: state }, the snapshot an Away stretch starts from.
pub(crate) fn item_states(st: &Store, desk: &str) -> Value {
    Value::Object(st.desk.items.iter().filter(|i| i.project == desk).map(|i| (i.id.clone(), json!(i.state))).collect())
}

/// The stretch that started at `since` and ends at `end`, for its record: the items that moved since `start` (item
/// states then) and the lead's calls in that time.
pub(crate) fn stretch(st: &Store, desk: &str, since: u64, end: u64, start: &Value) -> (Value, Value) {
    let items: Vec<Value> = st
        .desk
        .items
        .iter()
        .filter(|i| i.project == desk)
        .filter_map(|i| {
            let from = start.get(&i.id).and_then(Value::as_str).unwrap_or("");
            // Added and dropped while away, or not moved: nothing to say.
            (from != i.state && !(from.is_empty() && i.state == "dropped")).then(|| json!({ "id": i.id, "kind": i.kind, "title": i.title, "from": from, "to": i.state }))
        })
        .collect();
    let calls: Vec<Value> = st
        .desk
        .decisions
        .iter()
        .filter(|d| d.project == desk && d.call && d.at >= since / 1000 && d.at * 1000 <= end)
        .map(|d| json!({ "id": d.id, "item_id": d.item_id, "text": d.text, "why": d.why, "at": d.at, "minor": d.minor, "asked": d.asked }))
        .collect();
    (Value::Array(items), Value::Array(calls))
}

/// A stretch's record kept, newest last; the Desk's oldest dropped past `KEEP`.
pub(crate) fn keep(record: Value) {
    let desk = str_of(&record, "desk").to_string();
    let r = crate::config::update_with("desk.away_log", |c| {
        let mut list = c.pointer("/desk/away_log").and_then(Value::as_array).cloned().unwrap_or_default();
        list.push(record);
        let mine = list.iter().filter(|r| str_of(r, "desk") == desk).count();
        let mut drop = mine.saturating_sub(KEEP);
        list.retain(|r| {
            if drop > 0 && str_of(r, "desk") == desk {
                drop -= 1;
                return false;
            }
            true
        });
        Ok((Some(Value::Array(list)), ()))
    });
    if let Err(e) = r {
        eprintln!("cue: couldn't keep the Away stretch: {e}");
    }
}

/// Held step `h` (as desk/hub/away/away_holds.rs keeps it) noted, with who asked; again with your answer once given.
pub(crate) fn note_held(h: &Value, who: &str) {
    let id = str_of(h, "id").to_string();
    let r = crate::config::update_with("desk.held_log", |c| {
        let mut list = c.pointer("/desk/held_log").and_then(Value::as_array).cloned().unwrap_or_default();
        let entry = json!({ "id": id, "desk": h["desk"], "sid": h["sid"], "who": who, "kinds": h["kinds"], "command": h["command"], "at": h["at"], "answer": h["answer"] });
        match list.iter_mut().find(|x| str_of(x, "id") == id) {
            Some(x) => {
                x["answer"] = h["answer"].clone();
                if !who.is_empty() {
                    x["who"] = json!(who);
                }
            }
            None => list.push(entry),
        }
        let extra = list.len().saturating_sub(KEEP_HELD);
        list.drain(..extra);
        Ok((Some(Value::Array(list)), ()))
    });
    if let Err(e) = r {
        eprintln!("cue: couldn't note the held step: {e}");
    }
}

/// `state.desk.projects[i].away_log`: Desk `desk`'s past stretches, newest first, each with its held steps and its
/// calls' Undo as it stands now.
pub(crate) fn log_view(st: &Store, desk: &str) -> Value {
    let cfg = crate::config::load();
    let held: Vec<Value> = cfg.pointer("/desk/held_log").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter(|h| str_of(h, "desk") == desk).collect();
    let mut out: Vec<Value> = cfg.pointer("/desk/away_log").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter(|r| str_of(r, "desk") == desk).collect();
    out.reverse();
    for r in &mut out {
        let (since, end) = (r["since"].as_u64().unwrap_or(0), r["end"].as_u64().unwrap_or(0));
        let mine: Vec<Value> = held
            .iter()
            .filter(|h| h["at"].as_u64().is_some_and(|at| at >= since && at <= end))
            .map(|h| {
                let kinds: Vec<String> = h.get("kinds").and_then(Value::as_array).map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
                json!({ "id": h["id"], "who": h["who"], "what": crate::desk::hub::away::away_holds::plain(&kinds), "command": h["command"], "at": h["at"], "answer": h["answer"] })
            })
            .collect();
        r["held"] = Value::Array(mine);
        if let Some(calls) = r.get_mut("calls").and_then(Value::as_array_mut) {
            for c in calls {
                let id = str_of(c, "id").to_string();
                let live = st.desk.decisions.iter().find(|d| d.id == id);
                c["undo"] = json!(live.map(|d| d.undo.clone()).unwrap_or_else(|| "none".into()));
                // Closed (#863): kept by you, or gone from the Desk's decisions (too old to undo or keep).
                c["kept"] = json!(live.is_none_or(|d| d.kept));
            }
        }
    }
    Value::Array(out)
}

/// The ids of the calls in Desk `desk`'s past Away stretches.
fn logged_calls(desk: &str) -> Vec<String> {
    let cfg = crate::config::load();
    let log = cfg.pointer("/desk/away_log").and_then(Value::as_array).cloned().unwrap_or_default();
    log.iter()
        .filter(|r| str_of(r, "desk") == desk)
        .flat_map(|r| r.get("calls").and_then(Value::as_array).cloned().unwrap_or_default())
        .map(|c| str_of(&c, "id").to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

impl Hub {
    /// Your Keep (Got it, Keep all) on calls `ids` made while you were away (#863): closed, they leave what's still
    /// open in the lead chat's card and the Away tab. Nothing is sent: the call stands as it was made.
    pub(crate) fn desk_calls_keep(&self, ids: &[String]) -> Result<(), String> {
        let kept: Vec<String> = self.write(|st| {
            st.desk
                .decisions
                .iter_mut()
                .filter(|d| d.call && !d.kept && ids.contains(&d.id))
                .map(|d| {
                    d.kept = true;
                    d.id.clone()
                })
                .collect()
        });
        if !kept.is_empty() {
            self.desk_done(crate::desk::hub::core::desk::Effects { decisions: kept, ..Default::default() });
            self.changed();
        }
        Ok(())
    }

    /// You wrote to Desk `desk`'s lead (#863): the calls made while you were away that you left open (neither kept
    /// nor undone) are kept, as you've moved on. They stay in the Away tab.
    pub(crate) fn desk_calls_keep_left(&self, desk: &str) {
        let ids = logged_calls(desk);
        if ids.is_empty() {
            return;
        }
        let open: Vec<String> = self.read(|st| st.desk.decisions.iter().filter(|d| d.project == desk && !d.kept && d.undo != "asked" && ids.contains(&d.id)).map(|d| d.id.clone()).collect());
        if !open.is_empty() {
            let _ = self.desk_calls_keep(&open);
        }
    }
}
