//! A dialog open in a session's terminal (Claude Code's "Switch model?", a picker, a prompt Cue has no
//! hook for), read off its tmux pane: its title, what it says, and its numbered choices, so Cue's card
//! can offer them as buttons. Picking one presses that number in the pane.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Dialog {
    pub title: String,
    /// What it says under the title, if anything (cut short).
    pub detail: String,
    /// The choices, in order ("Yes, switch to Fable 5.1", "No, go back").
    pub options: Vec<String>,
    /// Which one its cursor is on.
    pub selected: usize,
}

/// A numbered choice line: ("❯ 1. Yes, switch" | "  2. No, go back") -> (number, text, has the cursor).
fn choice(line: &str) -> Option<(usize, String, bool)> {
    let t = line.trim();
    let (cursor, t) = match t.strip_prefix('❯').or_else(|| t.strip_prefix('>')) {
        Some(rest) => (true, rest.trim_start()),
        None => (false, t),
    };
    let (n, text) = t.split_once(". ")?;
    let n: usize = n.parse().ok().filter(|n| (1..=9).contains(n))?;
    let text = text.trim();
    (!text.is_empty()).then(|| (n, text.to_string(), cursor))
}

/// Does a line draw a frame: a rule or a box edge, possibly with a label in it ("──── cue-remote ─")?
fn is_rule(line: &str) -> bool {
    const RULE: &str = "─━═╭╮╰╯│┃┌┐└┘├┤-=";
    let t = line.trim();
    let run = |it: &mut dyn Iterator<Item = char>| it.take_while(|c| RULE.contains(*c)).count() >= 3;
    run(&mut t.chars()) || run(&mut t.chars().rev())
}

/// The dialog on `screen`, if one is open: the last run of choices numbered 1, 2, 3… at the bottom (at
/// most a hint line after it), with what stands above them up to a blank-separated paragraph or two.
pub fn parse(screen: &str) -> Option<Dialog> {
    let lines: Vec<&str> = screen.lines().collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    // The choices end the screen, or stand just above a hint ("Esc to cancel").
    let mut last = end;
    while last > 0 && choice(lines[last - 1]).is_none() && end - last < 2 {
        last -= 1;
    }
    let mut first = last;
    while first > 0 && choice(lines[first - 1]).is_some() {
        first -= 1;
    }
    let found: Vec<(usize, String, bool)> = lines[first..last].iter().filter_map(|l| choice(l)).collect();
    if found.len() < 2 || found.iter().enumerate().any(|(i, (n, _, _))| *n != i + 1) {
        return None;
    }
    // Above them: up to two paragraphs (blank-separated), stopping at a rule, a box edge, or the input box.
    let mut top = first;
    let mut blanks = 0;
    while top > 0 && first - top < 14 {
        let l = lines[top - 1].trim();
        if l.is_empty() {
            blanks += 1;
            if blanks > 3 {
                break;
            }
        } else if is_rule(l) || l.starts_with('❯') || l.starts_with("> ") {
            break;
        }
        top -= 1;
    }
    let mut above: Vec<&str> = lines[top..first].iter().map(|l| l.trim()).skip_while(|l| l.is_empty()).collect();
    while above.last().is_some_and(|l| l.is_empty()) {
        above.pop();
    }
    let title = above.first().copied().unwrap_or("").to_string();
    let detail: String = above.iter().skip(1).copied().filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    let detail: String = detail.chars().take(300).collect();
    Some(Dialog {
        title,
        detail,
        selected: found.iter().position(|(_, _, c)| *c).unwrap_or(0),
        options: found.into_iter().map(|(_, t, _)| t).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_switch_model_dialog_reads_as_title_detail_and_choices() {
        let screen = "\
 ❯ /model fable
 ──────────────────────────────── cue-remote ─

 Switch model?
 Your next response will be slower and use more tokens

 This conversation is cached for the current model. Switching to Fable 5.1 means the full history gets re-read on
 your next message.

 ❯ 1. Yes, switch to Fable 5.1
   2. No, go back


";
        let d = parse(screen).unwrap();
        assert_eq!(d.title, "Switch model?");
        assert!(d.detail.starts_with("Your next response will be slower") && d.detail.contains("Switching to Fable 5.1"), "{}", d.detail);
        assert_eq!(d.options, ["Yes, switch to Fable 5.1", "No, go back"]);
        assert_eq!(d.selected, 0);
    }

    #[test]
    fn a_permission_prompt_with_a_hint_under_it_and_the_cursor_lower_down() {
        let screen = "Do you want to proceed?\n  1. Yes\n  2. Yes, and don't ask again\n❯ 3. No, and tell Claude what to do differently\n  Esc to cancel\n";
        let d = parse(screen).unwrap();
        assert_eq!((d.title.as_str(), d.options.len(), d.selected), ("Do you want to proceed?", 3, 2));
    }

    #[test]
    fn plain_output_and_a_single_numbered_line_are_no_dialog() {
        assert_eq!(parse("Done.\n\n❯ \n"), None);
        assert_eq!(parse("Steps:\n1. open the file\n\n❯ \n"), None, "a list in prose isn't a dialog once the input box follows");
        assert_eq!(parse("1. one thing\n"), None, "one choice isn't a dialog");
        assert_eq!(parse("notes\n2. two\n3. three\n"), None, "choices start at 1");
    }
}
