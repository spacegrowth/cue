// The Desk's item list scrolls up and down only (#405): a done row with a long commit line ("committed 2f2448c7
// ef66ab27 · Thu 9:45pm") and its Undo made the list wider than its pane, so it slid sideways and cut off the group
// headings and row icons at its left edge. Done today (#738): a done row is its title over one line ("#d7 · took 3m ·
// landed 9:45pm"), no Undo on it; a long title or commit still mustn't widen it.
import { test, expect, openDesk } from "./cue.mjs";
import { openDone, statusOf } from "./desk.mjs";

test("a long done row doesn't widen the item list; it can't scroll sideways and its left edge shows in full", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const i of d.items.filter((x) => x.state === "done")) Object.assign(i, { commit: "2f2448c7 ef66ab27 9a1b2c3d", holder: { name: "cue-387-a-long-session-name" }, title: `${i.title} ${"and_a_very_long_unbroken_word_".repeat(8)}` });
  });
  await openDesk(page);
  await openDone(page);
  const list = page.locator('.dkn-items [data-dkn-list="done"]');
  await page.locator('[data-dkn-drawer="done"] [data-desk-item="d7"]').waitFor();
  const row = page.locator('[data-dkn-drawer="done"] [data-desk-item="d7"]');
  await expect(statusOf(page, "d7")).toHaveText(/landed /);
  // Undo is on its page now, not the row.
  await expect(row.locator(".dk-undo")).toHaveCount(0);
  const size = await list.evaluate((s) => ({ sw: s.scrollWidth, cw: s.clientWidth }));
  expect(size.sw).toBe(size.cw);
  // A sideways swipe moves nothing.
  await list.hover();
  await page.mouse.wheel(300, 0);
  await list.evaluate((s) => { s.scrollLeft = 200; });
  expect(await list.evaluate((s) => s.scrollLeft)).toBe(0);
  // Its title and its line end inside the list at the right; its #id starts inside it on the left.
  await row.hover();
  const [box, title, line, id] = await Promise.all([list.boundingBox(), row.locator(".dkn-a1").boundingBox(), row.locator(".dkn-a2").boundingBox(), row.locator(".dkn-a2 > .dkn-id").boundingBox()]);
  expect(title.x + title.width).toBeLessThanOrEqual(box.x + box.width);
  expect(line.x + line.width).toBeLessThanOrEqual(box.x + box.width);
  expect(id.x).toBeGreaterThanOrEqual(box.x);
  const head = await page.locator('[data-dkn-drawer="done"] .dkn-dh').boundingBox();
  expect(head.x).toBeGreaterThanOrEqual(box.x);
});
