// Step times for the desk scenario's items (#751): the scenario keeps none, and the strip and the line are drawn from
// them. As in the #745 canvas: one asking you while it builds, one ready to check after 9h, one at its tests.

/** Stamps d1, d2, d3 and d5's steps (ms, six long, 0 not reached) in the page's world and sends the state. */
export const stampSteps = (page) => page.evaluate(() => {
  const st = CueBridge.world.state, now = Date.now(), m = 60_000;
  const at = (...mins) => [...mins.map((x) => (x == null ? 0 : now - x * m)), 0, 0, 0, 0, 0, 0].slice(0, 6);
  const steps = {
    d1: at(45, 44),
    d2: at(571, 567, 567 - 1 / 60, 546, 546 - 1 / 60, 540),
    d3: at(35, 33, 12),
    d5: at(8, 6),
  };
  for (const p of st.desk.projects) for (const i of p.items || []) if (steps[i.id]) i.steps = steps[i.id];
  CueBridge.emit("state", CueBridge.world.snapshot());
});
