//! Typing into a tmux pane (stability plan §4, Typing, U and F): the word tmux is handed, the pause before
//! Enter, telling a shell's prompt from an agent's screen, and type_into against a stand-in tmux
//! (terminal::fake_tmux) that hangs, refuses Enter, shows a shell, or isn't installed.

use super::*;
use crate::terminal::fake_tmux::FakeTmux;
use std::time::{Duration, Instant};

fn pane() -> Origin {
    Origin { tmux_pane: "%9".into(), ..Default::default() }
}

#[test]
fn only_a_final_semicolon_is_escaped_for_tmux() {
    assert_eq!(tmux_word("ls;"), "ls\\;");
    assert_eq!(tmux_word(";"), "\\;");
    assert_eq!(tmux_word("a;b"), "a;b", "only a word that ends in one ends tmux's command");
    assert_eq!(tmux_word("done\\;"), "done\\;", "already escaped: left as it is");
    assert_eq!(tmux_word(""), "");
}

#[test]
fn the_pause_before_enter_grows_with_the_characters_and_stops_at_four_seconds() {
    assert_eq!(enter_gap(""), 0.6);
    assert!((enter_gap(&"e".repeat(1_000)) - 1.1).abs() < 1e-9);
    assert_eq!(enter_gap(&"é".repeat(1_000)), enter_gap(&"e".repeat(1_000)), "characters, not bytes");
    assert_eq!(enter_gap(&"x".repeat(1_000_000)), 4.0);
}

#[test]
fn a_shell_waiting_for_a_command_is_told_from_an_agents_screen() {
    for prompt in ["me@mac ~ % ", "user@box:~/src$ ", "root@box:/# ", "❯ ", "> ", "~/cue ➜", "$\n\n\n"] {
        assert!(at_shell_prompt(&format!("last output\n{prompt}")), "{prompt:?}");
    }
    let claude = "✻ Baked for 3s\n────\n❯ \n────\n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
    assert!(!at_shell_prompt(claude), "its status line, not a prompt");
    assert!(!at_shell_prompt("  ? for shortcuts"));
    assert!(!at_shell_prompt(""), "nothing on screen: not proof of a shell");
    assert!(!at_shell_prompt(&format!("{}$", "x".repeat(200))), "a long line ending in $ is output, not a prompt");
}

#[test]
#[ignore = "bug: oh-my-zsh's default prompt (robbyrussell, `➜  cue git:(main) ✗ `) ends in the folder or ✗, so at_shell_prompt misses it and a message is typed into the shell as a command once the agent has exited. Telling it from Claude Code's own `❯ ` box line (which starts the same way) is a design call: read the line's start too, or ask tmux for the pane's foreground process instead of its group leader."]
fn oh_my_zshs_default_prompt_is_a_shell_prompt() {
    assert!(at_shell_prompt("➜  cue git:(main) ✗ "));
}

#[test]
fn the_usual_shells_are_shells_and_agents_are_not() {
    for s in ["zsh", "-zsh", "bash", "fish", "sh", "login", "nu"] {
        assert!(is_shell(s), "{s}");
    }
    for s in ["claude", "node", "codex", "pi", "python3", ""] {
        assert!(!is_shell(s), "{s:?}");
    }
}

#[test]
fn a_pane_at_a_shell_prompt_gets_nothing_typed() {
    let tmux = FakeTmux::answering(r"display-message) printf 'zsh\nme@mac ~ %% ' ;;");
    let err = type_into(&pane(), "rm -rf build").unwrap_err();
    assert!(err.contains("shell prompt (zsh)"), "{err}");
    assert!(tmux.calls_of("set-buffer").is_empty() && tmux.calls_of("send-keys").is_empty(), "{:?}", tmux.calls());
}

#[test]
fn an_agent_started_through_sh_is_typed_into_though_tmux_names_the_shell() {
    let tmux = FakeTmux::answering(r"display-message) printf 'sh\n╭────╮\n│ >  │\n╰────╯' ;;");
    assert_eq!(type_into(&pane(), "go on").as_deref(), Ok("tmux pane %9"));
    assert_eq!(tmux.calls_of("send-keys").len(), 1);
}

#[test]
fn a_message_is_pasted_bracketed_and_enter_goes_once_it_shows() {
    let tmux = FakeTmux::agent();
    assert_eq!(type_into(&pane(), "run the tests").as_deref(), Ok("tmux pane %9"));
    assert_eq!(tmux.calls_of("capture-pane").len(), 1, "Enter at the first look that showed it, not after the whole pause");
    let calls = tmux.calls();
    let paste = calls.iter().position(|c| c.starts_with("set-buffer -b cue -- run the tests ; paste-buffer -p -d -b cue -t %9"));
    let enter = calls.iter().position(|c| c == "send-keys -t %9 Enter");
    assert!(paste.is_some() && enter.is_some() && paste < enter, "{calls:?}");
}

#[test]
fn enter_waits_out_the_pause_when_the_paste_never_shows() {
    let tmux = FakeTmux::answering("capture-pane) printf '> ' ;;");
    let t0 = Instant::now();
    type_into(&pane(), "hello").unwrap();
    assert!(t0.elapsed() >= Duration::from_secs_f64(enter_gap("hello")), "{:?}", t0.elapsed());
    assert!(tmux.calls_of("capture-pane").len() > 1, "it kept looking");
    assert_eq!(tmux.calls_of("send-keys").len(), 1);
}

#[test]
fn a_message_that_starts_with_a_dash_or_ends_with_a_semicolon_is_pasted_as_text() {
    let tmux = FakeTmux::agent();
    type_into(&pane(), "-v please").unwrap();
    type_into(&pane(), "then ls;").unwrap();
    let pastes = tmux.calls_of("set-buffer");
    assert!(pastes[0].starts_with("set-buffer -b cue -- -v please ;"), "{pastes:?}");
    assert!(pastes[1].starts_with("set-buffer -b cue -- then ls\\; ;"), "{pastes:?}");
}

#[test]
fn a_paste_whose_enter_tmux_refuses_fails_the_typing() {
    let tmux = FakeTmux::answering("send-keys) exit 1 ;;");
    let err = type_into(&pane(), "hello").unwrap_err();
    assert_eq!(err, "tmux pane %9 is gone");
    assert_eq!(tmux.calls_of("set-buffer").len(), 1, "the paste went in; its Enter didn't");
}

#[test]
fn a_tmux_that_doesnt_answer_is_cut_off_and_nothing_is_pasted() {
    let tmux = FakeTmux::answering("display-message) exec sleep 60 ;;");
    let t0 = Instant::now();
    assert!(type_into(&pane(), "hello").is_err());
    assert!(t0.elapsed() < Duration::from_secs(12), "cut off at its 10 s cap: {:?}", t0.elapsed());
    assert!(tmux.calls_of("set-buffer").is_empty());
}

#[test]
fn with_no_tmux_installed_typing_fails_at_once() {
    let _none = FakeTmux::missing();
    assert!(!tmux_installed());
    let t0 = Instant::now();
    assert_eq!(type_into(&pane(), "hello"), Err("tmux pane %9 is gone".into()));
    assert!(press_escape(&pane()).is_err());
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
}

#[test]
fn esc_into_a_pane_tmux_refuses_says_the_pane_is_gone() {
    let _tmux = FakeTmux::answering("send-keys) exit 1 ;;");
    assert_eq!(press_escape(&pane()), Err("tmux pane is gone".into()));
}
