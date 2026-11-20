// An item's page has the quick phrases over its box, as a chat does (#242). A click sends that phrase at once, as its own
// message, the way the page's Send would; what you'd typed stays in the box, and a double click sends it once (#477).
import { test, expect } from "./cue.mjs";

/** What the item page's Send sent, by any of its ways (a note to its holder, sent back, or to the lead). */
const sentTexts = async (cue) => [...await cue.calls("desk_item_act"), ...await cue.calls("desk_send")]
  .map((c) => c.args.text || "").filter((t) => t && !/^(seen|open)$/.test(t));

async function openItem(cue, page, kind = "task") {
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
}

for (const [kind, scheme] of [["task", "light"], ["feature", "dark"], ["bug", "light"]]) {
  test(`a ${kind} page's quick phrase sends at once, on its own (${scheme})`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await openItem(cue, page, kind);
    const quick = page.locator(".dk-right .dk-box .quick .qp");
    await expect(quick).toHaveText(["go ahead", "show me"]);
    // Over the box, not under it.
    // (Both measured at one moment: the page can redraw between two separate reads.)
    const gap = () => page.evaluate(() => {
      const q = document.querySelector(".dk-right .dk-box .quick"), row = document.querySelector(".dk-right .dk-box-row");
      return q && row ? row.getBoundingClientRect().top + 1 - q.getBoundingClientRect().bottom : null;
    });
    await expect.poll(gap).not.toBeNull();
    expect(await gap()).toBeGreaterThanOrEqual(0);
    // Its words readable on the page in either theme.
    const colors = () => page.evaluate(() => { const b = document.querySelector(".dk-right .dk-box .quick .qp"), c = b && getComputedStyle(b); return c?.color ? [c.color, c.backgroundColor] : null; });
    await expect.poll(colors).not.toBeNull();
    const [fg, bg] = await colors();
    expect(fg).toBeTruthy();
    expect(fg).not.toBe(bg);
    await quick.nth(1).click();
    await expect.poll(() => sentTexts(cue)).toEqual(["show me"]);
    await expect(page.locator(".dk-right .dk-box textarea")).toHaveValue("");
  });
}

test("an item page's quick phrase, double clicked: sent once, nothing in the box", async ({ cue, page }) => {
  await openItem(cue, page);
  await page.locator('.dk-right .dk-box .qp[data-phrase="go ahead"]').dblclick();
  await expect.poll(() => sentTexts(cue)).toEqual(["go ahead"]);
  await page.waitForTimeout(300);
  expect(await sentTexts(cue)).toEqual(["go ahead"]);
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveValue("");
});

test("an item page's quick phrase with words typed in the box: only the phrase goes, the words stay as they were", async ({ cue, page }) => {
  await openItem(cue, page);
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("the header, and");
  await page.locator('.dk-right .dk-box .qp[data-phrase="show me"]').click();
  await expect.poll(() => sentTexts(cue)).toEqual(["show me"]);
  await expect(box).toHaveValue("the header, and");
  // Clicked again later: sent again on its own, never stacked in the box.
  await page.waitForTimeout(1600);
  await page.locator('.dk-right .dk-box .qp[data-phrase="show me"]').click();
  await expect.poll(() => sentTexts(cue)).toEqual(["show me", "show me"]);
  await expect(box).toHaveValue("the header, and");
  // Its own Send still sends what's typed.
  await page.locator(".dk-right .dk-box [data-desk-send]").click();
  await expect.poll(() => sentTexts(cue)).toEqual(["show me", "show me", "the header, and"]);
});

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
