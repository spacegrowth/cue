// Layout: nothing on screen sits on top of something else or is cut off by its box. The screenshots catch a
// change; these say what's wrong in words, and keep saying it if a baseline is ever rewritten over a bug.
import { test, expect } from "./cue.mjs";

const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const box = async (loc) => { const b = await loc.boundingBox(); expect(b, "on screen").not.toBeNull(); return b; };
/** `a` lies wholly inside `b` (half a pixel's give for subpixel layout). */
async function expectInside(a, b, what) {
  const [r, o] = [await box(a), await box(b)];
  expect(r.x >= o.x - 0.5 && r.y >= o.y - 0.5 && r.x + r.width <= o.x + o.width + 0.5 && r.y + r.height <= o.y + o.height + 0.5,
    `${what}: ${JSON.stringify(r)} inside ${JSON.stringify(o)}`).toBe(true);
}
/** No two of `locs` overlap (each pair's boxes are apart, touching allowed). */
async function expectApart(locs, what) {
  const rs = [];
  for (const l of locs) for (const r of await Promise.all((await l.all()).map(box))) rs.push(r);
  for (let i = 0; i < rs.length; i++) for (let j = i + 1; j < rs.length; j++) {
    const [a, b] = [rs[i], rs[j]];
    const overlap = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 0.5 && Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 0.5;
    expect(overlap, `${what}: ${JSON.stringify(a)} and ${JSON.stringify(b)} overlap`).toBe(false);
  }
}

test.describe("a lead's chat with its review rows", () => {
  test.beforeEach(async ({ cue, page }) => { await cue.open("relay-review", { frozen: true }); await openCard(page, "n1"); await page.locator(".active-pane .rv-row").first().waitFor(); });

  test("opens with its first review row in full view", async ({ page }) => {
    await expectInside(page.locator(".rv-row").first(), page.locator(".rv-rows"), "the first review row");
  });
});
