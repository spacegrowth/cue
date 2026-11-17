import { test, expect } from "./cue.mjs";

test("two named Desks share a folder, keep separate items, and rename independently", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-home]").click();
  const workspaceFilter = await page.evaluate(() => projFilter);
  async function create(name) {
    await page.locator(".dh-add").click();
    await page.locator("#nd-folder").fill("/work/cue");
    await page.locator("#nd-name").fill(name);
    await page.locator('.nd [type="submit"]').click();
    await expect(page.locator(".nd")).toHaveCount(0);
    await expect(page.locator(".dk-sh b")).toHaveText(name);
    return page.evaluate(() => deskSelection());
  }
  const a = await create("Feature A");
  const qa = page.locator('.dk-qa-in[data-qa="line"]');
  await qa.fill("Only A");
  await qa.press("Enter");
  await expect(page.locator(".dk-items")).toContainText("Only A");
  await page.locator("[data-desk-home]").click();
  const b = await create("Feature B");
  expect(a).not.toBe(b);
  await expect(page.locator(".dk-items")).not.toContainText("Only A");
  await qa.fill("Only B");
  await qa.press("Enter");
  await expect(page.locator(".dk-items")).toContainText("Only B");
  await page.locator('[aria-label="Rename this Desk"]').click();
  await expect(page.locator(".pj-name-in")).toHaveValue("Feature B");
  await page.locator(".pj-name-in").fill("Bob's feature");
  await page.locator(".pj-name-in").press("Enter");
  await cue.called("desk_rename", { id: b, name: "Bob's feature" });
  await expect(page.locator(".dk-sh b")).toHaveText("Bob's feature");
  await page.locator("[data-desk-home]").click();
  await page.locator(`[data-desk-project="${a}"]`).click();
  await expect(page.locator(".dk-sh b")).toHaveText("Feature A");
  await expect(page.locator(".dk-items")).toContainText("Only A");
  await expect(page.locator(".dk-items")).not.toContainText("Only B");
  expect(await page.evaluate(() => projFilter)).toBe(workspaceFilter);
  expect(await cue.calls("set_setting")).toHaveLength(0);
});

test("a Desk can start in a folder with no recent project", async ({ cue, page }) => {
  await cue.open("world-empty");
  await page.locator("[data-new-menu]").click();
  await page.locator(".nw-pop [data-desk-new-desk]").click();
  await page.locator("#nd-folder").fill("/work/brand-new");
  await page.locator("#nd-name").fill("New workspace");
  await page.locator('.nd [type="submit"]').click();
  await cue.called("desk_start", { folder: "/work/brand-new", name: "New workspace" });
  await expect(page.locator(".dk-sh b")).toHaveText("New workspace");
});
