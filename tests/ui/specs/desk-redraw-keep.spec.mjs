// A redraw while you type in the Desk's "Add an item…" line, or scroll the Items list or Give to…'s menu, leaves each
// where it was (#308): the window is rebuilt on every update, and these went back to their start.
import { test, expect } from "./cue.mjs";

test.use({ browserName: "webkit" });

async function openDesk(cue, page) {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
}
/** Two frames gone by: a scroll's event and anything placed after a draw have run. */
const frames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
const tick = (cue, n) => cue.change((st, w) => { st.sessions[0].doing = w; }, `tick ${n}`);

test("the Add an item line keeps its words in view, not their start, through updates while you type", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(cue, page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.click();
  await line.pressSequentially("a long item title that runs well past the end of the box so it scrolls sideways while typing");
  const left = await line.evaluate((el) => el.scrollLeft);
  expect(left).toBeGreaterThan(0);
  await tick(cue, 1);
  await expect(line).toBeFocused();
  expect(await line.evaluate((el) => [el.scrollLeft, el.selectionStart === el.value.length])).toEqual([left, true]);
  await line.pressSequentially(" more");
  await tick(cue, 2);
  expect(await line.evaluate((el) => el.scrollLeft)).toBeGreaterThanOrEqual(left);
});

test("the Items list and Give to…'s menu stay where you scrolled them through updates", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    for (let n = 0; n < 30; n++) st.sessions.push({ ...st.sessions[0], session_id: `extra${n}`, name: `extra-${n}` });
    const p = st.desk.projects[0], fresh = p.items.find((i) => i.id === "d6");
    for (let n = 0; n < 30; n++) p.items.push({ ...fresh, id: `n${n}`, title: `New thing ${n}` });
  });
  await openDesk(cue, page);
  const list = page.locator('.dk-scroll[data-scroll-key^="desk-items"]');
  await list.evaluate((el) => { el.scrollTop = 400; });
  await tick(cue, 1);
  await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBe(400);

  await page.locator('[data-desk-give="n0"]').first().click();
  const menu = page.locator(".dk-givemenu");
  await expect.poll(() => menu.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  await menu.evaluate((el) => { el.scrollTop = 200; });
  await tick(cue, 2);
  await expect(menu).toBeVisible();
  await frames(page);   // past the menu's own placing (a microtask after the draw)
  expect(await menu.evaluate((el) => el.scrollTop)).toBe(200);
  // Scrolling the list under it moves the menu with its button, not to its top.
  await list.evaluate((el) => { el.scrollTop -= 10; });
  await frames(page);   // the scroll event, which places the menu again
  expect(await menu.evaluate((el) => el.scrollTop)).toBe(200);
});
