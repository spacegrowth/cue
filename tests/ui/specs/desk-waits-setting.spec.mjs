// When an item waiting on another starts (#808, desk/lands.rs "waits"): a row "Items waiting on another start" in a
// Desk's settings with two fixed choices, When it lands (every Desk's, Cue's own too) and When you've checked it. Saved
// with the rest by Save; Up next says which point a waiting item waits for. At a laptop window and the smallest,
// light and dark.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";
import { statusOf } from "./desk.mjs";

const SHOTS = process.env.SHOT_DIR || "";

const open = async (page, cue) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  // On the More tab (#913).
  await page.locator('.ld-dialog [data-ld-tab="more"]').click();
  return page.locator(".ld-dialog");
};
const row = (dlg) => dlg.locator(".ld-row", { has: dlg.page().locator(".ld-label", { hasText: /^Items waiting on another start$/ }) });

for (const [name, size] of [["a laptop window", { width: 1440, height: 900 }], ["the smallest window", { width: 900, height: 600 }]]) {
  for (const scheme of ["light", "dark"]) {
    test(`at ${name} (${scheme}): the row has its two choices, Cue's own Desk on When it lands, and fits`, async ({ page, cue }) => {
      await page.setViewportSize(size);
      await page.emulateMedia({ colorScheme: scheme });
      const dlg = await open(page, cue);
      const waits = row(dlg);
      await expect(waits.locator(".ld-seg button")).toHaveText(["When it lands", "When you've checked it"]);
      await expect(waits.locator("button.on")).toHaveText("When it lands");
      // One line of help, as each row of Settings has (#913).
      await expect(waits.locator(".ld-hint")).toHaveText("An item queued after another starts once that one is on main.");
      await expect(waits.locator("textarea, input")).toHaveCount(0);
      await waits.scrollIntoViewIfNeeded();
      const [d, ...bs] = await Promise.all([dlg.boundingBox(), ...[0, 1].map((i) => waits.locator(".ld-seg button").nth(i).boundingBox())]);
      for (const b of bs) expect(b.x + b.width).toBeLessThanOrEqual(d.x + d.width + 1);
      // The label stays on its column: nothing overlaps the choices.
      const lb = await waits.locator(".ld-label").boundingBox();
      expect(lb.x + lb.width).toBeLessThanOrEqual(bs[0].x + 1);
      if (SHOTS) await dlg.screenshot({ path: `${SHOTS}/desk-waits-setting-${size.width}-${scheme}.png` });
      await expect(dlg.locator("[title]")).toHaveCount(0);
    });
  }
}

test("a pick is saved by Save with the rest, and Up next then says it waits to be checked", async ({ page, cue }) => {
  const dlg = await open(page, cue);
  await cue.change((st) => {
    const it = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4");
    Object.assign(it, { state: "queued", after: "d3" });
    delete it.holder; delete it.next_for;
  });
  const waits = row(dlg);
  await waits.locator('[data-ld-pick="checked"]').click();
  await expect(waits.locator("button.on")).toHaveText("When you've checked it");
  await expect(dlg.locator(".ld-foot")).toContainText("Unsaved changes");
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args.lands).toMatchObject({ waits: "checked" });
  await expect(statusOf(page, "d4")).toHaveText("Waits for #d3 to be checked first");
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const again = row(page.locator(".ld-dialog"));
  await expect(again.locator("button.on")).toHaveText("When you've checked it");
  await again.locator('[data-ld-pick="landed"]').click();
  await page.locator('.ld-dialog [data-ld="save"]').click();
  await expect(statusOf(page, "d4")).toHaveText("Waits for #d3 to land first");
});
