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

test("+ New on a machine with hosted sessions on offers Hosted there, and starts it hosted on that machine", async ({ cue, page }) => {
  await cue.open("many-sessions", { frozen: true });
  await cue.change((st) => { st.settings = { ...st.settings, hosted: { on: true } }; });
  await page.keyboard.press("Meta+n");
  await page.locator('[data-lv="nf-more"]').click();
  await page.locator('[data-lv="machine"][data-machine="build-box"]').click();
  const where = page.locator('.nf-row:has(.nf-k:text-is("Runs in")) [data-lv="where"]');
  await expect(where).toHaveText(["Hosted", "Cue"]);
  await where.filter({ hasText: "Hosted" }).click();
  await expect(page.locator('.nf-row:has(.nf-k:text-is("Runs in")) .nf-hint')).toContainText("under cue-host");
  await page.locator('[data-text="new-cwd"]').fill("~/code/api");
  await page.locator('[data-lv="start"]').click();
  await cue.called("new_session", { machine: "build-box", hosted: true, cwd: "~/code/api" });
});

test("Hosted Claude Code in a folder it isn't trusted in: Cue asks first, and Trust and start starts it trusted", async ({ cue, page }) => {
  await cue.open("many-sessions", { frozen: true });
  await cue.change((st) => { st.settings = { ...st.settings, hosted: { on: true } }; });
  // The backend says the folder isn't trusted until the start says you trust it.
  await cue.override("new_session", (args) => {
    if (!args.trust) throw "untrusted folder: /Users/me/code/new-thing";
    return { session_id: "s-trusted", detail: "Started Claude Code, hosted in Cue" };
  });
  await page.keyboard.press("Meta+n");
  await page.locator('[data-lv="nf-more"]').click();
  await page.locator('[data-lv="where"][data-tmux="hosted"]').click();
  await page.locator('[data-text="new-cwd"]').fill("~/code/new-thing");
  await page.locator('[data-lv="start"]').click();
  await expect(page.locator("#trust-title")).toHaveText("Trust this folder?");
  await expect(page.locator(".end-dialog .end-text")).toContainText("~/code/new-thing");
  // Esc: no, and nothing starts.
  await page.keyboard.press("Escape");
  await expect(page.locator("#trust-title")).toHaveCount(0);
  expect((await cue.calls("new_session")).filter((c) => c.args.trust)).toHaveLength(0);
  await page.locator('[data-lv="start"]').click();
  await page.locator('[data-act="trust-start"]').click();
  await cue.called("new_session", { cwd: "~/code/new-thing", hosted: true, trust: true });
});
