//! The host: a socket only you can use, the sessions it runs, and the log each keeps. Clients (Cue on this Mac,
//! or `connect` over SSH for another) attach to sessions and get every record as it's kept; any number may be
//! connected, each sees everything, and none of them owns a child: the children run whether anyone's connected
//! or not.
//!
//! With no child running and no client connected for a while, it takes its socket away and exits; the next client
//! starts a new one (daemon.rs).

use crate::child::{Child, Said};
use crate::log::{self, Log};
use crate::proto::{Envelope, Event, FromHost, Hello, Record, Request, SessionInfo, What, VERSION};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long the host stays with nothing to do (no child running, no client connected).
pub const IDLE: Duration = Duration::from_secs(10 * 60);
/// A session's log nothing has been added to for this long is dropped when a host starts.
const FORGOTTEN: Duration = Duration::from_secs(30 * 24 * 3600);
/// How many of a session's last written messages are remembered, to refuse one sent again.
const MSGS_KEPT: usize = 100;

pub struct Host {
    home: PathBuf,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    clients: AtomicUsize,
    next_client: AtomicU64,
    /// When something last happened: a client came or went, a child exited.
    active: Mutex<Instant>,
    limit: u64,
}

struct Session {
    id: String,
    dir: PathBuf,
    meta: Mutex<Value>,
    state: Mutex<State>,
    child: Mutex<Option<Arc<Child>>>,
}

struct State {
    log: Log,
    next_seq: u64,
    running: bool,
    pid: Option<i32>,
    code: Option<i32>,
    /// Its latest run's first record.
    run_first: u64,
    /// The clients attached, by their number, each with where its lines go.
    subs: Vec<(u64, Sender<String>)>,
    msgs: VecDeque<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Host {
    /// The host for `home`, with the sessions an earlier host there left (each ended with it, if it was running).
    pub fn new(home: &Path, limit: u64) -> Arc<Host> {
        let host = Arc::new(Host { home: home.to_path_buf(), sessions: Mutex::default(), clients: AtomicUsize::new(0), next_client: AtomicU64::new(1), active: Mutex::new(Instant::now()), limit });
        host.load();
        host
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home.join("sessions")
    }

    fn load(&self) {
        let Ok(dirs) = std::fs::read_dir(self.sessions_dir()) else { return };
        for d in dirs.flatten() {
            // A session nothing has happened in for a month: its log goes (Cue forgot it long ago).
            let quiet = std::fs::metadata(d.path().join("log.jsonl")).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > FORGOTTEN);
            if quiet {
                let _ = std::fs::remove_dir_all(d.path());
                continue;
            }
            let id = d.file_name().to_string_lossy().into_owned();
            let meta: Value = std::fs::read(d.path().join("meta.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
            let Ok(s) = Session::open(&id, &d.path(), meta, self.limit) else { continue };
            // A child that was running when its host went went with it: its log says so.
            let ended = log::read_all(&d.path()).last().is_none_or(|r| matches!(r.what, What::Exited(_)));
            if !ended {
                s.append(What::Exited(None));
            }
            lock(&self.sessions).insert(id, Arc::new(s));
        }
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        lock(&self.sessions).get(id).cloned().ok_or_else(|| format!("no session {id} here"))
    }

    fn touch(&self) {
        *lock(&self.active) = Instant::now();
    }

    /// Whether it has nothing to do and hasn't for `idle`.
    fn idle_for(&self, idle: Duration) -> bool {
        self.clients.load(Ordering::SeqCst) == 0 && !lock(&self.sessions).values().any(|s| lock(&s.state).running) && lock(&self.active).elapsed() >= idle
    }

    /// One request, answered: what goes back to the client. `client`/`to` say where its attached sessions' records go.
    pub fn handle(self: &Arc<Self>, e: Envelope, client: u64, to: &Sender<String>) -> FromHost {
        let id = e.id;
        match self.apply(e.req, client, to) {
            Ok(v) => FromHost::ok(id, v),
            Err(why) => FromHost::err(id, why),
        }
    }

    fn apply(self: &Arc<Self>, req: Request, client: u64, to: &Sender<String>) -> Result<Value, String> {
        match req {
            Request::Hello { .. } => Ok(serde_json::to_value(self.hello()).unwrap_or_default()),
            Request::Start { session, argv, env, cwd, meta } => self.start(&session, &argv, &env, &cwd, meta).map(|(pid, after)| json!({ "pid": pid, "after": after })),
            Request::Write { session, line, msg } => self.session(&session)?.write(&line, msg).map(|wrote| json!({ "written": wrote })),
            Request::Attach { session, after } => {
                self.session(&session)?.attach(after, client, to.clone());
                Ok(Value::Null)
            }
            Request::Read { session, after, upto } => Ok(serde_json::to_value(self.session(&session)?.read(after, upto)).unwrap_or_default()),
            Request::Detach { session } => {
                self.session(&session)?.detach(client);
                Ok(Value::Null)
            }
            Request::Close { session } => {
                self.session(&session)?.close();
                Ok(Value::Null)
            }
            Request::Forget { session } => {
                let s = lock(&self.sessions).remove(&session);
                if let Some(s) = s {
                    s.close();
                    let _ = std::fs::remove_dir_all(&s.dir);
                }
                Ok(Value::Null)
            }
        }
    }

    pub fn hello(&self) -> Hello {
        let mut sessions: Vec<SessionInfo> = lock(&self.sessions).values().map(|s| s.info()).collect();
        sessions.sort_by(|a, b| a.session.cmp(&b.session));
        Hello { version: VERSION, pid: std::process::id() as i32, sessions }
    }

    /// A child for `id`: refused while one runs under that id; one that has run before carries its log on. Its pid,
    /// and the last record before it (attach after that for what it says).
    fn start(self: &Arc<Self>, id: &str, argv: &[String], env: &[(String, String)], cwd: &str, meta: Value) -> Result<(i32, u64), String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("“{id}” isn't a session id"));
        }
        let s = {
            let mut all = lock(&self.sessions);
            if let Some(s) = all.get(id) {
                if lock(&s.state).running {
                    return Err(format!("{id} is already running"));
                }
                s.clone()
            } else {
                let dir = self.sessions_dir().join(id);
                let s = Arc::new(Session::open(id, &dir, meta.clone(), self.limit).map_err(|e| format!("couldn't keep its log: {e}"))?);
                all.insert(id.to_string(), s.clone());
                s
            }
        };
        *lock(&s.meta) = meta.clone();
        let _ = std::fs::write(s.dir.join("meta.json"), meta.to_string());
        // Marked running before it can say anything, so its first lines and its exit find it so.
        let mut st = lock(&s.state);
        st.running = true;
        st.code = None;
        st.run_first = st.next_seq;
        let after = st.next_seq - 1;
        drop(st);
        let (said_to, host) = (s.clone(), Arc::downgrade(self));
        let child = Child::start(argv, env, cwd, move |said| match said {
            Said::Out(l) => said_to.append(What::Out(l)),
            Said::Err(l) => said_to.append(What::Err(l)),
            Said::Exited(code) => {
                said_to.exited(code);
                if let Some(h) = host.upgrade() {
                    h.touch();
                }
            }
        });
        match child {
            Ok(c) => {
                let pid = c.pid;
                // Kept only while it runs: one that exited at once has already been put away (exited()).
                let mut st = lock(&s.state);
                if st.running {
                    st.pid = Some(pid);
                    *lock(&s.child) = Some(Arc::new(c));
                }
                Ok((pid, after))
            }
            Err(e) => {
                lock(&s.state).running = false;
                Err(format!("couldn't start it: {e}"))
            }
        }
    }

    /// Serve on `listener` until idle for `idle`: then its socket at `sock` is taken away and this returns.
    pub fn serve(self: &Arc<Self>, listener: UnixListener, sock: &Path, idle: Duration) {
        use std::os::fd::AsRawFd;
        // Not blocking: between connections it looks every second whether it's time to go.
        if listener.set_nonblocking(true).is_err() {
            return;
        }
        loop {
            if self.idle_for(idle) {
                // The socket first: a client coming now finds none and starts a new host.
                let _ = std::fs::remove_file(sock);
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    // A connection takes the listener's non-blocking on some systems: its own reads block.
                    if stream.set_nonblocking(false).is_err() || !same_user(&stream) {
                        eprintln!("cue-host: refused a connection (not this user, or unusable)");
                        continue;
                    }
                    let host = self.clone();
                    std::thread::spawn(move || host.client(stream));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut p = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
                    // SAFETY: poll on one descriptor this process holds, for up to a second.
                    unsafe { libc::poll(&mut p, 1, 1000) };
                }
                Err(e) => {
                    eprintln!("cue-host: accept failed: {e}");
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    /// One client: its requests answered in order, its attached sessions' records sent as they come.
    pub fn client(self: &Arc<Self>, stream: UnixStream) {
        let n = self.next_client.fetch_add(1, Ordering::SeqCst);
        self.clients.fetch_add(1, Ordering::SeqCst);
        self.touch();
        let (to, lines) = channel::<String>();
        if let Ok(mut out) = stream.try_clone() {
            std::thread::spawn(move || {
                for mut l in lines {
                    l.push('\n');
                    if out.write_all(l.as_bytes()).is_err() {
                        break;
                    }
                }
            });
        }
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
            let answer = match serde_json::from_slice::<Envelope>(&buf) {
                Ok(e) => self.handle(e, n, &to),
                Err(e) => FromHost::err(serde_json::from_slice::<Value>(&buf).ok().and_then(|v| v["id"].as_u64()).unwrap_or(0), format!("not a request: {e}")),
            };
            buf.clear();
            if to.send(answer.line()).is_err() {
                break;
            }
        }
        for s in lock(&self.sessions).values() {
            s.detach(n);
        }
        self.clients.fetch_sub(1, Ordering::SeqCst);
        self.touch();
    }
}

impl Session {
    fn open(id: &str, dir: &Path, meta: Value, limit: u64) -> std::io::Result<Session> {
        let log = Log::open(dir, limit)?;
        let next_seq = log.last().map_or(1, |l| l + 1);
        let records = log.records();
        let code = records.last().and_then(|r| if let What::Exited(c) = r.what { c } else { None });
        let state = State { log, next_seq, running: false, pid: None, code, run_first: run_first(&records), subs: vec![], msgs: VecDeque::new() };
        Ok(Session { id: id.to_string(), dir: dir.to_path_buf(), meta: Mutex::new(meta), state: Mutex::new(state), child: Mutex::new(None) })
    }

    fn info(&self) -> SessionInfo {
        let st = lock(&self.state);
        SessionInfo { session: self.id.clone(), running: st.running, pid: st.pid.filter(|_| st.running), code: st.code, first_seq: st.log.first().unwrap_or(st.next_seq), last_seq: st.next_seq - 1, run_first: st.run_first, meta: lock(&self.meta).clone() }
    }

    /// The next record: numbered, kept, and sent to every client attached. Kept even if the disk won't take it (the
    /// clients still get it; it just can't be read back later).
    fn append(&self, what: What) {
        let mut st = lock(&self.state);
        let record = Record { seq: st.next_seq, what };
        st.next_seq += 1;
        if let Err(e) = st.log.append(&record) {
            eprintln!("cue-host: couldn't keep {}'s record {}: {e}", self.id, record.seq);
        }
        let line = FromHost::Event { session: self.id.clone(), event: Event::Record { record } }.line();
        st.subs.retain(|(_, to)| to.send(line.clone()).is_ok());
    }

    fn exited(&self, code: Option<i32>) {
        self.append(What::Exited(code));
        let mut st = lock(&self.state);
        st.running = false;
        st.pid = None;
        st.code = code;
        drop(st);
        lock(&self.child).take();
    }

    /// `line` to its child, and kept. A `msg` written before isn't written again: Ok(false).
    fn write(&self, line: &str, msg: Option<String>) -> Result<bool, String> {
        if let Some(m) = &msg {
            let mut st = lock(&self.state);
            if st.msgs.contains(m) {
                return Ok(false);
            }
            if st.msgs.len() == MSGS_KEPT {
                st.msgs.pop_front();
            }
            st.msgs.push_back(m.clone());
        }
        let child = lock(&self.child).clone().ok_or_else(|| format!("{} isn't running", self.id))?;
        child.write(line).map_err(|e| format!("it isn't taking input: {e}"))?;
        self.append(What::In(line.to_string()));
        Ok(true)
    }

    /// Everything after `after`, then what comes: under the state's lock, so nothing is missed or sent twice.
    fn attach(&self, after: u64, client: u64, to: Sender<String>) {
        let mut st = lock(&self.state);
        st.subs.retain(|(c, _)| *c != client);
        let first = st.log.first().unwrap_or(st.next_seq);
        if first > after + 1 {
            let _ = to.send(FromHost::Event { session: self.id.clone(), event: Event::Gap { from: first } }.line());
        }
        for record in st.log.records().into_iter().filter(|r| r.seq > after) {
            let _ = to.send(FromHost::Event { session: self.id.clone(), event: Event::Record { record } }.line());
        }
        st.subs.push((client, to));
    }

    /// The records after `after` up to `upto`.
    fn read(&self, after: u64, upto: u64) -> Vec<Record> {
        lock(&self.state).log.records().into_iter().filter(|r| r.seq > after && r.seq <= upto).collect()
    }

    fn detach(&self, client: u64) {
        lock(&self.state).subs.retain(|(c, _)| *c != client);
    }

    fn close(&self) {
        let child = lock(&self.child).clone();
        if let Some(c) = child {
            c.close();
        }
    }
}

/// Where the latest run in `records` began: the record after the last exit before its end (the first, if none).
fn run_first(records: &[Record]) -> u64 {
    let ended_before = records.iter().rev().skip(1).find(|r| matches!(r.what, What::Exited(_)));
    ended_before.map_or_else(|| records.first().map_or(1, |r| r.seq), |r| r.seq + 1)
}

/// Whether the other end of `s` is this user.
fn same_user(s: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let fd = s.as_raw_fd();
    // SAFETY: plain calls on a socket this process holds, into locals.
    let uid = unsafe {
        #[cfg(target_os = "linux")]
        {
            let mut cred: libc::ucred = std::mem::zeroed();
            let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
            if libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len) != 0 {
                return false;
            }
            cred.uid
        }
        #[cfg(not(target_os = "linux"))]
        {
            let (mut uid, mut gid) = (0, 0);
            if libc::getpeereid(fd, &mut uid, &mut gid) != 0 {
                return false;
            }
            uid
        }
    };
    // SAFETY: getuid has no failure.
    uid == unsafe { libc::getuid() }
}
