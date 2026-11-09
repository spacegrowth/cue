//! How a feed takes one transcript entry: a prompt starts a turn, the agent's words and tool calls become
//! items, and a tool's result fills in how its step went (and may start background work).

use super::feed::Feed;
use super::helper::{in_background, resumed_agent};
use super::model::{Item, Turn};
use super::queries::{is_handoff_note, HANDOFF_ASK_START};
use super::step_line::{outcome, result_text, step_of};
use crate::logs::transcript::{content_blocks, cut, stamp};
use serde_json::Value;

impl Feed {
    /// One transcript entry. Returns whether anything shown changed.
    pub(super) fn take(&mut self, e: &Value) -> bool {
        // A helper agent's own steps (older transcripts keep them inline) aren't this session's.
        if !self.helper && e.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let at = stamp(e);
        if self.helper {
            // Its answer ended: its report is written. A new message to it (its session can send one) puts it back to work.
            match e.get("type").and_then(Value::as_str) {
                Some("assistant") if e.pointer("/message/stop_reason").and_then(Value::as_str) == Some("end_turn") => self.ended_ms = at.max(1),
                Some("user") if !content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) => self.ended_ms = 0,
                _ => {}
            }
            // Its last line, for when it goes quiet (sessions/quiet.rs): its own words, a tool call, or anything it
            // was to go on from (a result, a message to it).
            let last = match e.get("type").and_then(Value::as_str) {
                Some("assistant") if content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) => Some(crate::sessions::quiet::Last::Call),
                Some("assistant") if content_blocks(e).iter().any(|b| b.get("type").and_then(Value::as_str) == Some("text")) => Some(crate::sessions::quiet::Last::Said),
                Some("user") => Some(crate::sessions::quiet::Last::Result),
                _ => None,
            };
            if let Some(last) = last {
                self.last_line = (at, last);
            }
        }
        match e.get("type").and_then(Value::as_str) {
            Some("attachment") => self.take_attachment(e, at),
            Some("user") => self.take_user(e, at),
            Some("assistant") if crate::logs::transcript::placeholder_reply(e) => false,
            Some("assistant") => self.take_assistant(e, at),
            _ => false,
        }
    }

    /// A queued command delivered mid-turn: a task notice comes this way too.
    fn take_attachment(&mut self, e: &Value, at: u64) -> bool {
        let a = e.get("attachment").cloned().unwrap_or(Value::Null);
        let prompt = a.get("prompt").and_then(Value::as_str).unwrap_or("");
        if a.get("commandMode").and_then(Value::as_str) == Some("task-notification") || prompt.contains("<task-notification>") {
            return self.helpers_finished(prompt, at);
        }
        // Your message, after a Stop hook sent it back: new work, an answer again, as a task's report starts.
        if crate::logs::transcript::yours_mid_turn(e) && self.turns.last().is_some_and(|t| t.hooked_ms > 0) {
            self.close_open("");
            self.turns.push(Turn::starting(at, String::new()));
            return true;
        }
        false
    }

    /// A user entry: tool results, a Stop hook's follow-up, or a new prompt (yours, or a wake-up).
    fn take_user(&mut self, e: &Value, at: u64) -> bool {
        let blocks = content_blocks(e);
        // A task notice wherever it sits in the text (wrapped in a system notice, beside a tool result).
        let ended = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).fold(false, |any, t| self.helpers_finished(t, at) || any);
        let results: Vec<&Value> = blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
        if !results.is_empty() {
            let extra = e.get("toolUseResult");
            return results.into_iter().fold(ended, |any, b| self.finish(b, extra, at) || any);
        }
        // A Stop hook sent it back to work (a meta entry, so before those are passed over): more of
        // the same turn, from here on the hook's follow-up.
        if crate::logs::transcript::is_stop_hook(e) {
            // Read from mid-file, its turn's start further back: the turn starts here, already hooked.
            if self.turns.is_empty() {
                self.turns.push(Turn::starting(at, String::new()));
            }
            let Some(turn) = self.turns.last_mut().filter(|t| t.hooked_ms == 0) else { return false };
            turn.hooked_ms = at.max(1);
            return true;
        }
        if e.get("isMeta").and_then(Value::as_bool) == Some(true) || e.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
            return ended;
        }
        let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
        self.take_prompt(&text, at, ended)
    }

    /// A user message's text: a stop, a local command, or a new turn. `ended`: a task ended on this entry.
    fn take_prompt(&mut self, text: &str, at: u64, ended: bool) -> bool {
        // A message Cue pasted in comes back wrapped (and cut into chunks): Cue's hand-off ask too.
        let text = crate::sessions::unwrap_pasted(text);
        let text = text.trim_start();
        if text.starts_with("[Request interrupted") {
            return self.close_open("stopped");
        }
        // A local command (/rename, /model) and its output: no turn of work.
        if text.starts_with("<command-") || text.starts_with("<local-command") {
            return ended;
        }
        self.close_open("");
        // A task's notice or a teammate's message: it wakes the session, but it isn't your prompt.
        let prompt = if crate::logs::transcript::harness_text(text) || crate::logs::transcript::peer_text(text) { String::new() } else { cut(text.trim(), 1200) };
        // Your prompt: it went on working after its note, so that note isn't where it stopped.
        if !prompt.is_empty() {
            self.note = None;
            self.note_calls.clear();
            self.prompted = (at, prompt.starts_with(HANDOFF_ASK_START));
        }
        self.turns.push(Turn::starting(at, prompt));
        ended // an empty turn isn't shown until something happens in it; a task that ended is
    }

    /// The agent's reply: its words and its tool calls, as items of the turn.
    fn take_assistant(&mut self, e: &Value, at: u64) -> bool {
        let mut changed = false;
        for b in content_blocks(e) {
            match b.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let t = b.get("text").and_then(Value::as_str).unwrap_or("").trim();
                    if !t.is_empty() {
                        self.push(Item::Say { text: t.to_string(), at_ms: at });
                        changed = true;
                    }
                }
                Some("tool_use") => {
                    self.take_tool_use(&b, at);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    /// A tool call: a step of the turn, counted in its work (and kept when it writes a hand-off note).
    fn take_tool_use(&mut self, b: &Value, at: u64) {
        let file = b.pointer("/input/file_path").and_then(Value::as_str).unwrap_or("");
        // Written, or (in the turn Cue's hand-off ask began) its older note edited into the new one.
        let tool = b.get("name").and_then(Value::as_str).unwrap_or("");
        let asked = self.turns.last().is_some_and(|t| t.prompt.starts_with(HANDOFF_ASK_START));
        if !self.helper && (tool == "Write" || (asked && matches!(tool, "Edit" | "MultiEdit"))) && is_handoff_note(file) {
            self.note_calls.insert(b.get("id").and_then(Value::as_str).unwrap_or("").to_string(), file.to_string());
        }
        self.push(step_of(b, at));
        if let Some(t) = self.turns.last_mut() {
            t.work.count(b);
        }
    }

    /// A tool call's result came in: fill in how it went.
    fn finish(&mut self, block: &Value, extra: Option<&Value>, at: u64) -> bool {
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { return false };
        // A hand-off note written: kept once the write went through.
        if let Some(note) = self.note_calls.remove(id) {
            if block.get("is_error").and_then(Value::as_bool) != Some(true) {
                self.note = Some((note, at));
            }
        }
        for turn in self.turns.iter_mut().rev().take(3) {
            for item in turn.items.iter_mut().rev() {
                if let Item::Step { id: sid, tool, verb, past, result, bad, done, subject, command, desc, .. } = item {
                    if sid == id {
                        let (r, b) = outcome(tool, block, extra);
                        *verb = past.to_string();
                        *result = r;
                        *bad = b;
                        *done = true;
                        let launched = extra.filter(|x| x.get("status").and_then(Value::as_str).is_some_and(in_background)).cloned();
                        let resumed = resumed_agent(extra);
                        let backgrounded = extra.filter(|x| x.get("backgroundTaskId").and_then(Value::as_str).is_some_and(|t| !t.is_empty())).cloned();
                        let (step, subject, command, desc) = (sid.clone(), subject.clone(), command.clone(), desc.clone());
                        if let Some(x) = launched {
                            self.add_helper(&step, &x, at);
                        } else if let Some(agent) = resumed {
                            self.resume_helper(&step, &agent, at);
                        } else if let Some(x) = backgrounded {
                            *result = "in the background".into();
                            *bad = false;
                            self.add_script(&step, &desc, &subject, &command, &result_text(block), &x, at);
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

}
