//! A terminal per session that records every key Cue sends it and answers as the test says (`Takes`, and
//! what its screen shows). Typing into a session whose agent takes it fires that agent's prompt hook, as
//! Claude Code would, through the World (`on_prompt`).

use crate::model::Origin;
use crate::terminal::dialog::Dialog;
use crate::terminal::keys::Keys;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the agent in a pane does with a typed message.
#[derive(Clone, Default)]
pub enum Takes {
    /// Takes it: its prompt hook fires.
    #[default]
    Prompt,
    /// Typed, and nothing ever says it was taken.
    Nothing,
    /// The terminal refuses it (the pane's gone): typing fails with this.
    Refuses(String),
    /// Typing doesn't return until `release` (a tmux server not answering), then takes it.
    Hangs(Arc<AtomicBool>),
}

/// One thing Cue did to a session's terminal.
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Typed(String),
    Enter,
    Escape,
    Keys(Vec<String>),
}

/// How a session's pane behaves, and what its screen shows.
#[derive(Clone, Default)]
pub struct Pane {
    pub takes: Takes,
    /// Whether a typed message is seen still in its box (None: its screen can't be read).
    pub in_box: Option<bool>,
    pub dialog: Option<Dialog>,
    pub gone: Option<bool>,
}

type OnPrompt = Box<dyn Fn(&Origin, &str) + Send + Sync>;

#[derive(Default)]
pub struct FakeTerminal {
    panes: Mutex<HashMap<String, Pane>>,
    /// What Cue did, by session, in order.
    keys: Mutex<Vec<(String, Key)>>,
    on_prompt: Mutex<Option<Arc<OnPrompt>>>,
    /// Typings held up in a hung pane right now.
    hung: std::sync::atomic::AtomicUsize,
}

impl FakeTerminal {
    /// What an agent that takes a message does: its prompt hook (the World sends it through the socket's path).
    pub fn on_prompt(&self, f: impl Fn(&Origin, &str) + Send + Sync + 'static) {
        *crate::locking::lock(&self.on_prompt, "fake terminal hook") = Some(Arc::new(Box::new(f)));
    }

    /// Change how session `sid`'s pane behaves.
    pub fn pane(&self, sid: &str, f: impl FnOnce(&mut Pane)) {
        f(crate::locking::lock(&self.panes, "fake panes").entry(sid.to_string()).or_default());
    }

    /// Everything Cue did to session `sid`'s terminal, in order.
    pub fn keys(&self, sid: &str) -> Vec<Key> {
        crate::locking::lock(&self.keys, "fake keys").iter().filter(|(s, _)| s == sid).map(|(_, k)| k.clone()).collect()
    }

    /// What was typed into session `sid`, in order.
    pub fn typed(&self, sid: &str) -> Vec<String> {
        self.keys(sid).into_iter().filter_map(|k| if let Key::Typed(t) = k { Some(t) } else { None }).collect()
    }

    /// Wait (for real, a few seconds at most) until a typing is held up in a hung pane.
    pub fn wait_hung(&self) {
        let t0 = std::time::Instant::now();
        while self.hung.load(Ordering::SeqCst) == 0 {
            assert!(t0.elapsed().as_secs() < 5, "nothing is typing into a hung pane");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn of(&self, o: &Origin) -> Pane {
        crate::locking::lock(&self.panes, "fake panes").get(&o.session_id).cloned().unwrap_or_default()
    }

    fn note(&self, o: &Origin, k: Key) {
        crate::locking::lock(&self.keys, "fake keys").push((o.session_id.clone(), k));
    }

    fn prompt_hook(&self, o: &Origin, text: &str) {
        let hook = crate::locking::lock(&self.on_prompt, "fake terminal hook").clone();
        if let Some(hook) = hook {
            hook(o, text.trim());
        }
    }
}

/// How long a hung pane waits to be released before the test is told it never was.
const HANG_MS: u64 = 20_000;

impl Keys for FakeTerminal {
    fn type_into(&self, o: &Origin, text: &str) -> Result<String, String> {
        let pane = self.of(o);
        match pane.takes {
            Takes::Refuses(why) => return Err(why),
            Takes::Hangs(release) => {
                self.hung.fetch_add(1, Ordering::SeqCst);
                let t0 = std::time::Instant::now();
                while !release.load(Ordering::SeqCst) {
                    assert!(t0.elapsed().as_millis() < HANG_MS as u128, "a hung pane was never released");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                self.hung.fetch_sub(1, Ordering::SeqCst);
            }
            Takes::Prompt | Takes::Nothing => {}
        }
        self.note(o, Key::Typed(text.to_string()));
        if matches!(self.of(o).takes, Takes::Prompt | Takes::Hangs(_)) {
            self.prompt_hook(o, text);
        }
        Ok(format!("fake pane {}", o.session_id))
    }

    fn press_enter(&self, o: &Origin) -> Result<(), String> {
        self.note(o, Key::Enter);
        Ok(())
    }

    fn press_escape(&self, o: &Origin) -> Result<String, String> {
        self.note(o, Key::Escape);
        Ok(format!("fake pane {}", o.session_id))
    }

    fn press_keys(&self, o: &Origin, keys: &[String]) -> Result<(), String> {
        self.note(o, Key::Keys(keys.to_vec()));
        Ok(())
    }

    fn text_in_box(&self, o: &Origin, _text: &str) -> Option<bool> {
        self.of(o).in_box
    }

    fn dialog_on(&self, o: &Origin) -> Option<Dialog> {
        self.of(o).dialog
    }

    fn agent_gone(&self, o: &Origin) -> Option<bool> {
        self.of(o).gone
    }
}
