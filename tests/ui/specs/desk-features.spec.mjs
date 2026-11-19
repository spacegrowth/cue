// A big feature's items show together under its name (#282, option A of the #162 mocks): one row in its state's
// group with "n of m done" and a bar; opened, its pieces show under it, indented, in place; its page has a Pieces tab.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

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
  // In progress (a piece is at work), with how many are done; its working piece isn't a row of its own.
  await expect(feat).toHaveCount(1);
  await expect(feat.locator(".dk-fp")).toHaveText("5 of 6 done");
  expect(await feat.evaluate((el) => { let h = el.previousElementSibling; while (h && !h.classList.contains("dk-gh")) h = h.previousElementSibling; return h?.textContent || ""; })).toMatch(/^In progress/);
  await expect(page.locator('.dk-items > .dk-scroll > .dk-it[data-desk-item="253"]')).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-it.pc")).toHaveCount(0);
  // Opened: lit, its pieces under it in order, and the Pieces tab on its page.
  await feat.click();
  await expect(feat).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-pcs .dk-it.pc .dk-t")).toHaveText(["How work lands 1", "How work lands 2", "How work lands 3", "How work lands 4", "How work lands 5", "How work lands 8"]);
  await expect(page.locator('.dk-itab[data-desk-itab="pieces"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-pcl .dk-pc")).toHaveCount(6);
  await expect(page.locator(".dk-state .dk-fp.big")).toContainText("5 of 6 done");
  // A piece opened (an old done one too): the feature's list stays open, the piece lit in place, and it says what it's part of.
  await page.locator('.dk-pcs .dk-it.pc[data-desk-item="247"]').click();
  await expect(page.locator('.dk-pcs .dk-it.pc[data-desk-item="247"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-ipane .dk-split").first()).toContainText("Part of How work lands");
});
