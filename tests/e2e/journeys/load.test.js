// Under load: hook events pouring in while you type, the window keeps answering and nothing is lost.
import assert from "node:assert/strict";
import { agentInTmux, openTerminal, typingSink } from "../lib/agents.js";
import { journey } from "../lib/journey.js";
import { waitFor } from "../lib/poll.js";
import { startStorm } from "../lib/storm.js";
import { assertResponsive, typeWatched } from "../lib/window.js";

journey("1,000 hook events at 100 a second while typing 1,000 keys: the window keeps answering, every key and event lands", async (world) => {
  const sink = typingSink(world);
  await agentInTmux(world, "e2e-typist", sink.command);
  const keys = await openTerminal(world, "e2e-typist");
  const storm = startStorm(world, { perSecond: 100, sessions: 20 });
  let samples, delivered;
  try {
    // The keys go in within a couple of seconds; the window is watched until the storm's 1,000th event too.
    samples = await typeWatched(world, keys, 1000, () => waitFor(() => storm.sent() >= 1000, "1,000 events sent", { timeout: 30_000 }));
  } finally {
    delivered = await storm.stop();
  }
  assertResponsive(samples);
  assert.equal(delivered.failed, 0, `every event reached Cue (${delivered.sent} sent)`);
  await waitFor(() => sink.got() === "y".repeat(1000), "all 1,000 keys in the pane");
  const storming = async () => (await world.list()).sessions.filter((s) => s.session_id.startsWith("e2e-storm-")).length;
  await waitFor(async () => (await storming()) === 20, "every stormed session on the board");
}, { timeout: 180_000 });
