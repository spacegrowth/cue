//! The thread: what's kept, and how echoes and pastes fold into one message.

use super::*;

#[test]
fn the_same_short_message_sent_twice_in_a_row_is_two_bubbles_and_its_echo_folds_into_the_second() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.note_sent("a", "yes", &[], now_ms(), 50);
    s.note_sent("a", "yes", &[], now_ms() + 1, 50);
    let yours = |s: &Sessions| get(s, "a").thread.iter().filter(|e| e.role == "you").count();
    assert_eq!(yours(&s), 2, "the agent got two");
    s.note("a", "you", "yes", 50);   // the agent's report of the second
    assert_eq!(yours(&s), 2, "its echo is that one, not a third");
}

#[test]
fn a_paste_taken_minutes_late_shows_once() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    let long = "please look at the hook and the parser, both of them read the same transcript differently today";
    s.note_sent("a", long, &[], 0, 50);
    s.set_pending("a", long, true);
    // Minutes pass (its Enter was lost); then it's taken, and the agent's report of it comes in.
    if let Some(e) = s.0.get_mut("a").and_then(|x| x.thread.last_mut()) {
        e.at_ms = now_ms().saturating_sub(10 * 60_000);
    }
    assert!(s.got_message("a", long, "its prompt hook"));
    s.note("a", "you", long, 50);
    assert_eq!(get(&s, "a").thread.iter().filter(|e| e.role == "you").count(), 1);
}

#[test]
fn a_long_message_is_kept_whole_and_shows_once() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "f".into(), ..Default::default() }, "waiting", None);
    let long = long_text();
    let t0 = now_ms();
    s.note("f", "you", &long, 50); // the agent's report of what it got, during the send
    s.note_sent("f", &long, &["/u/1.png".into()], t0, 50); // Cue's own copy, once the send returns
    let t = s.thread("f");
    assert_eq!(t.len(), 1, "one bubble for one message");
    assert_eq!(t[0].text, long.trim(), "kept whole, not cut at 6000");
    assert_eq!(t[0].images, ["/u/1.png"], "Cue's copy brings its images");
}

#[test]
fn what_the_agent_got_is_the_message_however_its_text_came_out() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "g".into(), ..Default::default() }, "waiting", None);
    let t0 = now_ms();
    // Typed into a box that still held something: the agent got both, glued.
    s.note("g", "you", "leftover from before and then what you sent", 50);
    s.note_sent("g", "what you sent", &[], t0, 50);
    let t = s.thread("g");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].text, "leftover from before and then what you sent");
}

#[test]
fn a_long_message_reported_after_cues_copy_folds_in_even_with_its_lines_joined() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "h".into(), ..Default::default() }, "waiting", None);
    let long = long_text();
    s.note_sent("h", &long, &[], now_ms(), 50); // nothing reported yet: Cue's copy goes in
    s.note("h", "you", &long.replace('\n', " "), 50); // then the agent's report, lines joined
    assert_eq!(s.thread("h").len(), 1);
    // An older copy cut short ("…") is still the same message.
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "i".into(), ..Default::default() }, "waiting", None);
    let cut = format!("{}…", long.chars().take(6000).collect::<String>());
    s.note("i", "you", &cut, 50);
    s.note("i", "you", &long, 50);
    assert_eq!(s.thread("i").len(), 1);
}

#[test]
fn a_command_sent_from_cue_and_reported_back_is_one_message() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "k".into(), ..Default::default() }, "working", None);
    s.note("k", "you", "/handoff", 50); // Claude Code's prompt hook
    s.note("k", "you", "/handoff", 50); // Cue's own copy, once the send returns
    assert_eq!(s.thread("k").iter().filter(|e| e.role == "you").count(), 1);
    s.note("k", "you", "/compact", 50); // another command: its own message
    assert_eq!(s.thread("k").iter().filter(|e| e.role == "you").count(), 2);
}

#[test]
fn the_same_short_reply_twice_is_two_messages() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "j".into(), ..Default::default() }, "waiting", None);
    s.note("j", "you", "yes", 50);
    s.note("j", "agent", "done", 50);
    s.note_sent("j", "yes", &[], now_ms() + 1, 50); // a second send, after the reply
    assert_eq!(s.thread("j").iter().filter(|e| e.role == "you").count(), 2);
}

#[test]
fn a_paste_split_into_chunks_reads_as_sent() {
    let wrapped = "<pasted_content id=\"bcfe\">\nIs it ready? ✅ **Naming:** c\n</pasted_content id=\"bcfe\">\n\n\n<pasted_content id=\"bcfe\">\nlaude-teams is public.\n\nNext line.\n</pasted_content id=\"bcfe\">\n";
    assert_eq!(unwrap_pasted(wrapped).trim(), "Is it ready? ✅ **Naming:** claude-teams is public.\n\nNext line.");
    assert_eq!(unwrap_pasted("no tags <here>"), "no tags <here>");
    assert_eq!(unwrap_pasted("cut <pasted_content id=\"x\""), "cut <pasted_content id=\"x\"");
}

#[test]
fn a_teammates_messages_go_in_as_from_it_each_its_own_and_the_log_is_read_up_to_the_newest() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "lead".into(), ..Default::default() }, "working", None);
    s.note_teammates("lead", &[(100, "reader".into(), "PROBE-SHORT".into()), (200, "reader".into(), "PROBE-LONG\nline 1".into())], 10);
    s.note("lead", "agent", "Both arrived.", 10);
    let t = s.thread("lead");
    assert_eq!(t.iter().map(|e| format!("{} {}: {}", e.role, e.from, e.text)).collect::<Vec<_>>(), ["peer reader: PROBE-SHORT", "peer reader: PROBE-LONG\nline 1", "agent : Both arrived."]);
    assert!(t.windows(2).all(|w| w[0].at_ms < w[1].at_ms), "each at its own moment, so none is saved over another");
    assert_eq!(s.teammates_read("lead"), 200);
    s.note_teammates("lead", &[], 10);
    assert_eq!(s.teammates_read("lead"), 200, "nothing new: stays");
}

#[test]
fn thread_keeps_the_last_n_and_skips_repeats() {
    let mut s = Sessions::default();
    let o = origin("t");
    s.mark(&o, "working", None);
    for n in 0..7 {
        s.note("t", "agent", &format!("msg {n}"), 5);
    }
    s.note("t", "agent", "msg 6", 5); // repeat of the last: ignored
    s.note("t", "you", "   ", 5); // blank: ignored
    let t = s.thread("t");
    assert_eq!(t.len(), 5);
    assert_eq!(t.first().unwrap().text, "msg 2");
    assert_eq!(t.last().unwrap().text, "msg 6");
    s.note("nobody", "you", "x", 5); // unknown session: no-op, no panic
}

#[test]
fn a_message_glued_to_one_you_stopped_is_one_bubble() {
    let mut s = Sessions::default();
    s.mark(&origin("g"), "working", None);
    s.note("g", "you", "ok commit , push , release", 9);
    s.note("g", "you", "after esc esc we are losing text box focus", 9); // sent from Cue
    // Claude Code had put the stopped message back in its box; it got both, glued.
    s.note("g", "you", "ok commit , push , releaseafter esc esc we are losing text box focus", 9);
    // The stopped one is already in the chat above: yours stays what you typed, marked as gone with it.
    let t: Vec<(String, bool)> = s.thread("g").iter().map(|e| (e.text.clone(), e.joined)).collect();
    assert_eq!(t, [("ok commit , push , release".to_string(), false), ("after esc esc we are losing text box focus".to_string(), true)]);
    // The same with a blank line between them (Cue types one when it stops a turn to send now).
    s.mark(&origin("h"), "working", None);
    s.note("h", "you", "how do I keep the focus in the box", 9);
    s.note("h", "you", "and not when I click elsewhere", 9);
    s.note("h", "you", "how do I keep the focus in the box\n\nand not when I click elsewhere", 9);
    assert_eq!(s.thread("h").iter().map(|e| (e.text.as_str(), e.joined)).collect::<Vec<_>>(), [("how do I keep the focus in the box", false), ("and not when I click elsewhere", true)]);
    // A short reply isn't matched by chance.
    s.note("g", "you", "yes", 9);
    s.note("g", "you", "I said yes", 9);
    assert_eq!(s.thread("g").len(), 4);
}

#[test]
fn a_reply_and_its_echoed_prompt_are_one_exchange() {
    let mut s = Sessions::default();
    s.mark(&origin("e"), "waiting", None);
    s.note_with("e", "you", "look at this", &["/u/1.png".into()], 5);
    s.note("e", "you", "look at this /u/1.png", 5); // the prompt Claude reports back
    s.note("e", "you", "o,look at  this", 5); // stray keys already in its prompt: still the same message
    let t = s.thread("e");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].images, vec!["/u/1.png".to_string()]);
}

#[test]
fn a_long_forward_and_its_wrapped_echo_are_one_exchange() {
    let mut s = Sessions::default();
    s.mark(&origin("f"), "waiting", None);
    s.note("f", "you", "check this\n\n[Forwarded from the backend session]\nIt's well built. claude-teams is public.", 5);
    let echo = "<pasted_content id=\"a\">\ncheck this\n\n[Forwarded from the backend session]\nIt's well built. c\n</pasted_content id=\"a\">\n\n\n<pasted_content id=\"a\">\nlaude-teams is public.\n</pasted_content id=\"a\">";
    s.note("f", "you", &unwrap_pasted(echo), 5);
    assert_eq!(s.thread("f").len(), 1);
}

#[test]
fn an_image_path_with_a_space_still_folds_into_one_message() {
    let mut s = Sessions::default();
    s.mark(&origin("g"), "waiting", None);
    let p = "/Users/v/Library/Application Support/dev.spacegrowth.cue/uploads/1-0.png";
    s.note_with("g", "you", "can you try showing time above the line", &[p.into()], 5);
    // What Claude reports back: the text plus the path, its space unescaped.
    s.note("g", "you", &format!("can you try showing time above the line {p}"), 5);
    // ...or still escaped, as it was typed.
    s.note("g", "you", &format!("can you try showing time above the line {}", crate::uploads::escape_path(p)), 5);
    let t = s.thread("g");
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].images, vec![p.to_string()]);
}

#[test]
fn an_echo_that_lands_first_still_gets_the_images_and_short_replies_stay_apart() {
    let mut s = Sessions::default();
    s.mark(&origin("f"), "waiting", None);
    s.note("f", "you", "o,show me the diff /u/2.png", 5); // the agent's report arrives first
    s.note_with("f", "you", "show me the diff", &["/u/2.png".into()], 5);
    let t = s.thread("f");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].text, "o,show me the diff /u/2.png", "keeps what the agent actually got");
    assert_eq!(t[0].images, vec!["/u/2.png".to_string()]);
    s.note("f", "you", "ok", 5);
    s.note("f", "you", "ok, ship it", 5);
    assert_eq!(s.thread("f").len(), 3, "two real messages, not an echo");
}
