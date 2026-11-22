// The Desk's item list (#430, B2 #666): WAITS ON YOU, WORKING and UP NEXT are plain heads now, showing 3 rows and Show
// all N; DONE TODAY (and CALLS WHILE AWAY) fold to one line. A click on that line, or Enter, opens or folds it; its ›
// turns, its count stays, and it stays as you left it, across restarts.
import { test, expect, openDesk } from "./cue.mjs";

const fold = (page) => page.locator('.dk-items [data-desk2b-fold="done"]');
const doneRow = (page) => page.locator('.dk-items [data-desk-item="d7"]');
const moving = (page) => page.evaluate(() => [...document.head.querySelectorAll("style")].find((el) => el.textContent.includes("transition:none!important"))?.remove());

test("DONE TODAY folds and opens from its line; its count stays; it stays as you left it after a restart (#430)", async ({ page, cue }) => {
  await cue.open("desk");
  await moving(page);
  await openDesk(page);
  // The groups are heads, not folds: no fold control on them.
  for (const key of ["you", "working", "queued"]) {
    const h = page.locator(`.dk-items [data-desk2b-group="${key}"]`);
    await expect(h).toBeVisible();
    await expect(h).not.toHaveAttribute("aria-expanded", /./);
  }
  // Folded at first: its line, its count, no rows.
  await expect(fold(page)).toHaveAttribute("aria-expanded", "false");
  const n = await fold(page).locator("i").textContent();
  await expect(doneRow(page)).toHaveCount(0);
  // The › turns in a short slide; no hover text.
  expect(await fold(page).locator("span").evaluate((el) => getComputedStyle(el).transitionDuration)).toMatch(/^0\.15s/);
  await expect(page.locator(".dk-items [data-desk2b-fold][title], .dk-items [data-desk2b-group][title]")).toHaveCount(0);

  // Opened: its rows under it, the count the same; nothing opened on the left.
  await fold(page).click();
  await expect(fold(page)).toHaveAttribute("aria-expanded", "true");
  await expect(doneRow(page)).toBeVisible();
  await expect(fold(page).locator("i")).toHaveText(n);
  await expect(page.locator(".dk-it.on")).toHaveCount(0);

  // A restart: still open.
  await cue.open("desk");
  await openDesk(page);
  await expect(fold(page)).toHaveAttribute("aria-expanded", "true");
  await expect(doneRow(page)).toBeVisible();

  // By the keyboard too: Enter folds it, and it stays folded after a restart.
  await fold(page).focus();
  await page.keyboard.press("Enter");
  await expect(fold(page)).toHaveAttribute("aria-expanded", "false");
  await expect(doneRow(page)).toHaveCount(0);
  await expect(fold(page).locator("i")).toHaveText(n);
  await cue.open("desk");
  await openDesk(page);
  await expect(fold(page)).toHaveAttribute("aria-expanded", "false");
  // The other groups' rows stay as they were.
  await expect(page.locator('.dk-items [data-desk-item="d3"]')).toBeVisible();
});

test("with reduced motion the fold opens at once, no turn of its › (#430)", async ({ page, cue }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await cue.open("desk");
  await moving(page);
  await openDesk(page);
  expect(await fold(page).locator("span").evaluate((el) => getComputedStyle(el).transitionDuration)).toBe("0s");
  await fold(page).click();
  await expect(doneRow(page)).toBeVisible();
  await fold(page).click();
  await expect(doneRow(page)).toHaveCount(0);
});
