//! The soak's sessions: each a scripted agent whose hooks reach Cue over its socket, round after round:
//! a prompt, a working stretch whose transcript grows, now and then a permission ask held open until it's
//! answered (by `answer`, as you would, a moment later), then its turn's end and a pause, waiting for you.

use super::wire;
use super::{Ctx, Dice};
use crate::testing::soak::tally::add;
use crate::testing::Transcript;
use serde_json::{json, Value};
use std::time::Duration;

/// How long a hook waits for its decision before it gives up (its agent's own timeout).
const HOOK_WAITS: Duration = Duration::from_secs(30);
/// One transcript entry while it works, about this many bytes, every WORK_EVERY.
const ENTRY_BYTES: usize = 1500;
const WORK_EVERY: Duration = Duration::from_millis(500);

/// Session `n` of the soak, as its hooks describe it: in tmux pane %n (Cue types into it through the World's
/// FakeTerminal), its transcript in the World's folder.
pub struct Session {
    pub sid: String,
    origin: Value,
    transcript: Transcript,
}

impl Session {
    pub fn new(ctx: &Ctx, n: usize) -> Session {
        let sid = format!("soak-{n}");
        let path = ctx.home.join("claude").join(format!("{sid}.jsonl"));
        let origin = json!({"harness": "claude", "session_id": sid, "cwd": format!("/work/soak-{}", n % 8), "transcript_path": path, "tmux_pane": format!("%{n}"), "term_program": "tmux"});
        Session { sid, origin, transcript: Transcript::at(path) }
    }

    /// This session's hook message: its origin with `fields`.
    pub fn message(&self, fields: Value) -> Value {
        let mut m = self.origin.clone();
        m.as_object_mut().expect("an object").extend(fields.as_object().expect("an object").clone());
        m
    }

    /// One of its hook events, told over the socket and counted.
    pub fn event(&self, ctx: &Ctx, event: &str, message: &str) {
        ctx.tell(&self.message(json!({"type": "event", "event": event, "message": message})));
    }
}

/// Session `n`'s rounds until the run ends.
pub fn run(ctx: &Ctx, n: usize) {
    let s = Session::new(ctx, n);
    let mut dice = Dice::new(n as u64 + 1);
    let mut round = 0;
    while ctx.going() {
        turn(ctx, &s, round, &mut dice);
        round += 1;
    }
}

fn turn(ctx: &Ctx, s: &Session, round: usize, dice: &mut Dice) {
    s.transcript.prompt(&format!("task {round}"));
    s.event(ctx, "active", &format!("task {round}"));
    for _ in 0..3 + dice.below(6) {
        s.transcript.assistant(&filler(round));
        std::thread::sleep(WORK_EVERY);
    }
    if round % 3 == 1 {
        ask(ctx, s, round);
    }
    s.transcript.assistant(&format!("Done with task {round}."));
    s.event(ctx, "stopped", &format!("Done with task {round}."));
    std::thread::sleep(Duration::from_millis(2000 + dice.below(4000) as u64));
}

/// What the agent writes while it works: about ENTRY_BYTES of text.
fn filler(round: usize) -> String {
    format!("Working on task {round}: ").repeat(ENTRY_BYTES / 24)
}

/// A permission ask held open, as Claude Code's hook holds it, until its decision comes.
fn ask(ctx: &Ctx, s: &Session, round: usize) {
    let msg = s.message(json!({"type": "ask", "kind": "permission", "tool_name": "Bash", "tool_input": {"command": format!("make test-{round}")}}));
    let Some(open) = wire::ask(&ctx.sock, &msg) else {
        add(&ctx.tally.counts.events_failed);
        return;
    };
    add(&ctx.tally.counts.asks);
    let id = open.id.clone();
    answer(ctx, id.clone());
    if open.decision(HOOK_WAITS).is_some() {
        ctx.tally.heard(&id);
    }
}

/// Answer ask `id` as you would, a moment after it shows, on a thread of its own.
fn answer(ctx: &Ctx, id: String) {
    let ctx = ctx.clone();
    ctx.client.clone().spawn(move |c| {
        let mut dice = Dice::new(id.len() as u64 + ctx.tally.at() as u64);
        std::thread::sleep(Duration::from_millis(200 + dice.below(1300) as u64));
        ctx.tally.given(&id);
        let said = c.ask(json!({"type": "respond", "id": id, "decision": {"behavior": "allow"}}));
        if said["ok"] != true {
            ctx.tally.not_given(&id);
        }
    });
}
