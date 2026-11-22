// A big feature's items show together under its name (#282, option A of the #162 mocks): one row in its state's
// group with "n of m done" and a bar; opened, its pieces show under it, indented, in place; its page has a Pieces tab.
import { test, expect, openDesk } from "./cue.mjs";
import { groupOf } from "./desk.mjs";


test("the How work lands steps show as one feature with its pieces under it (#282)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    const d = state.desk.projects.find((x) => x.project === "cue");
    const now = Math.floor(Date.now() / 1000), day = 86400;
    const piece = (id, n, st, extra = {}) => ({ id, project: "cue", kind: "task", title: `How work lands ${n}`, state: st, created: now - 3 * day, updated: now - day, part_of: "253f", ...extra });
    // Done days ago: only in the feature's pieces, as the hub sends them.
    const old = ["246", "247", "248", "249", "252"].map((id, k) => piece(id, k + 1, "done", { done_at: now - 2 * day }));
    const working = piece("253", 8, "working", { holder: { kind: "session", sid: "host", name: "host" }, updated: now });
    d.items.push(working, { id: "253f", project: "cue", kind: "feature", title: "How work lands", state: "queued", created: now - 3 * day, updated: now, pieces: [...old, working] });
    renderMain();
  });
  const feat = page.locator('.dk-items .dk-it.dk-feat[data-desk-item="253f"]');
  // Working (a piece is at work, B2 #666), with how many are done on its status line; its working piece isn't a row of its own.
  await expect(feat).toHaveCount(1);
  await expect(feat.locator(".dk-fp")).toHaveText("5 of 6 done");
  await expect(feat.locator(".dk2b-ts .dk-fp")).toHaveCount(1);
  expect(await groupOf(page, "253f")).toBe("WORKING");
  await expect(page.locator('.dk-items > .dk-scroll > .dk-it[data-desk-item="253"]')).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-it.pc")).toHaveCount(0);
  // Opened (#418: the item at the right, in the Items list's place): its page on the Pieces tab, its pieces in order.
  await feat.click();
  await expect(page.locator('.dk-panel[data-side-for="253f"]')).toBeVisible();
  await expect(page.locator('.dk-itab[data-desk-itab="pieces"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-pcl .dk-pc .dk-pc-t")).toHaveText(["How work lands 1", "How work lands 2", "How work lands 3", "How work lands 4", "How work lands 5", "How work lands 8"]);
  await expect(page.locator(".dk-panel[data-side-for] .dk-pmeta .dk-fp.big")).toContainText("5 of 6 done");
  // A piece opened from them (an old done one too): its page says what it's part of.
  await page.locator('.dk-pcl .dk-pc[data-desk-item="247"]').click();
  await expect(page.locator('.dk-panel[data-side-for="247"]')).toBeVisible();
  await expect(page.locator(".dk-ipane .dk-split").first()).toContainText("Part of How work lands");
});
