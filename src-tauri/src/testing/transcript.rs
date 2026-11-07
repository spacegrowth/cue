//! Claude Code's transcript as it writes it (`~/.claude/projects/…/<session>.jsonl`), one JSON line per
//! entry, stamped by the World's clock. Only the fields Cue reads are written.
//! SHORTCUT: Codex's rollout (`event_msg` lines) has no builder yet; add one with the first Codex case
//! that reads it (its queued-message state, transcript::codex_queued).

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Transcript {
    path: PathBuf,
}

impl Transcript {
    pub fn at(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        Self { path }
    }

    /// One entry, appended, stamped now; the file's modified time is now too, by the World's clock (Cue
    /// reads "written lately" off it).
    pub fn entry(&self, e: Value) -> &Self {
        self.entry_at(e, crate::clock::now_ms())
    }

    /// One entry, appended, stamped `at_ms` (and the file's modified time with it).
    fn entry_at(&self, mut e: Value, at_ms: u64) -> &Self {
        e["timestamp"] = json!(iso(at_ms));
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path).unwrap();
        writeln!(f, "{e}").unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(at_ms)).unwrap();
        self
    }

    /// A turn that ended `ago_ms` ago, its reply's prompt cache `context` tokens held for `ttl_s` (3600: an
    /// hour's cache, else five minutes'), as Claude Code logs its usage: what a cache's countdown is read from.
    pub fn cached_reply(&self, ago_ms: u64, context: u64, ttl_s: u64) -> &Self {
        let at = crate::clock::now_ms() - ago_ms;
        let (h1, m5) = if ttl_s >= 3600 { (1000, 0) } else { (0, 1000) };
        let usage = json!({"input_tokens": 10, "cache_read_input_tokens": context - 1010, "cache_creation_input_tokens": 1000,
            "cache_creation": {"ephemeral_1h_input_tokens": h1, "ephemeral_5m_input_tokens": m5}, "output_tokens": 50});
        self.entry_at(json!({"type": "user", "message": {"content": "go on"}}), at - 5000);
        self.entry_at(json!({"type": "assistant", "message": {"id": "m1", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "Done."}], "usage": usage}}), at)
    }

    /// Your prompt, as Claude Code logs it.
    pub fn prompt(&self, text: &str) -> &Self {
        self.entry(json!({"type": "user", "message": {"content": text}}))
    }

    /// A prompt with Claude's id for it and who sent it (`origin.kind`: "human", "peer", "task-notification" …).
    pub fn prompt_from(&self, text: &str, prompt_id: &str, origin: Value) -> &Self {
        self.entry(json!({"type": "user", "promptId": prompt_id, "origin": origin, "message": {"content": text}}))
    }

    /// The agent's text.
    pub fn assistant(&self, text: &str) -> &Self {
        self.entry(json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}))
    }

    pub fn tool_use(&self, id: &str, name: &str, input: Value) -> &Self {
        self.entry(json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}}))
    }

    /// The tool call's result: it was answered (here or in its terminal).
    pub fn tool_result(&self, id: &str) -> &Self {
        self.entry(json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": "ok"}]}}))
    }

    /// A background agent `id` started (the Agent tool, run in the background), as Claude Code logs its launch.
    pub fn background_agent(&self, id: &str, description: &str) -> &Self {
        let call = format!("call-{id}");
        self.tool_use(&call, "Agent", json!({"description": description, "prompt": description, "run_in_background": true}));
        self.entry(json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": call, "content": "Async agent launched"}]},
            "toolUseResult": {"isAsync": true, "status": "async_launched", "agentId": id, "description": description}}))
    }

    /// Claude Code telling the session background task `id` is done (the prompt it wakes the session with).
    pub fn task_done(&self, id: &str) -> String {
        let text = format!("<task-notification>\n<task-id>{id}</task-id>\n<status>completed</status>\n</task-notification>");
        self.prompt(&text);
        text
    }

    /// Claude Code's mark that a compaction is done.
    pub fn compact_boundary(&self) -> &Self {
        self.entry(json!({"type": "system", "subtype": "compact_boundary"}))
    }

    /// Claude Code's mark that a turn is really over (after its Stop hooks).
    pub fn turn_end(&self) -> &Self {
        self.entry(json!({"type": "system", "subtype": "turn_duration"}))
    }

    /// Your prompt `text` with the status list Cue's prompt hook asked for (`ask`), as the hook's added context.
    pub fn prompt_with_status_ask(&self, uuid: &str, text: &str, ask: &str) -> &Self {
        self.entry(json!({"type": "user", "uuid": uuid, "message": {"content": [{"type": "text", "text": text}]}}));
        self.entry(json!({"type": "attachment", "parentUuid": uuid, "attachment": {"type": "hook_additional_context", "content": [ask]}}))
    }

    /// Your latest request, as Claude Code rewrites it each turn.
    pub fn last_prompt(&self, text: &str) -> &Self {
        self.entry(json!({"type": "last-prompt", "lastPrompt": text}))
    }
}

/// A transcript timestamp for `ms` ("2026-10-12T10:00:00.123Z").
pub fn iso(ms: u64) -> String {
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::gmtime_r(&secs, &mut tm) };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, ms % 1000)
}
