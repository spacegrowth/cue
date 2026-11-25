//! The terminal layer under Cue's own shells (shells.rs): a client in a pseudo-terminal, streamed to the window
//! (xterm.js draws it); what you type goes back the same way. A shell kept in tmux is a tmux client attached to
//! exactly its session (`attach_local`); closing it ends only that client.
use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use crate::terminal::input::Input;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

struct Open {
    /// Which opening it is (the "term" events' `gen`): a close names the one it means (`close_if`).
    gen: u64,
    /// Its own lock, apart from `open_terms`: a resize takes the terminal out of that and resizes it
    /// with that lock released (a resize is quick; nothing slow is ever done under this one).
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// What's typed into it goes through here, never written under `open_terms` (input.rs).
    input: Input,
    child: Box<dyn Child + Send + Sync>,
}

impl Open {
    fn new(sid: &str, gen: u64, master: Box<dyn MasterPty + Send>, child: Box<dyn Child + Send + Sync>) -> Result<Open, String> {
        let input = Input::new(sid, master.take_writer().map_err(|e| e.to_string())?)?;
        Ok(Open { gen, master: Arc::new(Mutex::new(master)), input, child })
    }

    /// End its client.
    fn end(self) {
        self.kill();
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An opening in flight: what it came to (its `gen`, or why it failed), and the opens that joined it, each
/// waiting for the answer on a channel of its own (so each can give up after JOIN_BOUND).
#[derive(Default)]
struct Pending(Mutex<Answer>);

#[derive(Default)]
struct Answer {
    result: Option<Result<u64, String>>,
    waiting: Vec<std::sync::mpsc::Sender<Result<u64, String>>>,
}

/// How long an open that joined one in flight waits for it. The first opening's SSH round trip can hang (a
/// machine asleep); the open that joined it then says so rather than waiting with it for good.
const JOIN_BOUND: Duration = Duration::from_secs(20);

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

/// A tmux session on this Mac (a shell of Cue's own: shells.rs), its terminal a client attached to
/// exactly that session. Opened, deduped and closed as `open`'s are (one per `sid`).
pub(crate) fn attach_local(sid: &str, tmux_session: &str, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<u64, String> {
    once(sid, cols, rows, || launch(sid, tmux_session, cols, rows, out))
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
    once_within(sid, cols, rows, JOIN_BOUND, launch)
}

/// `once`, an open that joins one in flight waiting `join_bound` at most.
fn once_within(sid: &str, cols: u16, rows: u16, join_bound: Duration, launch: impl FnOnce() -> Result<(u64, Open), String>) -> Result<u64, String> {
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
            let gen = joined(&p, join_bound);
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

/// An open that joined opening `p`: its answer, once it's in, or an error after `bound`.
fn joined(p: &Pending, bound: Duration) -> Result<u64, String> {
    let answer = {
        let mut a = crate::locking::lock(&p.0, "term opening");
        if let Some(r) = &a.result {
            return r.clone();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        a.waiting.push(tx);
        rx
    };
    answer.recv_timeout(bound).unwrap_or_else(|_| Err("its terminal is still opening (its machine may not be answering): try again".into()))
}

/// Hand an opening's outcome to the opens waiting on it.
fn answer(p: &Pending, result: Result<u64, String>) {
    let mut a = crate::locking::lock(&p.0, "term opening");
    for w in a.waiting.drain(..) {
        let _ = w.send(result.clone());
    }
    a.result = Some(result);
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

/// Start the client, a tmux client attached to session `name`, in a pseudo-terminal, its output streamed to `out` from
/// a thread of its own.
fn launch(sid: &str, name: &str, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<(u64, Open), String> {
    let gen = next_gen();
    let chain = attach_args(name).ok_or(format!("“{name}” isn't a tmux session name Cue will attach to"))?;
    let pair = native_pty_system().openpty(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
    let mut cmd = CommandBuilder::new(crate::terminal::focus::tmux_bin());
    for a in &chain {
        cmd.arg(a);
    }
    // Not nested in the terminal that started Cue: tmux quits a client that is ("sessions should be
    // nested with care"), and the terminal would close as it opened.
    for k in crate::terminal::shells::NOT_HERE {
        cmd.env_remove(k);
    }
    cmd.env("TERM", "xterm-256color");
    start(sid, gen, pair, cmd, out)
}

/// `cmd` in the pseudo-terminal `pair`, its output streamed to `out` from a thread of its own
/// ({sid, gen, data} as it comes, then {sid, gen, end: true}).
fn start(sid: &str, gen: u64, pair: portable_pty::PtyPair, cmd: CommandBuilder, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<(u64, Open), String> {
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let open = Open::new(sid, gen, pair.master, child)?;
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
    Ok((gen, open))
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
    t.open
        .iter()
        .filter(|(id, _)| id.starts_with(prefix))
        .map(|(id, o)| (id.clone(), o.child.process_id(), crate::locking::lock(&o.master, "pty master").process_group_leader()))
        .collect()
}

/// What you typed in it (and the terminal's own replies, as xterm.js produces them). Written with no
/// lock held and no thread waiting, in order; a terminal that isn't reading makes this an error within
/// input::BOUND, and every write after it too, until `resume`.
pub async fn write(sid: &str, data: &str) -> Result<(), String> {
    input_of(sid)?.write(data.as_bytes()).await
}

/// Typing into it again after a write didn't go (input.rs): what was waiting to go in is dropped.
pub fn resume(sid: &str) -> Result<(), String> {
    input_of(sid)?.resume();
    Ok(())
}

/// Tests: `write`, waited for on this thread.
#[cfg(test)]
pub(crate) fn type_in(sid: &str, data: &str) -> Result<(), String> {
    tauri::async_runtime::block_on(write(sid, data))
}

fn input_of(sid: &str) -> Result<Input, String> {
    let input = crate::locking::lock(open_terms(), "open_terms").open.get(sid).map(|o| o.input.clone());
    input.ok_or_else(|| "its terminal isn't open".into())
}

pub fn resize(sid: &str, cols: u16, rows: u16) -> Result<(), String> {
    let master = crate::locking::lock(open_terms(), "open_terms").open.get(sid).map(|o| o.master.clone());
    let master = master.ok_or("its terminal isn't open")?;
    let size = PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 };
    let resized = crate::locking::lock(&master, "pty master").resize(size);
    resized.map_err(|e| e.to_string())
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

/// Close the client opening `gen` gave, if it's still the one open; anything newer (an open that replaced
/// it, or one on its way) is left alone. A close and the open after it go to Cue at once (neither waits for
/// the other, ui/invoke.js), and whichever lands first, the new one stays.
pub fn close_if(sid: &str, gen: u64) {
    let open = {
        let mut t = crate::locking::lock(open_terms(), "open_terms");
        let mine = t.open.get(sid).is_some_and(|o| o.gen == gen);
        if mine { t.open.remove(sid) } else { None }
    };
    if let Some(o) = open {
        o.end();
    }
}

#[cfg(test)]
mod support;
#[cfg(test)]
mod edges;

#[cfg(test)]
mod tests {
    use super::support::*;
    use super::*;
    use std::sync::{Arc, Mutex as M};

    fn tmux(args: &[&str]) -> String {
        let o = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
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
        assert_eq!(crate::locking::lock(&starts, "starts").len(), 1, "one client, not two");
        assert_eq!(g1, g2, "the second joined the first");
        let pid1 = crate::locking::lock(&starts, "starts")[0];
        assert!(alive(pid1), "the client runs");
        // Opened again once it's open (it ended, say): the old client is ended, the new one recorded.
        let g3 = once(&sid, 20, 4, || fake(&starts, 0)).unwrap();
        assert_ne!(g3, g1);
        let pid2 = crate::locking::lock(&starts, "starts")[1];
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
        let pid3 = crate::locking::lock(&starts, "starts")[2];
        assert!(until(|| !alive(pid3)), "nobody wanted it: ended");
        assert!(type_in(&sid, "x").is_err(), "nothing open");
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
    fn typing_into_a_terminal_whose_client_stopped_reading_fails_in_time_and_holds_up_nothing_else() {
        let (stuck, fine) = (format!("stuck-{}", std::process::id()), format!("fine-{}", std::process::id()));
        let (go, release) = std::sync::mpsc::channel();
        let got = Arc::new(M::new(vec![]));
        let stuck_pid = record(&stuck, sleeper(), Some(Box::new(crate::terminal::input::tests::Stalled { release, got })));
        record(&fine, sleeper(), None);
        let s = stuck.clone();
        let t0 = std::time::Instant::now();
        let typing = std::thread::spawn(move || type_in(&s, "lost in a stopped client"));
        // While that write waits: the other terminal takes input, this one resizes, and opening and closing go on.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let (f, s2) = (fine.clone(), stuck.clone());
        assert_eq!(within(500, move || type_in(&f, "x")), Some(Ok(())), "another terminal's write isn't held");
        assert_eq!(within(500, move || resize(&s2, 30, 6)), Some(Ok(())), "a resize isn't held");
        let s3 = stuck.clone();
        assert!(within(500, move || foreground(&s3).len()) == Some(1), "the open terminals can be read");
        let err = typing.join().unwrap().unwrap_err();
        let took = t0.elapsed();
        assert!(took >= crate::terminal::input::BOUND && took < crate::terminal::input::BOUND + std::time::Duration::from_secs(1), "{took:?}");
        assert!(err.contains("isn't responding"), "{err}");
        // Still stopped: refused at once, not another wait.
        let s4 = stuck.clone();
        assert!(matches!(within(300, move || type_in(&s4, "y")), Some(Err(_))), "refused at once while stuck");
        let s5 = stuck.clone();
        assert_eq!(within(1000, move || close(&s5)), Some(()), "closing it isn't held by the stuck write");
        assert!(until(|| !alive(stuck_pid)), "its client ended");
        drop(go);
        close(&fine);
    }

    #[test]
    fn a_real_pty_whose_reader_never_reads_fills_up_and_its_write_fails_in_time() {
        let sid = format!("full-{}", std::process::id());
        // Raw mode, as a tmux client's terminal is, and nothing ever reading it: the pty's input buffer fills.
        let mut c = CommandBuilder::new("/bin/sh");
        c.args(["-c", "stty raw -echo; exec sleep 30"]);
        let pid = record(&sid, c, None);
        std::thread::sleep(std::time::Duration::from_millis(300)); // stty has run
        let chunk = "k".repeat(4096);
        let t0 = std::time::Instant::now();
        let mut failed = None;
        for _ in 0..256 {
            if let Err(e) = type_in(&sid, &chunk) {
                failed = Some(e);
                break;
            }
        }
        let took = t0.elapsed();
        close(&sid);
        assert!(until(|| !alive(pid)));
        let err = failed.expect("a megabyte into a pty nobody reads can't all go in");
        assert!(err.contains("isn't responding"), "{err}");
        assert!(took < crate::terminal::input::BOUND + std::time::Duration::from_secs(1), "{took:?}");
    }

    #[test]
    fn an_open_that_joined_a_hung_opening_gives_up_in_time_and_a_close_lets_the_next_open_start() {
        let sid = format!("hung-{}", std::process::id());
        let starts = Arc::new(M::new(vec![]));
        let (go, hang) = std::sync::mpsc::channel::<()>();
        let (a, s1) = (sid.clone(), starts.clone());
        // Its SSH round trip never comes back (until the test lets it).
        let first = std::thread::spawn(move || {
            once(&a, 20, 4, || {
                let _ = hang.recv();
                fake(&s1, 0)
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        let t0 = std::time::Instant::now();
        let joined = once_within(&sid, 20, 4, std::time::Duration::from_millis(300), || panic!("joined: never launches its own"));
        assert!(joined.unwrap_err().contains("still opening"));
        assert!(t0.elapsed() < std::time::Duration::from_secs(2), "{:?}", t0.elapsed());
        // Closed (no opening named: whatever's in flight goes), the next open launches its own at once.
        close(&sid);
        let s2 = starts.clone();
        let second = within(5000, { let sid = sid.clone(); move || once(&sid, 20, 4, || fake(&s2, 0)) });
        assert!(matches!(second, Some(Ok(_))), "not held by the hung one: {second:?}");
        go.send(()).unwrap();
        first.join().unwrap().unwrap();
        let pids = crate::locking::lock(&starts, "starts").clone();
        assert_eq!(pids.len(), 2);
        assert!(until(|| !alive(pids[1])), "the hung one's client, landing after the close, is ended");
        assert!(alive(pids[0]), "the new one runs");
        close(&sid);
    }

    #[test]
    fn a_close_that_names_an_earlier_opening_leaves_the_one_opened_since() {
        let sid = format!("closeif-{}", std::process::id());
        let starts = Arc::new(M::new(vec![]));
        let g1 = once(&sid, 20, 4, || fake(&starts, 0)).unwrap();
        // Opened again (the window closed it and opened it again, and the open landed first).
        let g2 = once(&sid, 20, 4, || fake(&starts, 0)).unwrap();
        let pids = crate::locking::lock(&starts, "starts").clone();
        close_if(&sid, g1);
        assert!(until(|| !alive(pids[0])), "the first is gone (replaced)");
        assert!(alive(pids[1]) && foreground(&sid).len() == 1, "the close of the first left the second open");
        close_if(&sid, g2);
        assert!(until(|| !alive(pids[1])), "naming it closes it");
    }

    #[test]
    fn a_terminal_attaches_to_its_own_tmux_session_by_its_whole_name() {
        assert_eq!(attach_args("run-act_2"), Some(vec!["-u".to_string(), "attach-session".into(), "-t".into(), "=run-act_2".into()]));
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
                crate::locking::lock(&seen, "seen").push_str(&String::from_utf8_lossy(&bytes));
            }
            if v["end"] == true {
                *crate::locking::lock(&ended, "ended") = true;
            }
        };
        let out = watch(seen.clone(), ended.clone());
        once("ex", 80, 12, || launch("ex", &name, 80, 12, out)).unwrap();
        let attached = until(|| tmux(&["list-clients", "-t", &name, "-F", "#{client_name}"]).lines().count() == 1 && !crate::locking::lock(&seen, "seen").is_empty());
        type_in("ex", "typed into the session\r").unwrap();
        let reached = until(|| tmux(&["capture-pane", "-p", "-t", &pane]).contains("typed into the session"));
        let elsewhere = tmux(&["capture-pane", "-p", "-t", &other]);
        close("ex");
        let detached = until(|| tmux(&["list-clients", "-t", &name, "-F", "#{client_name}"]).trim().is_empty());
        let heard_end = until(|| *crate::locking::lock(&ended, "ended"));
        let after = tmux(&["list-sessions", "-F", "#{session_name}"]);
        // Its session gone: a session that only starts with its name is never attached instead.
        tmux(&["kill-session", "-t", &format!("={name}")]);
        let (seen2, ended2) = (Arc::new(M::new(String::new())), Arc::new(M::new(false)));
        let out2 = watch(seen2.clone(), ended2.clone());
        let r = once("ex2", 80, 12, || launch("ex2", &name, 80, 12, out2));
        let gone_ends = r.is_ok() && until(|| *crate::locking::lock(&ended2, "ended2"));
        let longer_clients = tmux(&["list-clients", "-t", &format!("={longer}"), "-F", "#{client_name}"]);
        close("ex2");
        tmux(&["kill-session", "-t", &format!("={longer}")]);
        assert!(attached, "one client, on its own session");
        assert!(reached, "what you type reaches it");
        assert!(!elsewhere.contains("typed into the session"), "and nothing else: {elsewhere:?}");
        assert!(detached && heard_end, "closing ends the client");
        assert!(after.lines().any(|l| l == name), "its session keeps running: {after:?}");
        assert!(gone_ends, "a session that's gone ends the terminal: {:?}", &*crate::locking::lock(&seen2, "seen2"));
        assert!(longer_clients.trim().is_empty(), "never the longer-named session: {longer_clients:?}");
    }
}
