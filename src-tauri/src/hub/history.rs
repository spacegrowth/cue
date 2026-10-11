//! Answered cards (saved to cue.db the moment they are answered) and the text a card shows.

use super::*;

// ---------- history: saved to cue.db the moment it's answered ----------

pub(super) fn push_history(st: &mut Store, it: Item) {
    crate::db::save_answered(&it);
    let day_ago = now_ms().saturating_sub(crate::db::DAY_MS);
    st.answers.retain(|a| a.resolved_ms >= day_ago);
    st.answers.insert(0, Answer::of(&it));
    st.history.push_front(it);
    st.history.truncate(crate::config::history_keep());
}

/// Pick what a finished card shows. "answer" (default): the agent's final message from before any
/// Stop hook sent it back to work (not its mid-turn narration), with the hook's follow-up kept
/// separately. "last": just the very last message.
/// A Claude Code session's name as Claude Code has it now: `/rename` in its terminal updates Claude
/// Code's own list of running sessions (Cue's rename sends `/rename` too), and Cue's record may not know.
pub fn split_turn(last: String, turn: &[TurnPart], mode: &str) -> (String, String) {
    if mode == "last" || turn.is_empty() {
        return (last, String::new());
    }
    let answer = turn.iter().rev().find(|p| !p.hooked && !p.text.trim().is_empty()).map(|p| p.text.trim().to_string());
    let after = turn.iter().filter(|p| p.hooked).map(|p| p.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    match answer {
        Some(a) => (a, after),
        None => (if after.is_empty() { last } else { after }, String::new()),
    }
}

pub(super) fn first_line(s: &str, max: usize) -> String {
    let l = s.lines().next().unwrap_or("").trim();
    if l.chars().count() > max {
        format!("{}…", l.chars().take(max).collect::<String>())
    } else {
        l.to_string()
    }
}

/// One-line summary of what's being asked, for notifications.
pub fn summary(it: &Item) -> String {
    let i = &it.tool_input;
    let s = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match it.kind.as_str() {
        "question" => i.pointer("/questions/0/question").and_then(Value::as_str).map(String::from).unwrap_or_else(|| s("question")),
        "waiting" => it.context.last().map(|c| c.text.clone()).unwrap_or_else(|| "Finished, waiting for you".into()),
        // Asking in its terminal about a tool call Cue couldn't see: just what Claude Code said.
        "terminal" if it.tool_name.is_empty() => it.message.clone(),
        _ => match it.tool_name.to_lowercase().as_str() {
            "bash" => s("command"),
            "edit" | "write" | "multiedit" | "notebookedit" => format!("{} {}", it.tool_name, s("file_path")),
            "webfetch" => format!("Fetch {}", s("url")),
            _ => it.tool_name.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_turn_shows_the_answer_and_keeps_the_hook_follow_up_aside() {
        let part = |t: &str, h: bool| TurnPart { text: t.into(), hooked: h };
        let turn = vec![part("Here's the fix.", false), part("All done, tests pass.", false), part("Checked: all tests pass.", true)];
        let (msg, follow) = split_turn("Checked: all tests pass.".into(), &turn, "answer");
        assert_eq!(msg, "All done, tests pass.", "the final answer, not the mid-turn narration");
        assert_eq!(follow, "Checked: all tests pass.");
        assert_eq!(split_turn("Checked: all tests pass.".into(), &turn, "last"), ("Checked: all tests pass.".into(), String::new()));
        // No hook in the turn: the final message is the answer.
        let plain = vec![part("a", false), part("b", false)];
        assert_eq!(split_turn("b".into(), &plain, "answer"), ("b".into(), String::new()));
        // Nothing before the hook: fall back to what there is.
        assert_eq!(split_turn("x".into(), &[part("x", true)], "answer"), ("x".into(), String::new()));
        // Old clients send no turn at all.
        assert_eq!(split_turn("last".into(), &[], "answer"), ("last".into(), String::new()));
    }

    #[test]
    fn todays_answers_are_all_kept_and_counted_however_short_history_is() {
        let dir = std::env::temp_dir().join(format!("cue-today-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        std::fs::write(dir.join("config.json"), r#"{"history":{"keep":10}}"#).unwrap();
        crate::db::reset();
        let now = now_ms();
        let item = |n: u64, at: u64| Item {
            id: format!("t{n}"), kind: "waiting".into(), origin: Origin::default(), project: "cue".into(),
            tool_name: String::new(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: String::new(),
            created_ms: at - 1000, status: "answered".into(), outcome: "replied".into(), resolved_ms: Some(at), thread: vec![], tool_use_id: None, scan_from: 0, at_prompt: false, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        let mut st = Store::default();
        push_history(&mut st, item(0, now - crate::db::DAY_MS - 60_000)); // yesterday, beyond the cap: goes
        for n in 1..=15 {
            push_history(&mut st, item(n, now - 60_000 + n));
        }
        assert_eq!(st.history.len(), 10, "the History list keeps its setting");
        assert_eq!(st.answers.len(), 15, "but every answer of the last day is counted");
        crate::db::reset();
        assert_eq!(crate::db::history(10).len(), 10);
        let kept = crate::db::answers_since(now - crate::db::DAY_MS);
        assert_eq!(kept.len(), 15, "the database keeps the last day's answers past the cap");
        assert_eq!(kept[0].resolved_ms, now - 60_000 + 15, "newest first");
        std::env::remove_var("CUE_HOME");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn history_keeps_the_configured_number_newest_first_and_trims_the_database() {
        let dir = std::env::temp_dir().join(format!("cue-hist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let item = |n: u64| Item {
            id: format!("h{n}"), kind: "permission".into(), origin: Origin::default(), project: String::new(),
            tool_name: "Bash".into(), tool_input: Value::Null, suggestions: Value::Null, context: vec![], message: String::new(),
            created_ms: n, status: "answered".into(), outcome: "allowed".into(), resolved_ms: Some(n), thread: vec![], tool_use_id: None, scan_from: 0, at_prompt: false, followup: String::new(), interrupted: false, back_ms: None, images: vec![],
        };
        let mut st = Store::default();
        for n in 1..=15 {
            push_history(&mut st, item(n));
        }
        assert_eq!(crate::config::history_keep(), crate::config::HISTORY_DEFAULT as usize);
        crate::config::set("history.keep", json!(10)).unwrap();
        let h = crate::db::history(crate::config::history_keep());
        assert_eq!(h.len(), 10);
        assert_eq!(h.front().unwrap().id, "h15", "newest first");
        assert_eq!(h.back().unwrap().id, "h6");
        crate::db::reset();
        assert_eq!(crate::db::history(100).len(), 10, "older rows deleted from the database too");
        std::fs::write(dir.join("config.json"), r#"{"pi":{"gate":"all"},"history":{"keep":12}}"#).unwrap();
        crate::config::set("history.keep", json!(20)).unwrap();
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        assert_eq!(cfg["pi"]["gate"], "all", "other settings untouched");
        assert_eq!(cfg["history"]["keep"], 20);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
