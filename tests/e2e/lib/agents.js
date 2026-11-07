// Stand-in agents: a shell line in a pane of the test's tmux server that Cue knows as a Claude Code
// session (its Stop hook reported, with the pane), and that session's built-in terminal opened in the window.
import fs from "node:fs";
import path from "node:path";
import { stopped } from "./hooks.js";
import { waitFor } from "./poll.js";

/** Session `sid` running `command` in its own tmux session (named `sid`), its turn just ended: its
 *  "waiting" card is up. The pane's first process is the agent (`exec` in `command` keeps it the
 *  program itself). Returns the pane and the card. */
export async function agentInTmux(world, sid, command, { message = "done" } = {}) {
  const pane = world.tmux.newSession(sid, command, { CUE_HOME: world.cueHome, HOME: world.home });
  const pid = Number(world.tmux.run("display-message", "-p", "-t", pane, "#{pane_pid}"));
  await world.tell(stopped(sid, { pane, pid, message }));
  const card = await waitFor(async () => (await world.list()).items.find((i) => i.session_id === sid && i.kind === "waiting"), `${sid}'s card`);
  return { pane, card };
}

/** Session `sid`'s built-in terminal, opened from its strip in the window the way you'd click it: waits
 *  for Cue's tmux client to attach. Returns the terminal's keyboard input (xterm's textarea). */
export async function openTerminal(world, sid) {
  await world.ui.click(`.tstrip[data-sid="${sid}"]`);
  await waitFor(() => world.tmux.clients().length === 1, "Cue's terminal client attached");
  return world.ui.find(".xterm-helper-textarea");
}

/** Whether the terminal's "isn't responding" line is showing. */
export const stalledBar = (world) => world.ui.run(() => !!document.querySelector(".term-stalled"));

/** A pane program that writes what's typed into it to a file, byte for byte as it comes (raw: a line over
 *  the terminal's 1 KB line limit would otherwise be cut and never handed on). `got()`: the file so far. */
export function typingSink(world) {
  const file = path.join(world.root, "typed");
  return { command: `stty raw -echo; exec cat > ${file}`, got: () => (fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "") };
}
