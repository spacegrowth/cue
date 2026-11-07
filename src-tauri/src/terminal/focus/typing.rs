//! Typing a reply into the agent's terminal, and the keys that go with it (Enter, Esc).
//!
//! Learned the hard way: text + newline in one burst reads as a paste and sits unsubmitted in Claude
//! Code's input box. So: put the text in as one paste with no newline, pause (longer for longer text),
//! then send a separate Enter. One paste means bracketed (the markers the agent asked its terminal for):
//! everything between them is text, however it arrives in pieces, so an Enter can't land in the middle
//! of a long one and send half of it.

use super::apps::{iterm_predicate, iterm_session_do, kitty, terminal_app_script};
use super::exec::{osa, run};
use super::screen::{paste_shows, paste_tail};
use super::tmux::{pane_now, session_screen, tmux_for, tmux_try_for, tmux_word};
use crate::model::Origin;

/// Pause between typing and Enter, in seconds: 0.6s plus 0.05s per 100 characters, at most 4s.
pub(super) fn enter_gap(text: &str) -> f64 {
    (0.6 + 0.05 * text.chars().count() as f64 / 100.0).min(4.0)
}

/// In tmux Cue can see the pane, so it doesn't sleep the whole gap: it watches until the paste shows in
/// the agent's input box (the end of its text, or Claude Code's "[Pasted text …]" for a long one), then
/// Enter goes straight away. Most messages go in a few tens of milliseconds; a machine's pane is read over
/// SSH, so there it's as long as the paste really takes. Never longer than `enter_gap`.
fn wait_for_paste(o: &Origin, text: &str, before: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(enter_gap(text));
    let tail = paste_tail(text);
    while std::time::Instant::now() < end {
        if let Ok(now) = session_screen(o) {
            if paste_shows(before, &now, &tail) {
                // Drawn means taken in; a moment more for the agent to be ready for a key.
                std::thread::sleep(std::time::Duration::from_millis(40));
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Type `text` into the agent's terminal and submit it: tmux, iTerm, Terminal.app, WezTerm or kitty.
pub fn type_into(o: &Origin, text: &str) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return type_into_tmux(o, &o.tmux_pane, text);
    }
    if let Some(pred) = iterm_predicate(o) {
        return type_into_iterm(o, &pred, text);
    }
    if !o.wezterm_pane.is_empty() {
        let typed = run("wezterm", &["cli", "send-text", "--pane-id", &o.wezterm_pane, text]);
        return enter_after(o, text, typed, format!("WezTerm pane {} is gone", o.wezterm_pane), format!("WezTerm pane {}", o.wezterm_pane));
    }
    if !o.kitty_window_id.is_empty() {
        let typed = kitty(o, &["send-text", "--bracketed-paste", "enable", "--match", &format!("id:{}", o.kitty_window_id), text]);
        return enter_after(o, text, typed, "kitty refused. Turn on allow_remote_control in kitty.conf".into(), format!("kitty window {}", o.kitty_window_id));
    }
    if o.term_program == "Apple_Terminal" && !o.tty.is_empty() {
        // `do script` sends the text plus a newline; Claude Code reads that burst as a paste, so a
        // second, empty `do script` is the Enter that submits it.
        // Terminal.app has no paste API: newlines would submit early, so lines are joined.
        let one_line = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
        let typed = terminal_app_script(&o.tty, &one_line)?;
        return enter_after(o, text, typed, "that Terminal tab is gone".into(), "Terminal tab".into());
    }
    Err("Cue can't type into this terminal (it works with iTerm, Terminal, tmux, WezTerm and kitty). Use Go to session".into())
}

/// Into a tmux pane. First, one trip for what's in the pane and the screen as it is: the agent must be
/// what's there (back at its shell, the paste plus Enter would run the message as a command). Second
/// trip: the paste (bracketed when the app asked for it, so a multi-line reply can't submit early).
/// "--": the text is the buffer even when it starts with "-"; a final ";" would end the tmux command
/// instead of being typed, so it goes escaped (tmux turns "\;" back into ";").
fn type_into_tmux(o: &Origin, pane: &str, text: &str) -> Result<String, String> {
    let now = pane_now(o, pane)?;
    if now.shell_in_front {
        return Err(format!("its pane shows a shell prompt ({}), not the agent: nothing typed", now.running));
    }
    let word = tmux_word(text);
    tmux_try_for(o, pane, &["set-buffer", "-b", "cue", "--", &word, ";", "paste-buffer", "-p", "-d", "-b", "cue", "-t", pane])?;
    wait_for_paste(o, &word, &now.screen);
    press_enter(o)?;
    Ok(format!("tmux pane {pane}"))
}

/// Into an iTerm session. iTerm types what it's given: the paste markers go around it by hand
/// (ESC [200~ … ESC [201~), as tmux, kitty and WezTerm do themselves.
fn type_into_iterm(o: &Origin, pred: &str, text: &str) -> Result<String, String> {
    let action = format!(
        "tell s to write text ((character id 27) & \"[200~\" & \"{}\" & (character id 27) & \"[201~\") newline NO\n                   delay {:.2}\n                   tell s to write text \"\"",
        osa(text),
        enter_gap(text)
    );
    if iterm_session_do(o, pred, &action)? {
        return Ok("iTerm session".into());
    }
    Err("that iTerm session is gone (its tab was closed?)".into())
}

/// After `text` was typed where Cue can't watch the paste land (`typed`: whether the terminal took it,
/// else `gone` is the error): the whole pause, then Enter; `where_` is where it went.
fn enter_after(o: &Origin, text: &str, typed: bool, gone: String, where_: String) -> Result<String, String> {
    if !typed {
        return Err(gone);
    }
    std::thread::sleep(std::time::Duration::from_secs_f64(enter_gap(text)));
    press_enter(o)?;
    Ok(where_)
}

/// Esc, the key that interrupts Claude Code and Codex mid-turn.
pub fn press_escape(o: &Origin) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return if tmux_for(o, &["send-keys", "-t", &o.tmux_pane, "Escape"]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err("tmux pane is gone".into()) };
    }
    if let Some(pred) = iterm_predicate(o) {
        return match iterm_session_do(o, &pred, "tell s to write text (ASCII character 27) newline NO")? {
            true => Ok("iTerm session".into()),
            false => Err("that iTerm session is gone (its tab was closed?)".into()),
        };
    }
    if !o.wezterm_pane.is_empty() {
        return if run("wezterm", &["cli", "send-text", "--no-paste", "--pane-id", &o.wezterm_pane, "\x1b"]) { Ok(format!("WezTerm pane {}", o.wezterm_pane)) } else { Err("WezTerm pane is gone".into()) };
    }
    if !o.kitty_window_id.is_empty() {
        let m = format!("id:{}", o.kitty_window_id);
        return if kitty(o, &["send-text", "--match", &m, "\x1b"]) { Ok(format!("kitty window {}", o.kitty_window_id)) } else { Err("kitty refused".into()) };
    }
    // Terminal.app only takes whole lines from scripts, never a bare key.
    Err("Cue can't press Esc in Terminal.app: use Go to tab and press Esc there".into())
}

/// One bare Enter: submits a typed reply, and is the retry when one didn't submit.
pub fn press_enter(o: &Origin) -> Result<(), String> {
    if !o.tmux_pane.is_empty() {
        return tmux_try_for(o, &o.tmux_pane, &["send-keys", "-t", &o.tmux_pane, "Enter"]).map(|_| ());
    }
    if let Some(pred) = iterm_predicate(o) {
        return iterm_session_do(o, &pred, "tell s to write text \"\"").map(|_| ());
    }
    if !o.wezterm_pane.is_empty() {
        return if run("wezterm", &["cli", "send-text", "--no-paste", "--pane-id", &o.wezterm_pane, "\r"]) { Ok(()) } else { Err("WezTerm pane is gone".into()) };
    }
    if !o.kitty_window_id.is_empty() {
        let m = format!("id:{}", o.kitty_window_id);
        return if kitty(o, &["send-text", "--match", &m, "\r"]) { Ok(()) } else { Err("kitty refused".into()) };
    }
    if o.term_program == "Apple_Terminal" && !o.tty.is_empty() {
        return terminal_app_script(&o.tty, "").map(|_| ());
    }
    Err("no terminal to press Enter in".into())
}
