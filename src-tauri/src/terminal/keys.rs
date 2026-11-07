//! What the hub does to a session's terminal when it types into it: type a message, press Enter or Esc, press
//! keys in its pane, and look at its screen (is the text still in the box, a dialog open, a shell back at its
//! prompt). One trait, so tests can stand in a terminal that records what was typed and answers as told
//! (testing::FakeTerminal). `Terminals` is the real one: focus.rs, which reaches tmux, iTerm, Terminal.app,
//! WezTerm and kitty.
//!
//! SHORTCUT: only the hub's typing goes through here. The pane reads of hub/activity.rs (session_screen,
//! tmux_screen) and the built-in terminal (term.rs) still call focus.rs and the PTY directly; the full
//! `Terminal` port of the stability plan (§6 step 5) folds those in.

use crate::model::Origin;
use crate::terminal::dialog::Dialog;
use crate::terminal::focus;

pub trait Keys: Send + Sync {
    /// Type `text` and submit it; where it was typed ("tmux pane %3").
    fn type_into(&self, o: &Origin, text: &str) -> Result<String, String>;
    fn press_enter(&self, o: &Origin) -> Result<(), String>;
    /// Esc, the key that stops a turn; where it was pressed.
    fn press_escape(&self, o: &Origin) -> Result<String, String>;
    /// Keys pressed in its tmux pane, by tmux's names ("1", "Enter").
    fn press_keys(&self, o: &Origin, keys: &[String]) -> Result<(), String>;
    /// Is `text` still in its input box? None when its screen can't be read.
    fn text_in_box(&self, o: &Origin, text: &str) -> Option<bool>;
    /// The dialog open on its screen, if one is and the screen can be read.
    fn dialog_on(&self, o: &Origin) -> Option<Dialog>;
    /// Is a shell back at its prompt where the agent was? None when its screen can't be read.
    fn agent_gone(&self, o: &Origin) -> Option<bool>;
}

/// The terminals on this Mac and your machines, as focus.rs reaches them.
pub struct Terminals;

impl Keys for Terminals {
    fn type_into(&self, o: &Origin, text: &str) -> Result<String, String> {
        focus::type_into(o, text)
    }
    fn press_enter(&self, o: &Origin) -> Result<(), String> {
        focus::press_enter(o)
    }
    fn press_escape(&self, o: &Origin) -> Result<String, String> {
        focus::press_escape(o)
    }
    fn press_keys(&self, o: &Origin, keys: &[String]) -> Result<(), String> {
        focus::tmux_keys(o, keys)
    }
    fn text_in_box(&self, o: &Origin, text: &str) -> Option<bool> {
        focus::text_in_box(o, text)
    }
    fn dialog_on(&self, o: &Origin) -> Option<Dialog> {
        focus::dialog_on(o)
    }
    fn agent_gone(&self, o: &Origin) -> Option<bool> {
        focus::agent_gone(o)
    }
}
