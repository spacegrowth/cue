//! What the window looks like to the pointer and the keyboard right now (ui/page_look.js), asked over the socket
//! (`{"type":"page"}`) for when the cursor acts up while Cue runs (#456). Read only: the page changes nothing.

use super::*;
use std::sync::mpsc;

/// The asks still waiting for the window's answer, by id.
static WAITING: Mutex<Vec<(u64, mpsc::Sender<Value>)>> = Mutex::new(Vec::new());

/// Each ask's id: two asks never share one, however close together.
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// How long the window has to answer: one that doesn't is itself the answer (its page is stuck).
const PATIENCE_MS: u64 = 3000;

impl Hub {
    /// The window's answer, or why there is none. Blocking: call off the window's thread.
    pub fn page_look(&self) -> Value {
        let Some(app) = &self.app else { return json!({ "answered": false, "why": "no window" }) };
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        {
            let mut w = crate::locking::lock(&WAITING, "page looks waiting");
            w.push((id, tx));
        }
        use tauri::Emitter;
        let sent = app.emit_to("main", "page-look", json!({ "id": id }));
        let got = sent.ok().and_then(|_| rx.recv_timeout(std::time::Duration::from_millis(PATIENCE_MS)).ok());
        {
            let mut w = crate::locking::lock(&WAITING, "page looks waiting");
            w.retain(|(i, _)| *i != id);
        }
        match got {
            Some(look) => json!({ "answered": true, "look": look }),
            None => json!({ "answered": false, "why": format!("the window didn't answer in {} s: its page is stuck or busy", PATIENCE_MS / 1000) }),
        }
    }
}

/// The window's answer to ask `id`.
pub(crate) fn page_look_said(id: u64, look: Value) {
    let w = crate::locking::lock(&WAITING, "page looks waiting");
    if let Some((_, tx)) = w.iter().find(|(i, _)| *i == id) {
        let _ = tx.send(look);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_answer_goes_to_the_ask_with_its_id_and_no_other() {
        let (a, ra) = mpsc::channel();
        let (b, rb) = mpsc::channel();
        crate::locking::lock(&WAITING, "page looks waiting").extend([(900_001, a), (900_002, b)]);
        page_look_said(900_002, json!({ "active": "textarea" }));
        page_look_said(900_003, json!({ "active": "nobody asked" }));
        assert_eq!(rb.try_recv().ok(), Some(json!({ "active": "textarea" })));
        assert!(ra.try_recv().is_err(), "the other ask got an answer that wasn't its own");
        crate::locking::lock(&WAITING, "page looks waiting").retain(|(i, _)| *i < 900_000);
    }
}
