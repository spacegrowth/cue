// A redraw puts back every box you scrolled, not just the lists named in app/render.js (#309): the window is
// rebuilt on every update, and anything else went back to its start (a picker, an ask, a menu, a wide table).
import { test, expect } from "./cue.mjs";

test.use({ browserName: "webkit", viewport: { width: 1100, height: 260 } });

const frames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
const tick = (cue, n) => cue.change((st, w) => { st.sessions[0].doing = w; }, `tick ${n}`);

test("the project picker stays where you scrolled it through updates", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await page.locator('[data-pj="toggle"]').click();
  const pop = page.locator(".pj-pop");
  await expect.poll(() => pop.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  const top = await pop.evaluate((el) => { el.scrollTop = 40; return el.scrollTop; });
  expect(top).toBeGreaterThan(0);
  await tick(cue, 1);
  await expect(pop).toBeVisible();
  expect(await pop.evaluate((el) => el.scrollTop)).toBe(top);
  // Two updates before its scroll event comes round still keep it.
  await tick(cue, 2);
  await tick(cue, 3);
  await frames(page);
  expect(await pop.evaluate((el) => el.scrollTop)).toBe(top);
});
