//! Real Claude Code's MCP control requests, as `/mcp` in a hosted session uses them (ipc/mcp_live.rs): status, turning
//! a server off and on, reconnecting, and a sign-in link. Opt-in like compat_tests.rs, whose `Real` runs them:
//!
//!   CUE_CLAUDE_COMPAT=1 cargo test --lib hosted::mcp_compat -- --ignored
//!
//! The server it changes is one given to this session alone (`--mcp-config`, with `--strict-mcp-config`), so nothing
//! of yours is touched: one that can't start, which is all turning off and on needs.

use super::compat_tests::{quiet, Real};
use super::Start;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(20);

/// Server `name`'s row in its status, asked until `ok` accepts it (it may still be connecting).
fn status_until(r: &Real, name: &str, ok: impl Fn(&str) -> bool) -> Value {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let st = r.s().control(json!({ "subtype": "mcp_status" }), WAIT).expect("mcp_status");
        let row = st["mcpServers"].as_array().into_iter().flatten().find(|s| s["name"] == name).cloned().unwrap_or(Value::Null);
        if ok(row["status"].as_str().unwrap_or_default()) || Instant::now() > end {
            return row;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
#[ignore = "real Claude: CUE_CLAUDE_COMPAT=1"]
fn compat_mcp_status_off_on_and_reconnect_through_claude_codes_own_requests() {
    let config = json!({ "mcpServers": { "cue-scratch": { "command": "/usr/bin/false" } } }).to_string();
    let Some(r) = Real::start("mcp", quiet(), Start::fresh(), &["--mcp-config", &config]) else { return };
    let row = status_until(&r, "cue-scratch", |s| s == "failed");
    assert_eq!(row["status"], "failed", "a server that can't start: {row}");
    let again = r.s().control(json!({ "subtype": "mcp_reconnect", "serverName": "cue-scratch" }), WAIT);
    assert!(again.is_err(), "reconnecting one that still can't start says so: {again:?}");
    r.s().control(json!({ "subtype": "mcp_toggle", "serverName": "cue-scratch", "enabled": false }), WAIT).expect("off");
    assert_eq!(status_until(&r, "cue-scratch", |s| s == "disabled")["status"], "disabled");
    r.s().control(json!({ "subtype": "mcp_toggle", "serverName": "cue-scratch", "enabled": true }), WAIT).ok();
    assert_ne!(status_until(&r, "cue-scratch", |s| s != "disabled")["status"], "disabled", "on again");
    let unknown = r.s().control(json!({ "subtype": "mcp_reconnect", "serverName": "no-such-server" }), WAIT);
    assert!(unknown.unwrap_err().contains("no-such-server"), "a refusal names the server");
    // Nothing of yours changed: the server lived in this session alone.
    let yours = std::fs::read_to_string(crate::testing::sandbox::real_home().join(".claude.json")).unwrap_or_default();
    assert!(!yours.contains("cue-scratch"), "~/.claude.json never heard of it");
}
