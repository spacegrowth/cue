// The Inbox (ui/inbox.js): its list grouped Needs you → Working → Done, a line opening its session in the list's place
// on the Board's own chat pane, "N of M ›" stepping to the next that needs you, and Esc or ‹ Inbox back to the list.
import { test, expect } from "./cue.mjs";

/** The Inbox open from the header, its list drawn. */
const openInbox = async (page) => {
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator(".inbox .ib-row").first().waitFor();
};

test("the header's Inbox says how many need you, and its list groups them first", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const needs = await page.evaluate(() => inboxNeeds().length);
  expect(needs).toBeGreaterThan(1);
  await expect(page.locator('.switch-view [data-view="inbox"] .ib-count')).toHaveText(String(needs));
  await openInbox(page);
  await expect(page.locator(".ib-sect").first()).toHaveText(/needs you/i);
  // Needs you's lines, in needsYou()'s order, a session once each.
  const sids = await page.locator(".ib-row").evaluateAll((els) => els.map((el) => el.dataset.inboxOpen));
  expect(sids.slice(0, needs)).toEqual(await page.evaluate(() => inboxNeeds().map((i) => i.session_id)));
  expect(new Set(sids).size).toBe(sids.length);
});

test("a line opens its session on the Board's chat pane; › steps to the next; Esc goes back to the list", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  const first = await page.locator(".ib-row").first().getAttribute("data-inbox-open");
  await page.locator(".ib-row").first().click();
  await expect(page.locator(`.inbox-page[data-inbox-page="${first}"] .active-pane`)).toBeVisible();
  await expect(page.locator(".ib-next")).toHaveText(/^1 of \d+ ›$/);
  await expect(page.locator('.switch-view [data-view="inbox"]')).toHaveClass(/\bon\b/);
  await page.locator(".ib-next").click();
  await expect(page.locator(".ib-next")).toHaveText(/^2 of \d+ ›$/);
  const second = await page.locator(".inbox-page").getAttribute("data-inbox-page");
  expect(second).not.toBe(first);
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Escape");
  await expect(page.locator(".inbox .ib-row").first()).toBeVisible();
  // The line last opened is lit; nothing moved.
  await expect(page.locator(`.ib-row.on[data-inbox-open="${second}"]`)).toBeVisible();
  await page.locator(`.ib-row[data-inbox-open="${second}"]`).click();
  await page.locator(".ib-back").click();
  await expect(page.locator(".inbox .ib-row").first()).toBeVisible();
});

test("answered and at work again, the open session gives way to the list; still asking, it stays", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await openInbox(page);
  const sid = await page.locator(".ib-row").first().getAttribute("data-inbox-open");
  await page.locator(".ib-row").first().click();
  await expect(page.locator(`.inbox-page[data-inbox-page="${sid}"]`)).toBeVisible();
  // A redraw while it still asks: it stays open.
  await page.evaluate(() => renderMain());
  await expect(page.locator(`.inbox-page[data-inbox-page="${sid}"]`)).toBeVisible();
  // Its ask answered (gone from Cue's items) and the session working again: back to the list, its line under Working.
  await page.evaluate((id) => {
    state.items = state.items.filter((i) => i.session_id !== id);
    const s = sessionOf(id);
    s.state = "working";
    s.since_ms = now();
    document.activeElement?.blur();
    renderMain();
  }, sid);
  await expect(page.locator(".inbox-page")).toHaveCount(0);
  await expect(page.locator(`.ib-row.on[data-inbox-open="${sid}"] .ib-dot .pdot.live`)).toBeVisible();
});
