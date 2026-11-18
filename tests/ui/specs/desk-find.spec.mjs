// ⌘K from the Desk opens what it finds on the Desk, not the Board (#316).
import { test, expect } from "./cue.mjs";

test("a session found with ⌘K on the Desk opens in the Desk's middle", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("host-lead");
  await page.locator('.lv-pop .lv-row[data-sid="host"]').click();
  await expect(page.locator(".lv-pop")).toHaveCount(0);
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  await expect(page.locator(".dk-right")).toContainText("host-lead");
});

test("with All Desks shown, a found session opens on its own Desk", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("site-lead");
  await page.keyboard.press("Enter");
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  await expect(page.locator(".dk-right")).toContainText("site-lead");
});

test("on the Board it still opens there", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="board"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("host-lead");
  await page.keyboard.press("Enter");
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/on/);
});
