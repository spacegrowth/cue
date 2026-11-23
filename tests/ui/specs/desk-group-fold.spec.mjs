// The Desk's Items (#430, C #748): DONE TODAY on the green folds to its heading line. A click on that line, or Enter,
// opens or folds it; its arrow turns, its count stays, and it stays as you left it, across restarts. Its rows slide in
// as it opens, at once with reduced motion.
import { test, expect, openDesk } from "./cue.mjs";

const fold = (page) => page.locator('.dk-items [data-desk-c-fold="done"]');
const doneRow = (page) => page.locator('.dk-items [data-desk-item="d7"]');
const moving = (page) => page.evaluate(() => [...document.head.querySelectorAll("style")].find((el) => el.textContent.includes("transition:none!important"))?.remove());

test("DONE TODAY folds and opens from its line; its count stays; it stays as you left it after a restart (#430)", async ({ page, cue }) => {
  await cue.open("desk");
  await moving(page);
  await openDesk(page);
  // Open at first: its line, its count, its rows; no hover text.
  await expect(fold(page)).toHaveAttribute("aria-expanded", "true");
  const n = await fold(page).locator(".dkn-dn").textContent();
  await expect(doneRow(page)).toBeVisible();
  await expect(fold(page).locator(".dkn-fa")).toHaveText("▾");
  await expect(page.locator(".dk-items [data-desk-c-fold][title], .dk-items [data-desk2b-group][title]")).toHaveCount(0);

  // Folded: its line, the count the same, no rows; nothing opened on the left.
  await fold(page).click();
  await expect(fold(page)).toHaveAttribute("aria-expanded", "false");
  await expect(doneRow(page)).toHaveCount(0);
  await expect(fold(page).locator(".dkn-fa")).toHaveText("▸");
  await expect(fold(page).locator(".dkn-dn")).toHaveText(n);
  await expect(page.locator(".dk-it.on")).toHaveCount(0);

  // A restart: still folded.
  await cue.open("desk");
  await openDesk(page);
  await expect(fold(page)).toHaveAttribute("aria-expanded", "false");
  await expect(doneRow(page)).toHaveCount(0);

  // By the keyboard too: Enter opens it, its rows slide in, and it stays open after a restart.
  await moving(page);
  await fold(page).focus();
  await page.keyboard.press("Enter");
  await expect(fold(page)).toHaveAttribute("aria-expanded", "true");
  await expect(doneRow(page)).toBeVisible();
  await expect(fold(page).locator(".dkn-dn")).toHaveText(n);
  await cue.open("desk");
  await openDesk(page);
  await expect(fold(page)).toHaveAttribute("aria-expanded", "true");
  await expect(doneRow(page)).toBeVisible();
  // The other groups' rows stay as they were.
  await expect(page.locator('.dk-items [data-desk-item="d3"]')).toBeVisible();
});

test("opened, DONE TODAY's rows slide in; with reduced motion they show at once (#430)", async ({ page, cue }) => {
  for (const reducedMotion of ["no-preference", "reduce"]) {
    await page.emulateMedia({ reducedMotion });
    await cue.open("desk");
    await moving(page);
    await openDesk(page);
    if ((await fold(page).getAttribute("aria-expanded")) === "true") await fold(page).click();
    await expect(doneRow(page)).toHaveCount(0);
    // Opened and read in one go, as the click draws it (a redraw after the slide drops its class).
    const anim = await page.evaluate(() => {
      document.querySelector('.dk-items [data-desk-c-fold="done"]').click();
      const el = document.querySelector('.dk-items [data-dkn-list="done"]');
      return el ? getComputedStyle(el).animationName : "not open";
    });
    await expect(doneRow(page)).toBeVisible();
    expect(anim, reducedMotion).toBe(reducedMotion === "reduce" ? "none" : "dkn-open");
    await fold(page).click();
    await expect(doneRow(page)).toHaveCount(0);
  }
});
