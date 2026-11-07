// The e2e soak (stability plan §5): the real debug app on a private Cue (its own CUE_HOME, HOME and tmux
// server), under a busy day's load (load.js) for CUE_SOAK_MINUTES (10), with CUE_SOAK_SESSIONS sessions
// (40). It measures into CUE_SOAK_OUT; `tests/run.sh soak` judges that with the World soak's run
// (`cargo run -p xtask -- soak-report`). Run alone: tests/e2e/run.sh soak/soak.test.js (measures only).
// Chaos knobs (CUE_CHAOS_*) set for this process reach the app: it honours them, being a debug build.
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { waitFor } from "../lib/poll.js";
import { tell } from "../lib/socket.js";
import { World } from "../lib/world.js";
import { agent, bursts, makeSession, mainThread, poller, terminals, typist } from "./load.js";
import { Tally } from "./tally.js";

const MINUTES = Number(process.env.CUE_SOAK_MINUTES || 10);
const SESSIONS = Number(process.env.CUE_SOAK_SESSIONS || 40);
const OUT = process.env.CUE_SOAK_OUT || path.join(process.env.CUE_E2E_ROOT || "/tmp", "cue-soak-e2e.json");

/** What every part of the load shares. */
class Soak {
  constructor(world, until) {
    this.world = world;
    this.until_ = until;
    this.tally = new Tally();
    this.latest = { items: [] };
  }

  going() {
    return Date.now() < this.until_;
  }

  /** A hook event told over the socket, counted. */
  async tell(msg) {
    try {
      await tell(this.world.sock, msg);
      this.tally.counts.eventsSent++;
    } catch {
      this.tally.counts.eventsFailed++;
    }
  }

  /** Session `sid`'s card that `which` picks, in the state last polled. */
  card(sid, which) {
    return this.latest.items.find((i) => i.session_id === sid && which(i));
  }

  /** `probe` truthy within `ms`: its value, or null. */
  until(probe, ms) {
    return waitFor(probe, "", { timeout: ms, every: 100 }).catch(() => null);
  }
}

/** The CUE_CHAOS_* knobs set for this run, as the app reads them. */
function chaos() {
  return Object.fromEntries(Object.entries(process.env).filter(([k, v]) => k.startsWith("CUE_CHAOS_") && Number(v) > 0).map(([k, v]) => [k, Number(v)]));
}

/** What Cue logged that the detectors read: panics (cue.log) and lock reports (locks.log). */
function logs(world) {
  const read = (f) => (fs.existsSync(f) ? fs.readFileSync(f, "utf8").split("\n") : []);
  const panics = read(path.join(world.root, "cue.log")).filter((l) => l.includes("panicked at"));
  const log = read(path.join(world.cueHome, "locks.log")).filter((l) => /LOCK (REENTRY|WAIT)|main thread stalled/.test(l));
  return { panics, log };
}

test(`e2e soak: ${SESSIONS} sessions for ${MINUTES} min`, { timeout: (MINUTES + 5) * 60_000 }, async () => {
  const world = new World("soak");
  let kept = true;
  try {
    await world.launch(chaos());
    const soak = new Soak(world, Date.now() + MINUTES * 60_000);
    const sessions = Array.from({ length: SESSIONS }, (_, n) => makeSession(soak, n));
    await Promise.all([
      ...sessions.map((s, n) => agent(soak, s, n)),
      ...Array.from({ length: 10 }, (_, n) => typist(soak, sessions, n)),
      bursts(soak, sessions), poller(soak), mainThread(soak), terminals(soak, sessions),
    ]);
    const probe = await world.call({ type: "probe" });
    const { panics, log } = logs(world);
    const about = { run: "e2e", sessions: SESSIONS, warmup_s: Math.min(Math.max(MINUTES * 12, 60), MINUTES * 30), chaos: probe.chaos,
      answer_what: "the window's main thread (an evaluated script's round trip)" };
    fs.writeFileSync(OUT, JSON.stringify(soak.tally.json(about, probe, panics, log)));
    console.log(`measurements: ${OUT}`);
    kept = false;
  } finally {
    await world.stop({ keep: kept });
  }
});
