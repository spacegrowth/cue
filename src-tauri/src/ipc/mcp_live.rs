//! `/mcp` in a hosted Claude Code session: its MCP servers as Claude Code itself has them right now, and what its own
//! `/mcp` screen does to them (sign in, reconnect, turn on or off, sign out), asked of the running session through
//! Claude Code's control requests (hosted/driver.rs `control`), with no terminal. ui/chat/mcp.js shows them.
//!
//! Signing in: Claude Code gives a link to open in your browser. A claude.ai connector's link is claude.ai's own page;
//! another server's ends on a page of Claude Code's on `localhost`, which a session on this Mac hears by itself. On
//! another machine that page is the machine's, so the browser can't reach it: you paste the address it ended on
//! (`callbackExpected`), and Cue hands it over (`mcp_signed_in`).

use super::with_hub;
use crate::hub::Hub;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::State;

/// A hosted Claude Code session's servers, as `rows` gives them.
#[tauri::command]
pub(crate) async fn mcp_live(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<Value, String> {
    with_hub(&hub, move |h| mcp_live_of(h, &session_id)).await?
}

/// What `mcp_live` answers with.
/// `pinned`: it runs only the servers picked for it (⋯ → MCP servers in a terminal session, before it was hosted:
/// `--strict-mcp-config`), so the folder's others, and Claude Code's own (claude.ai connectors), aren't in it at all;
/// `mcp_apply_clear` gives it the folder's again.
pub(crate) fn mcp_live_of(h: &Arc<Hub>, session_id: &str) -> Result<Value, String> {
    let mut v = ask(h, session_id, json!({ "subtype": "mcp_status" })).map(|v| rows(&v))?;
    v["pinned"] = json!(crate::logs::mcp::read_selection(&crate::logs::mcp::selection_path(session_id)).is_some());
    Ok(v)
}

/// Server `server` of a hosted Claude Code session: `action` one of "sign-in" (its link to open, and whether to paste
/// back the address the browser ends on), "reconnect", "on", "off", "sign-out".
#[tauri::command]
pub(crate) async fn mcp_act(hub: State<'_, Arc<Hub>>, session_id: String, server: String, action: String) -> Result<Value, String> {
    with_hub(&hub, move |h| mcp_act_on(h, &session_id, &server, &action, true)).await?
}

/// What `mcp_act` does. `browser_here`: the sign-in page opens in a browser on this Mac (Cue's window); not (another
/// device asked), Claude Code's own page on `localhost` is out of its reach too, so its address is pasted back.
pub(crate) fn mcp_act_on(h: &Arc<Hub>, session_id: &str, server: &str, action: &str, browser_here: bool) -> Result<Value, String> {
    let said = ask(h, session_id, request(server, action)?)?;
    let elsewhere = !browser_here || h.hosted_origin(session_id).is_some_and(|o| !o.machine.is_empty());
    Ok(if action == "sign-in" { sign_in(&said, elsewhere) } else { json!({}) })
}

/// The address the browser ended on after signing in to `server` (a session on another machine: see the top).
#[tauri::command]
pub(crate) async fn mcp_signed_in(hub: State<'_, Arc<Hub>>, session_id: String, server: String, url: String) -> Result<(), String> {
    with_hub(&hub, move |h| mcp_signed_in_now(h, &session_id, &server, &url)).await?
}

/// What `mcp_signed_in` does.
pub(crate) fn mcp_signed_in_now(h: &Arc<Hub>, session_id: &str, server: &str, url: &str) -> Result<(), String> {
    ask(h, session_id, json!({ "subtype": "mcp_oauth_callback_url", "serverName": server, "callbackUrl": url.trim() })).map(|_| ())
}

/// `body` asked of hosted Claude Code session `session_id`: its answer.
fn ask(h: &Arc<Hub>, session_id: &str, body: Value) -> Result<Value, String> {
    let o = h.hosted_origin(session_id).ok_or("only a hosted session's servers can be changed from here")?;
    if o.harness != "claude" {
        return Err("only Claude Code has MCP servers here".into());
    }
    if !h.hosted().running(session_id) {
        return Err("it's paused: send it a message to start it again".into());
    }
    h.hosted().control(session_id, body)
}

/// The control request for `action` on `server`.
fn request(server: &str, action: &str) -> Result<Value, String> {
    let (subtype, more) = match action {
        "sign-in" => ("mcp_authenticate", json!({})),
        "reconnect" => ("mcp_reconnect", json!({})),
        "on" => ("mcp_toggle", json!({ "enabled": true })),
        "off" => ("mcp_toggle", json!({ "enabled": false })),
        "sign-out" => ("mcp_clear_auth", json!({})),
        _ => return Err(format!("no such thing to do to an MCP server: {action}")),
    };
    let mut body = json!({ "subtype": subtype, "serverName": server });
    if let (Some(b), Some(m)) = (body.as_object_mut(), more.as_object()) {
        b.extend(m.clone());
    }
    Ok(body)
}

/// What a sign-in answer means for the window: the link, and whether to ask for the address the browser ends on
/// (only when the browser can't reach Claude Code's own page: `elsewhere`, and only when Claude Code waits on one).
fn sign_in(said: &Value, elsewhere: bool) -> Value {
    let waits = said["callbackExpected"].as_bool().unwrap_or(false);
    json!({ "url": said["authUrl"].as_str().unwrap_or_default(), "paste_back": waits && elsewhere })
}

/// Claude Code's `mcp_status` answer as the window shows it: each server's name, state ("connected", "needs-auth",
/// "pending", "failed", "disabled"), where it comes from, its tools' names, and why it failed.
pub(crate) fn rows(status: &Value) -> Value {
    let rows: Vec<Value> = status["mcpServers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let kind = s.pointer("/config/type").and_then(Value::as_str).unwrap_or_default();
            json!({
                "name": s["name"],
                "title": s.pointer("/serverInfo/title").cloned().unwrap_or(Value::Null),
                "status": s["status"],
                "scope": s["scope"],
                "error": s.get("error").cloned().unwrap_or(Value::Null),
                "tools": s["tools"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str()).collect::<Vec<_>>(),
                // Only a server reached over the web signs in with its own account (Claude Code's own sign-out works
                // on no other kind); a claude.ai connector is signed out of on claude.ai.
                "can_sign_out": matches!(kind, "http" | "sse") && s["status"] == "connected",
            })
        })
        .collect();
    json!({ "servers": rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_status_reads_as_rows_with_tool_names_and_only_web_servers_signed_out_of() {
        let status = json!({ "mcpServers": [
            { "name": "linear", "status": "connected", "config": { "type": "http" }, "scope": "user", "serverInfo": { "title": "Linear" }, "tools": [{ "name": "list_issues" }, { "name": "get_issue" }] },
            { "name": "claude.ai Gmail", "status": "needs-auth", "config": { "type": "claudeai-proxy" }, "scope": "claudeai" },
            { "name": "local", "status": "failed", "config": { "type": "stdio" }, "scope": "project", "error": "spawn ENOENT" },
        ]});
        let r = rows(&status);
        let s = r["servers"].as_array().unwrap();
        assert_eq!(s[0]["tools"], json!(["list_issues", "get_issue"]));
        assert_eq!((s[0]["title"].as_str(), s[0]["can_sign_out"].as_bool()), (Some("Linear"), Some(true)));
        assert_eq!((s[1]["status"].as_str(), s[1]["can_sign_out"].as_bool()), (Some("needs-auth"), Some(false)));
        assert_eq!(s[2]["error"], "spawn ENOENT");
        assert_eq!(rows(&json!({}))["servers"], json!([]));
    }

    #[test]
    fn each_action_is_claude_codes_own_request_and_nothing_else_is_one() {
        assert_eq!(request("x", "off").unwrap(), json!({ "subtype": "mcp_toggle", "serverName": "x", "enabled": false }));
        assert_eq!(request("x", "sign-in").unwrap(), json!({ "subtype": "mcp_authenticate", "serverName": "x" }));
        assert_eq!(request("x", "sign-out").unwrap()["subtype"], "mcp_clear_auth");
        assert!(request("x", "delete").is_err());
    }

    #[test]
    fn the_address_is_asked_for_only_on_another_machine_when_claude_code_waits_on_its_own_page() {
        let waits = json!({ "authUrl": "https://auth.example/x", "callbackExpected": true });
        assert_eq!(sign_in(&waits, true), json!({ "url": "https://auth.example/x", "paste_back": true }));
        assert_eq!(sign_in(&waits, false)["paste_back"], false, "on this Mac Claude Code hears the browser itself");
        assert_eq!(sign_in(&json!({ "authUrl": "https://claude.ai/x", "callbackExpected": false }), true)["paste_back"], false, "a claude.ai connector");
    }
}
