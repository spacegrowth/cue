// The e2e soak's load on a real Cue: sessions whose hooks reach it over its socket round after round (a
// prompt, a growing transcript, now and then a permission ask through the real hook, the turn's end),
// bursts of hook events, typing from you into their real tmux panes, the window polling Cue's state, the
// built-in terminal opened and closed, and the window's main thread timed throughout.
import fs from "node:fs";
import path from "node:path";
import { permission, runHook } from "../lib/hooks.js";
import { call } from "../lib/socket.js";

const pause = (ms) => new Promise((r) => setTimeout(r, ms));
/** A small seeded dice: the soak's choices differ between sessions and come out the same each run. */
export function dice(seed) {
  let s = (seed * 2654435761) >>> 0 || 1;
  return (n) => {
    s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0;
    return s % n;
  };
}

/** Session `n`: its tmux pane (a program that takes what's typed and shows nothing) and its transcript. */
export function makeSession(soak, n) {
  const sid = `soak-${n}`;
  const pane = soak.world.tmux.newSession(sid, "stty -echo; exec cat >/dev/null");
  const transcript = path.join(soak.world.root, "transcripts", `${sid}.jsonl`);
  fs.mkdirSync(path.dirname(transcript), { recursive: true });
  const origin = { harness: "claude", session_id: sid, cwd: `/tmp/cue-soak/p${n % 8}`, transcript_path: transcript, tmux_pane: pane, term_program: "tmux" };
  return { sid, pane, transcript, origin };
}

function write(s, entry) {
  fs.appendFileSync(s.transcript, JSON.stringify({ ...entry, timestamp: new Date().toISOString() }) + "\n");
}

/** Session `s`'s rounds until the run ends. */
export async function agent(soak, s, n) {
  const roll = dice(n + 1);
  for (let round = 0; soak.going(); round++) {
    write(s, { type: "user", message: { content: `task ${round}` } });
    await soak.tell({ type: "event", event: "active", message: `task ${round}`, ...s.origin });
    for (let k = 0; k < 3 + roll(6); k++) {
      write(s, { type: "assistant", message: { content: [{ type: "text", text: `Working on task ${round}: `.repeat(60) }] } });
      await pause(500);
    }
    if (round % 3 === 1) await ask(soak, s, round);
    await soak.tell({ type: "event", event: "stopped", message: `Done with task ${round}.`, ...s.origin });
    await pause(2000 + roll(4000));
  }
}

/** A permission ask through the real hook, answered a moment after its card shows, as you would. */
async function ask(soak, s, round) {
  const hook = runHook(soak.world, "permission", { ...permission(s.sid, { input: { command: `make test-${round}`, description: "run the tests" } }), transcript_path: s.transcript });
  soak.tally.counts.asks++;
  const card = await soak.until(() => soak.card(s.sid, (i) => i.kind !== "waiting"), 30_000);
  if (!card) return void hook.proc.kill();
  await pause(200 + (round * 97) % 1300);
  const given = performance.now();
  const said = await call(soak.world.sock, { type: "respond", id: card.id, decision: { behavior: "allow" } }).catch(() => ({}));
  if (said.ok !== true) return void hook.proc.kill();
  const heard = await Promise.race([hook.decision, pause(30_000).then(() => null)]);
  if (heard) soak.tally.decisionsMs.push(performance.now() - given);
  else soak.tally.undecided++;
}

/** Every 5 s, 200 hook events within a second: status-line costs and compactions. */
export async function bursts(soak, sessions) {
  const roll = dice(7);
  while (soak.going()) {
    for (let k = 0; k < 200; k++) {
      const s = sessions[roll(sessions.length)];
      soak.tell(k % 2 ? { type: "event", event: "compacting", message: "auto", ...s.origin } : { type: "usage", session_id: s.sid, cost: k / 100 });
      await pause(5);
    }
    await pause(4000);
  }
}

/** One typist: a message a second into a random session's pane, as a reply to its finished turn's card,
 *  or (at work) as a message held until its turn ends. */
export async function typist(soak, sessions, n) {
  const roll = dice(100 + n);
  while (soak.going()) {
    await pause(1000);
    const s = sessions[roll(sessions.length)];
    const card = soak.card(s.sid, (i) => i.kind === "waiting");
    const msg = card ? { type: "reply", id: card.id, text: `go on, ${s.sid}` } : { type: "send_to", session_id: s.sid, text: `also, ${s.sid}`, now: false };
    soak.tally.counts.typingSent++;
    const said = await call(soak.world.sock, msg, 60_000).catch((e) => ({ detail: e.message }));
    if (said.ok !== true) soak.tally.refuse(said.detail || "no reason given");
  }
}

/** The window's own time to take in each state it's sent (setState, its draw included), counted in the page:
 *  setState wrapped once, each call's time kept in the page until `drainDraws` takes them. */
export function timeDraws(ui) {
  return ui.runDirect(() => {
    const kept = (window.__soakDraws = []);
    const take = setState;
    setState = (s) => { const t = performance.now(); try { take(s); } finally { kept.push(performance.now() - t); } };
    return "timed";
  });
}

/** The draws counted since the last time, taken out of the page (a plain list of numbers). */
async function drainDraws(soak) {
  const ms = await soak.world.ui.runDirect(() => (window.__soakDraws || []).splice(0).map(Number));
  soak.tally.drawsMs.push(...ms);
}

/** The window's poll of Cue's state, five times a second, timed; memory, the state's size and the window's
 *  draws every 10 s. */
export async function poller(soak) {
  let sampled = -Infinity;
  while (soak.going()) {
    const t0 = performance.now();
    const state = await call(soak.world.sock, { type: "list" }).catch(() => null);
    if (state) {
      soak.tally.counts.polls++;
      soak.latest = state;
      if (performance.now() - sampled >= 10_000) {
        const probe = await call(soak.world.sock, { type: "probe" });
        soak.tally.sample(probe.resident_bytes, JSON.stringify(state).length + 1);
        await drainDraws(soak);
        sampled = performance.now();
      }
    } else soak.tally.counts.eventsFailed++;
    await pause(Math.max(0, 200 - (performance.now() - t0)));
  }
}

/** The window's main thread, timed: a script evaluated in it every 250 ms. */
export async function mainThread(soak) {
  while (soak.going()) {
    soak.tally.answerMs.push(await soak.world.ui.latency());
    await pause(250);
  }
}

/** A session's built-in terminal opened and closed again, every 15 s: its chat opened, then its terminal,
 *  through the window's own functions (what its strip's click runs). */
export async function terminals(soak, sessions) {
  const roll = dice(31);
  const clients = () => soak.world.tmux.clients().length;
  while (soak.going()) {
    await pause(15_000);
    const s = sessions[roll(sessions.length)];
    await soak.world.ui.runDirect((id) => { setActive(null, id); openTerm(id); return true; }, s.sid);
    if (!(await soak.until(() => clients() > 0, 10_000))) continue;
    soak.tally.counts.opened++;
    await pause(3000);
    await soak.world.ui.runDirect(() => { closeTerm(); return true; });
    if (await soak.until(() => clients() === 0, 10_000)) soak.tally.counts.closed++;
  }
}
