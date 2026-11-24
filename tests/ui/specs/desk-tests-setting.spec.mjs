// Each Desk picks how much its sessions test (#780, desk/lands.rs "tests"): a Tests row in its settings with three
// fixed choices, All tests (a new Desk's), Only what changed (Cue's own Desk) and Changed, full run at landing.
// Saved with the rest by Save. At a laptop window and the smallest, light and dark.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";

const SHOTS = process.env.SHOT_DIR || "";

const open = async (page, cue) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  return page.locator(".ld-dialog");
};
const row = (dlg) => dlg.locator(".ld-row", { has: dlg.page().locator(".ld-label", { hasText: /^Tests$/ }) });

for (const [name, size] of [["a laptop window", { width: 1440, height: 900 }], ["the smallest window", { width: 900, height: 600 }]]) {
  for (const scheme of ["light", "dark"]) {
    test(`at ${name} (${scheme}): the Tests row has its three choices, Cue's own Desk on Only what changed, and fits`, async ({ page, cue }) => {
      await page.setViewportSize(size);
      await page.emulateMedia({ colorScheme: scheme });
      const dlg = await open(page, cue);
      const tests = row(dlg);
      await expect(tests.locator(".ld-seg button")).toHaveText(["All tests", "Only what changed", "Changed, full run at landing"]);
      await expect(tests.locator("button.on")).toHaveText("Only what changed");
      // Its labels say it: no words under it, so the screen stays short (#447).
      await expect(tests.locator(".ld-hint")).toHaveCount(0);
      // Fixed choices only: no box to type a command in.
      await expect(tests.locator("textarea, input")).toHaveCount(0);
      await tests.scrollIntoViewIfNeeded();
      // Every choice inside the dialog: nothing runs past its right edge.
      const [d, ...bs] = await Promise.all([dlg.boundingBox(), ...[0, 1, 2].map((i) => tests.locator(".ld-seg button").nth(i).boundingBox())]);
      for (const b of bs) expect(b.x + b.width).toBeLessThanOrEqual(d.x + d.width + 1);
      if (SHOTS) await dlg.screenshot({ path: `${SHOTS}/desk-tests-setting-${size.width}-${scheme}.png` });
      await expect(dlg.locator("[title]")).toHaveCount(0);
    });
  }
}

test("a pick is saved by Save with the rest, and shows next time", async ({ page, cue }) => {
  const dlg = await open(page, cue);
  const tests = row(dlg);
  await tests.locator('[data-ld-pick="changed_full"]').click();
  await expect(tests.locator("button.on")).toHaveText("Changed, full run at landing");
  await expect(dlg.locator(".ld-foot")).toContainText("Unsaved changes");
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args.lands).toMatchObject({ tests: "changed_full" });
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  await expect(row(page.locator(".ld-dialog")).locator("button.on")).toHaveText("Changed, full run at landing");
});
