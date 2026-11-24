// A session's life in the window: a reply typed into its real terminal, parking it, and Cue's own
// settings, each across a relaunch of Cue where it matters.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { agentInTmux } from "../lib/agents.js";
import { active, stopped } from "../lib/hooks.js";
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
  const toggle = '[data-set="notify.cold"]';
  await world.ui.click("[data-act=open-settings]");
  assert.equal(await (await world.ui.find(toggle)).getAttribute("aria-checked"), "true");
  await world.ui.click(toggle);
  await world.ui.waitUntil(() => state.settings.notify?.cold === false, "the setting saved");
  await world.relaunch();
  assert.equal(await world.ui.run(() => state.settings.notify?.cold), false, "Cue read it back");
  await world.ui.click("[data-act=open-settings]");
  assert.equal(await (await world.ui.find(toggle)).getAttribute("aria-checked"), "false", "the window shows it off");
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


journey("a Codex Desk handoff stays clean in the native chat and Up Arrow recall", async (world) => {
  const sid = "e2e-desk-handoff";
  const note = "Successor now runs Smoke Test's Desk. Please route Fix the header.";
  const context = '<desk id="desk-smoke" name="Smoke Test" project="cue-e2e-project">\nDesk routes: other (sid:abcdef, 142k)\n</desk>\n\n';
  await world.tell({ ...active(sid, { message: context + note }), harness: "codex" });
  await world.tell({ ...stopped(sid, { message: "I’ll route it." }), harness: "codex" });
  await world.ui.waitUntil((id) => state.sessions.some((s) => s.session_id === id), "the Codex session", 10_000, sid);
  await world.ui.run((id) => openSession(id), sid);
  await world.ui.waitText(note);
  const chat = await world.ui.run(() => document.querySelector("[data-chat]").textContent);
  assert.ok(chat.includes(note));
  assert.doesNotMatch(chat, /<desk|abcdef|142k/);
  const composer = await world.ui.find(`textarea[data-text="s:${sid}"]`);
  await composer.click();
  await world.ui.b.keys("ArrowUp");
  assert.equal(await composer.getValue(), note);
});
