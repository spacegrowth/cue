// ⌘K from the Desk opens what it finds on the Desk, not the Board (#316).
import { test, expect, openDesk } from "./cue.mjs";

test("a session found with ⌘K on the Desk opens in the Desk's middle", async ({ cue, page }) => {
  await cue.open("desk");
  await openDesk(page);
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
  await page.evaluate(() => { deskSelect(""); renderMain(); });
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("site-lead");
  await page.keyboard.press("Enter");
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  await expect(page.locator(".dk-right")).toContainText("site-lead");
});

test("a session on another Desk isn't listed on this one", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.evaluate(() => { deskSelect("cue"); renderMain(); });
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("site-lead");
  await expect(page.locator('.lv-pop .lv-row[data-sid="site"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.locator('.switch-view [data-view="board"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').fill("site-lead");
  await expect(page.locator('.lv-pop .lv-row[data-sid="site"]')).toHaveCount(1);
});

test("on the Board it still opens there", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="board"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially("host-lead");
  await page.keyboard.press("Enter");
  await expect(page.locator('.switch-view [data-view="board"]')).toHaveClass(/on/);
});

test("on the Desk, ⌘K lists the Desk's items and its own sessions only; an item found opens there", async ({ cue, page }) => {
  await cue.open("desk");
  const d = await page.evaluate(() => { const d = state.desk.projects.find((x) => x.project === "cue"); return { item: d.items.find((i) => i.state !== "done" && i.state !== "dropped"), members: [d.lead?.sid, ...(d.members || [])] }; });
  await openDesk(page);
  await page.keyboard.press("Meta+k");
  await expect(page.locator(".lv-pop .lv-ghead").first()).toContainText("Items");
  // Sessions from outside this Desk aren't listed.
  for (const sid of await page.locator(".lv-pop .lv-row[data-sid]").evaluateAll((els) => els.map((e) => e.dataset.sid))) expect(d.members).toContain(sid);
  await page.locator('[data-text="find-live"]').pressSequentially(d.item.title.split(" ").slice(0, 2).join(" "));
  const row = page.locator(".lv-pop .lv-here", { hasText: d.item.title });
  await expect(row).toHaveCount(1);
  await row.click();
  await expect(page.locator(".lv-pop")).toHaveCount(0);
  await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/on/);
  expect(await page.evaluate(() => deskOpen)).toMatchObject({ kind: "item", id: d.item.id });
});

test("on the Desk, a kind and a number find items: \"#id\" picks it first, Enter opens it", async ({ cue, page }) => {
  await cue.open("desk");
  const it = await page.evaluate(() => state.desk.projects.find((x) => x.project === "cue").items.find((i) => i.kind && i.state !== "dropped"));
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.keyboard.press("Meta+k");
  await page.locator('[data-text="find-live"]').pressSequentially(it.kind);
  await expect(page.locator(".lv-pop .lv-here", { hasText: it.title })).toHaveCount(1);
  await page.locator('[data-text="find-live"]').fill("");
  await page.locator('[data-text="find-live"]').pressSequentially(`#${it.id}`);
  await expect(page.locator(".lv-pop .lv-here").first()).toContainText(it.title);
  await page.keyboard.press("Enter");
  expect(await page.evaluate(() => deskOpen)).toMatchObject({ kind: "item", id: it.id });
});
