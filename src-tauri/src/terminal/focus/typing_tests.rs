//! Typing into a tmux pane (stability plan §4, Typing, U and F): the word tmux is handed, the pause before
//! Enter, telling a shell's prompt from an agent's screen, and type_into against a stand-in tmux
//! (terminal::fake_tmux) that hangs, refuses Enter, shows a shell, or isn't installed; and whether the agent
//! has exited back to its shell, read from the processes in the pane's foreground (a stand-in `ps`).

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
fn the_usual_shells_are_shells_and_agents_are_not() {
    for s in ["zsh", "-zsh", "bash", "fish", "sh", "login", "nu"] {
        assert!(is_shell(s), "{s}");
    }
    for s in ["claude", "node", "codex", "pi", "python3", ""] {
        assert!(!is_shell(s), "{s:?}");
    }
}

/// tmux naming `running` on /dev/ttys009 with `screen`, and `ps` listing `foreground` there.
fn pane_with(running: &str, screen: &str, foreground: &str) -> FakeTmux {
    FakeTmux::answering(&format!("display-message) printf '%s\\n%s' '{running}|/dev/ttys009' '{screen}' ;;")).foreground(foreground)
}

/// Typing into that pane is refused, saying the shell's name, and nothing is pasted or pressed.
#[track_caller]
fn refused(tmux: &FakeTmux, shell: &str) {
    let err = type_into(&pane(), "rm -rf build").unwrap_err();
    assert!(err.contains(&format!("shell prompt ({shell})")), "{err}");
    assert!(tmux.calls_of("set-buffer").is_empty() && tmux.calls_of("send-keys").is_empty(), "{:?}", tmux.calls());
}

#[test]
fn oh_my_zshs_default_prompt_gets_nothing_typed() {
    let tmux = pane_with("zsh", "➜  cue git:(main) ✗ ", "Ss+  -zsh");
    refused(&tmux, "zsh");
    assert_eq!(tmux.calls_of("ps"), ["ps -o stat=,comm= -t ttys009"], "decided by what runs there, not by the prompt");
}

#[test]
fn a_bash_prompt_gets_nothing_typed() {
    refused(&pane_with("bash", "user@box:~/src$ ", "Ss+  bash"), "bash");
}

#[test]
fn a_fish_prompt_gets_nothing_typed() {
    refused(&pane_with("fish", "me@mac ~/cue (main)> ", "Ss+  /opt/homebrew/bin/fish"), "fish");
}

#[test]
fn a_plain_zsh_prompt_gets_nothing_typed() {
    refused(&pane_with("zsh", "mac% ", "Ss+  /bin/zsh"), "zsh");
}

#[test]
fn a_shell_in_front_of_a_cleared_screen_gets_nothing_typed() {
    refused(&pane_with("zsh", "", "Ss+  -zsh"), "zsh");
}

#[test]
fn with_its_foreground_unreadable_a_shells_prompt_on_screen_still_stops_it() {
    let tmux = FakeTmux::answering(r"display-message) printf 'zsh\nme@mac ~ %% ' ;;");
    refused(&tmux, "zsh");
}

#[test]
fn an_agent_whose_binary_is_node_is_typed_into_without_asking_ps() {
    let tmux = pane_with("node", "> ", "Ss+  -zsh");
    assert_eq!(type_into(&pane(), "go on").as_deref(), Ok("tmux pane %9"));
    assert!(tmux.calls_of("ps").is_empty());
    assert_eq!(agent_gone(&pane()), Some(false));
}

#[test]
fn an_agent_started_through_sh_is_typed_into_though_tmux_names_the_shell_and_its_screen_ends_in_a_dollar() {
    let tmux = pane_with("sh", "spent so far: 4$", "Ss+  sh\nS+   claude");
    assert_eq!(type_into(&pane(), "go on").as_deref(), Ok("tmux pane %9"));
    assert_eq!(tmux.calls_of("send-keys").len(), 1);
}

#[test]
fn a_pane_back_at_its_shell_is_where_the_agent_has_gone() {
    let _tmux = pane_with("zsh", "➜  cue git:(main) ✗ ", "Ss+  -zsh");
    assert_eq!(agent_gone(&pane()), Some(true));
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
    let _cap = crate::terminal::test_tmux::cap_tmux(Duration::from_millis(200));
    let t0 = Instant::now();
    assert!(type_into(&pane(), "hello").is_err());
    assert!(t0.elapsed() < Duration::from_secs(2), "cut off at its cap: {:?}", t0.elapsed());
    assert!(tmux.calls_of("set-buffer").is_empty());
}

#[test]
fn tmux_is_cut_off_after_ten_seconds_unless_a_test_says_otherwise() {
    assert_eq!(tmux_cap(), Duration::from_secs(10));
    let _cap = crate::terminal::test_tmux::cap_tmux(Duration::from_millis(5));
    assert_eq!(tmux_cap(), Duration::from_millis(5));
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

#[test]
fn a_real_pane_back_at_a_shell_is_read_from_its_foreground_and_gets_nothing_typed() {
    if !tmux_installed() {
        return;
    }
    let name = format!("cue-shell-front-{}", std::process::id());
    let pane = tmux_out(&["new-session", "-d", "-s", &name, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "/bin/sh"]).unwrap();
    let o = Origin { tmux_pane: pane, ..Default::default() };
    let t0 = Instant::now();
    while !session_screen(&o).is_ok_and(|s| s.trim_end().ends_with('$')) && t0.elapsed() < Duration::from_secs(10) {
        std::thread::yield_now();
    }
    let typed = type_into(&o, "echo SHOULD-NOT-RUN");
    let gone = agent_gone(&o);
    run("tmux", &["kill-session", "-t", &format!("={name}")]);
    let err = typed.unwrap_err();
    assert!(err.contains("not the agent: nothing typed"), "{err}");  // /bin/sh is bash on a Mac
    assert_eq!(gone, Some(true));
}
