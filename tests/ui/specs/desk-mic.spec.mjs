// The Desk's mic looks and behaves as the Board's (#493): same size, corners, colours, at rest, on hover and while
// listening, in light and dark; in the lead's box and on an item's page. (It had no size of its own, so listening
// lit a bare square around the icon.)
import { test, expect, openDesk } from "./cue.mjs";

/** What the mic looks like: everything that shows. */
const look = (page, sel) => page.locator(sel).evaluate((el) => {
  const c = getComputedStyle(el);
  return { width: c.width, height: c.height, radius: c.borderRadius, display: c.display, color: c.color, background: c.backgroundColor, border: c.borderTopWidth, shadow: c.boxShadow, outline: c.outlineStyle };
});
/** The mic at rest, under the pointer, and listening (as a click starts it; the fake bridge never answers). */
async function states(page, sel) {
  await page.mouse.move(0, 0);
  const rest = await look(page, sel);
  await page.locator(sel).hover();
  const hover = await look(page, sel);
  await page.evaluate((s) => { const b = document.querySelector(s); dictating = { key: b.dataset.mic, base: "", stopping: false }; renderMain(); }, sel);
  await page.mouse.move(0, 0);
  await expect(page.locator(sel)).toHaveClass(/\bon\b/);
  const on = await look(page, sel);
  await page.evaluate(() => { dictating = null; renderMain(); });
  return { rest, hover, on };
}
for (const scheme of ["light", "dark"]) {
  test(`the Desk's mic looks as the Board's, at rest, on hover and listening (${scheme})`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await page.locator(".nrow").first().click();
    const board = await states(page, ".field [data-mic]");
    expect(board.rest.width).toBe("30px");
    expect(board.hover.background, "hover tints it").not.toBe(board.rest.background);
    expect(board.on.background, "listening lights it").not.toBe(board.rest.background);

    await openDesk(page);
    expect(await states(page, ".dk-box [data-mic]"), "the lead's box").toEqual(board);
    await page.locator('[data-desk-item="d2"]').click();
    await page.locator(".dk-right .dk-box [data-mic]").waitFor();
    expect(await states(page, ".dk-right .dk-box [data-mic]"), "an item's page").toEqual(board);
  });
}
