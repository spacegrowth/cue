//! Typing into a session, its edges (stability plan §4, Typing): what a message can look like (unicode, very
//! long, blank, a command, " /" for a message that starts with a slash), images with it, a message kept while
//! the session works and what becomes of it, ⌘↵ and Stop around it, and reviews queued behind it.

use super::typing_support::{card_id, detail, image, send_with, waiting};
use crate::testing::{Key, Takes, World};
use serde_json::json;

#[test]
fn a_unicode_message_is_typed_exactly_and_shows_as_sent() {
    let w = World::new();
    waiting(&w, "a");
    let text = "naïve café — 日本語でお願い ✅ 🚀 ½";
    assert_eq!(w.send_to("a", text)["ok"], true);
    assert_eq!(w.terminal.typed("a"), [text]);
    assert_eq!(w.snapshot().last_yours("a")["text"], text);
}

#[test]
fn a_very_long_message_goes_as_one_paste_and_shows_whole() {
    let w = World::new();
    waiting(&w, "a");
    let text = "line of a long brief, ".repeat(2_000);
    let r = w.send_to("a", &text);
    assert_eq!(detail(&r), "sent via fake pane a");
    assert_eq!(w.terminal.typed("a"), [text.trim()], "one paste, never split into two messages");
    assert_eq!(w.snapshot().last_yours("a")["text"], text.trim());
}

#[test]
fn a_message_of_only_spaces_tabs_and_newlines_is_refused_and_nothing_is_typed() {
    let w = World::new();
    waiting(&w, "a");
    assert_eq!(detail(&w.send_to("a", " \n\t \n ")), "nothing to send");
    assert!(w.terminal.keys("a").is_empty());
}

#[test]
fn an_empty_reply_is_refused_and_its_card_stays() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    let r = w.reply(&id, "  \n ");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "nothing to send".to_string()));
    assert!(w.terminal.keys("a").is_empty(), "a bare Enter could pick whatever its screen has highlighted");
    assert_eq!(w.snapshot().card("a")["id"], id.as_str());
}

#[test]
fn a_message_around_its_words_is_sent_trimmed() {
    let w = World::new();
    waiting(&w, "a");
    w.send_to("a", "\n\n  run the tests  \n");
    assert_eq!(w.terminal.typed("a"), ["run the tests"]);
}

#[test]
fn a_slash_command_is_typed_as_a_command_and_compact_shows_compacting() {
    let w = World::new();
    waiting(&w, "a");
    let r = w.send_to("a", "/compact");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(w.terminal.typed("a"), ["/compact"]);
    assert!(w.snapshot().session("a")["compacting_ms"].as_u64().is_some_and(|t| t > 0), "{}", w.snapshot().session("a"));
}

#[test]
fn a_message_that_starts_with_space_slash_keeps_one_space_so_it_is_not_run_as_a_command() {
    let w = World::new();
    waiting(&w, "a");
    w.send_to("a", "   /reomte is a typo of /remote");
    assert_eq!(w.terminal.typed("a"), [" /reomte is a typo of /remote"]);
}

#[test]
fn an_unknown_command_that_leaves_no_trace_is_watched_not_called_taken() {
    let w = World::new();
    waiting(&w, "a");
    w.terminal.pane("a", |p| p.takes = Takes::Nothing);
    let r = w.send_to("a", "/reomte");
    assert_eq!(detail(&r), "via fake pane a; Cue is watching for it to take it");
    assert_eq!(w.terminal.enters("a"), 0, "no Enter again for a command: it might answer what the command opened");
}

#[test]
fn a_command_that_opens_a_dialog_is_taken_and_its_dialog_becomes_a_card() {
    let w = World::new();
    waiting(&w, "a");
    let dialog = crate::terminal::dialog::Dialog { title: "Select model".into(), options: vec!["Opus".into(), "Sonnet".into()], ..Default::default() };
    w.terminal.pane("a", |p| {
        p.takes = Takes::Nothing;
        p.dialog = Some(dialog);
    });
    let r = w.send_to("a", "/model");
    assert_eq!(r["ok"], true, "{r}");
    let s = w.snapshot();
    assert_ne!(s.last_yours("a")["pending"], true, "the dialog says it ran");
    assert!(s.cards_of("a").iter().any(|c| c["tool_input"]["dialog"]["title"] == "Select model"), "{:?}", s.cards_of("a"));
    assert_eq!(w.terminal.enters("a"), 0, "never a bare Enter into a dialog");
}

#[test]
fn a_message_with_an_image_types_its_saved_path_after_the_words() {
    let w = World::new();
    waiting(&w, "a");
    let r = send_with(&w, "a", "what's wrong here?", vec![image("shot.png")]);
    assert_eq!(r["ok"], true, "{r}");
    let typed = w.terminal.typed("a");
    let uploads = w.home().join("uploads").to_string_lossy().into_owned();
    assert!(typed[0].starts_with(&format!("what's wrong here? {uploads}/")) && typed[0].ends_with(".png"), "{typed:?}");
    let saved = typed[0].rsplit(' ').next().unwrap();
    assert!(std::path::Path::new(saved).is_file(), "the image is on disk where the agent is told it is");
    assert_eq!(w.snapshot().last_yours("a")["images"][0], saved);
}

#[test]
fn an_image_alone_is_sent_as_its_path() {
    let w = World::new();
    waiting(&w, "a");
    let r = send_with(&w, "a", "", vec![image("a.png"), image("b.png")]);
    assert_eq!(r["ok"], true, "{r}");
    let typed = w.terminal.typed("a");
    let paths: Vec<&str> = typed[0].split(' ').collect();
    assert_eq!(paths.len(), 2, "{typed:?}");
    assert!(paths.iter().all(|p| p.ends_with(".png") && std::path::Path::new(p).is_file()), "{typed:?}");
}

#[test]
fn a_file_that_isnt_an_image_is_refused_and_nothing_is_typed() {
    let w = World::new();
    waiting(&w, "a");
    let r = send_with(&w, "a", "see attached", vec![json!({"name": "notes.pdf", "mime": "application/pdf", "data": "JVBERi0="})]);
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "notes.pdf isn't an image".to_string()));
    assert!(w.terminal.keys("a").is_empty());
}

#[test]
fn an_image_that_wont_decode_is_refused_naming_it() {
    let w = World::new();
    waiting(&w, "a");
    let r = send_with(&w, "a", "look", vec![json!({"name": "broken.png", "mime": "image/png", "data": "data:image/png;base64,@@@"})]);
    assert_eq!(r["ok"], false);
    assert!(detail(&r).starts_with("broken.png:"), "{r}");
    assert!(w.terminal.keys("a").is_empty());
}

#[test]
fn a_kept_message_keeps_its_image_and_types_its_path_when_the_turn_ends() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("long job");
    send_with(&w, "a", "and this one", vec![image("x.png")]);
    let held = w.snapshot().session("a")["held"].clone();
    assert_eq!(held["images"].as_array().map(Vec::len), Some(1), "{held}");
    a.stop("done");
    let typed = w.terminal.typed("a");
    assert!(typed[0].starts_with("and this one ") && typed[0].ends_with(&*held["images"][0].as_str().unwrap()), "{typed:?}");
}

#[test]
fn a_reply_with_an_image_says_so_in_history() {
    let w = World::new();
    waiting(&w, "a");
    let id = card_id(&w, "a");
    let r = w.ask(json!({"type": "reply", "id": id, "text": "this one", "images": [image("x.png")]}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(w.snapshot().history(&id)["outcome"], "replied: “this one” + 1 image");
}

#[test]
fn messages_sent_one_after_another_while_it_works_go_together_once_in_the_order_written() {
    let w = World::new();
    waiting(&w, "a");
    let words = ["one", "two", "three", "four", "five"];
    for t in words {
        w.send_to("a", t);
    }
    assert_eq!(w.terminal.typed("a"), ["one"], "the first started a turn; the rest were kept");
    w.session("a").stop("did one");
    assert_eq!(w.terminal.typed("a"), ["one", "two\n\nthree\n\nfour\n\nfive"]);
}

#[test]
fn the_same_words_sent_twice_are_two_messages() {
    let w = World::new();
    let a = w.session("a");
    waiting(&w, "a");
    w.send_to("a", "yes");
    a.stop("ok");
    w.send_to("a", "yes");
    let yours: Vec<String> = w.snapshot().thread("a").into_iter().filter(|(r, _)| r == "you").map(|(_, t)| t).collect();
    assert_eq!(yours.iter().filter(|t| *t == "yes").count(), 2, "{yours:?}");
}

#[test]
fn a_message_to_a_session_cue_never_heard_of_is_refused() {
    let w = World::new();
    let r = w.send_to("nobody", "hello");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "that session is gone".to_string()));
}

#[test]
fn a_message_kept_for_a_session_that_ends_is_not_typed_anywhere() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("long job");
    w.send_to("a", "after that");
    a.ended();
    w.advance(5_000);
    assert!(w.terminal.typed("a").is_empty());
}

#[test]
fn send_now_to_a_session_at_its_prompt_types_without_esc_or_a_blank_line() {
    let w = World::new();
    waiting(&w, "a");
    w.send_now("a", "go");
    assert_eq!(w.terminal.keys("a"), [Key::Typed("go".into())]);
}

#[test]
fn send_now_to_a_working_codex_session_stops_it_and_types_with_no_blank_line() {
    let w = World::new();
    w.session("c").codex().prompt("long job");
    w.send_now("c", "do this instead");
    assert_eq!(w.terminal.keys("c"), [Key::Escape, Key::Typed("do this instead".into())], "the blank line is for Claude Code's handed-back prompt");
}

#[test]
fn send_now_whose_esc_is_refused_fails_and_types_nothing() {
    let w = World::new();
    w.session("a").prompt("long job");
    w.terminal.pane("a", |p| p.no_escape = Some("tmux pane is gone".into()));
    let r = w.send_now("a", "stop and do this");
    assert_eq!((r["ok"].as_bool(), detail(&r)), (Some(false), "tmux pane is gone".to_string()));
    assert!(w.terminal.typed("a").is_empty());
}

#[test]
fn send_now_leaves_a_message_kept_earlier_to_go_when_the_new_turn_ends() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("long job");
    w.send_to("a", "kept for later");
    w.send_now("a", "urgent");
    assert_eq!(w.terminal.typed("a"), ["\n\nurgent"]);
    assert_eq!(w.snapshot().session("a")["held"]["text"], "kept for later");
    a.stop("did the urgent thing");
    assert_eq!(w.terminal.typed("a"), ["\n\nurgent", "kept for later"]);
}

#[test]
fn a_kept_message_sent_now_from_its_bubble_stops_the_turn_and_goes() {
    let w = World::new();
    w.session("a").prompt("long job");
    w.send_to("a", "this first");
    let r = w.hub().send_held("a", true);
    w.settle();
    assert!(r.as_deref().is_ok_and(|d| d.starts_with("sent via")), "{r:?}");
    assert_eq!(w.terminal.keys("a"), [Key::Escape, Key::Typed("\n\nthis first".into())]);
    assert!(w.snapshot().session("a")["held"].is_null());
}

#[test]
fn a_kept_message_taken_back_is_not_typed_when_the_turn_ends() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("long job");
    w.send_to("a", "never mind");
    assert_eq!(w.hub().unhold("a").map(|e| e.text).as_deref(), Some("never mind"));
    a.stop("done");
    w.advance(2_000);
    assert!(w.terminal.typed("a").is_empty());
}

#[test]
fn a_kept_message_that_failed_goes_once_sent_again_and_loses_its_failure_mark() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("go");
    w.send_to("a", "next step");
    w.terminal.pane("a", |p| p.takes = Takes::Refuses("tmux pane is gone".into()));
    a.stop("done");
    assert_eq!(w.snapshot().session("a")["held"]["unsent"], true);
    w.terminal.pane("a", |p| p.takes = Takes::Prompt);
    let r = w.hub().send_held("a", false);
    w.settle();
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(w.terminal.typed("a"), ["next step"]);
    let s = w.snapshot();
    assert!(s.session("a")["held"].is_null());
    assert_ne!(s.last_yours("a")["unsent"], true);
}

#[test]
fn the_message_sent_right_after_stop_goes_apart_and_one_much_later_does_not() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("job");
    w.interrupt("a");
    w.send_to("a", "right after");
    a.stop("ok");
    w.advance(31_000);
    w.send_to("a", "much later");
    assert_eq!(w.terminal.typed("a"), ["\n\nright after", "much later"]);
}

#[test]
fn a_review_queued_behind_a_message_kept_for_its_lead_waits_until_that_message_goes() {
    use crate::leads::{install_index, reported_exec, Member};
    let w = World::new();
    let lead = Member { role: "lead".into(), plugin: "relay".into(), name: "lead".into(), ..Default::default() };
    let exec = Member { name: "E1-topic".into(), id: "relay-E1".into(), ready_late: true, ..reported_exec() };
    install_index([("L".to_string(), lead), ("E1".to_string(), exec)].into());
    let l = w.session("L");
    l.prompt("lead");
    w.send_to("L", "check the budget first");
    w.hub().review("L", "E1", "/relay:review E1").unwrap();
    // Its turn ends with its pane gone a moment: your message stays kept.
    w.terminal.pane("L", |p| p.takes = Takes::Refuses("tmux pane is gone".into()));
    l.stop("ok");
    w.terminal.pane("L", |p| p.takes = Takes::Prompt);
    w.advance(2_000);
    assert!(w.terminal.typed("L").is_empty(), "the review never jumps ahead of what you kept for it");
    w.hub().send_held("L", false).unwrap();
    l.stop("budget checked");
    w.advance(2_000);
    assert_eq!(w.terminal.typed("L"), ["check the budget first", "/relay:review E1"]);
}

/// The blank line after Stop goes on a message sent within 30 s of it, by the clock that only moves
/// forward: the Mac's clock set an hour on, or back, in between changes nothing.
#[test]
fn the_moment_after_stop_is_measured_by_the_forward_only_clock() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("long job");
    w.interrupt("a");
    w.clock().set_wall(3_600_000);
    w.send_to("a", "right after");
    a.stop("on it");
    w.advance(31_000);
    w.clock().set_wall(-7_200_000);
    w.send_to("a", "much later");
    assert_eq!(w.terminal.typed("a"), ["\n\nright after", "much later"]);
}
