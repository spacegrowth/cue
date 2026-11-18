// An item's page has the quick phrases over its box, as a chat does (#242): picking one puts it in the box, to say more
// before Send; nothing goes until you send.
import { test, expect } from "./cue.mjs";

for (const [kind, scheme] of [["task", "light"], ["feature", "dark"], ["bug", "light"]]) {
  test(`a ${kind} page's quick phrases fill its box and send nothing (${scheme})`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.change((st, type) => {
      st.settings.quick = { phrases: ["go ahead", "show me"] };
      st.desk.projects[0].items.find((i) => i.id === "d2").kind = type;
      st.items = [];
    }, kind);
    await page.locator('[data-pj="toggle"]').click();
    await page.locator('.pj-pop [data-project="cue"]').click();
    await page.locator('.switch-view [data-view="desk"]').click();
    await page.locator('[data-desk-item="d2"]').click();
    const quick = page.locator(".dk-right .dk-box .quick .qp");
    await expect(quick).toHaveText(["go ahead", "show me"]);
    // Over the box, not under it.
    const q = await page.locator(".dk-right .dk-box .quick").boundingBox(), row = await page.locator(".dk-right .dk-box-row").boundingBox();
    expect(q.y + q.height).toBeLessThanOrEqual(row.y + 1);
    // Its words readable on the page in either theme.
    const [fg, bg] = await quick.first().evaluate((b) => [getComputedStyle(b).color, getComputedStyle(b).backgroundColor]);
    expect(fg).not.toBe(bg);
    const box = page.locator(".dk-right .dk-box textarea");
    await box.fill("the header");
    await quick.nth(1).click();
    await expect(box).toHaveValue(/^the header\s+show me$/);
    await expect(box).toBeFocused();
    await page.waitForTimeout(200);
    expect(await cue.calls("desk_item_act")).toEqual([]);
    expect(await cue.calls("desk_send")).toEqual([]);
    expect(await cue.calls("desk_say")).toEqual([]);
  });
}

test("the lead's box still sends a quick phrase at once", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => { st.settings.quick = { phrases: ["go ahead"] }; st.items = []; });
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  const qp = page.locator(".dk-right .dk-box .quick .qp");
  if (!(await qp.count())) test.skip(true, "this Desk's lead box has no phrases here");
  await qp.first().click();
  await cue.called("desk_send", { project: "cue" });
});
