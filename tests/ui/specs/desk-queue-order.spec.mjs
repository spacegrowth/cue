// The Queued list in your order (#387): drag a queued item up or down and it takes its place (desk_item_act
// "reorder"); one can't go above the item it waits on: it snaps back and says why; a running item doesn't move.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const drag = async (page, from, to) => {
  await page.locator(`.dk-items [data-desk-item="${from}"]`).scrollIntoViewIfNeeded();
  const a = await page.locator(`.dk-items [data-desk-item="${from}"]`).boundingBox(), b = await page.locator(`.dk-items [data-desk-item="${to}"]`).boundingBox();
  await page.mouse.move(a.x + 60, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + 60, a.y + a.height / 2 + 8, { steps: 2 });
  await page.mouse.move(a.x + 60, b.y + (b.y < a.y ? 2 : b.height - 2), { steps: 6 });
  await page.mouse.up();
};
const order = (page) => page.locator(".dk-items .dk-it[data-desk-qg]").evaluateAll((rows) => rows.map((r) => r.dataset.deskItem));

test("Queued items drag into your order; one can't go above the item it waits on", async ({ page, cue }) => {
  await cue.open("desk");
  // d4 waits on d3 (at work); d6 is new; d10 waits on d4.
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), d4 = d.items.find((i) => i.id === "d4");
    d.items.push({ ...d4, id: "d10", title: "Footer links", after: "d4", after_why: "same file", created: Math.max(...d.items.map((i) => i.created)) + 1 });
  });
  await openDesk(page);
  await expect(page.locator(".dk-items .dk-gh", { hasText: "Queued" })).toContainText("drag to reorder");
  expect(await order(page)).toEqual(["d4", "d6", "d10"]);
  // Footer links above Rail dot, which it waits on: back in its place, and why.
  await drag(page, "d10", "d4");
  await expect(page.locator(".toast").filter({ hasText: "#d10 waits on #d4" })).toBeVisible();
  expect(await order(page)).toEqual(["d4", "d6", "d10"]);
  expect(await cue.calls("desk_item_act")).toEqual([]);
  // Tidy the Settings copy to the top: it takes its place, and stays there.
  await drag(page, "d6", "d4");
  await cue.called("desk_item_act", { id: "d6", act: "reorder", text: "d6 d4 d10" });
  await expect.poll(() => order(page)).toEqual(["d6", "d4", "d10"]);
  // A drag isn't a click: nothing opened.
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
  // A running item isn't in Queued: nothing to drag.
  await expect(page.locator('[data-desk-item="d3"][data-desk-qg]')).toHaveCount(0);
});

test("a dropped item stays where you let it go, selects no text, and the list scrolls when you hold it at an edge", async ({ page, cue }) => {
  await cue.open("desk");
  // Twelve queued, more than the list shows at once.
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), d6 = d.items.find((i) => i.id === "d6"), t = Math.max(...d.items.map((i) => i.created));
    for (let k = 1; k <= 10; k++) d.items.push({ ...d6, id: `q${k}`, title: `Queued thing ${k}`, created: t + k });
  });
  await openDesk(page);
  // Cue is slow to save it: a change that comes in meanwhile still shows your order.
  await cue.override("desk_item_act", () => new Promise(() => {}));
  await drag(page, "d6", "d4");
  await cue.change(() => {});
  await expect.poll(() => order(page)).toEqual(["d6", "d4", ...Array.from({ length: 10 }, (_, k) => `q${k + 1}`)]);
  expect(await page.evaluate(() => getSelection().toString())).toBe("");
  // A short window, so the list scrolls.
  await page.setViewportSize({ width: 1280, height: 560 });
  // The last one, held at the list's top edge: the list scrolls up and it goes to the top.
  const last = page.locator('.dk-items [data-desk-item="q10"]');
  await last.scrollIntoViewIfNeeded();
  const a = await last.boundingBox(), box = await page.locator(".dk-items .dk-scroll").boundingBox();
  await page.mouse.move(a.x + 60, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + 60, a.y + a.height / 2 - 10, { steps: 2 });
  await page.mouse.move(a.x + 60, box.y + 4, { steps: 8 });
  await expect.poll(() => page.locator(".dk-items .dk-scroll").evaluate((el) => el.scrollTop), { timeout: 5000 }).toBe(0);
  await page.waitForTimeout(300);
  await page.mouse.up();
  await expect.poll(async () => (await order(page))[0]).toBe("q10");
});
