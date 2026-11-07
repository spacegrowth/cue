//! The soak's load beside its sessions: bursts of hook events, typing from you (replies to finished turns,
//! messages to sessions at work), the window polling Cue's state, and a machine whose line drops.

use super::agents::Session;
use super::tally::add;
use super::{wire, Ctx, Dice};
use crate::machines::{self, hooks};
use crate::testing::{FakeSsh, RUNS_IT};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// A burst of hook events every BURST_EVERY: BURST_SIZE of them within a second.
const BURST_EVERY: Duration = Duration::from_secs(5);
const BURST_SIZE: usize = 200;
/// The window asks for Cue's state this often; memory and the state's size are sampled every SAMPLE_EVERY.
const POLL_EVERY: Duration = Duration::from_millis(200);
const SAMPLE_EVERY: Duration = Duration::from_secs(10);

/// Bursts of hook events: status-line costs, and prompts again from sessions, up to 200 a second.
pub fn bursts(ctx: &Ctx) {
    let sessions: Vec<Session> = (0..ctx.sessions).map(|n| Session::new(ctx, n)).collect();
    let mut dice = Dice::new(7);
    while ctx.going() {
        for k in 0..BURST_SIZE {
            let s = &sessions[dice.below(sessions.len())];
            let msg = if k % 2 == 0 { json!({"type": "usage", "session_id": s.sid, "cost": k as f64 / 100.0}) } else { s.message(json!({"type": "event", "event": "compacting", "message": "auto"})) };
            ctx.tell(&msg);
            std::thread::sleep(Duration::from_secs(1) / BURST_SIZE as u32);
        }
        std::thread::sleep(BURST_EVERY - Duration::from_secs(1));
    }
}

/// One typist: a message a second, to a random session. Its finished turn's card gets a reply; a session
/// at work gets a message (held until its turn ends).
pub fn typist(ctx: &Ctx, n: u64) {
    let mut dice = Dice::new(100 + n);
    while ctx.going() {
        std::thread::sleep(Duration::from_secs(1));
        let sid = format!("soak-{}", dice.below(ctx.sessions));
        let card = ctx.latest_card(&sid);
        let said = match card {
            Some(id) => ctx.client.ask(json!({"type": "reply", "id": id, "text": format!("go on, {sid}")})),
            None => ctx.client.send_to(&sid, &format!("also, {sid}"), false),
        };
        add(&ctx.tally.counts.typing_sent);
        if said["ok"] != true {
            ctx.tally.refused(said["detail"].as_str().unwrap_or("no reason given"));
        }
    }
}

/// The window's poll of Cue's state, timed; every SAMPLE_EVERY, memory and the state's size.
pub fn poller(ctx: &Ctx) {
    let mut sampled: Option<Instant> = None;
    while ctx.going() {
        let t0 = Instant::now();
        match wire::call(&ctx.sock, &json!({"type": "list"})) {
            Some((state, bytes)) => {
                ctx.tally.answered(t0.elapsed().as_secs_f64() * 1000.0);
                add(&ctx.tally.counts.polls);
                *crate::locking::lock(&ctx.latest, "soak latest state") = state;
                if sampled.is_none_or(|t| t.elapsed() >= SAMPLE_EVERY) {
                    ctx.tally.sample(crate::locking::stats::resident_bytes(), bytes);
                    sampled = Some(Instant::now());
                }
            }
            None => add(&ctx.tally.counts.events_failed),
        }
        std::thread::sleep(POLL_EVERY.saturating_sub(t0.elapsed()));
    }
}

/// The machine's watcher, as it reports its sessions' hooks: a prompt or a turn's end a second, for
/// `drop_every` seconds, then the line drops (ssh's exit 255). Everything else Cue runs there runs for real.
fn machine_script(drop_every: u64) -> String {
    format!(
        r#"case "$1" in
  *cue-watch*) echo @@hb; i=0; while [ $i -lt {drop_every} ]; do i=$((i+1)); s=r$((i % 3))
    if [ $((i % 2)) -eq 0 ]; then ev=prompt; f='"prompt":"go"'; else ev=stop; f='"last_assistant_message":"done"'; fi
    printf '@@%s-%s%s {{"event":"%s","harness":"claude","pane":"","tmux":"","input":{{"session_id":"%s","cwd":"/home/v/app","transcript_path":"/home/v/%s.jsonl",%s}}}}\n' "$(date +%s)" "$$" "$i" "$ev" "$s" "$s" "$f"
    sleep 1; done; exit 255 ;;
  *cue-copy*) exit 0 ;;
esac
{RUNS_IT}"#
    )
}

/// A machine (+ New → Machine) on `ssh`, its watcher started again each time its line drops.
pub fn machine(ctx: &Ctx, ssh: &FakeSsh, drop_every: u64) -> impl FnOnce(crate::testing::Client) + Send + 'static {
    ssh.host("soak-box", &machine_script(drop_every));
    let m = machines::add("soak-box", "").expect("the soak's machine added");
    let ctx = ctx.clone();
    move |_| {
        let stop = AtomicBool::new(false);
        while ctx.going() {
            let _ = hooks::stream(&m, &stop);
            add(&ctx.tally.counts.machine_drops);
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// The card waiting for you from session `sid` in the state last polled, if there is one.
pub fn waiting_card(state: &Value, sid: &str) -> Option<String> {
    state["items"].as_array()?.iter().find(|i| i["session_id"] == sid && i["kind"] == "waiting").and_then(|i| i["id"].as_str()).map(String::from)
}
