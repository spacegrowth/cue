// Asks: what an agent asks reaches the window as a card, and the button you click is what the agent hears.
import assert from "node:assert/strict";
import { journey } from "../lib/journey.js";
import { permission, runHook } from "../lib/hooks.js";
import { waitFor } from "../lib/poll.js";

const pendingFor = async (world, sid) => (await world.list()).items.filter((i) => i.session_id === sid && i.kind !== "waiting");

journey("a fresh Cue shows an empty board", async (world) => {
  await world.ui.waitText("Nothing waiting.");
  const { items, sessions } = await world.list();
  assert.deepEqual([items.length, sessions.length], [0, 0]);
  const png = await world.ui.screenshot();
  assert.ok(png.length > 10_000 && png.subarray(1, 4).toString() === "PNG", "a real screenshot of the window");
});

for (const [button, expected] of [["allow", { behavior: "allow" }], ["deny", { behavior: "deny", message: "Denied in Cue." }]]) {
  journey(`a permission ask, answered with ${button} in the window, reaches the agent`, async (world) => {
    const sid = `e2e-ask-${button}`;
    const hook = runHook(world, "permission", permission(sid, { input: { command: "touch e2e", description: "make e2e" } }));
    const [it] = await waitFor(() => pendingFor(world, sid).then((a) => a.length && a), "the ask in Cue");
    await world.ui.waitText("touch e2e");
    await world.ui.click(`[data-act=${button}][data-id="${it.id}"]`);
    assert.deepEqual(await hook.decision, expected);
    await waitFor(async () => !(await pendingFor(world, sid)).length, "the card gone");
  });
}
