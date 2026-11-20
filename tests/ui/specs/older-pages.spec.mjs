// A session's chat: Earlier messages loads the page before the oldest message shown and shows it, and pressed again
// goes further back, until the start of what Cue has (#530: the page used to go in above, out of view, so a press
// looked like it did nothing).
import { test, expect } from "./cue.mjs";

test("Earlier messages shows an older page, then another, until the start", async ({ cue, page }) => {
  await cue.open("terminal");
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

