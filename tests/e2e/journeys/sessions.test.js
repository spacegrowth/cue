// A session's life in the window: a reply typed into its real terminal, parking it, and Cue's own
// settings, each across a relaunch of Cue where it matters.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { agentInTmux } from "../lib/agents.js";
import { journey } from "../lib/journey.js";
import { waitFor } from "../lib/poll.js";

const CWD = "/tmp/cue-e2e-project";

journey("a reply written on a finished turn's card is typed into its tmux pane, exactly", async (world) => {
  const sid = "e2e-reply";
  const out = path.join(world.root, "reply");
  // A stand-in Claude Code: takes one line from its terminal, keeps it, and starts its next turn the way
  // Claude's UserPromptSubmit hook reports it. It reads with `head`, a process other than a shell in its
  // pane's foreground, as an agent is: Cue never types where only a shell is (focus::pane_now).
  const agent = `line=$(head -n 1); printf %s "$line" > ${out}; echo '{"session_id":"${sid}","cwd":"${CWD}"}' | "$CUE_HOME/bin/cue-hook" prompt; exec sleep 600`;
  const { card } = await agentInTmux(world, sid, agent, { message: "All done. Ship it?" });
  const text = 'ship it, then run "make test"';
  await (await world.ui.find(`textarea[data-text="s:${sid}"]`)).setValue(text);
  await world.ui.click(`[data-act=send][data-id="${card.id}"]:not([disabled])`);
  await waitFor(() => fs.existsSync(out) && fs.readFileSync(out, "utf8") === text, "the exact text in the pane");
  const answered = await waitFor(async () => (await world.list()).history.find((h) => h.id === card.id), "the card in history");
  assert.equal(answered.outcome, `replied: “${text}”`);
});

journey("a setting changed in the window is still set after Cue restarts", async (world) => {
  const toggle = '[data-set="appearance.project"]';
  await world.ui.click("[data-act=open-settings]");
  assert.equal(await (await world.ui.find(toggle)).getAttribute("aria-checked"), "false");
  await world.ui.click(toggle);
  await world.ui.waitUntil(() => state.settings.appearance?.project === true, "the setting saved");
  await world.relaunch();
  assert.equal(await world.ui.run(() => state.settings.appearance?.project), true, "Cue read it back");
  await world.ui.click("[data-act=open-settings]");
  assert.equal(await (await world.ui.find(toggle)).getAttribute("aria-checked"), "true", "the window shows it on");
});

journey("parking a session from its ⋯ menu ends its terminal and keeps it under Parked across a restart", async (world) => {
  const sid = "e2e-park";
  await agentInTmux(world, sid, "exec sleep 600");
  await world.ui.click(`[data-more="${sid}"]`);
  await world.ui.click(`[data-park-session="${sid}"]`);
  await waitFor(async () => (await world.list()).parked?.some((p) => p.session_id === sid), "it parked");
  assert.throws(() => world.tmux.run("has-session", "-t", sid), "its tmux session ended");
  await world.relaunch();
  const { parked, sessions } = await world.list();
  assert.ok(parked.some((p) => p.session_id === sid), "still parked after the restart");
  assert.ok(!sessions.some((s) => s.session_id === sid), "not back as a live session");
});
