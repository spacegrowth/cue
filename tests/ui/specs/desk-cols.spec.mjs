// The Desk's two columns resize like the Board's (board/columns.js deskColDrag, styles/desk-cols.css): a grip in the gap
// between the lead's chat and Items trades their width, Items never under 520px and the chat never under 560px; the
// width is kept across reloads and a double-click on the grip puts back today's (the chat 878px at 1800 wide). The
// Board's own grips still work.
import { test, expect, openDesk } from "./cue.mjs";

const chat = (page) => page.locator(".board.dk-b2 > .dk-card").first();
const items = (page) => page.locator(".dk-card.dk-items");
const grip = (page) => page.locator('.board.dk-b2 [data-col-grip="items"]');
const width = (loc) => loc.evaluate((el) => Math.round(el.getBoundingClientRect().width));
/** Press on a grip's middle, drag it `dx` px, let go. */
async function drag(page, loc, dx) {
  const b = await loc.boundingBox();
  const x = b.x + b.width / 2, y = b.y + b.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y, { steps: 4 });
  await page.mouse.move(x + dx, y, { steps: 4 });
  await page.mouse.up();
}
const open = async (page, cue) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
};

test("the grip widens and narrows Items; the chat gives what Items takes", async ({ page, cue }) => {
  await open(page, cue);
  expect(await width(chat(page))).toBe(878);
  const i0 = await width(items(page));
  await drag(page, grip(page), -150);
  await expect.poll(() => width(items(page))).toBe(i0 + 150);
  expect(await width(chat(page))).toBe(878 - 150);
  await drag(page, grip(page), 250);
  await expect.poll(() => width(items(page))).toBe(i0 - 100);
  expect(await width(chat(page))).toBe(878 + 100);
  // The grip follows the gap after the chat.
  const [c, g] = await Promise.all([chat(page).boundingBox(), grip(page).boundingBox()]);
  expect(Math.round(g.x)).toBe(Math.round(c.x + c.width));
});

test("Items stops at 520px and the chat at 560px, and a narrower window keeps both", async ({ page, cue }) => {
  await open(page, cue);
  await drag(page, grip(page), 900);
  await expect.poll(() => width(items(page))).toBe(520);
  await drag(page, grip(page), -1400);
  await expect.poll(() => width(chat(page))).toBe(560);
  // Items as wide as it went, then the window narrows: the chat keeps its 560.
  await page.setViewportSize({ width: 1300, height: 1095 });
  await expect.poll(() => width(chat(page))).toBe(560);
  expect(await width(items(page))).toBeGreaterThanOrEqual(520);
  // Too narrow for both: today's split, no grip.
  await page.setViewportSize({ width: 1000, height: 1095 });
  await expect.poll(() => width(chat(page))).toBe(Math.round((960 - 28) * 1.9 / 3.75));
  await expect(grip(page)).toBeHidden();
});

test("the width is kept across a reload, and a double-click on the grip puts it back", async ({ page, cue }) => {
  await open(page, cue);
  const i0 = await width(items(page));
  await drag(page, grip(page), -200);
  await expect.poll(() => width(items(page))).toBe(i0 + 200);
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page);
  await expect.poll(() => width(items(page))).toBe(i0 + 200);
  await grip(page).dblclick();
  await expect.poll(() => width(chat(page))).toBe(878);
  expect(await width(items(page))).toBe(i0);
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await openDesk(page);
  await expect.poll(() => width(chat(page))).toBe(878);
});

test("the Board's grips still trade width and reset", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("board-waiting");
  const waiting = page.locator(".board .col-waiting");
  const wGrip = page.locator('.board [data-col-grip="waiting"]');
  await expect(wGrip).toHaveCount(1);
  const w0 = await width(waiting);
  // Waiting grows leftward up to its 480px; dragged right it narrows.
  await drag(page, wGrip, -100);
  await expect.poll(() => width(waiting)).toBe(480);
  await drag(page, wGrip, 140);
  await expect.poll(() => width(waiting)).toBe(340);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("cue.colWidths")).waiting)).toBeLessThan(1);
  await wGrip.dblclick();
  await expect.poll(() => width(waiting)).toBe(w0);
  // The Desk's width is its own.
  expect(await page.evaluate(() => localStorage.getItem("cue.deskCols"))).toBeNull();
});
