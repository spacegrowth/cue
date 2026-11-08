// + New opens short: a name, your three recent folders (the one it'll use filled), and Start. A click on another
// folder picks it without moving the others; … opens All options at the folder field.
import { test, expect } from "./cue.mjs";

test("+ New starts in the recent folder you clicked, the others staying where they were", async ({ cue, page }) => {
  await cue.open("many-sessions", { frozen: true });
  await page.keyboard.press("Meta+n");
  const pills = page.locator(".nf-recent .nf-rf[data-cwd]");
  await expect(pills).toHaveCount(3);
  const before = await pills.allInnerTexts();
  await pills.nth(1).click();
  await expect(pills.nth(1)).toHaveClass(/\bon\b/);
  expect(await pills.allInnerTexts()).toEqual(before);
  const cwd = await pills.nth(1).getAttribute("data-cwd");
  await page.locator('[data-lv="start"]').click();
  await cue.called("new_session", { cwd });
});

test("+ New's … opens All options with the folder field ready to type", async ({ cue, page }) => {
  await cue.open("many-sessions", { frozen: true });
  await page.keyboard.press("Meta+n");
  await page.locator('[data-lv="folder-more"]').click();
  await expect(page.locator('[data-text="new-cwd"]')).toBeFocused();
});

test("All options' Folder field lists the folders on disk as you type, and Tab goes on into one", async ({ cue, page }) => {
  await cue.open("many-sessions", { frozen: true });
  await page.keyboard.press("Meta+n");
  await page.locator('[data-lv="nf-more"]').click();
  const field = page.locator('[data-text="new-cwd"]');
  await field.fill("~/code/api-t");
  await expect(page.locator(".nf-drop .nf-dopt")).toHaveText(["~/code/api-tools"]);
  await field.press("Tab");
  await expect(field).toHaveValue("~/code/api-tools/");
  await expect(field).toBeFocused();
  await expect(page.locator(".nf-drop .nf-dopt")).toHaveText(["~/code/api-tools/cli", "~/code/api-tools/lib"]);
  await field.press("ArrowDown"); await field.press("ArrowDown"); await field.press("Enter");
  await expect(field).toHaveValue("~/code/api-tools/lib");
  await page.locator('[data-lv="start"]').click();
  await cue.called("new_session", { cwd: "~/code/api-tools/lib" });
});
