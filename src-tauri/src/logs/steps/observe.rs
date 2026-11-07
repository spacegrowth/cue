//! The model, context, tokens and cost a feed reads from each raw log entry, the way each agent logs them.

use super::feed::Feed;
use super::kind::Kind;
use super::model::{Meta, Turn};
use super::pi::pi_window;
use super::windows::known_window;
use crate::logs::transcript::stamp;
use serde_json::Value;

impl Feed {
    /// The model, context and cost from one raw log entry (each agent logs them its own way).
    /// Returns whether they changed.
    pub(super) fn observe(&mut self, e: &Value) -> bool {
        let before = self.meta.clone();
        match self.kind {
            Kind::Claude => self.observe_claude(e),
            Kind::Codex => self.observe_codex(e),
            Kind::Pi => self.observe_pi(e),
        }
        self.meta != before
    }

    fn observe_claude(&mut self, e: &Value) {
        if e.get("type").and_then(Value::as_str) == Some("assistant") && (self.helper || e.get("isSidechain").and_then(Value::as_bool) != Some(true)) {
            let msg = e.get("message").cloned().unwrap_or(Value::Null);
            if let Some(u) = msg.get("usage") {
                self.claude_usage(e, &msg, u);
            }
        }
        // Compacted (by you or on its own): its context is what the compact left, now, not at its next
        // reply. Till then the insights went on as if it were still as big (a hand-off offered at once).
        if e.get("subtype").and_then(Value::as_str) == Some("compact_boundary") && e.get("isSidechain").and_then(Value::as_bool) != Some(true) {
            let post = n(e.pointer("/compactMetadata/postTokens"));
            if post > 0 {
                self.meta.context = post;
            }
        }
    }

    /// A Claude Code reply's usage line: its context, tokens and cache, its model and window.
    fn claude_usage(&mut self, e: &Value, msg: &Value, u: &Value) {
        let (fresh, read, wrote) = (n(u.get("input_tokens")), n(u.get("cache_read_input_tokens")), n(u.get("cache_creation_input_tokens")));
        let all = fresh + read + wrote;
        if all > 0 {
            self.meta.context = all;
        }
        self.tokens += all + n(u.get("output_tokens"));
        let reply = msg.get("id").and_then(Value::as_str).unwrap_or("");
        if reply.is_empty() || reply != self.last_reply {
            self.last_reply = reply.to_string();
            self.claude_reply(e, u, (fresh, read, wrote));
        }
        let m = &mut self.meta;
        let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
        if !model.is_empty() && !model.starts_with('<') {
            m.model = model.to_string();
        }
        // The transcript doesn't say the window. Claude Code's status line does (for this
        // session, or another on its model); until then 1M once the model's name says so or
        // it's past 200k, else unknown (0): the chat shows the tokens, not a guessed share.
        m.window = match known_window(&self.path, &m.model) {
            Some(w) => w,
            None if m.model.contains("[1m]") || m.context > 200_000 || m.window == 1_000_000 => 1_000_000,
            None => 0,
        };
    }

    /// A reply counted once (Claude Code writes one line per content block, each with the same usage):
    /// the turn's tokens by kind, and when the reply came and how long its cache is kept.
    fn claude_reply(&mut self, e: &Value, u: &Value, (fresh, read, wrote): (u64, u64, u64)) {
        let all = fresh + read + wrote;
        let m = &mut self.meta;
        if let Some(t) = self.turns.last_mut() {
            t.tokens += all + n(u.get("output_tokens"));
            // Claude Code splits what it wrote by how long it's kept; an older log doesn't.
            let cc = u.get("cache_creation");
            let hour = n(cc.and_then(|c| c.get("ephemeral_1h_input_tokens"))).min(wrote);
            t.usage.add(fresh, read, wrote - hour, hour, n(u.get("output_tokens")), stamp(e));
            if all > 0 {
                turn_context(t, m, all, e);
            }
        }
        m.last_call_ms = stamp(e).max(m.last_call_ms);
        let cc = u.get("cache_creation");
        m.cache_ttl_s = if n(cc.and_then(|c| c.get("ephemeral_1h_input_tokens"))) > 0 {
            3600
        } else if n(cc.and_then(|c| c.get("ephemeral_5m_input_tokens"))) > 0 {
            300
        } else if read == 0 && wrote == 0 {
            0
        } else {
            m.cache_ttl_s // only reads this time: the cache it read keeps its lifetime
        };
    }

    fn observe_codex(&mut self, e: &Value) {
        let p = e.get("payload").cloned().unwrap_or(Value::Null);
        if e.get("type").and_then(Value::as_str) == Some("turn_context") {
            if let Some(model) = p.get("model").and_then(Value::as_str) {
                self.meta.model = model.to_string();
            }
        }
        if p.get("type").and_then(Value::as_str) == Some("token_count") {
            if let Some(info) = p.get("info").filter(|i| i.is_object()) {
                self.codex_tokens(e, info);
            }
        }
    }

    /// Codex's token count: the context of its last call, the model's window, and the call's tokens.
    fn codex_tokens(&mut self, e: &Value, info: &Value) {
        let m = &mut self.meta;
        let last = info.get("last_token_usage");
        let input = n(last.and_then(|l| l.get("input_tokens")));
        if input > 0 {
            m.context = input;
            if let Some(t) = self.turns.last_mut() {
                turn_context(t, m, input, e);
            }
        }
        let w = n(info.get("model_context_window"));
        if w > 0 {
            m.window = w;
        }
        let total = n(info.get("total_token_usage").and_then(|t| t.get("total_tokens")));
        // A new call (the session's running total grew; it repeats when nothing did). Its own
        // usage is `last`: the first total read may be everything before, when the read
        // started mid-file, so it isn't this turn's.
        if total > self.codex_total {
            if let Some(t) = self.turns.last_mut() {
                t.tokens += if self.codex_total > 0 { total - self.codex_total } else { n(last.and_then(|l| l.get("total_tokens"))) };
                let cached = n(last.and_then(|l| l.get("cached_input_tokens")));
                t.usage.add(input.saturating_sub(cached), cached, 0, 0, n(last.and_then(|l| l.get("output_tokens"))), stamp(e));
            }
            self.codex_total = total;
            m.last_call_ms = stamp(e).max(m.last_call_ms);
        }
    }

    fn observe_pi(&mut self, e: &Value) {
        let msg = e.get("message").cloned().unwrap_or(Value::Null);
        let m = &mut self.meta;
        if e.get("type").and_then(Value::as_str) == Some("model_change") {
            if let Some(model) = e.get("modelId").and_then(Value::as_str) {
                m.model = model.to_string();
                m.window = pi_window(e.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
            }
        }
        if msg.get("role").and_then(Value::as_str) == Some("assistant") {
            if let Some(u) = msg.get("usage") {
                self.pi_usage(e, u);
            }
            let m = &mut self.meta;
            if let Some(model) = msg.get("model").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                if m.model != model || m.window == 0 {
                    m.window = pi_window(msg.get("provider").and_then(Value::as_str).unwrap_or(""), model).unwrap_or(m.window);
                }
                m.model = model.to_string();
            }
        }
    }

    /// A Pi reply's usage: its context, the turn's tokens, and its cost.
    fn pi_usage(&mut self, e: &Value, u: &Value) {
        let m = &mut self.meta;
        let (fresh, read, wrote) = (n(u.get("input")), n(u.get("cacheRead")), n(u.get("cacheWrite")));
        let all = fresh + read + wrote;
        if all > 0 {
            m.context = all;
        }
        if let Some(t) = self.turns.last_mut() {
            t.tokens += all + n(u.get("output"));
            t.usage.add(fresh, read, wrote, 0, n(u.get("output")), stamp(e));
            if all > 0 {
                turn_context(t, m, all, e);
            }
        }
        m.last_call_ms = stamp(e).max(m.last_call_ms);
        if let Some(c) = u.pointer("/cost/total").and_then(Value::as_f64) {
            m.cost = Some(m.cost.unwrap_or(0.0) + c);
        }
    }
}

/// A number in a log entry; 0 when it's missing.
fn n(v: Option<&Value>) -> u64 {
    v.and_then(Value::as_u64).unwrap_or(0)
}

/// A turn's context as of this reply, which is also the session's first context when none was seen yet.
fn turn_context(t: &mut Turn, m: &mut Meta, context: u64, e: &Value) {
    t.context = context;
    if m.start_context == 0 {
        m.start_context = context;
        m.start_ms = stamp(e);
    }
}
