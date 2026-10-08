//! The built-in terminal: a session's real screen in Cue's window (xterm.js draws it), for sessions in
//! tmux. Opening it starts a tmux client of Cue's own in a pseudo-terminal and streams it to the window;
//! what you type goes back the same way. The client views a session of its own grouped with the
//! session's (same windows, its own current window, no status bar), so it never moves what any other
//! terminal attached to tmux shows. Closing it ends only that client: the session keeps running.
use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

struct Open {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn open_terms() -> &'static Mutex<HashMap<String, Open>> {
    static M: OnceLock<Mutex<HashMap<String, Open>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_gen() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// Open session `sid`'s terminal (its tmux pane, on this Mac or over SSH on its machine) at `cols`×`rows`. Its output arrives as "term" events
/// ({sid, gen, data: base64} and finally {sid, gen, end: true}); `gen` tells this opening from an earlier one.
pub fn open(app: AppHandle, sid: &str, o: &crate::model::Origin, cols: u16, rows: u16) -> Result<u64, String> {
    let host = if o.machine.is_empty() { None } else { Some(crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?.host) };
    open_with(sid, &o.tmux_pane, host.as_deref(), cols, rows, move |v| {
        let _ = app.emit("term", v);
    })
}

/// The same, handing each event to `out` (the window's, or a test's).
fn open_with(sid: &str, pane: &str, host: Option<&str>, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<u64, String> {
    close(sid);
    let tmux = crate::focus::tmux_bin();
    // tmux here, or on the machine (over SSH).
    let tmux_say = |args: &[&str]| -> String {
        match host {
            Some(h) => crate::machines::tmux(h, args).unwrap_or_default(),
            None => std::process::Command::new(&tmux).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default(),
        }
    };
    let where_ = tmux_say(&["display-message", "-p", "-t", pane, "#{session_name}\t#{window_id}"]).trim().to_string();
    let (session, window) = where_.split_once('\t').filter(|(s, w)| !s.is_empty() && !w.is_empty()).ok_or(format!("tmux pane {pane} is gone"))?;
    let gen = next_gen();
    let view = format!("cue-view-{gen}");
    // The window follows the size of whichever terminal was used last (this one while you type here).
    tmux_say(&["set-option", "-w", "-t", window, "window-size", "latest"]);
    let pair = native_pty_system().openpty(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
    // One client launch: a session grouped with the session's (its windows, its own current window), set
    // up for a pane inside Cue, gone once this client detaches.
    let view_window = format!("{view}:{window}");
    let chain = ["new-session", "-t", session, "-s", &view, ";", "set-option", "status", "off", ";", "set-option", "mouse", "on", ";", "set-option", "destroy-unattached", "on", ";", "select-window", "-t", &view_window, ";", "select-pane", "-t", pane];
    let mut cmd = match host {
        None => {
            let mut c = CommandBuilder::new(&tmux);
            for a in chain {
                c.arg(a);
            }
            c
        }
        // On a machine: the same client, through ssh (with a terminal of its own).
        Some(h) => {
            let mut c = CommandBuilder::new("/usr/bin/ssh");
            for a in crate::machines::shared() {
                c.arg(a);
            }
            for a in ["-o", "BatchMode=yes", "-tt", h] {
                c.arg(a);
            }
            c.arg(crate::machines::remote_command(&format!("exec tmux {}", chain.iter().map(|a| crate::machines::shq(a)).collect::<Vec<_>>().join(" "))));
            c
        }
    };
    cmd.env("TERM", "xterm-256color");
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let id = sid.to_string();
    std::thread::spawn(move || {
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out(serde_json::json!({ "sid": id, "gen": gen, "data": b64.encode(&buf[..n]) })),
            }
        }
        out(serde_json::json!({ "sid": id, "gen": gen, "end": true }));
    });
    open_terms().lock().unwrap().insert(sid.to_string(), Open { master: pair.master, writer, child });
    Ok(gen)
}

/// What you typed in it (and the terminal's own replies, as xterm.js produces them).
pub fn write(sid: &str, data: &str) -> Result<(), String> {
    let mut terms = open_terms().lock().unwrap();
    let t = terms.get_mut(sid).ok_or("its terminal isn't open")?;
    t.writer.write_all(data.as_bytes()).and_then(|_| t.writer.flush()).map_err(|e| e.to_string())
}

pub fn resize(sid: &str, cols: u16, rows: u16) -> Result<(), String> {
    let terms = open_terms().lock().unwrap();
    let t = terms.get(sid).ok_or("its terminal isn't open")?;
    t.master.resize(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
}

/// Close it: Cue's client goes (its grouped session with it); the session itself keeps running.
pub fn close(sid: &str) {
    if let Some(mut t) = open_terms().lock().unwrap().remove(sid) {
        let _ = t.child.kill();
        let _ = t.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as M};

    fn tmux(args: &[&str]) -> String {
        let o = std::process::Command::new(crate::focus::tmux_bin()).args(args).output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// Wait until `ok` holds (checked every 50 ms), up to 10 s: a busy Mac takes longer, never too long.
    fn until(mut ok: impl FnMut() -> bool) -> bool {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < end {
            if ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        ok()
    }

    #[test]
    fn the_built_in_terminal_shows_a_session_takes_what_you_type_and_leaves_it_running() {
        if !crate::focus::tmux_installed() {
            return;
        }
        let name = format!("cue-term-test-{}", std::process::id());
        let pane = tmux(&["new-session", "-d", "-s", &name, "-x", "80", "-y", "12", "-P", "-F", "#{pane_id}", "cat"]);
        let seen = Arc::new(M::new(String::new()));
        let ended = Arc::new(M::new(false));
        let (s2, e2) = (seen.clone(), ended.clone());
        open_with("t", &pane, None, 80, 12, move |v| {
            if let Some(d) = v["data"].as_str() {
                let bytes = base64::engine::general_purpose::STANDARD.decode(d).unwrap();
                s2.lock().unwrap().push_str(&String::from_utf8_lossy(&bytes));
            }
            if v["end"] == true {
                *e2.lock().unwrap() = true;
            }
        })
        .unwrap();
        // Its viewer is up (it has drawn something) before anything is typed into it.
        let views_open = until(|| tmux(&["list-sessions", "-F", "#{session_name}"]).lines().any(|l| l.starts_with("cue-view-")) && !seen.lock().unwrap().is_empty());
        write("t", "hello from cue\r").unwrap();
        let shown = until(|| seen.lock().unwrap().contains("hello from cue"));
        let reached = until(|| tmux(&["capture-pane", "-p", "-t", &pane]).contains("hello from cue"));
        close("t");
        let views_gone = until(|| !tmux(&["list-sessions", "-F", "#{session_name}"]).lines().any(|l| l.starts_with("cue-view-")));
        let heard_end = until(|| *ended.lock().unwrap());
        let after = tmux(&["list-sessions", "-F", "#{session_name}"]);
        tmux(&["kill-session", "-t", &name]);
        assert!(views_open, "a viewer of Cue's own while open");
        assert!(shown, "it shows what the session prints: {:?}", seen.lock().unwrap());
        assert!(reached, "what you type reaches the session");
        assert!(views_gone, "the viewer is gone once closed: {after:?}");
        assert!(after.lines().any(|l| l == name), "the session keeps running: {after:?}");
        assert!(heard_end, "the window hears it ended");
        assert!(write("t", "x").is_err(), "nothing open any more");
    }
}
