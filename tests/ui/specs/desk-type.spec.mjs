// Clicking the Desk's text box puts the cursor in it and you can type (#411): the lead's chat and an item's page,
// with the list redrawing many times a second (agents at work), as it does in the app.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
// The hub's state comes in again and again, the items changing as they do while work goes on.
const churn = (page) => page.evaluate(() => {
  window.__churn = setInterval(() => {
    for (const it of CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items) it.updated = (it.updated || 0) + 1;
    CueBridge.emit("state", CueBridge.world.snapshot());
  }, 40);
});
// Nothing over the window: the box is what's under the pointer, with a text cursor; a button shows the hand.
const pointerFine = async (page, box) => {
  expect(await box.evaluate((t) => { const r = t.getBoundingClientRect(); return document.elementFromPoint(r.x + 20, r.y + r.height / 2) === t; })).toBe(true);
  expect(await box.evaluate((t) => getComputedStyle(t).cursor)).toMatch(/^(auto|text)$/);
  expect(await page.locator('.dk-items [data-desk-items-view="list"]').evaluate((b) => [getComputedStyle(b).cursor, document.elementFromPoint(b.getBoundingClientRect().x + 4, b.getBoundingClientRect().y + 4) === b || b.contains(document.elementFromPoint(b.getBoundingClientRect().x + 4, b.getBoundingClientRect().y + 4))])).toEqual(["pointer", true]);
  expect(await page.evaluate(() => [getComputedStyle(document.body).cursor, getComputedStyle(document.body).pointerEvents])).toEqual(["auto", "auto"]);
};
const clickAndType = async (page, box) => {
  await pointerFine(page, box);
  await page.evaluate(() => document.activeElement?.blur());
  await expect(box).not.toBeFocused();
  await box.click();
  await expect(box).toBeFocused();
  await page.keyboard.type("the dot ");
  await page.waitForTimeout(200);
  await page.keyboard.type("is too big");
  await expect(box).toHaveValue("the dot is too big");
};

test("clicking the lead's text box and an item's puts the cursor there and typing works, while the list redraws", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await churn(page);
  const box = page.locator(".dk-right .dk-box textarea[data-text]").first();
  await expect(box).toHaveAttribute("data-text", 'desk:["cue",""]');
  await clickAndType(page, box);
  // An item's page: its own box.
  await page.locator('.dk-items [data-desk-item="d3"] .dk-t').click();
  await expect(box).toHaveAttribute("data-text", /"d3"\]$/);
  await clickAndType(page, box);
  // The Progress view (the whole board, redrawing too) and back: the box still takes the cursor.
  await page.locator('[data-desk-lead-chat]').first().click();
  await page.locator('.dk-items [data-desk-items-view="progress"]').click();
  await expect(page.locator(".dk-prog")).toBeVisible();
  await page.waitForTimeout(300);
  await page.locator('.dk-items [data-desk-items-view="list"]').click();
  await expect(box).toHaveValue("the dot is too big");
  await box.fill("");
  await clickAndType(page, box);
  // Back to the lead's chat: what you typed is still there, and the box still takes the cursor.
  await page.locator('[data-desk-lead-chat]').first().click();
  await expect(box).toHaveAttribute("data-text", 'desk:["cue",""]');
  await expect(box).toHaveValue("the dot is too big");
});
