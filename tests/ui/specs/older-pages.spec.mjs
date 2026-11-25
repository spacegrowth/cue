// A session's chat: Earlier messages loads the page before the oldest message shown and shows it, and pressed again
// goes further back, until the start of what Cue has (#530: the page used to go in above, out of view, so a press
// looked like it did nothing).
import { test, expect } from "./cue.mjs";

test("Earlier messages shows an older page, then another, until the start", async ({ cue, page }) => {
  await cue.open("hosted-relay");
  await page.locator('.nrow[data-big="n1"]').click();
  const chat = page.locator("[data-chat]");
  const older = chat.locator('[data-act="load-older"]');
  const msg = (n) => chat.getByText(new RegExp(`^Earlier message ${n}$`));
  // The fake log holds 75 earlier messages (1 oldest … 75 newest), 30 a page.
  await older.click();
  await expect(chat.getByText(/^Earlier message \d+$/)).toHaveCount(30);
  await expect(msg(46)).toBeInViewport();
  await expect(older).toBeInViewport();
  await older.click();
  await expect(chat.getByText(/^Earlier message \d+$/)).toHaveCount(60);
  await expect(msg(16)).toBeInViewport();
  await expect(older).toBeInViewport();
  await older.click();
  await expect(chat.getByText(/^Earlier message \d+$/)).toHaveCount(75);
  await expect(msg(1)).toBeInViewport();
  await expect(chat.locator(".cv-older")).toContainText("Start of the conversation in Cue");
});


// #530, sent back for flickering: a press on the button counted as you scrolling the chat (as a press on its scrollbar
// does), so a scroll while it was held, at the chat's top, loaded a page as scrolling up does, keeping your place (the
// view jumped down); then the press's own page put it back at the top and loaded another. One press: one page, and the
// view goes to its top without going anywhere else first.
test("One press of Earlier messages loads one page, and the view goes straight to it", async ({ cue, page }) => {
  await cue.open("hosted-relay");
  await page.locator('.nrow[data-big="n1"]').click();
  const chat = page.locator("[data-chat]");
  const older = chat.locator('[data-act="load-older"]');
  await chat.evaluate((c) => { c.scrollTop = 0; });
  await expect(older).toBeInViewport();
  await page.clock.runFor(1500);   // nothing of yours has moved the chat: what comes next is the press alone
  // Every place the chat goes from here on, and every page it asks for.
  await chat.evaluate((c) => { window.__tops = []; c.addEventListener("scroll", () => window.__tops.push(c.scrollTop)); });
  const asked = (await cue.calls("session_log_page")).length;
  for (const shown of [30, 60]) {
    const b = await older.boundingBox();
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    await page.mouse.down();
    // The browser telling of a scroll while the press is held (the chat kept in range as its top row changes).
    await chat.dispatchEvent("scroll");
    await page.mouse.up();
    await expect(chat.getByText(/^Earlier message \d+$/)).toHaveCount(shown);
    await expect(older).toBeInViewport();
  }
  await page.clock.runFor(500);
  expect((await cue.calls("session_log_page")).length - asked, "pages asked for, two presses").toBe(2);
  expect(await page.evaluate(() => window.__tops.filter((t) => t > 60)), "the view went down before going to the top").toEqual([]);
  // The row at the top is one height with the button or loading, so the swap doesn't nudge the chat below it.
  const row = chat.locator(".cv-older"), rowH = await row.evaluate((r) => r.offsetHeight);
  await cue.override("session_log_page", () => new Promise(() => {}));
  await older.click();
  await expect(row).toHaveText("Loading earlier messages…");
  expect(await row.evaluate((r) => r.offsetHeight), "the row's height, loading").toBe(rowH);
});
