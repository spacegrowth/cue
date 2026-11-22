// The Board's group labels (IN CONVERSATION, NEWEST FIRST, WORKING, IDLE…) stay at the top of their column while
// you scroll through their group (#706).
import { test, expect } from "./cue.mjs";

/** Scrolls `scroller` a card's height down and says how far its first group label then sits from the column's top. */
const firstLabelAfterScroll = (page, scroller) => page.evaluate((s) => {
  const box = document.querySelector(s);
  box.scrollTop = 60;
  const sub = box.querySelector(".sec-body > .col-sub");
  return { scrolled: box.scrollTop, gap: Math.round(sub.getBoundingClientRect().top - box.getBoundingClientRect().top) };
}, scroller);

test("the Waiting and Sessions group labels stay at the top as their lists scroll", async ({ cue, page }) => {
  await cue.open("many-sessions");
  await page.setViewportSize({ width: 1800, height: 420 });
  await expect(page.locator(".col-waiting .col-sub").first()).toBeVisible();
  const w = await firstLabelAfterScroll(page, ".col.split.col-waiting");
  expect(w.scrolled).toBe(60);
  expect(Math.abs(w.gap)).toBeLessThanOrEqual(1);
  const s = await firstLabelAfterScroll(page, ".sec-sessions > .sec-body");
  expect(s.scrolled).toBe(60);
  expect(Math.abs(s.gap)).toBeLessThanOrEqual(1);
});
