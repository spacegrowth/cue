// The built-in terminal: a session's real tmux pane, opened in the window, typed into with the keyboard.
import assert from "node:assert/strict";
import { agentInTmux, openTerminal, stalledBar, typingSink } from "../lib/agents.js";
import { journey } from "../lib/journey.js";
import { waitFor } from "../lib/poll.js";
import { assertResponsive, typeWatched } from "../lib/window.js";

const ENTER = "";
/** The window must answer within this (ms) the whole time, or it counts as frozen. */
const RESPONSIVE_MS = 500;

journey("the built-in terminal opens on the session's pane, takes typing, and closes", async (world) => {
  const sink = typingSink(world);
  await agentInTmux(world, "e2e-term", sink.command);
  const keys = await openTerminal(world, "e2e-term");
  await keys.addValue(`hello from the window${ENTER}`);
  await waitFor(() => sink.got().includes("hello from the window"), "the typing in the pane");
  await world.ui.click('.tstrip[data-sid="e2e-term"]');
  await waitFor(() => world.tmux.clients().length === 0, "Cue's client gone after close");
  assert.equal(await world.ui.run(() => document.querySelector(".term-host")), null, "the terminal is off the window");
});

// What a stopped tmux client or a stopped program in the pane does to typing: nothing Cue has to catch.
// tmux's server reads the client's terminal itself, and holds what the pane hasn't read yet; so all of it
// arrives, in order, once the program runs again, and Cue never says the terminal isn't responding.
for (const [who, pidOf] of [
  ["Cue's tmux client", (world) => world.tmux.clients()[0]],
  ["the program in the pane", (world) => Number(world.tmux.run("display-message", "-p", "-t", "e2e-held", "#{pane_pid}"))],
]) {
  journey(`1,000 keys typed while ${who} is stopped all arrive, and the window never stalls`, async (world) => {
    const sink = typingSink(world);
    await agentInTmux(world, "e2e-held", sink.command);
    const keys = await openTerminal(world, "e2e-held");
    const pid = pidOf(world);
    process.kill(pid, "SIGSTOP");
    let samples;
    try {
      samples = await typeWatched(world, keys, 1000);
    } finally {
      process.kill(pid, "SIGCONT");
    }
    assertResponsive(samples);
    await waitFor(() => sink.got() === "y".repeat(1000), "all 1,000 keys in the pane");
    assert.equal(await stalledBar(world), false, "nothing said the terminal isn't responding");
  });
}

// The freeze regression. Stopping the tmux *client* doesn't reproduce it: tmux's server reads the client's
// terminal itself, so a stopped client's input still flows (docs/dev/e2e.md). A stopped *server* is what
// leaves Cue's pseudo-terminal unread: its ~1 KB buffer fills, and a write would wait forever.
journey("typing into a terminal nobody reads never freezes the window; the stuck line shows and Resume recovers", async (world) => {
  const sink = typingSink(world);
  await agentInTmux(world, "e2e-stuck", sink.command);
  const keys = await openTerminal(world, "e2e-stuck");
  await keys.addValue(`before${ENTER}`);
  await waitFor(() => sink.got().includes("before"), "typing before the stall");
  const server = Number(world.tmux.run("display-message", "-p", "#{pid}"));
  process.kill(server, "SIGSTOP");
  try {
    // 2,000 keys: the pseudo-terminal holds about 1,000 bytes, so this is well past full.
    const samples = await typeWatched(world, keys, 2000, () => waitFor(() => stalledBar(world), "the terminal isn't responding line", { timeout: 15_000 }));
    assertResponsive(samples);
  } finally {
    process.kill(server, "SIGCONT");
  }
  await world.ui.click(".term-stalled button");
  await waitFor(async () => !(await stalledBar(world)), "the line gone after Resume");
  await keys.addValue(`after${ENTER}`);
  await waitFor(() => sink.got().includes("after"), "typing goes in again after Resume");
});
