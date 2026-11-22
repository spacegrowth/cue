// The new Desk layout (#738, desk/c.js): "Try the new Desk layout" in Desk settings, off by default and remembered. On:
// the Desk's sessions at the left on the green (the lead on top, a heading per machine), the lead's chat with a panel of
// only Now and Activity, Items as one card (Waiting on you and Working, six steps a row) with Up next and Done today side
// by side under it, each folding to its heading; the header's count the Desk's own. Off: today's Desk, untouched.
import { test, expect, openDesk } from "./cue.mjs";

/** Desk settings, from the header's Desks menu. */
async function settings(page) {
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-set]").click();
  return page.locator(".ld-dialog [data-desk-c-try]");
}
/** The new layout turned on from Desk settings, which then close. */
async function turnOn(page) {
  const sw = await settings(page);
  await expect(sw).toHaveText("Try the new Desk layout");
  await expect(sw).toHaveAttribute("aria-checked", "false");
  await sw.click();
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(page.locator(".ld-dialog")).toHaveCount(0);
}
const open = async (page, cue, size = { width: 1800, height: 1095 }) => {
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await turnOn(page);
};

test("off by default; on, the three columns; off again, today's Desk", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(0);
  await expect(page.locator(".dk-board.dk-b2 .dk2b-tabs")).toHaveCount(1);
  await turnOn(page);
  const [s, c, i] = await Promise.all([".dk-c > .dkn-side", ".dk-c > .dk-right", ".dk-c > .dkn-items"].map((q) => page.locator(q).boundingBox()));
  expect(s.x + s.width).toBeLessThan(c.x);
  expect(c.x + c.width).toBeLessThan(i.x);
  // No tabs and no List | Cards: every group is on screen.
  await expect(page.locator(".dk-c .dk2b-tabs, .dk-c .dk-cards-sw")).toHaveCount(0);
  // Remembered across a reload.
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
  // Off: today's Desk is back.
  const sw = await settings(page);
  await expect(sw).toHaveAttribute("aria-checked", "true");
  await sw.click();
  await page.keyboard.press("Escape");
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(0);
  await expect(page.locator(".dk-board.dk-b2 .dk2b-tabs")).toHaveCount(1);
});

test("the sessions column: the lead on top under a line, a heading per machine, a click opens a chat", async ({ page, cue }) => {
  await open(page, cue);
  const side = page.locator(".dkn-side");
  await expect(side.locator(".dkn-lead")).toContainText("web-lead");
  await expect(side.locator(".dkn-lead")).toHaveClass(/\bon\b/);
  await expect(side.locator(".dkn-rule")).toHaveCount(1);
  await expect(side.locator(".dkn-mg").first()).toHaveText(/^THIS MAC· \d+ (of \d+ )?busy$/);
  const host = side.locator('[data-desk-chat-of="host"]');
  await expect(host).toContainText("#d5 · Building");
  await host.click();
  await expect(host).toHaveClass(/\bon\b/);
  await expect(side.locator(".dkn-lead")).not.toHaveClass(/\bon\b/);
  // Its item is lit in the list, and its chat is in the middle.
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d5"]')).toHaveCount(1);
  await expect(page.locator(".dk-c > .dk-right .dk-ph")).toContainText("host-lead");
  // The lead's block brings its chat back.
  await side.locator(".dkn-lead").click();
  await expect(side.locator(".dkn-lead")).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-c > .dk2b-chat")).toHaveCount(1);
});

test("one count: the header, the lead's strip and Waiting on you say the same; the lead's panel is Now and Activity", async ({ page, cue }) => {
  await open(page, cue);
  const head = page.locator(".top [data-desk-c-head]");
  await expect(head).toHaveText(/^\d+ wait on you$/);
  const n = (await head.textContent()).match(/^\d+/)[0];
  await expect(page.locator('.dkn-card .dkn-gh[data-desk2b-group="you"] span')).toHaveText(n);
  const panel = page.locator(".dk-c .dk2b-chat .dk-panel");
  await expect(panel.locator(".dk-pnow")).toHaveCount(1);
  await expect(panel.locator(".dk-psh")).not.toContainText([/Waits on you/, /^Working/]);
  // The header's count opens the first that waits on you.
  await head.click();
  await expect(page.locator('.dk-c > .dk-right [data-desk-page="item"]')).toHaveCount(1);
  // Hidden panel: the strip says the same number.
  await page.locator(".dk-c .dk-right .dk-back, .dk-c .dk-right [data-desk-back]").first().click();
  await page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]').click();
  await expect(page.locator(".dk2b-chat .dk2b-wn")).toHaveText(`${n} wait on you`);
});

test("Items: a row per item with its six steps, the one it's at striped; Check opens it", async ({ page, cue }) => {
  await open(page, cue);
  const card = page.locator(".dkn-card");
  await expect(card.locator(".dkn-th")).toContainText("Picked up");
  const d2 = card.locator('.dkn-row[data-desk-item="d2"]');
  await expect(d2.locator(".dkn-stp")).toHaveCount(6);
  await expect(d2.locator(".dkn-stp.done")).toHaveCount(5);
  await expect(d2.locator(".dkn-stp.cur")).toHaveCount(1);
  // The step it's at in the Board's waiting stripes.
  const bg = await d2.locator(".dkn-stp.cur i").evaluate((el) => getComputedStyle(el).backgroundImage);
  expect(bg).toContain("repeating-linear-gradient");
  await d2.locator("[data-desk-chk-item]").click();
  await expect(page.locator(".dk-c > .dk-right .dk-ph")).toContainText("#d2");
  await expect(d2).toHaveClass(/\bon\b/);
});

test("Up next and Done today sit side by side on the green and fold to their headings, remembered", async ({ page, cue }) => {
  await open(page, cue);
  const next = page.locator('[data-dkn-drawer="next"]'), done = page.locator('[data-dkn-drawer="done"]');
  const [a, b] = await Promise.all([next.boundingBox(), done.boundingBox()]);
  expect(Math.round(a.y)).toBe(Math.round(b.y));
  expect(a.x + a.width).toBeLessThan(b.x);
  // On the green: no card under them.
  expect(await next.evaluate((el) => getComputedStyle(el.closest(".dkn-low")).backgroundColor)).toBe("rgba(0, 0, 0, 0)");
  await expect(next.locator(".dkn-nx")).toHaveCount(2);
  await expect(next.locator(".dkn-dh")).toContainText("drag to reorder");
  const head = next.locator("[data-desk-c-fold]");
  await head.click();
  await expect(head).toHaveAttribute("aria-expanded", "false");
  await expect(next.locator(".dkn-nx")).toHaveCount(0);
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page).catch(() => {});
  await expect(page.locator('[data-dkn-drawer="next"] [data-desk-c-fold]')).toHaveAttribute("aria-expanded", "false");
  await page.locator('[data-dkn-drawer="next"] [data-desk-c-fold]').click();
  await expect(page.locator('[data-dkn-drawer="next"] .dkn-nx')).toHaveCount(2);
});

test("a laptop's window: the sessions a strip, Items never under 520; the smallest window fits", async ({ page, cue }) => {
  await open(page, cue, { width: 1440, height: 900 });
  const w = (q) => page.locator(q).evaluate((el) => Math.round(el.getBoundingClientRect().width));
  expect(await w(".dk-c > .dkn-side")).toBe(64);
  expect(await w(".dk-c > .dkn-items")).toBeGreaterThanOrEqual(520);
  await expect(page.locator(".dkn-side .dkn-head")).toBeHidden();
  await page.setViewportSize({ width: 900, height: 600 });
  await expect.poll(() => w(".dk-c > .dkn-side")).toBe(64);
  // The cards redraw once they've measured themselves after a resize.
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(900);
  // Narrow, the steps are today's short bar with its words.
  await expect(page.locator('.dkn-row[data-desk-item="d2"] .dkn-bar')).toBeVisible();
  await expect(page.locator('.dkn-row[data-desk-item="d2"] .dkn-steps')).toBeHidden();
});

test("no tooltips on the new layout", async ({ page, cue }) => {
  await open(page, cue);
  await expect(page.locator(".dk-c [title]")).toHaveCount(0);
  await expect(page.locator(".top [data-desk-c-head][title]")).toHaveCount(0);
});
