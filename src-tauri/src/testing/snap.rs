//! What the window would show, as a client's `list` gets it, with the questions tests ask of it. Each
//! lookup that fails says what was there instead.

use serde_json::Value;

pub struct Snap(pub Value);

impl Snap {
    fn list(&self, key: &str) -> Vec<&Value> {
        self.0[key].as_array().map(|a| a.iter().collect()).unwrap_or_default()
    }

    fn ids(rows: &[&Value]) -> Vec<String> {
        rows.iter().map(|r| r["session_id"].as_str().unwrap_or("?").to_string()).collect()
    }

    /// Session `sid`'s row, if it has one.
    pub fn find(&self, sid: &str) -> Option<&Value> {
        self.list("sessions").into_iter().find(|s| s["session_id"] == sid)
    }

    /// Session `sid`'s row; a panic naming the sessions there are, if it has none.
    #[track_caller]
    pub fn session(&self, sid: &str) -> &Value {
        self.find(sid).unwrap_or_else(|| panic!("no session {sid}; sessions: {:?}", Self::ids(&self.list("sessions"))))
    }

    #[track_caller]
    pub fn state(&self, sid: &str) -> String {
        self.session(sid)["state"].as_str().unwrap_or("").to_string()
    }

    /// Its chat, as (role, text).
    #[track_caller]
    pub fn thread(&self, sid: &str) -> Vec<(String, String)> {
        self.session(sid)["thread"].as_array().map(|t| t.iter().map(|e| (e["role"].as_str().unwrap_or("").into(), e["text"].as_str().unwrap_or("").into())).collect()).unwrap_or_default()
    }

    /// Its last message of yours in the chat.
    #[track_caller]
    pub fn last_yours(&self, sid: &str) -> Value {
        self.session(sid)["thread"].as_array().and_then(|t| t.iter().rev().find(|e| e["role"] == "you").cloned()).unwrap_or_else(|| panic!("nothing of yours in {sid}'s chat: {:?}", self.thread(sid)))
    }

    /// The pending cards, oldest first.
    pub fn cards(&self) -> Vec<&Value> {
        self.list("items")
    }

    /// Session `sid`'s pending cards.
    pub fn cards_of(&self, sid: &str) -> Vec<&Value> {
        self.cards().into_iter().filter(|i| i["session_id"] == sid).collect()
    }

    /// Session `sid`'s one pending card; a panic saying what cards there are otherwise.
    #[track_caller]
    pub fn card(&self, sid: &str) -> &Value {
        let mine = self.cards_of(sid);
        assert_eq!(mine.len(), 1, "expected one card for {sid}; cards: {:?}", self.cards().iter().map(|c| (c["session_id"].clone(), c["kind"].clone())).collect::<Vec<_>>());
        mine[0]
    }

    /// History's entry for card `id`.
    #[track_caller]
    pub fn history(&self, id: &str) -> &Value {
        self.list("history").into_iter().find(|h| h["id"] == id).unwrap_or_else(|| panic!("card {id} isn't in History: {:?}", self.list("history").iter().map(|h| h["id"].clone()).collect::<Vec<_>>()))
    }

    /// The sessions under Closed (ended while Cue ran), newest first.
    pub fn ended(&self) -> Vec<String> {
        Self::ids(&self.list("ended"))
    }

    pub fn session_ids(&self) -> Vec<String> {
        Self::ids(&self.list("sessions"))
    }
}
