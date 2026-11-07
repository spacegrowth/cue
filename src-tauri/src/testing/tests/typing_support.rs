//! What the typing cases share: a session at its prompt, a send's answer, a card's id, an image to attach,
//! and a send held up in a hung pane.

use crate::testing::{Client, Takes, World};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Session `sid` at its prompt: one turn done, its card up.
pub fn waiting(w: &World, sid: &str) {
    w.session(sid).prompt("start").stop("Done.");
}

/// What a send or reply said back.
pub fn detail(v: &Value) -> String {
    v["detail"].as_str().unwrap_or("").to_string()
}

pub fn card_id(w: &World, sid: &str) -> String {
    w.snapshot().card(sid)["id"].as_str().unwrap().to_string()
}

/// An image as the window attaches it (a data: URL), named `name`.
pub fn image(name: &str) -> Value {
    json!({"name": name, "mime": "image/png", "data": "data:image/png;base64,iVBORw0KGgo="})
}

/// Send `text` with `images` to session `sid`, as the box does.
pub fn send_with(w: &World, sid: &str, text: &str, images: Vec<Value>) -> Value {
    let v = w.client().ask(json!({"type": "send_to", "session_id": sid, "text": text, "images": images}));
    w.settle();
    v
}

/// A send to `sid` stuck typing into its hung pane: its thread, and what lets it go.
pub struct Stuck {
    pub send: std::thread::JoinHandle<Value>,
    release: Arc<AtomicBool>,
}

impl Stuck {
    /// `text` sent to `sid` from a thread of its own, its pane not answering until `release`.
    pub fn sending(w: &World, sid: &str, text: &str) -> Stuck {
        let release = Arc::new(AtomicBool::new(false));
        w.terminal.pane(sid, |p| p.takes = Takes::Hangs(release.clone()));
        let (sid, text) = (sid.to_string(), text.to_string());
        let send = w.client().spawn(move |c: Client| c.send_to(&sid, &text, false));
        w.terminal.wait_hung();
        Stuck { send, release }
    }

    /// Its pane answers: the send finishes; what it said.
    pub fn release(self) -> Value {
        self.release.store(true, Ordering::SeqCst);
        self.send.join().unwrap()
    }
}
