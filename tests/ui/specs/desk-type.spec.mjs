// Clicking the Desk's text box puts the cursor in it and you can type (#411): the lead's chat and an item's page,
// with the list redrawing many times a second (agents at work), as it does in the app.
import { test, expect, openDesk } from "./cue.mjs";

// The hub's state comes in again and again at its own pace (about every 0.3 s), the items changing as they do while work goes on.
const churn = (page) => page.evaluate(() => {
  window.__churn = setInterval(() => {
    for (const it of CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items) it.updated = (it.updated || 0) + 1;
    CueBridge.emit("state", CueBridge.world.snapshot());
  }, 300);
});
// Nothing over the window: the box is what's under the pointer, with a text cursor; a button shows the hand.
const pointerFine = async (page, box) => {
  expect(await box.evaluate((t) => { const r = t.getBoundingClientRect(); return document.elementFromPoint(r.x + 20, r.y + r.height / 2) === t; })).toBe(true);
  expect(await box.evaluate((t) => getComputedStyle(t).cursor)).toMatch(/^(auto|text)$/);
  expect(await page.evaluate(() => { const b = document.querySelector(".dkn-items .dkn-ih [data-desk-c-line]"); return [getComputedStyle(b).cursor, document.elementFromPoint(b.getBoundingClientRect().x + 4, b.getBoundingClientRect().y + 4) === b || b.contains(document.elementFromPoint(b.getBoundingClientRect().x + 4, b.getBoundingClientRect().y + 4))]; })).toEqual(["pointer", true]);
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
  const box = page.locator(".dk-right :is(.dk-box, .foot:not(.held) .composer) textarea[data-text]").first();
  await expect(box).toHaveAttribute("data-text", 'desk:["cue",""]');
  await clickAndType(page, box);
  // An item's page: its own box.
  await page.locator('.dkn-items [data-desk-item="d3"] .dkn-tt').click();
  await expect(box).toHaveAttribute("data-text", /"d3"\]$/);
  await clickAndType(page, box);
  // Back (the item opened in the chat's place, Items beside it): the lead's chat: what you typed is still there.
  await page.locator(".dk-right .dk-back").click();
  await expect(box).toHaveAttribute("data-text", 'desk:["cue",""]');
  await expect(box).toHaveValue("the dot is too big");
  // The Progress popover open (redrawing too) and closed again: the box still takes the cursor. Items has no Progress
  // button in its head (#666); deskProgToggle still opens it.
  await expect(page.locator('.dkn-items [data-desk-prog-toggle]')).toHaveCount(0);
  await page.evaluate(() => deskProgToggle(true));
  await expect(page.locator(".dk-prog")).toBeVisible();
  await page.waitForTimeout(300);
  await page.evaluate(() => deskProgToggle(false));
  await expect(page.locator(".dk-prog")).toHaveCount(0);
  await expect(box).toHaveValue("the dot is too big");
  await box.fill("");
  await clickAndType(page, box);
  await expect(box).toHaveValue("the dot is too big");
});
