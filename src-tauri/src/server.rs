//! Unix-socket listener at <Cue's data folder>/cue.sock. Newline-delimited JSON, one connection per message
//! (an Ask connection stays open until it's answered). See PROTOCOL.md.

use crate::hub::{Hub, Reply};
use crate::model::ClientMsg;
use crate::{focus, transcript};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Where Cue keeps everything: the standard place for a Mac app's data,
/// ~/Library/Application Support/dev.spacegrowth.cue. CUE_HOME overrides it (tests use that).
pub fn cue_dir() -> std::path::PathBuf {
    match std::env::var("CUE_HOME") {
        Ok(dir) => dir.into(),
        Err(_) => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/dev.spacegrowth.cue"),
    }
}

/// Before it moved to Application Support, Cue kept everything in ~/.cue. The first launch of a
/// newer Cue moves it all over (same disk, so it's renames, nothing copied). What stays behind in
/// ~/.cue: bin/ (the installer keeps forwarders there for sessions started before the move) and
/// cue.sock, now a link to the new socket so already-running Pi sessions still reach Cue.
/// Returns the old folder's path when it moved something, so saved image paths can be rewritten.
pub fn move_from_old_folder(old: &std::path::Path, new: &std::path::Path) -> Option<String> {
    if !old.is_dir() || new.join("cue.db").exists() {
        return None;
    }
    std::fs::create_dir_all(new).ok()?;
    let mut moved = false;
    for entry in std::fs::read_dir(old).ok()?.flatten() {
        let name = entry.file_name();
        if name == "bin" || name == "cue.sock" {
            continue;
        }
        let to = new.join(&name);
        if to.is_dir() {
            continue; // never merge folders blindly; leave it for a person to look at
        }
        moved |= std::fs::rename(entry.path(), &to).is_ok();
    }
    let sock = old.join("cue.sock");
    let _ = std::fs::remove_file(&sock);
    let _ = std::os::unix::fs::symlink(new.join("cue.sock"), &sock);
    moved.then(|| old.to_string_lossy().into_owned())
}

pub fn socket_path() -> std::path::PathBuf {
    cue_dir().join("cue.sock")
}

pub async fn serve(hub: Arc<Hub>) -> std::io::Result<()> {
    let dir = cue_dir();
    std::fs::create_dir_all(&dir)?;
    let path = socket_path();
    // A second Cue would steal the socket from the first; refuse instead.
    if UnixStream::connect(&path).await.is_ok() {
        return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another Cue is already running"));
    }
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    loop {
        let (stream, _) = listener.accept().await?;
        let hub = hub.clone();
        tokio::spawn(async move {
            let _ = handle(hub, stream).await;
        });
    }
}

async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, v: Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    w.write_all(s.as_bytes()).await
}

/// The hook fires before Claude writes the prompt to its transcript, and only the transcript says
/// who sent it, so look for up to ~3s. Not found (or Codex, which sends no id): count it as yours.
async fn settle_prompt(hub: Arc<Hub>, sid: String, path: String, prompt_id: String, text: String) {
    use crate::transcript::Sender;
    // Claude Code's own notice of a finished background task is never yours, whatever the transcript
    // says yet. (Handed over mid-turn, it carries the turn's promptId: that prompt is yours.)
    if crate::transcript::harness_text(&text) {
        return;
    }
    if !prompt_id.is_empty() && !path.is_empty() {
        for _ in 0..15 {
            // Handed over from Claude Code's queue: only the attachment says so, so look there first.
            if crate::transcript::queued_from_harness(&path, &text) {
                return;
            }
            match crate::transcript::prompt_sender(&path, &prompt_id) {
                Some(Sender::Harness) => return,
                Some(Sender::Peer { name, pid, body }) => return hub.prompt_from_peer(&sid, &name, pid, &body),
                Some(Sender::Human) => break,
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            }
        }
    }
    hub.prompt_from_you(&sid, &text);
}

async fn handle(hub: Arc<Hub>, stream: UnixStream) -> std::io::Result<()> {
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();
    let Some(first) = lines.next_line().await? else { return Ok(()) };
    let msg: ClientMsg = match serde_json::from_str(&first) {
        Ok(m) => m,
        Err(e) => return send(&mut w, json!({"type": "error", "error": e.to_string()})).await,
    };
    match msg {
        ClientMsg::Ask { origin, kind, tool_name, tool_input, suggestions, context } => {
            let (id, rx) = hub.add_ask(origin, kind, tool_name, tool_input, suggestions, context);
            // The agent can give up before it even hears back: then the request is gone, not pending forever.
            if send(&mut w, json!({"type": "queued", "id": id})).await.is_err() {
                hub.finish(&id, "gone", "agent stopped waiting");
                return Ok(());
            }
            tokio::select! {
                reply = rx => match reply {
                    Ok(Reply::Decision(d)) => {
                        let mut v = serde_json::to_value(&d).unwrap_or_default();
                        v["type"] = json!("decision");
                        send(&mut w, v).await?;
                    }
                    Ok(Reply::Cancel(reason)) => send(&mut w, json!({"type": "cancel", "reason": reason})).await?,
                    Err(_) => {}
                },
                line = lines.next_line() => {
                    // The client spoke again (it got answered in its own terminal) or hung up.
                    match line.ok().flatten().and_then(|l| serde_json::from_str::<ClientMsg>(&l).ok()) {
                        Some(ClientMsg::Resolved { by, behavior }) => {
                            let by = if by.is_empty() { "terminal".to_string() } else { by };
                            hub.finish(&id, "answered_elsewhere", &format!("{behavior} in the {by}").trim().to_string());
                        }
                        _ => hub.finish(&id, "gone", "agent stopped waiting"),
                    }
                }
            }
        }
        ClientMsg::Event { origin, event, message, turn, driven_by, prompt_id, error_type } => {
            if event == "failed" {
                hub.failed(origin, &error_type, &message);
                return Ok(());
            }
            // A session on a machine keeps its transcript there: nothing to look up here, so it's yours
            // straight away (waiting on a file that never shows let its reply land first).
            let path = if origin.machine.is_empty() { origin.transcript_path.clone() } else { String::new() };
            let prompt = (event == "active").then(|| (origin.session_id.clone(), path, message.clone()));
            let driven_by = crate::leads::resolve_driver(&origin.session_id, &driven_by);
            hub.event(origin, &event, message, turn, &driven_by);
            if let Some((sid, path, text)) = prompt {
                tokio::spawn(settle_prompt(hub, sid, path, prompt_id, text));
            }
        }
        ClientMsg::Usage { rate_limits, session_id, cost } => {
            // The plan's limits first: whether there are any decides if the session's cost is shown.
            if rate_limits.is_object() {
                let h = hub.clone();
                let _ = tokio::task::spawn_blocking(move || h.set_rates(&rate_limits)).await;
            }
            if let (false, Some(c)) = (session_id.is_empty(), cost) {
                hub.set_cost(&session_id, c);
            }
        }
        ClientMsg::List => send(&mut w, hub.snapshot()).await?,
        ClientMsg::Respond { id, decision } => {
            let ok = hub.respond(&id, decision);
            send(&mut w, json!({"ok": ok})).await?;
        }
        ClientMsg::Focus { id } => {
            let r = match hub.get(&id) {
                Some(it) => focus::focus(&it.origin),
                None => Err("no such item".into()),
            };
            send(&mut w, json!({"ok": r.is_ok(), "detail": r.unwrap_or_else(|e| e)})).await?;
        }
        ClientMsg::Reply { id, text, images } => {
            let h = hub.clone();
            let r = tokio::task::spawn_blocking(move || h.reply(&id, &text, &images)).await.unwrap_or_else(|e| Err(e.to_string()));
            send(&mut w, json!({"ok": r.is_ok(), "detail": r.unwrap_or_else(|e| e)})).await?;
        }
        ClientMsg::SendTo { session_id, text, images, now } => {
            let h = hub.clone();
            let r = tokio::task::spawn_blocking(move || h.send_to_session(&session_id, &text, &images, now)).await.unwrap_or_else(|e| Err(e.to_string()));
            send(&mut w, json!({"ok": r.is_ok(), "detail": r.unwrap_or_else(|e| e)})).await?;
        }
        ClientMsg::Interrupt { session_id } => {
            let h = hub.clone();
            let r = tokio::task::spawn_blocking(move || h.interrupt(&session_id)).await.unwrap_or_else(|e| Err(e.to_string()));
            send(&mut w, json!({"ok": r.is_ok(), "detail": r.unwrap_or_else(|e| e)})).await?;
        }
        ClientMsg::Subscribe { origin } => {
            let sid = origin.session_id.clone();
            if sid.is_empty() {
                return Ok(());
            }
            let (n, mut rx) = hub.subscribe(&origin);
            loop {
                tokio::select! {
                    msg = rx.recv() => match msg {
                        Some(msg) => send(&mut w, msg).await?,
                        None => break,
                    },
                    line = lines.next_line() => if !matches!(line, Ok(Some(_))) { break },
                }
            }
            hub.unsubscribe(&sid, n);
        }
        ClientMsg::Resolved { .. } => {}
    }
    Ok(())
}

pub(crate) fn alive(pid: i32) -> bool {
    // Signal 0: existence check only. EPERM still means the process exists.
    unsafe { libc::kill(pid, 0) == 0 || *libc::__error() == libc::EPERM }
}

/// Once a second: clear Claude requests answered in the terminal, and anything whose agent died.
pub async fn watch(hub: Arc<Hub>) {
    let mut tick = tokio::time::interval(Duration::from_millis(1000));
    let mut n: u64 = 0;
    loop {
        tick.tick().await;
        let h = hub.clone();
        let _ = tokio::task::spawn_blocking(move || h.refresh_activity()).await;
        for (sid, pid) in hub.session_pids() {
            if pid > 0 && !alive(pid) {
                hub.drop_session(&sid);
            }
        }
        for it in hub.pending() {
            if let Some(pid) = it.origin.agent_pid {
                if pid > 0 && !alive(pid) {
                    if it.kind == "waiting" {
                        hub.remove_waiting(&it.id);
                    } else {
                        hub.finish(&it.id, "gone", "session ended");
                    }
                    continue;
                }
            }
            if it.kind == "waiting" || it.origin.harness != "claude" || it.origin.transcript_path.is_empty() {
                continue;
            }
            let path = &it.origin.transcript_path;
            let tool_use_id = match &it.tool_use_id {
                Some(t) => t.clone(),
                None => match transcript::find_tool_use_id(path, it.scan_from, &it.tool_name, &it.tool_input) {
                    Some(t) => {
                        hub.set_tool_use_id(&it.id, t.clone());
                        t
                    }
                    None => continue,
                },
            };
            if transcript::has_result(path, it.scan_from, &tool_use_id) {
                hub.finish(&it.id, "answered_elsewhere", "answered in the terminal");
            }
        }
        n += 1;
        if n % 10 == 0 {
            let h = hub.clone();
            let _ = tokio::task::spawn_blocking(move || h.usage_tick()).await;
        }
    }
}
