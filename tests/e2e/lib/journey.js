// A journey: one node:test case with a fresh private Cue already launched. On a failure the world's
// folder is kept with a screenshot of the window next to Cue's log, and its path is printed.
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { World } from "./world.js";

export function journey(name, body, { timeout = 120_000 } = {}) {
  test(name, { timeout }, async () => {
    const world = new World(name);
    let failed = true;
    try {
      await world.launch();
      await body(world);
      failed = false;
    } finally {
      if (failed) await keepEvidence(world);
      await world.stop({ keep: failed });
    }
  });
}

async function keepEvidence(world) {
  try {
    if (world.ui) fs.writeFileSync(path.join(world.root, "window.png"), await world.ui.screenshot());
  } catch {}
  console.error(`  kept: ${world.root} (cue.log, window.png)`);
}
