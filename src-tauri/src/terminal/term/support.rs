//! What the built-in terminal's tests share: waiting for a condition, a stand-in client in a real pty
//! (`fake`, `record`), whether a process lives, and a call bounded in time.

use super::*;
use std::sync::{Arc, Mutex as M};

/// Wait until `ok` holds (checked every 50 ms), up to 10 s: a busy Mac takes longer, never too long.
pub(super) fn until(mut ok: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < end {
        if ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    ok()
}

/// A stand-in for the client (what `ssh -tt` is for a machine's session): `sleep` in a pty, after
/// `delay` (the SSH round trip that finds the pane). Counts its launches; gives its pid.
pub(super) fn fake(starts: &Arc<M<Vec<u32>>>, delay: u64) -> Result<(u64, Open), String> {
    std::thread::sleep(std::time::Duration::from_millis(delay));
    let pair = native_pty_system().openpty(PtySize { rows: 4, cols: 20, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
    let mut c = CommandBuilder::new("/bin/sleep");
    c.arg("30");
    let child = pair.slave.spawn_command(c).map_err(|e| e.to_string())?;
    drop(pair.slave);
    crate::locking::lock(&starts, "starts").push(child.process_id().unwrap_or(0));
    let gen = next_gen();
    Ok((gen, Open::new("fake", gen, pair.master, child)?))
}

pub(super) fn alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill").args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

/// `cmd` in a pty of its own, recorded as terminal `sid` (whatever's typed in it goes to `input`, or
/// to the pty when None), the way `once` records one.
pub(super) fn record(sid: &str, cmd: CommandBuilder, input: Option<Box<dyn std::io::Write + Send>>) -> u32 {
    let pair = native_pty_system().openpty(PtySize { rows: 4, cols: 20, pixel_width: 0, pixel_height: 0 }).unwrap();
    let child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let pid = child.process_id().unwrap_or(0);
    let mut o = Open::new(sid, next_gen(), pair.master, child).unwrap();
    if let Some(w) = input {
        o.input = Input::new(sid, w).unwrap();
    }
    crate::locking::lock(open_terms(), "open_terms").open.insert(sid.to_string(), o);
    pid
}

pub(super) fn sleeper() -> CommandBuilder {
    let mut c = CommandBuilder::new("/bin/sleep");
    c.arg("30");
    c
}

/// `f` on a thread of its own, its answer if it's back within `ms`.
pub(super) fn within<R: Send + 'static>(ms: u64, f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}
