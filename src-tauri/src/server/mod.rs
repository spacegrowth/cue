//! Unix-socket listener at <Cue's data folder>/cue.sock. Newline-delimited JSON, one connection per message
//! (an Ask connection stays open until it's answered). See PROTOCOL.md.

pub mod inbound;

use crate::hub::{Hub, Reply};
use crate::model::ClientMsg;
use crate::logs::transcript;
use inbound::{Outcome, Prompt};
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

/// `f` with the hub, on the blocking pool (ipc::with_hub): hub calls take the store's lock and read files,
/// and here, in a connection's task, they'd hold up every other connection served by the same worker.
async fn on_hub<R: Send + 'static>(hub: &Arc<Hub>, f: impl FnOnce(&Arc<Hub>) -> R + Send + 'static) -> std::io::Result<R> {
    crate::ipc::with_hub(hub, f).await.map_err(std::io::Error::other)
}

/// The hook fires before Claude writes the prompt to its transcript, and only the transcript says
/// who sent it, so look for up to ~3s: every 200 ms (each look a file read, on the blocking pool), 15
/// times at most. Not found (or Codex, which sends no id): count it as yours.
async fn settle_prompt(hub: Arc<Hub>, p: Prompt) {
    let mut said = None;
    for _ in 0..15 {
        let look = p.clone();
        said = crate::ipc::blocking(move || inbound::sender(&look)).await.ok().flatten();
        if said.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let _ = on_hub(&hub, move |h| inbound::settle(h, p, said)).await;
}

/// The most one connection may send, in all: a message is a few KB, a reply with images a few MB.
/// Past it the connection just ends, so a client that never sends a newline can't grow Cue's memory.
const MAX_CONNECTION_BYTES: u64 = 64 << 20;

/// The agent session the connecting process belongs to, if any: it, or one of its ancestors, is a
/// process Cue knows as an agent's. (Hooks and `cue-ctl` run as the agent's children; Pi's extension
/// is the agent itself.) None when the peer is unknown or under none of them.
fn agent_behind(hub: &Hub, peer: Option<i32>) -> Option<String> {
    let peer = peer?;
    let agents: std::collections::HashMap<i32, String> = hub.agent_pids().into_iter().map(|(sid, pid)| (pid, sid)).collect();
    if agents.is_empty() {
        return None;
    }
    let out = std::process::Command::new("/bin/ps").args(["-axo", "pid=,ppid="]).output().ok()?;
    let parents: std::collections::HashMap<i32, i32> = String::from_utf8_lossy(&out.stdout).lines()
        .filter_map(|l| { let mut it = l.split_whitespace(); Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?)) })
        .collect();
    let mut pid = peer;
    for _ in 0..64 {
        if let Some(sid) = agents.get(&pid) {
            return Some(sid.clone());
        }
        pid = *parents.get(&pid)?;
        if pid <= 1 {
            return None;
        }
    }
    None
}

/// Deciding, replying, sending to a session, stopping one, going to one: yours, never an agent's.
/// Any process of this user can reach the socket, and the agents Cue gates are such processes: one
/// that was talked into it (a page it read, a repo it opened) could otherwise allow its own command
/// or type into another session's terminal. The agent session such a message came from inside, if it
/// did: it's refused, and told so. (Not by the private test Cue, CUE_QUIET: its tests answer their own
/// asks, and run under an agent when an agent runs them.)
async fn sent_by_an_agent(hub: &Arc<Hub>, msg: &ClientMsg, peer: Option<i32>) -> Option<String> {
    let control = matches!(msg, ClientMsg::Respond { .. } | ClientMsg::Reply { .. } | ClientMsg::SendTo { .. } | ClientMsg::Interrupt { .. } | ClientMsg::Focus { .. });
    if !control || std::env::var_os("CUE_QUIET").is_some() {
        return None;
    }
    on_hub(hub, move |h| agent_behind(h, peer)).await.unwrap_or(None)
}

type Lines = tokio::io::Lines<BufReader<tokio::io::Take<tokio::net::unix::OwnedReadHalf>>>;
type Writer = tokio::net::unix::OwnedWriteHalf;

/// One connection: its first line parsed and applied (inbound.rs), then whatever that leaves it to do.
async fn handle(hub: Arc<Hub>, stream: UnixStream) -> std::io::Result<()> {
    let peer = stream.peer_cred().ok().and_then(|c| c.pid());
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(tokio::io::AsyncReadExt::take(r, MAX_CONNECTION_BYTES)).lines();
    let Some(first) = lines.next_line().await? else { return Ok(()) };
    let msg: ClientMsg = match serde_json::from_str(&first) {
        Ok(m) => m,
        Err(e) => return send(&mut w, json!({"type": "error", "error": e.to_string()})).await,
    };
    if let Some(sid) = sent_by_an_agent(&hub, &msg, peer).await {
        eprintln!("cue: refused a {} from inside agent session {sid} (pid {})", first.split('"').nth(3).unwrap_or("control message"), peer.unwrap_or(0));
        return send(&mut w, json!({"ok": false, "error": format!("refused: sent from inside agent session {sid}; Cue takes this only from you")})).await;
    }
    // The state as last built needs no store and no file: answered here, not behind the blocking pool.
    if let (ClientMsg::List, Some(state)) = (&msg, hub.kept_state()) {
        return send(&mut w, state).await;
    }
    // A control message is told why its hub call never ran; anything else just ends.
    let tells = matches!(msg, ClientMsg::Focus { .. } | ClientMsg::Reply { .. } | ClientMsg::SendTo { .. } | ClientMsg::Interrupt { .. });
    let outcome = match on_hub(&hub, move |h| inbound::apply(h, msg, peer)).await {
        Ok(o) => o,
        Err(e) if tells => return send(&mut w, inbound::told(Err(e))).await,
        Err(e) => return Err(e),
    };
    match outcome {
        Outcome::Quiet => Ok(()),
        Outcome::Say(v) => send(&mut w, v).await,
        Outcome::Asked { id, answer } => await_answer(hub, id, answer, &mut w, &mut lines).await,
        Outcome::Settle(p) => {
            tokio::spawn(settle_prompt(hub, p));
            Ok(())
        }
        Outcome::Subscribed { session_id, n, messages } => serve_subscription(hub, session_id, n, messages, &mut w, &mut lines).await,
    }
}

/// An Ask, queued as `id`: its answer to the client, or the request finished when the client gives up or
/// got its answer somewhere else.
async fn await_answer(hub: Arc<Hub>, id: String, rx: tokio::sync::oneshot::Receiver<Reply>, w: &mut Writer, lines: &mut Lines) -> std::io::Result<()> {
    // The agent can give up before it even hears back: then the request is gone, not pending forever.
    if send(w, json!({"type": "queued", "id": id})).await.is_err() {
        return on_hub(&hub, move |h| inbound::gone(h, &id)).await;
    }
    tokio::select! {
        reply = rx => match reply {
            Ok(Reply::Decision(d)) => {
                let mut v = serde_json::to_value(&d).unwrap_or_default();
                v["type"] = json!("decision");
                send(w, v).await?;
            }
            Ok(Reply::Cancel(reason)) => send(w, json!({"type": "cancel", "reason": reason})).await?,
            Err(_) => {}
        },
        line = lines.next_line() => {
            // The client spoke again (it got answered in its own terminal) or hung up.
            let said = line.ok().flatten().and_then(|l| serde_json::from_str::<ClientMsg>(&l).ok());
            on_hub(&hub, move |h| inbound::ask_ended(h, &id, said)).await?;
        }
    }
    Ok(())
}

/// A session that takes replies directly (Pi): Cue's messages for it go down this connection until either
/// side hangs up; its commands, sent again on it, are kept.
async fn serve_subscription(hub: Arc<Hub>, sid: String, n: u64, mut rx: tokio::sync::mpsc::UnboundedReceiver<Value>, w: &mut Writer, lines: &mut Lines) -> std::io::Result<()> {
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(msg) => send(w, msg).await?,
                None => break,
            },
            line = lines.next_line() => match line {
                // The session's commands again (Refresh asked for them); anything else said here is ignored.
                Ok(Some(l)) => if let Ok(ClientMsg::Commands { commands }) = serde_json::from_str::<ClientMsg>(&l) {
                    let sid = sid.clone();
                    on_hub(&hub, move |h| inbound::set_commands(h, &sid, &commands)).await?;
                },
                _ => break,
            },
        }
    }
    on_hub(&hub, move |h| h.unsubscribe(&sid, n)).await
}

pub(crate) fn alive(pid: i32) -> bool {
    // Signal 0: existence check only. EPERM still means the process exists.
    unsafe { libc::kill(pid, 0) == 0 || *libc::__error() == libc::EPERM }
}

/// Once a second: clear Claude requests answered in the terminal, and anything whose agent died. Each tick's
/// work (the hub's store, transcripts, `kill -0`) on the blocking pool.
pub async fn watch(hub: Arc<Hub>) {
    let mut every = tokio::time::interval(Duration::from_millis(1000));
    let mut n: u64 = 0;
    loop {
        every.tick().await;
        n += 1;
        let usage = n % 10 == 0;
        let _ = on_hub(&hub, move |h| tick(h, usage)).await;
    }
}

/// One of the watcher's ticks: what each working session is doing, the gone and the answered cleared,
/// and (`usage`, every tenth) the usage and the turns that ended without a word.
pub(crate) fn tick(h: &Hub, usage: bool) {
    h.refresh_activity();
    settle_the_gone(h);
    if usage {
        h.usage_tick();
    }
}

/// Sessions and requests whose agent exited, gone; Claude requests its transcript shows answered, finished.
fn settle_the_gone(hub: &Hub) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// One connection served (as `serve` does), and its client end.
    fn connect(hub: &Arc<Hub>) -> UnixStream {
        let (server, client) = UnixStream::pair().unwrap();
        tokio::spawn(handle(hub.clone(), server));
        client
    }

    /// Everything the connection says until it closes, if it closes within `ms`.
    async fn said(mut c: UnixStream, ms: u64) -> Option<String> {
        let mut out = String::new();
        tokio::time::timeout(Duration::from_millis(ms), c.read_to_string(&mut out)).await.ok()?.ok()?;
        Some(out)
    }

    /// One async worker, as a busy Cue's pool can be: a connection whose hub call waits on the store (another
    /// thread holds it) waits off that worker, so another client is still answered meanwhile.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn a_client_is_answered_while_another_clients_hub_call_waits_on_the_store() {
        let dir = std::env::temp_dir().join(format!("cue-serve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Arc::new(Hub::new(None));
        let _ = hub.state();
        let (held_tx, held) = std::sync::mpsc::channel();
        let (go, release) = std::sync::mpsc::channel::<()>();
        let h = hub.clone();
        let holder = std::thread::spawn(move || h.hold_store(held_tx, release));
        held.recv().unwrap();
        // A session's cost: a hub call, waiting on the store.
        let mut slow = connect(&hub);
        slow.write_all(b"{\"type\":\"usage\",\"session_id\":\"s1\",\"cost\":1.5}\n").await.unwrap();
        // Another client asks for the state meanwhile. Timed by the wall clock, not the runtime's: with its one
        // worker stuck, the runtime's own timers wouldn't fire either.
        let mut other = connect(&hub);
        other.write_all(b"{\"type\":\"list\"}\n").await.unwrap();
        let t0 = std::time::Instant::now();
        let answer = said(other, 4000).await;
        let took = t0.elapsed();
        go.send(()).unwrap();
        holder.join().unwrap();
        let slow_done = said(slow, 3000).await;
        let answer = answer.expect("answered");
        assert!(took < Duration::from_secs(1), "answered while the other connection's hub call waits, not after: {took:?}");
        let v: Value = serde_json::from_str(answer.trim()).unwrap();
        assert!(v["items"].is_array(), "{answer}");
        assert_eq!(slow_done.as_deref(), Some(""), "the slow one finishes once the store is free");
    }
}
