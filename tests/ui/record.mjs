// Records the named scenarios the UI tests load (tests/ui/scenarios/<name>.json) from the preview's sample world
// (scenarios/preview.js), at a fixed clock so the files and the screenshots taken from them never change by
// themselves. A scenario file is a plain snapshot, as hub/mod.rs snapshot() builds it: the Rust harness's
// recorded ones drop in beside these the same way. Run after changing preview.js or a recipe here:
//   node tests/ui/record.mjs
import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { NOW } from "./specs/clock.mjs";

const { previewWorld } = createRequire(import.meta.url)("./scenarios/preview.js");
const M = 60000;
const world = (flags = "") => previewWorld(new URLSearchParams(flags), NOW).state;
const relayOf = (s) => s.sessions.find((x) => x.session_id === "relay");

/** 50 sessions over 10 projects, some with long names, in every state the board shows. */
function many(s) {
  const states = ["working", "waiting", "working", "deciding", "agent"];
  const projects = ["api-server", "web-app", "order-engine", "data-pipeline", "a-rather-long-project-name-for-the-row", "filings-bot", "backtester", "alerts", "docs-site", "infra"];
  s.sessions = Array.from({ length: 50 }, (_, i) => {
    const project = projects[i % projects.length], state = states[i % states.length], since = 1 + (i * 7) % 55;
    return { session_id: `m${i}`, project, harness: i % 4 === 3 ? "pi" : "claude", cwd: `/Users/me/code/${project}`, state, since_ms: NOW - since * M,
      prompt: state === "working" ? `Step ${i}: ${i % 3 ? "fix the flaky test in the importer" : "rework the long-running migration so it can resume halfway through without redoing the batches already written"}` : "",
      segments: [{ kind: "working", start_ms: NOW - 30 * M, end_ms: state === "working" ? null : NOW - since * M }, ...(state === "working" ? [] : [{ kind: "waiting", start_ms: NOW - since * M, end_ms: null }])],
      ...(i % 6 === 0 ? { name: `session ${i} with a name long enough to be cut short on its card` } : {}) };
  });
  s.items = s.sessions.filter((x) => x.state === "waiting").slice(0, 6).map((x, n) => ({ id: `mi${n}`, kind: "waiting", harness: x.harness, session_id: x.session_id, cwd: x.cwd, project: x.project, message: `Finished step ${n}: the tests pass.`, context: [], created_ms: x.since_ms, status: "pending", outcome: "" }));
  return s;
}

/** agent-relay's last reply ends with its status list, and the list is on for it. */
function statusList(s) {
  const r = relayOf(s), last = r.thread.at(-1);
  last.text += "\n\n**Where we are**\n✓ Spend column on the board\n✓ Footer total\n→ relay:list prints the same number\n○ ∥ Carry spend over on retire\n○ ∥ README section\nWaiting on you: carry spend over to the successor, or not?";
  s.settings.status = { on: true, sessions: ["relay"] };
  return s;
}

/** agent-relay runs in tmux, so its terminal opens in Cue. */
function inTmux(s) {
  relayOf(s).tmux_pane = "%1";
  s.settings.sessions = { tmux: true, tmux_installed: true, terminal: "iTerm" };
  return s;
}

const SCENARIOS = {
  "board-empty": () => Object.assign(world(), { sessions: [], items: [], history: [], parked: [], ended: [] }),
  "board-waiting": () => world(),
  "many-sessions": () => many(world()),
  "relay-review": () => world("crew=1&rq=1"),
  "status-list": () => statusList(world()),
  "terminal": () => inTmux(world()),
  "parked-ended": () => world("park=1&live=1"),
  // A relay lead on build-box whose executor dns-cutover runs in a tmux session of its own there; Cue holds it too.
  "crew-remote": () => world("crew=1&remote=1&held=1"),
};

for (const [name, make] of Object.entries(SCENARIOS)) {
  writeFileSync(new URL(`scenarios/${name}.json`, import.meta.url), JSON.stringify(make(), null, 1) + "\n");
  console.log(`recorded ${name}`);
}
