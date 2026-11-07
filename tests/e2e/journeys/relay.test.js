// relay's review cards: Review typed into a lead while another of its executors' reports waits. Building
// those cards once had a lead check that read the agent index while it was held: the window froze (2e9abc0).
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { agentInTmux } from "../lib/agents.js";
import { journey } from "../lib/journey.js";
import { waitFor } from "../lib/poll.js";
import { relayCrew } from "../lib/relay.js";
import { assertResponsive, watchLatency } from "../lib/window.js";

const LEAD = "e2e-lead";
const card = async (world, exec) => (await world.ui.run(() => state.reviews || {}))[exec];

/** A lead with two reported executors (finder, parser), both cards up and saying the lead missed them,
 *  then Review on finder's card, typed into the lead, which runs its turn on it. Returns once finder's
 *  card says reviewing and parser's says the lead is busy, with the window's answer times since the click. */
async function reviewWhileAnotherWaits(world) {
  const typed = path.join(world.root, "lead-typed");
  // A stand-in lead: takes the line typed into it, then runs a turn on it as Claude Code's hooks report one.
  const hook = (event, extra = "") => `echo '{"session_id":"${LEAD}","cwd":"/tmp/cue-e2e-project"${extra}}' | "$CUE_HOME/bin/cue-hook" ${event}`;
  const { pane } = await agentInTmux(world, LEAD, `line=$(head -n 1); printf %s "$line" > ${typed}; ${hook("prompt")}; ${hook("stop", ',"last_assistant_message":"Reviewed."')}; exec sleep 600`);
  const pid = Number(world.tmux.run("display-message", "-p", "-t", pane, "#{pane_pid}"));
  relayCrew(world, { lead: LEAD, pid, executors: [{ id: "ex-a", session: "e2e-exec-a", topic: "finder" }, { id: "ex-b", session: "e2e-exec-b", topic: "parser" }] });
  // Both reports waited past relay's wake-up: each card says the lead hasn't picked it up.
  await waitFor(async () => (await card(world, "e2e-exec-a"))?.missed && (await card(world, "e2e-exec-b"))?.missed, "both review cards, missed", { timeout: 20_000 });
  await world.ui.waitText("its lead hasn't picked this up");
  const probe = watchLatency(world.ui);
  try {
    await world.ui.click('[data-crew=review][data-crew-sid="e2e-exec-a"]');
    await waitFor(() => fs.existsSync(typed) && fs.readFileSync(typed, "utf8").includes("/relay:review ex-a"), "the review typed into the lead");
    await waitFor(async () => (await card(world, "e2e-exec-a"))?.state === "reviewing", "finder's card reviewing");
    // While the lead settles after the review, every redraw asks whether it's free for parser's report:
    // the path that froze.
    await waitFor(async () => (await card(world, "e2e-exec-b"))?.missed === false, "parser's card while the lead is busy");
  } finally {
    await probe.stop();
  }
  return probe.samples;
}

journey("Review sent to a lead while another report waits: cards render and the window never stalls", async (world) => {
  assertResponsive(await reviewWhileAnotherWaits(world));
  assert.ok((await world.list()).sessions.some((s) => s.session_id === LEAD), "Cue's socket still answers");
});

journey("once the lead has settled after a review, the other waiting report says it was missed again", async (world) => {
  await reviewWhileAnotherWaits(world);
  // The lead settles 8 s after the review (REVIEW_SETTLE_MS): the card changes then, with nothing else happening.
  await waitFor(async () => (await card(world, "e2e-exec-b"))?.missed === true, "parser's card after the lead settled", { timeout: 12_000 });
});
