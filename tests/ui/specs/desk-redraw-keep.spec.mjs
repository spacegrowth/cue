// A redraw while you type in the Desk's "Add an item…" line, or scroll the Items list or Give to…'s menu, leaves each
// where it was (#308): the window is rebuilt on every update, and these went back to their start.
import { test, expect, openDesk } from "./cue.mjs";

test.use({ browserName: "webkit" });

/** Two frames gone by: a scroll's event and anything placed after a draw have run. */
const frames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
const tick = (cue, n) => cue.change((st, w) => { st.sessions[0].doing = w; }, `tick ${n}`);

test("the Add an item line keeps its words in view, not their start, through updates while you type", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.click();
  await line.pressSequentially("a long item title that runs well past the end of the box so it scrolls sideways while typing, even in the wide Add line of the Desk's item list");
  const left = await line.evaluate((el) => el.scrollLeft);
  expect(left).toBeGreaterThan(0);
  await tick(cue, 1);
  await frames(page);   // the redraw done: read the line on screen, not the one it replaced
  await expect(line).toBeFocused();
  expect(await line.evaluate((el) => [el.scrollLeft, el.selectionStart === el.value.length])).toEqual([left, true]);
  await line.pressSequentially(" more");
  await tick(cue, 2);
  await frames(page);
  expect(await line.evaluate((el) => el.scrollLeft)).toBeGreaterThanOrEqual(left);
});

test("Up next's list and Give to…'s menu stay where you scrolled them through updates", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    for (let n = 0; n < 30; n++) st.sessions.push({ ...st.sessions[0], session_id: `extra${n}`, name: `extra-${n}` });
    const p = st.desk.projects[0], fresh = p.items.find((i) => i.id === "d6");
    for (let n = 0; n < 30; n++) p.items.push({ ...fresh, id: `n${n}`, title: `New thing ${n}` });
  });
  await openDesk(page);
  // New items wait in Up next (#738), a list of its own that scrolls.
  const list = page.locator('.dkn-items [data-dkn-list="next"]');
  await expect(list.locator('[data-desk-item="n29"]')).toHaveCount(1);
  await list.evaluate((el) => { el.scrollTop = 400; });
  await tick(cue, 1);
  await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBe(400);

  // Give to… is on the item's page (#748), not its row: the row opened, the list stays where it was.
  await expect(page.locator('.dkn-items [data-desk-item="n0"] [data-desk-give]')).toHaveCount(0);
  await page.locator('.dkn-items [data-desk-item="n0"]').click();
  const at = await list.evaluate((el) => el.scrollTop);
  await page.locator('[data-desk-give="n0"]').first().click();
  const menu = page.locator(".dk-givemenu");
  await expect.poll(() => menu.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  await menu.evaluate((el) => { el.scrollTop = 200; });
  await tick(cue, 2);
  await expect(menu).toBeVisible();
  await frames(page);   // past the menu's own placing (a microtask after the draw)
  expect(await menu.evaluate((el) => el.scrollTop)).toBe(200);
  expect(await list.evaluate((el) => el.scrollTop)).toBe(at);
  // Scrolling the list beside it leaves the menu where you scrolled it, not at its top.
  await list.evaluate((el) => { el.scrollTop -= 10; });
  await frames(page);   // the scroll event, which places the menu again
  expect(await menu.evaluate((el) => el.scrollTop)).toBe(200);
});
