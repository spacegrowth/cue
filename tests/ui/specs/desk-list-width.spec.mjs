// The Desk's item list scrolls up and down only (#405): a done row with a long commit line ("committed 2f2448c7
// ef66ab27 · Thu 9:45pm") and its Undo made the list wider than its pane, so it slid sideways and cut off the group
// headings and row icons at its left edge.
import { test, expect, openDesk } from "./cue.mjs";

test("a long done row doesn't widen the item list; it can't scroll sideways and its left edge shows in full", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const i of d.items.filter((x) => x.state === "done")) Object.assign(i, { commit: "2f2448c7 ef66ab27 9a1b2c3d", holder: { name: "cue-387-a-long-session-name" } });
  });
  await openDesk(page);
  const list = page.locator(".dk-items .dk-scroll");
  await page.locator('.dk-items [data-desk-item="d7"]').waitFor();
  const row = page.locator('.dk-items [data-desk-item="d7"]');
  await expect(row.locator(".dk-age")).toContainText("committed 2f2448c7");
  const size = await list.evaluate((s) => ({ sw: s.scrollWidth, cw: s.clientWidth }));
  expect(size.sw).toBe(size.cw);
  // A sideways swipe moves nothing.
  await list.hover();
  await page.mouse.wheel(300, 0);
  await list.evaluate((s) => { s.scrollLeft = 200; });
  expect(await list.evaluate((s) => s.scrollLeft)).toBe(0);
  // Undo shows on the row, inside the list; the row's icon starts inside it on the left.
  await row.hover();
  const [box, undo, icon] = await Promise.all([list.boundingBox(), row.locator(".dk-undo").boundingBox(), row.locator(".dk-si").boundingBox()]);
  expect(undo.x + undo.width).toBeLessThanOrEqual(box.x + box.width);
  expect(icon.x).toBeGreaterThanOrEqual(box.x);
  const head = await page.locator(".dk-items .dk-gh:visible").first().boundingBox();
  expect(head.x).toBeGreaterThanOrEqual(box.x);
});
