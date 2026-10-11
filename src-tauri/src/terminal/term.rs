//! The built-in terminal: a session's real screen in Cue's window (xterm.js draws it), for sessions in
//! tmux. Opening it starts a tmux client of Cue's own in a pseudo-terminal and streams it to the window;
//! what you type goes back the same way. The client views a session of its own grouped with the
//! session's (same windows, its own current window, no status bar), so it never moves what any other
//! terminal attached to tmux shows. Closing it ends only that client: the session keeps running.
use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

struct Open {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    /// A machine's view session (host, name), Cue's own: removed there by Cue on close, before its client
    /// goes. Some SSH servers never hang up a dropped client, so the view would stay attached
    /// for good, one more for every terminal ever opened; and tmux 3.0a can take its whole server down
    /// when a grouped view is left to remove itself once its client is gone (destroy-unattached), so
    /// the view goes first, by its exact name, and its client ends with it.
    view: Option<(String, String)>,
}

impl Open {
    /// End its client (and the ssh carrying it, for a machine's session). A machine's view is removed
    /// first, off this thread (one SSH trip), then the client. A removal that fails or hasn't come back
    /// in 5 s (a machine asleep, a dead line under the shared connection) doesn't hold the client: it
    /// ends anyway and the view stays behind, nothing else touched.
    fn end(mut self) {
        match self.view.take() {
            Some((host, view)) => {
                std::thread::spawn(move || {
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = crate::machines::tmux(&host, &["kill-session", "-t", &format!("={view}")]);
                        let _ = tx.send(());
                    });
                    let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
                    self.kill();
                });
            }
            None => self.kill(),
        }
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An opening in flight: what it came to (its `gen`, or why it failed), for the opens that join it.
#[derive(Default)]
struct Pending {
    result: Mutex<Option<Result<u64, String>>>,
    ready: Condvar,
}

/// The terminals open, and the ones being opened, by session: one lock for both, so two opens of one
/// session (two clicks, a click and the shortcut) can never both start a client.
#[derive(Default)]
struct Terms {
    open: HashMap<String, Open>,
    opening: HashMap<String, Arc<Pending>>,
}

fn open_terms() -> &'static Mutex<Terms> {
    static M: OnceLock<Mutex<Terms>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(Terms::default()))
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

/// A relay / pi-lead executor on machine `machine`, running in a detached tmux session of its own
/// (`tmux_session`, from its lead's listing there): its terminal is a client attached to exactly that
/// session, never to its lead's. Opened, deduped and closed as `open`'s are (one per `sid`).
pub fn open_attached(app: AppHandle, sid: &str, machine: &str, tmux_session: &str, cols: u16, rows: u16) -> Result<u64, String> {
    let host = crate::machines::get(machine).ok_or(format!("{machine} isn't one of your machines any more (+ New → Machine)"))?.host;
    let target = Target::Session(tmux_session);
    once(sid, cols, rows, || launch(sid, &target, Some(&host), cols, rows, move |v| {
        let _ = app.emit("term", v);
    }))
}

/// What a terminal shows: a session's pane (through a view of Cue's own, grouped with the pane's tmux
/// session), or a whole tmux session it attaches to (an executor's own, on a machine).
enum Target<'a> {
    Pane(&'a str),
    Session(&'a str),
}

/// The same, handing each event to `out` (the window's, or a test's).
fn open_with(sid: &str, pane: &str, host: Option<&str>, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<u64, String> {
    // No pane is not "any pane": tmux reads `-t ''` as the current session, which would be someone else's.
    if pane.trim().is_empty() {
        return Err("this session isn't in a tmux pane Cue knows".into());
    }
    once(sid, cols, rows, || launch(sid, &Target::Pane(pane), host, cols, rows, out))
}

/// The tmux command line that attaches to session `name` and nothing else: `=` makes tmux match the
/// whole name (never a longer session that starts with it). None for a name that isn't plain
/// letters, digits, `_` and `-`.
fn attach_args(name: &str) -> Option<Vec<String>> {
    let plain = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    plain.then(|| vec!["-u".into(), "attach-session".into(), "-t".into(), format!("={name}")])
}

/// One opening per session at a time. An open while another is in flight joins it (waits for it, then
/// has its size), so a slow SSH round trip never ends up as two clients, the first left running. An
/// open after it finished replaces it: the old client is ended first. Whatever is still recorded when
/// the new one lands is ended too, and an opening that was closed (or replaced) meanwhile ends its own.
fn once(sid: &str, cols: u16, rows: u16, launch: impl FnOnce() -> Result<(u64, Open), String>) -> Result<u64, String> {
    let (mine, old) = {
        let mut t = crate::locking::lock(open_terms(), "open_terms");
        let joined = t.opening.get(sid).cloned();
        match joined {
            Some(p) => (Err(p), None),
            None => {
                let p = Arc::new(Pending::default());
                t.opening.insert(sid.to_string(), p.clone());
                (Ok(p), t.open.remove(sid))
            }
        }
    };
    let p = match mine {
        Ok(p) => p,
        // Joined: the first opening's answer, at this one's size.
        Err(p) => {
            let mut r = crate::locking::lock(&p.result, "term opening");
            while r.is_none() {
                r = p.ready.wait(r).unwrap_or_else(|e| e.into_inner());
            }
            let gen = r.clone().unwrap_or_else(|| Err("its terminal didn't open".into()));
            drop(r);
            if gen.is_ok() {
                let _ = resize(sid, cols, rows);
            }
            return gen;
        }
    };
    // From here until its answer is in, a panic (in `launch`, or ending the old client) still clears the
    // opening and answers whoever joined it: otherwise every later open of this session would wait forever.
    let mut guard = Opening { sid, p: p.clone(), answered: false };
    if let Some(o) = old {
        o.end();
    }
    let launched = launch();
    let (result, stray) = {
        let mut t = crate::locking::lock(open_terms(), "open_terms");
        let still_mine = t.opening.get(sid).is_some_and(|q| Arc::ptr_eq(q, &p));
        if still_mine {
            t.opening.remove(sid);
        }
        match launched {
            Ok((gen, o)) if still_mine => (Ok(gen), t.open.insert(sid.to_string(), o)),
            // Closed while it was opening: nobody wants this client.
            Ok((gen, o)) => (Ok(gen), Some(o)),
            Err(e) => (Err(e), None),
        }
    };
    answer(&p, result.clone());
    guard.answered = true;
    if let Some(o) = stray {
        o.end();
    }
    result
}

/// Hand an opening's outcome to the opens waiting on it.
fn answer(p: &Pending, result: Result<u64, String>) {
    *crate::locking::lock(&p.result, "term opening") = Some(result);
    p.ready.notify_all();
}

/// An opening in flight, owned by the open that started it. Dropped before it was answered (its launch
/// panicked): its entry goes, if it's still the one recorded, and the opens that joined it get an error.
struct Opening<'a> {
    sid: &'a str,
    p: Arc<Pending>,
    answered: bool,
}

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        if self.answered {
            return;
        }
        {
            let mut t = crate::locking::lock(open_terms(), "open_terms");
            let still_mine = t.opening.get(self.sid).is_some_and(|q| Arc::ptr_eq(q, &self.p));
            if still_mine {
                t.opening.remove(self.sid);
            }
        }
        answer(&self.p, Err("its terminal didn't open".into()));
    }
}

/// Start the client: one SSH round trip to find the pane (for a machine's session), then the client
/// itself in a pseudo-terminal, its output streamed to `out` from a thread of its own.
fn launch(sid: &str, target: &Target, host: Option<&str>, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<(u64, Open), String> {
    let tmux = crate::terminal::focus::tmux_bin();
    let gen = next_gen();
    let (chain, view): (Vec<String>, Option<String>) = match target {
        Target::Pane(pane) => {
            let (chain, view) = pane_chain(&tmux, pane, host, gen)?;
            (chain, Some(view))
        }
        Target::Session(name) => (attach_args(name).ok_or(format!("“{name}” isn't a tmux session name Cue will attach to"))?, None),
    };
    let pair = native_pty_system().openpty(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
    let mut cmd = match host {
        None => {
            let mut c = CommandBuilder::new(&tmux);
            for a in &chain {
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
            // tmux by its full path when Cue knows it (no login shell to wait for), else through your shell.
            let args = chain.iter().map(|a| crate::machines::shq(a)).collect::<Vec<_>>().join(" ");
            c.arg(match crate::machines::tmux_path(h) {
                Some(p) => format!("exec /bin/sh -c {}", crate::machines::shq(&format!("exec {} {args}", crate::machines::shq(&p)))),
                None => crate::machines::remote_command(&format!("exec tmux {args}")),
            });
            c
        }
    };
    cmd.env("TERM", "xterm-256color");
    let (gen, mut o) = start(sid, gen, pair, cmd, out)?;
    o.view = host.zip(view).map(|(h, v)| (h.to_string(), v));
    Ok((gen, o))
}

/// `cmd` in the pseudo-terminal `pair`, its output streamed to `out` from a thread of its own
/// ({sid, gen, data} as it comes, then {sid, gen, end: true}).
fn start(sid: &str, gen: u64, pair: portable_pty::PtyPair, cmd: CommandBuilder, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<(u64, Open), String> {
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
    Ok((gen, Open { master: pair.master, writer, child, view: None }))
}

/// "cue-view-<when>-<pid>-": the start of every view this launch of Cue opens (when: the second it first
/// opened one). The time keeps a later launch, which may get the same pid, off an earlier one's names; the
/// pid keeps two Macs' Cues apart when they open a first view on one machine in the same second.
fn view_prefix() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| format!("cue-view-{}-{}-", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()), std::process::id()))
}

/// A pane's client: one round trip (over SSH for a machine's session) to find the pane, then the tmux
/// command line for a view of Cue's own on its window, and that view's name.
fn pane_chain(tmux: &str, pane: &str, host: Option<&str>, gen: u64) -> Result<(Vec<String>, String), String> {
    // Where the pane is, and (the same trip) its window follows the size of whichever terminal was used
    // last (this one while you type here): one trip to the machine, not two. Two tmux commands, not one
    // line: tmux before 3.1 has no "latest" and fails the line, which lost the answer ("pane is gone").
    // The pane's session group, not its session: with Cue's views open, tmux names the newest view as the
    // pane's session, and each view grouped with the one before it ("-t cue-view-8 -s cue-view-9"),
    // or failed to find one that had just gone. A session in no group: its own name.
    let ask = ["display-message", "-p", "-t", pane, "#{?session_group,#{session_group},#{session_name}}|#{window_id}"];
    let also = ["set-option", "-w", "-t", pane, "window-size", "latest"];
    let where_ = match host {
        // Not reached: say so, not that the pane is gone.
        Some(h) => crate::machines::tmux_then(h, &ask, &also)?,
        None => {
            let out = std::process::Command::new(tmux).args(ask).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            let _ = std::process::Command::new(tmux).args(also).output();
            out
        }
    };
    let where_ = where_.trim().to_string();
    // Split at the last "|": a session's name may have one, a window id ("@3") never does. Not a tab: some
    // tmux versions print it as "_".
    let (session, window) = where_.rsplit_once('|').filter(|(s, w)| !s.is_empty() && !w.is_empty()).ok_or(format!("tmux pane {pane} is gone"))?;
    // This launch's own names (view_prefix): views count from 1 again at every launch, and one left on a machine from
    // an earlier run would refuse the same name ("duplicate session"). Removed by its exact name, its
    // client leaves with it: never switched to another session (a tmux.conf's detach-on-destroy off).
    let view = format!("{}{gen}", view_prefix());
    // One client launch: a session grouped with the session's (its windows, its own current window), set
    // up for a pane inside Cue, gone once this client detaches.
    let view_window = format!("{view}:{window}");
    // -u: the view speaks UTF-8 whatever the locale where it starts (the window's terminal always does).
    let chain = ["-u", "new-session", "-t", session, "-s", &view, ";", "set-option", "status", "off", ";", "set-option", "mouse", "on", ";", "set-option", "destroy-unattached", "on", ";", "set-option", "detach-on-destroy", "on", ";", "select-window", "-t", &view_window, ";", "select-pane", "-t", pane];
    Ok((chain.iter().map(|a| a.to_string()).collect(), view))
}

/// Any other command in a terminal of Cue's own (a plain shell: shells.rs), opened, deduped, written to,
/// resized and closed by `sid` as a session's are, its output the same "term" events.
pub(crate) fn spawn(sid: &str, cmd: CommandBuilder, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<u64, String> {
    once(sid, cols, rows, || {
        let pair = native_pty_system().openpty(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
        start(sid, next_gen(), pair, cmd, out)
    })
}

/// For each open terminal whose id starts with `prefix`: its process, and the process group in the
/// foreground of its terminal (the same as its process while it sits at its prompt).
pub(crate) fn foreground(prefix: &str) -> Vec<(String, Option<u32>, Option<i32>)> {
    let t = crate::locking::lock(open_terms(), "open_terms");
    t.open.iter().filter(|(id, _)| id.starts_with(prefix)).map(|(id, o)| (id.clone(), o.child.process_id(), o.master.process_group_leader())).collect()
}

/// What you typed in it (and the terminal's own replies, as xterm.js produces them).
pub fn write(sid: &str, data: &str) -> Result<(), String> {
    let mut terms = crate::locking::lock(open_terms(), "open_terms");
    let t = terms.open.get_mut(sid).ok_or("its terminal isn't open")?;
    t.writer.write_all(data.as_bytes()).and_then(|_| t.writer.flush()).map_err(|e| e.to_string())
}

pub fn resize(sid: &str, cols: u16, rows: u16) -> Result<(), String> {
    let terms = crate::locking::lock(open_terms(), "open_terms");
    let t = terms.open.get(sid).ok_or("its terminal isn't open")?;
    t.master.resize(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
}

/// Close it: Cue's client goes (its grouped session with it); the session itself keeps running. One
/// still opening ends its client as soon as it's up (nobody waits for it any more).
pub fn close(sid: &str) {
    let open = {
        let mut t = crate::locking::lock(open_terms(), "open_terms");
        t.opening.remove(sid);
        t.open.remove(sid)
    };
    if let Some(o) = open {
        o.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as M};

    fn tmux(args: &[&str]) -> String {
        let o = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).output().unwrap();
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

    /// A stand-in for the client (what `ssh -tt` is for a machine's session): `sleep` in a pty, after
    /// `delay` (the SSH round trip that finds the pane). Counts its launches; gives its pid.
    fn fake(starts: &Arc<M<Vec<u32>>>, delay: u64) -> Result<(u64, Open), String> {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let pair = native_pty_system().openpty(PtySize { rows: 4, cols: 20, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
        let mut c = CommandBuilder::new("/bin/sleep");
        c.arg("30");
        let child = pair.slave.spawn_command(c).map_err(|e| e.to_string())?;
        drop(pair.slave);
        starts.lock().unwrap().push(child.process_id().unwrap_or(0));
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        Ok((next_gen(), Open { master: pair.master, writer, child, view: None }))
    }

    fn alive(pid: u32) -> bool {
        std::process::Command::new("/bin/kill").args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
    }

    #[test]
    fn two_opens_at_once_start_one_client_and_a_replaced_one_is_ended() {
        let sid = format!("dedupe-{}", std::process::id());
        let starts = Arc::new(M::new(vec![]));
        // Two clicks while the first opening is still on its SSH round trip.
        let (s1, s2, a, b) = (starts.clone(), starts.clone(), sid.clone(), sid.clone());
        let first = std::thread::spawn(move || once(&a, 20, 4, || fake(&s1, 400)));
        std::thread::sleep(std::time::Duration::from_millis(100));
        let second = std::thread::spawn(move || once(&b, 20, 4, || fake(&s2, 400)));
        let (g1, g2) = (first.join().unwrap().unwrap(), second.join().unwrap().unwrap());
        assert_eq!(starts.lock().unwrap().len(), 1, "one client, not two");
        assert_eq!(g1, g2, "the second joined the first");
        let pid1 = starts.lock().unwrap()[0];
        assert!(alive(pid1), "the client runs");
        // Opened again once it's open (it ended, say): the old client is ended, the new one recorded.
        let g3 = once(&sid, 20, 4, || fake(&starts, 0)).unwrap();
        assert_ne!(g3, g1);
        let pid2 = starts.lock().unwrap()[1];
        assert!(until(|| !alive(pid1)), "the replaced client is gone");
        assert!(alive(pid2), "the new one runs");
        // Closed while an opening is in flight: that opening ends its own client when it lands.
        let (s3, c) = (starts.clone(), sid.clone());
        close(&sid);
        assert!(until(|| !alive(pid2)));
        let late = std::thread::spawn(move || once(&c, 20, 4, || fake(&s3, 300)));
        std::thread::sleep(std::time::Duration::from_millis(100));
        close(&sid);
        late.join().unwrap().unwrap();
        let pid3 = starts.lock().unwrap()[2];
        assert!(until(|| !alive(pid3)), "nobody wanted it: ended");
        assert!(write(&sid, "x").is_err(), "nothing open");
    }

    #[test]
    fn an_opening_that_panics_answers_the_open_waiting_on_it_and_never_blocks_the_next() {
        let sid = format!("panics-{}", std::process::id());
        let (started_tx, started) = std::sync::mpsc::channel();
        let (go, go_rx) = std::sync::mpsc::channel::<()>();
        let a = sid.clone();
        // The first opening is mid-launch when a second open joins it; then its launch panics.
        let first = std::thread::spawn(move || {
            once(&a, 20, 4, || {
                started_tx.send(()).unwrap();
                go_rx.recv().unwrap();
                panic!("launch blew up");
            })
        });
        started.recv().unwrap();
        let (joined_tx, joined) = std::sync::mpsc::channel();
        let b = sid.clone();
        std::thread::spawn(move || {
            let _ = joined_tx.send(once(&b, 20, 4, || Err("the joiner launched its own".into())));
        });
        std::thread::sleep(std::time::Duration::from_millis(200));
        go.send(()).unwrap();
        assert!(first.join().is_err(), "the launch panicked");
        let r = joined.recv_timeout(std::time::Duration::from_secs(5)).expect("the open that joined it is answered, not left waiting");
        assert_eq!(r, Err("its terminal didn't open".into()));
        // Nothing is left in flight: the next open launches its own, at once.
        let (next_tx, next) = std::sync::mpsc::channel();
        let c = sid.clone();
        std::thread::spawn(move || {
            let _ = next_tx.send(once(&c, 20, 4, || Err("launched".into())));
        });
        assert_eq!(next.recv_timeout(std::time::Duration::from_secs(5)).expect("the next open isn't stuck behind the panicked one"), Err("launched".into()));
        let pending = crate::locking::lock(open_terms(), "open_terms").opening.contains_key(&sid);
        assert!(!pending, "no opening left behind");
    }

    #[test]
    fn an_executors_terminal_attaches_to_its_own_tmux_session_by_its_whole_name() {
        assert_eq!(attach_args("relay-exec_2"), Some(vec!["-u".to_string(), "attach-session".into(), "-t".into(), "=relay-exec_2".into()]));
        for bad in ["", "a b", "lead:0", "x;y", "$(id)", "a.b"] {
            assert_eq!(attach_args(bad), None, "{bad:?}");
        }
        if !crate::terminal::focus::tmux_installed() {
            return;
        }
        // Its session, and a longer name that starts with it (what a plain `-t` would also match).
        let name = format!("cue-attach-test-{}", std::process::id());
        let longer = format!("{name}-lead");
        let pane = tmux(&["new-session", "-d", "-s", &name, "-x", "80", "-y", "12", "-P", "-F", "#{pane_id}", "cat"]);
        let other = tmux(&["new-session", "-d", "-s", &longer, "-x", "80", "-y", "12", "-P", "-F", "#{pane_id}", "cat"]);
        let seen = Arc::new(M::new(String::new()));
        let ended = Arc::new(M::new(false));
        let watch = |seen: Arc<M<String>>, ended: Arc<M<bool>>| move |v: serde_json::Value| {
            if let Some(d) = v["data"].as_str() {
                let bytes = base64::engine::general_purpose::STANDARD.decode(d).unwrap();
                seen.lock().unwrap().push_str(&String::from_utf8_lossy(&bytes));
            }
            if v["end"] == true {
                *ended.lock().unwrap() = true;
            }
        };
        let target = Target::Session(&name);
        let out = watch(seen.clone(), ended.clone());
        once("ex", 80, 12, || launch("ex", &target, None, 80, 12, out)).unwrap();
        let attached = until(|| tmux(&["list-clients", "-t", &name, "-F", "#{client_name}"]).lines().count() == 1 && !seen.lock().unwrap().is_empty());
        write("ex", "typed into the executor\r").unwrap();
        let reached = until(|| tmux(&["capture-pane", "-p", "-t", &pane]).contains("typed into the executor"));
        let elsewhere = tmux(&["capture-pane", "-p", "-t", &other]);
        close("ex");
        let detached = until(|| tmux(&["list-clients", "-t", &name, "-F", "#{client_name}"]).trim().is_empty());
        let heard_end = until(|| *ended.lock().unwrap());
        let after = tmux(&["list-sessions", "-F", "#{session_name}"]);
        // Its session gone: a session that only starts with its name is never attached instead.
        tmux(&["kill-session", "-t", &format!("={name}")]);
        let (seen2, ended2) = (Arc::new(M::new(String::new())), Arc::new(M::new(false)));
        let out2 = watch(seen2.clone(), ended2.clone());
        let r = once("ex2", 80, 12, || launch("ex2", &target, None, 80, 12, out2));
        let gone_ends = r.is_ok() && until(|| *ended2.lock().unwrap());
        let longer_clients = tmux(&["list-clients", "-t", &format!("={longer}"), "-F", "#{client_name}"]);
        close("ex2");
        tmux(&["kill-session", "-t", &format!("={longer}")]);
        assert!(attached, "one client, on its own session");
        assert!(reached, "what you type reaches it");
        assert!(!elsewhere.contains("typed into the executor"), "and nothing else: {elsewhere:?}");
        assert!(detached && heard_end, "closing ends the client");
        assert!(after.lines().any(|l| l == name), "its session keeps running: {after:?}");
        assert!(gone_ends, "a session that's gone ends the terminal: {:?}", seen2.lock().unwrap());
        assert!(longer_clients.trim().is_empty(), "never the longer-named session: {longer_clients:?}");
    }

    #[test]
    fn a_session_without_a_pane_is_never_opened_as_whichever_pane_is_current() {
        let err = open_with("none", "", None, 80, 12, |_| {}).unwrap_err();
        assert!(err.contains("isn't in a tmux pane"), "{err}");
        assert!(open_with("none", "  ", Some("build-box"), 80, 12, |_| {}).is_err(), "nor on a machine");
    }

    #[test]
    fn the_built_in_terminal_shows_a_session_takes_what_you_type_and_leaves_it_running() {
        if !crate::terminal::focus::tmux_installed() {
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

/// Timing on a real machine, by hand: CUE_PERF_HOST=user@host cargo test -q perf -- --ignored --nocapture.
/// A throwaway tmux session there (removed after), opened in the built-in terminal five times: how long
/// until its screen shows; then the closed strip's screen check, five times.
#[cfg(test)]
mod perf {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    #[ignore]
    fn remote_terminal_timing() {
        let Ok(host) = std::env::var("CUE_PERF_HOST") else { return };
        let name = format!("cue-perf-{}", std::process::id());
        // CUE_PERF_PANE: an existing pane instead (left as it is), until its screen shows CUE_PERF_SEE.
        let own = std::env::var("CUE_PERF_PANE").is_err();
        let pane = match std::env::var("CUE_PERF_PANE") {
            Ok(p) => p,
            Err(_) => crate::machines::tmux(&host, &["new-session", "-d", "-s", &name, "-x", "80", "-y", "12", "-P", "-F", "#{pane_id}", "sh", "-c", "echo PERF-MARK; cat"]).unwrap().trim().to_string(),
        };
        let mark = std::env::var("CUE_PERF_SEE").unwrap_or("PERF-MARK".into());
        let mut opens = vec![];
        for _ in 0..5 {
            let seen = Arc::new(Mutex::new(String::new()));
            let s2 = seen.clone();
            let t0 = Instant::now();
            open_with("p", &pane, Some(&host), 80, 12, move |v| {
                if let Some(d) = v["data"].as_str() {
                    s2.lock().unwrap().push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
                }
            })
            .unwrap();
            while !seen.lock().unwrap().contains(&mark) && t0.elapsed() < Duration::from_secs(15) {
                std::thread::sleep(Duration::from_millis(5));
            }
            opens.push(t0.elapsed().as_millis());
            close("p");
            std::thread::sleep(Duration::from_millis(300));
        }
        let mut checks = vec![];
        for _ in 0..5 {
            let t0 = Instant::now();
            let _ = crate::machines::tmux(&host, &["capture-pane", "-p", "-J", "-t", &pane]);
            checks.push(t0.elapsed().as_millis());
        }
        if own {
            let _ = crate::machines::tmux(&host, &["kill-session", "-t", &name]);
        }
        println!("PERF open terminal until its screen shows (ms): {opens:?}");
        println!("PERF screen check (ms): {checks:?}");
    }
}

/// Views on a real machine, by hand: CUE_VIEW_HOST=host cargo test -q views_on_a_machine -- --ignored --nocapture.
/// Cue's own terminal opened and closed on a throwaway tmux session there ("cue-vt-<pid>", removed after),
/// next to a view an earlier run left with the same name Cue would once have used: each opening finds the
/// session (not a view), targets it, and leaves no view behind once closed, however the machine's SSH
/// server treats a dropped client.
#[cfg(test)]
mod views {
    use super::*;
    use std::time::{Duration, Instant};

    fn views(host: &str, mine: &str) -> Vec<String> {
        crate::machines::tmux(host, &["list-sessions", "-F", "#{session_name}"]).unwrap_or_default().lines().filter(|l| l.starts_with(mine)).map(String::from).collect()
    }

    #[test]
    #[ignore]
    fn views_on_a_machine() {
        let Ok(host) = std::env::var("CUE_VIEW_HOST") else { return };
        let s = format!("cue-vt-{}", std::process::id());
        let mine = view_prefix().to_string();
        let pane = crate::machines::tmux(&host, &["new-session", "-d", "-s", &s, "-x", "80", "-y", "12", "-P", "-F", "#{pane_id}", "sh", "-c", "echo VT-MARK; cat"]).unwrap().trim().to_string();
        // A view an earlier Cue left there, named as views were before (it counted from 1 every launch).
        let _ = crate::machines::tmux(&host, &["new-session", "-d", "-t", &s, "-s", "cue-view-1"]);
        let mut seen_targets = vec![];
        for round in 1..=3 {
            let t0 = Instant::now();
            open_with("vt", &pane, Some(&host), 80, 12, |_| {}).unwrap_or_else(|e| panic!("round {round}: {e}"));
            let open_now = loop {
                let v = views(&host, &mine);
                if !v.is_empty() || t0.elapsed() > Duration::from_secs(10) {
                    break v;
                }
                std::thread::sleep(Duration::from_millis(100));
            };
            assert_eq!(open_now.len(), 1, "round {round}: one view of this run open, got {open_now:?}");
            // What the view was grouped with: the session itself, never an earlier view.
            seen_targets.push(crate::machines::tmux(&host, &["display-message", "-p", "-t", &format!("={}:", open_now[0]), "#{session_group}"]).unwrap_or_default().trim().to_string());
            close("vt");
            let t1 = Instant::now();
            while !views(&host, &mine).is_empty() && t1.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(100));
            }
            let left = views(&host, &mine);
            println!("VT round {round}: opened {:?}, after close {left:?}", open_now);
            assert!(left.is_empty(), "round {round}: closing left {left:?} on the machine");
            // Everything else there is as it was: the session and the earlier run's view.
            let all = views(&host, "");
            assert!(all.contains(&s) && all.iter().any(|v| v == "cue-view-1"), "round {round}: closing took more than its view: {all:?}");
        }
        let clients = crate::machines::tmux(&host, &["list-clients", "-F", "#{client_session}"]).unwrap_or_default();
        let _ = crate::machines::tmux(&host, &["kill-session", "-t", "=cue-view-1"]);
        let _ = crate::machines::tmux(&host, &["kill-session", "-t", &format!("={s}")]);
        println!("VT grouped with: {seen_targets:?}; clients left: {:?}", clients.lines().collect::<Vec<_>>());
        assert!(seen_targets.iter().all(|t| *t == s), "every view grouped with {s}, got {seen_targets:?}");
        // No client of Cue's stays behind: not on its views, and never moved onto the session itself or the
        // earlier run's view (where tmux would move it first: the session before, by name).
        assert!(!clients.lines().any(|l| l.starts_with("cue-view-") || l == s), "clients left behind: {clients:?}");
    }
}
