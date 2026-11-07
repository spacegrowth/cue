//! Reading a session's screen, wherever its terminal is: the agent's input box and what's in it, a paste
//! landing, a dialog, and a shell back at its prompt where the agent was.

use super::apps::{current_tty, iterm_contents, iterm_predicate, terminal_app_contents};
use super::tmux::{pane_now, session_screen};
use crate::model::Origin;

/// Whether Claude Code's input box is empty on `screen` (session_screen): its prompt line, "❯ " (or "> "
/// inside the older │ box), in the last lines. None when no prompt line shows (a question or a permission
/// prompt has the screen, or another agent's): unknown, so nothing should be typed into it unasked.
pub fn box_empty(screen: &str) -> Option<bool> {
    let lines: Vec<&str> = screen.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.iter().rev().take(12).find_map(|l| {
        let l = l.trim().trim_start_matches('│').trim_end_matches('│').trim();
        let rest = l.strip_prefix('❯').or_else(|| l.strip_prefix('>'))?;
        Some(rest.trim().is_empty())
    })
}

/// What the session's terminal shows right now, wherever it is: its tmux pane (this Mac's or a
/// machine's), its iTerm session (`contents of session`) or its Terminal.app tab (`contents of tab`).
/// Read only: nothing is raised or focused. None when Cue has no way to see it (WezTerm, kitty, an
/// editor's terminal), or the terminal is gone.
pub(super) fn screen_of(o: &Origin) -> Option<String> {
    if !o.tmux_pane.is_empty() {
        return session_screen(o).ok();
    }
    if let Some(pred) = iterm_predicate(o) {
        return iterm_contents(o, &pred);
    }
    if o.term_program == "Apple_Terminal" && !o.tty.is_empty() {
        return terminal_app_contents(&o.tty);
    }
    None
}

/// Claude Code's input box on `screen`, as one line: its prompt line ("❯ …", or "│ > … │" in the older
/// box) and the lines under it up to the rule that closes the box, where a long message wraps. The last
/// prompt line in the bottom of the screen is the box; the agent's echo of a message it took (also
/// "> …", in older versions) stands above the box's top rule and is never it. None when no box shows
/// (a question or a permission prompt has the screen, or it's another agent's).
pub(super) fn box_text(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen.lines().collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let inner = |l: &str| l.trim().trim_start_matches('│').trim_end_matches('│').trim().to_string();
    // Claude Code's ❯ (> in older versions), Codex's ›.
    let prompt = |l: &str| inner(l).starts_with(['❯', '>', '›']);
    let at = (end.saturating_sub(20)..end).rev().find(|i| prompt(lines[*i]))?;
    let body: Vec<String> = lines[at..end].iter().take_while(|l| !crate::terminal::dialog::is_rule(l) || prompt(l)).map(|l| inner(l)).collect();
    Some(body.join(" "))
}

/// Is `text` still sitting in the session's input box? None when Cue can't see its terminal, or no
/// box shows on it. What one look can and can't say: see `still_in_box`.
pub fn text_in_box(o: &Origin, text: &str) -> Option<bool> {
    let screen = screen_of(o)?;
    Some(still_in_box(&box_text(&screen)?, text))
}

/// Does the input box (`box_text`) show `text` waiting: the end of the text, or the
/// "[Pasted text #1 +12 lines]" placeholder Claude Code folds a paste of several lines into (the text
/// itself is then nowhere on screen, and that used to read as "gone", so no Enter was pressed again).
/// One look is a sign, not proof: the agent may be mid-redraw. A verdict that a message wasn't taken
/// needs the text seen there again later, with nothing else saying it was taken (hub::send).
pub(super) fn still_in_box(box_text: &str, text: &str) -> bool {
    let line = squash(box_text);
    let tail = paste_tail(text);
    (!tail.is_empty() && line.contains(&tail)) || (text.contains('\n') && line.contains("[Pastedtext#"))
}

/// The agent is gone from its terminal, so nothing typed there reaches one: in tmux, a shell has the pane's
/// foreground (`shell_in_front`); elsewhere a shell's prompt has the screen and the agent's own process is
/// nowhere to be found (iTerm, Terminal.app). None when Cue can't see the terminal.
pub fn agent_gone(o: &Origin) -> Option<bool> {
    if !o.tmux_pane.is_empty() {
        return pane_now(o, &o.tmux_pane).ok().map(|p| p.shell_in_front);
    }
    let screen = screen_of(o)?;
    Some(at_shell_prompt(&screen) && current_tty(o).is_none())
}

/// A dialog open on the session's screen (its choices numbered), if Cue can see its terminal.
pub fn dialog_on(o: &Origin) -> Option<crate::terminal::dialog::Dialog> {
    crate::terminal::dialog::parse(&screen_of(o)?)
}

/// A screen without spaces or the input box's side borders: how a wrapped line reads joined up again.
fn squash(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace() && *c != '│').collect()
}

/// The last characters of a message as its input box shows them.
pub(super) fn paste_tail(text: &str) -> String {
    let t: Vec<char> = squash(text).chars().collect();
    t[t.len().saturating_sub(24)..].iter().collect()
}

/// Has the paste landed on screen since `before`? Its end shows once more than it did, or one more
/// "[Pasted text" placeholder does (the text may be on screen already, from an earlier message).
pub(super) fn paste_shows(before: &str, now: &str, tail: &str) -> bool {
    let n = |s: &str, pat: &str| if pat.is_empty() { 0 } else { s.matches(pat).count() };
    let (b, a) = (squash(before), squash(now));
    n(&a, tail) > n(&b, tail) || n(&a, "[Pastedtext") > n(&b, "[Pastedtext")
}

/// A process named `command` is a shell, not an agent: the usual shells by the name tmux or ps gives them
/// (a login shell shows with a leading "-").
pub(super) fn is_shell(command: &str) -> bool {
    matches!(command.trim_start_matches('-'), "zsh" | "bash" | "sh" | "fish" | "dash" | "ksh" | "tcsh" | "csh" | "nu" | "login")
}

/// The screen ends the way a shell waiting for a command does: its last line is a prompt, "user@host:~$ ",
/// "% ", "❯ ". An agent's screen ends in its input box or status line, never like that.
pub(super) fn at_shell_prompt(screen: &str) -> bool {
    let last = screen.lines().rev().map(str::trim_end).find(|l| !l.is_empty()).unwrap_or("").trim_end();
    last.len() <= 160 && last.ends_with(['$', '%', '#', '>', '❯', '➜'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_shows_once_its_end_is_on_screen_once_more() {
        let before = "> earlier: fix the login\n────\n> ";
        let tail = paste_tail("now fix the login");
        assert!(!paste_shows(before, before, &tail), "nothing new yet");
        assert!(!paste_shows(before, "> earlier: fix the login\n────\n> now fix", &tail), "only part of it");
        assert!(paste_shows(before, "> earlier: fix the login\n────\n> now fix the\n  login", &tail), "wrapped is fine");
        // The same words as an earlier message: only a second copy counts.
        let tail = paste_tail("fix the login");
        assert!(!paste_shows(before, "> earlier: fix the login\n────\n> ", &tail));
        assert!(paste_shows(before, "> earlier: fix the login\n────\n> fix the login", &tail));
    }

    #[test]
    fn a_long_paste_shows_as_its_placeholder() {
        let tail = paste_tail(&"a long message ".repeat(80));
        assert!(paste_shows("> ", "> [Pasted text #1 +12 lines]", &tail));
        assert!(!paste_shows("> [Pasted text #1 +3 lines]", "> [Pasted text #1 +3 lines]", &tail));
    }

    #[test]
    fn the_box_shows_a_message_by_its_end_or_by_the_placeholder_a_paste_folds_into() {
        assert!(still_in_box("│ > now fix the login page please │", "now fix the login page please"));
        assert!(still_in_box("  and then run the login page tests please", "now fix the\nand then run the login page tests please"), "wrapped: the end is on the cursor's line");
        // The last 24 characters are what's looked for: a wrapped line whose last row holds fewer than
        // that isn't seen (then there's no verdict, which is the safe side).
        assert!(!still_in_box("  login page please", "now fix the\nlogin page please"));
        assert!(!still_in_box("│ > ", "now fix the login page please"), "an empty box");
        assert!(!still_in_box("⠋ Thinking…", "now fix the login page please"));
        // Several lines pasted: Claude Code shows "[Pasted text #1 +12 lines]" and none of the text.
        let long = "you can make the change\n\nbut after that, check why I did not see\n\nWant me to do that now?";
        assert!(still_in_box("│ > [Pasted text #1 +4 lines] │", long));
        assert!(!still_in_box("│ > [Pasted text #1 +4 lines] │", "a one-line message"), "a one-liner is never folded: that placeholder is someone else's");
        assert!(!still_in_box("│ > ", long));
    }

    /// Screens as tmux, iTerm and Terminal.app give them: the box is the last prompt line near the
    /// bottom with what wraps under it, up to the rule that closes it; the echo above it never counts.
    #[test]
    fn the_input_box_is_found_on_a_screen_whole_and_the_echo_above_it_is_not_it() {
        let rule = "─".repeat(40);
        let status = format!("{rule}\n  Opus 5.5  .../cue  git:main*\n  ⏵⏵ bypass permissions on (shift+tab to cycle)\n\n");
        let text = "now fix the login page, then run the whole suite and tell me what broke";
        let new_box = format!("> {text}\n\n⏺ On it.\n\n{rule}\n❯ {}\n  {}\n{status}", &text[..40], &text[40..]);
        assert_eq!(box_text(&new_box).as_deref(), Some(format!("❯ {} {}", &text[..40], &text[40..]).as_str()));
        assert!(still_in_box(&box_text(&new_box).unwrap(), text), "wrapped across the box's rows, found whole");
        let taken = format!("> {text}\n\n⏺ On it.\n\n{rule}\n❯ \n{status}");
        assert!(!still_in_box(&box_text(&taken).unwrap(), text), "its echo above the box is not the box");
        let old_box = format!("> {text}\n\n╭{rule}╮\n│ > [Pasted text #1 +4 lines] │\n╰{rule}╯\n  ? for shortcuts");
        assert_eq!(box_text(&old_box).as_deref(), Some("> [Pasted text #1 +4 lines]"));
        assert!(still_in_box(&box_text(&old_box).unwrap(), "several\nlines\nof\ntext"));
        assert_eq!(box_text("Do you want to proceed?\n  1. Yes\n  2. No\n\n"), None, "a question has the screen: no box");
        assert_eq!(box_text(""), None);
        // Codex's composer: its › prompt is the box too.
        let codex = "• Ran cargo test\n\n› fix the flaky test\n\n";
        assert_eq!(box_text(codex).as_deref(), Some("› fix the flaky test"));
        assert!(still_in_box(&box_text(codex).unwrap(), "fix the flaky test"));
    }

    #[test]
    fn an_empty_box_is_told_from_one_with_something_typed_in_it() {
        let rule = "─".repeat(40);
        let screen = |prompt: &str| format!("✻ Baked for 1m 50s\n{rule}\n{prompt}\n{rule}\n  Opus 5.5  .../cue  git:main*\n  ⏵⏵ bypass permissions on (shift+tab to cycle)");
        assert_eq!(box_empty(&screen("❯ ")), Some(true));
        assert_eq!(box_empty(&screen("❯ yes go ahead, mac first")), Some(false));
        assert_eq!(box_empty("╭──────╮\n│ >    │\n╰──────╯"), Some(true), "the older box");
        assert_eq!(box_empty("╭──────╮\n│ > hi │\n╰──────╯"), Some(false));
        assert_eq!(box_empty("Do you want to proceed?\n  1. Yes\n  2. No"), None, "no box: a question has the screen");
    }
}
