// The Board's group labels (IN CONVERSATION, NEWEST FIRST, WORKING, IDLE…) stay at the top of their column while
// you scroll through their group (#706).
import { test, expect } from "./cue.mjs";

/** Scrolls `scroller` a card's height down and says how far its first group label then sits from the column's top. */
const firstLabelAfterScroll = (page, scroller) => page.evaluate((s) => {
  const box = document.querySelector(s);
  box.scrollTop = 60;
  const sub = box.querySelector(".sec-body > .col-sub");
  const [b, l] = [box.getBoundingClientRect(), sub.getBoundingClientRect()];
  // Its band reaches both edges of the column, so no strip of a card passing under it shows beside it.
  const covers = l.left <= b.left + 0.5 && l.right >= b.right - 0.5;
  return { scrolled: box.scrollTop, gap: Math.round(l.top - b.top), covers };
}, scroller);

test("the Waiting and Sessions group labels stay at the top as their lists scroll", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await page.setViewportSize({ width: 1800, height: 420 });
  await expect(page.locator(".col-waiting .col-sub").first()).toBeVisible();
  const w = await firstLabelAfterScroll(page, ".col.split.col-waiting");
  expect(w.scrolled).toBe(60);
  expect(Math.abs(w.gap)).toBeLessThanOrEqual(1);
  expect(w.covers, "Waiting's label spans the column edge to edge").toBe(true);
  const s = await firstLabelAfterScroll(page, ".sec-sessions > .sec-body");
  expect(s.scrolled).toBe(60);
  expect(Math.abs(s.gap)).toBeLessThanOrEqual(1);
  expect(s.covers, "Sessions' label spans the column edge to edge").toBe(true);
});

test("a card Cue scrolls up to stops below the pinned label, not under it", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await page.setViewportSize({ width: 1000, height: 667 });
  await expect(page.locator('.nrow[data-big="n5"]')).toBeVisible();
  // The column at its end, so the second card's top is cut off above; then Cue shows it (as opening it does).
  await page.evaluate(() => { const c = document.querySelector(".col.split.col-waiting"); c.scrollTop = c.scrollHeight; });
  expect(await page.evaluate(() => document.querySelector('.nrow[data-big="n5"]').getBoundingClientRect().top
    < document.querySelector(".col.split.col-waiting").getBoundingClientRect().top)).toBe(true);
  await page.evaluate(() => glideIntoView(document.querySelector('.nrow[data-big="n5"]')));
  await expect.poll(() => page.evaluate(() => {
    const label = document.querySelector(".sec-waiting .sec-body > .col-sub").getBoundingClientRect();
    return Math.round(document.querySelector('.nrow[data-big="n5"]').getBoundingClientRect().top - label.bottom);
  })).toBeGreaterThanOrEqual(0);
});
