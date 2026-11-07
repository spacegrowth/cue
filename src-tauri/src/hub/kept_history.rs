//! History as the store keeps it: every answered card in full, newest first, and the same list in brief for
//! the window's state. The state goes out on every change, so a row there carries only what a row shows (who,
//! what it wanted in a line, what you did, when); the conversation, the request in full and its options are
//! asked for when a row is opened (`Hub::history_item`). The brief rows are kept beside the cards, made once
//! per card as it's answered, so building the state never walks the cards under the store's lock.

use crate::model::Item;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::Arc;

/// How much of a long text a brief row keeps (a row shows one line of it).
const BRIEF_CHARS: usize = 300;
/// The request's parts a row's one line is made of (ui/core.js `summary`).
const SUMMARY_KEYS: [&str; 5] = ["command", "file_path", "path", "url", "description"];

#[derive(Default)]
pub(super) struct History {
    items: VecDeque<Item>,
    /// The brief rows, in the same order; None after a change that touched cards in place (rebuilt when asked).
    brief: Option<Arc<Vec<Value>>>,
}

impl History {
    pub(super) fn new(items: VecDeque<Item>) -> History {
        History { items, brief: None }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Item> {
        self.items.iter()
    }

    /// The cards to change in place: their brief rows are made again on the next ask.
    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Item> {
        self.brief = None;
        self.items.iter_mut()
    }

    pub(super) fn len(&self) -> usize {
        self.items.len()
    }

    /// `it` as the newest, the oldest let go past `keep`.
    pub(super) fn push_front(&mut self, it: Item, keep: usize) {
        if let Some(rows) = self.brief.as_mut() {
            let rows = Arc::make_mut(rows);
            rows.insert(0, brief(&it));
            rows.truncate(keep);
        }
        self.items.push_front(it);
        self.items.truncate(keep);
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&Item) -> bool) {
        self.items.retain(keep);
        self.brief = None;
    }

    /// The brief rows, newest first: an `Arc` handed out, so the state takes them without copying under the lock.
    pub(super) fn brief(&mut self) -> Arc<Vec<Value>> {
        self.brief.get_or_insert_with(|| Arc::new(self.items.iter().map(brief).collect())).clone()
    }
}

/// A card as a History row shows it: everything but its conversation (`thread`), its suggestions and its
/// follow-up; its message, its last line of context and its request cut to what one line shows. `brief`
/// says the full card is to be asked for.
pub(super) fn brief(it: &Item) -> Value {
    let Ok(Value::Object(mut row)) = serde_json::to_value(it) else { return Value::Null };
    row.insert("brief".into(), json!(true));
    row.insert("thread".into(), json!([]));
    row.insert("suggestions".into(), Value::Null);
    row.insert("followup".into(), json!(""));
    row.insert("message".into(), json!(cut(&it.message)));
    row.insert("context".into(), json!(it.context.last().map(|c| json!({ "role": c.role, "text": cut(&c.text) })).into_iter().collect::<Vec<_>>()));
    row.insert("tool_input".into(), brief_input(&it.tool_input));
    Value::Object(row)
}

/// The request's parts a row's line is made of, each cut short; a question's first question alone.
fn brief_input(input: &Value) -> Value {
    let mut out = Map::new();
    for k in SUMMARY_KEYS {
        if let Some(s) = input.get(k).and_then(Value::as_str) {
            out.insert(k.into(), json!(cut(s)));
        }
    }
    if let Some(q) = input.pointer("/questions/0/question").and_then(Value::as_str) {
        out.insert("questions".into(), json!([{ "question": cut(q) }]));
    }
    Value::Object(out)
}

fn cut(s: &str) -> String {
    if s.chars().count() <= BRIEF_CHARS {
        return s.to_string();
    }
    format!("{}…", s.chars().take(BRIEF_CHARS).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, command: &str) -> Item {
        serde_json::from_value(json!({
            "id": id, "kind": "permission", "session_id": "s", "harness": "claude", "cwd": "/x", "project": "x", "tool_name": "Bash",
            "tool_input": { "command": command, "description": "run it", "content": "x".repeat(10_000) }, "suggestions": [{ "rule": "Bash(make:*)" }],
            "context": [{ "role": "agent", "text": "first" }, { "role": "agent", "text": "last" }], "message": "",
            "thread": [{ "role": "agent", "text": "a long reply ".repeat(500), "at_ms": 1 }],
            "created_ms": 1, "status": "answered", "outcome": "allowed", "resolved_ms": 2,
        }))
        .unwrap()
    }

    #[test]
    fn a_brief_row_keeps_what_a_row_shows_and_none_of_the_conversation() {
        let row = brief(&card("h1", &"make ".repeat(200)));
        assert_eq!(row["id"], "h1");
        assert_eq!(row["outcome"], "allowed");
        assert_eq!(row["session_id"], "s");
        assert_eq!(row["brief"], true);
        assert_eq!(row["thread"], json!([]));
        assert_eq!(row["context"], json!([{ "role": "agent", "text": "last" }]));
        assert!(row["tool_input"].get("content").is_none(), "only what the line is made of");
        assert_eq!(row["tool_input"]["description"], "run it");
        assert_eq!(row["tool_input"]["command"].as_str().unwrap().chars().count(), BRIEF_CHARS + 1, "cut, with an ellipsis");
        assert!(row.to_string().len() < 2_000, "{}", row.to_string().len());
    }

    #[test]
    fn the_brief_rows_follow_the_cards_newest_first_and_capped() {
        let mut h = History::default();
        h.push_front(card("a", "x"), 2);
        let first = h.brief();
        assert_eq!(first.len(), 1);
        h.push_front(card("b", "x"), 2);
        h.push_front(card("c", "x"), 2);
        let ids = |h: &mut History| h.brief().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&mut h), ["c", "b"]);
        assert_eq!(first.len(), 1, "a list already handed out is never changed under its holder");
        h.retain(|i| i.id != "c");
        assert_eq!(ids(&mut h), ["b"]);
        for it in h.iter_mut() {
            it.outcome = "denied".into();
        }
        assert_eq!(h.brief()[0]["outcome"], "denied", "a card changed in place is made again");
    }
}
