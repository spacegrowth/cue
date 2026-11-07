//! The soak (stability plan §5): a World served on its socket, under the load of a busy day for a long
//! while, judged by xtask::soak's detectors. `tests/run.sh soak [minutes]` runs it, with the e2e soak
//! against the real app beside it; alone: `cargo test -- --ignored soak::soak --nocapture`.
//!
//! The load: CUE_SOAK_SESSIONS sessions (40) each a scripted agent (agents.rs); bursts of hook events up
//! to 200 a second; ten typists, a message a second each; the window's poll of Cue's state five times a
//! second; a machine whose line drops every 30 s (CUE_CHAOS_SSH_DROP_EVERY). The chaos knobs (crate::chaos)
//! slow the World's fakes down as they slow the real adapters: tmux (the FakeTerminal), Notification
//! Center (FakeNotes), the database (held by another connection), the disk (transcript reads).
//!
//! Measured (tally.rs) into CUE_SOAK_OUT (JSON): the poll's answer times, resident memory and the state's
//! size every 10 s, each decision's trip to its hook, every panic, and Cue's own probe (lock waits by
//! lock, reentries). Not here: the main thread and the built-in terminal's PTY, which only the real app
//! has (tests/e2e/soak.js).

mod agents;
mod load;
mod tally;
mod wire;

use crate::chaos::{self, Knob};
use crate::testing::{Client, FakeSsh, World};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tally::Tally;

/// The run as the environment sets it.
struct Plan {
    length: Duration,
    sessions: usize,
    out: PathBuf,
}

impl Plan {
    fn from_env() -> Plan {
        let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0);
        let out = std::env::var_os("CUE_SOAK_OUT").map_or_else(|| std::env::temp_dir().join("cue-soak-rust.json"), PathBuf::from);
        Plan { length: Duration::from_secs_f64(num("CUE_SOAK_MINUTES").unwrap_or(10.0) * 60.0), sessions: num("CUE_SOAK_SESSIONS").unwrap_or(40.0) as usize, out }
    }

    /// Warm-up: a fifth of the run, at least a minute, at most half of it.
    fn warmup(&self) -> Duration {
        (self.length / 5).max(Duration::from_secs(60)).min(self.length / 2)
    }
}

/// What every worker shares.
#[derive(Clone)]
pub struct Ctx {
    sock: PathBuf,
    home: PathBuf,
    client: Client,
    tally: Arc<Tally>,
    sessions: usize,
    until: Instant,
    latest: Arc<Mutex<Value>>,
}

impl Ctx {
    fn going(&self) -> bool {
        Instant::now() < self.until
    }

    /// A hook event told over the socket, counted.
    fn tell(&self, msg: &Value) {
        let ok = wire::tell(&self.sock, msg);
        tally::add(if ok { &self.tally.counts.events_sent } else { &self.tally.counts.events_failed });
    }

    fn latest_card(&self, sid: &str) -> Option<String> {
        load::waiting_card(&crate::locking::lock(&self.latest, "soak latest state"), sid)
    }
}

/// A small, seeded dice: the soak's choices differ between sessions and come out the same each run.
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Dice {
        Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// 0..n.
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// The World's fakes slowed as the chaos knobs say, and cue.db held by another connection when told to.
fn chaos_on(w: &World) {
    if let Some(ms) = chaos::get(Knob::TmuxDelayMs) {
        w.terminal.lags(ms);
    }
    if let Some(ms) = chaos::get(Knob::XpcDelayMs) {
        w.notes.lags(ms);
    }
    chaos::db::start();
}

/// Every worker started, each on a thread that reads the World's clock.
fn start(ctx: &Ctx, ssh: &FakeSsh) -> Vec<std::thread::JoinHandle<()>> {
    let mut workers = Vec::new();
    let mut go = |f: Box<dyn FnOnce(Client) + Send>| workers.push(ctx.client.spawn(f));
    for n in 0..ctx.sessions {
        let c = ctx.clone();
        go(Box::new(move |_| agents::run(&c, n)));
    }
    for n in 0..10 {
        let c = ctx.clone();
        go(Box::new(move |_| load::typist(&c, n)));
    }
    let (b, p) = (ctx.clone(), ctx.clone());
    go(Box::new(move |_| load::bursts(&b)));
    go(Box::new(move |_| load::poller(&p)));
    go(Box::new(load::machine(ctx, ssh, chaos::get(Knob::SshDropEvery).unwrap_or(30))));
    workers
}

/// The World's own work while the run lasts, as the app's tickers do it: its follow-up work as it falls
/// due, and the watcher's tick every second (usage every tenth).
fn keep_time(w: &World, ctx: &Ctx) {
    let mut ticks = 0u64;
    let mut ticked = Instant::now();
    while ctx.going() {
        w.settle();
        if ticked.elapsed() >= Duration::from_secs(1) {
            ticks += 1;
            w.tick(ticks % 10 == 0);
            ticked = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "the soak runs for minutes: tests/run.sh soak"]
fn soak() {
    let plan = Plan::from_env();
    let w = World::new();
    let served = w.serve();
    let ssh = FakeSsh::new();
    chaos_on(&w);
    tally::panics::catch();
    let ctx = Ctx { sock: crate::server::socket_path(), home: w.home().to_path_buf(), client: w.client(), tally: Arc::new(Tally::new()), sessions: plan.sessions, until: Instant::now() + plan.length, latest: Arc::default() };
    let workers = start(&ctx, &ssh);
    keep_time(&w, &ctx);
    for t in workers {
        let _ = t.join();
    }
    let probe = crate::server::inbound::probe();
    let about = json!({"run": "rust", "sessions": plan.sessions, "warmup_s": plan.warmup().as_secs_f64(), "answer_what": "the state over the socket (list)", "chaos": probe["chaos"]});
    let run = ctx.tally.json(about, probe, tally::panics::seen());
    drop(served);
    std::fs::write(&plan.out, serde_json::to_string(&run).unwrap()).unwrap();
    let run = xtask::soak::Run(run);
    println!("{}\nmeasurements: {}", xtask::soak::page(std::slice::from_ref(&run)), plan.out.display());
    assert!(run.failures().is_empty(), "the soak failed: {:#?}", run.failures());
}
