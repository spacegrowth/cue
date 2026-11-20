// Items to check say where they are, in one plain line (#436): in the app since its install with what to try, landing
// with the next install, or mocks with the canvas to open. On the Desk row and the item page; it changes by itself as
// the install finishes.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const patchCheck = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").checks.find((c) => c.id === k), p); }, [id, patch]);
const clock = (page, ms) => page.evaluate((t) => clockAt(t), ms);

test("an item installed says In the app since its install, and what to try, on its row and its page (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = await page.evaluate(() => now() - 30 * 60000);
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  const want = `In the app since ${await clock(page, at)} · try: Quick phrases sit at the right of Send; More opens the rest.`;
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-h")).toHaveText(want);
  // Said once: no You'll see line under it as well.
  await expect(row.locator(".dk-see")).toHaveCount(0);
  await row.locator(".dk-t").click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-pickl")).toHaveText(want);
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
});

test("an item not in the installed app yet says Lands with the next install, and turns to In the app when the install is done (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Its work changed after the running Cue was installed: nothing to try yet.
  await patchCheck(cue, "d2", { edited: 1793984400000 + 60000, at: 1793984400000 + 60000 });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-h")).toHaveText("Lands with the next install");
  await expect(row.locator(".dk-see")).toHaveText("You'll see: Quick phrases sit at the right of Send; More opens the rest.");
  // The install finishes: the hub says so, and the line changes with nothing pressed.
  const at = await page.evaluate(() => now());
  await patchItem(cue, "d2", { finish: { step: "checked", at, commit: "c71e0b4" } });
  await expect(row.locator(".dk-h")).toHaveText(`In the app since ${await clock(page, at)} · try: Quick phrases sit at the right of Send; More opens the rest.`);
});

test("a mocks item says Mocks · open the canvas, the link opening it, on its row and its page (#436)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Mocks, notes or plans that changed nothing the app runs (#408), its canvas among what it made.
  await patchItem(cue, "d2", { finish: { step: "checked", at: Date.now(), commit: "c71e0b4", mocks: true }, made: ["https://claude.ai/artifact/abc"] });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-h")).toHaveText("Mocks · open the canvas");
  await expect(row.locator('.dk-h a[data-href="https://claude.ai/artifact/abc"]')).toHaveText("open the canvas");
  // The link opens the canvas, not the item.
  await row.locator(".dk-h a").click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/abc" });
  await expect(row).not.toHaveClass(/\bon\b/);
  await row.locator(".dk-t").click();
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-pickl")).toHaveText("Mocks · open the canvas");
  // Its DESIGNS link, when it gave one, is the canvas.
  await patchItem(cue, "d2", { finish: null, made: [], designs: "https://claude.ai/artifact/xyz" });
  await expect(page.locator('.dk-panel[data-side-for] .dk-pnow .dk-pickl a[data-href="https://claude.ai/artifact/xyz"]')).toHaveText("open the canvas");
});
