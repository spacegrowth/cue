// A big feature's items show together under its name (#282, option A of the #162 mocks): one row in its state's
// group with "n of m done" and a bar where it stands; its pieces aren't rows of their own; opened, its page says it too.
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
  const feat = page.locator('.dkn-items .dkn-row[data-desk-item="253f"]');
  // Working (a piece is at work, B2 #666), with how many are done where it stands; its working piece isn't a row of its own.
  await expect(feat).toHaveCount(1);
  await expect(feat.locator(".dk-fp")).toHaveText("5 of 6 done");
  await expect(feat.locator(".dkn-now .dk-fp")).toHaveCount(1);
  expect(await groupOf(page, "253f")).toBe("WORKING");
  await expect(page.locator('.dkn-items [data-desk-item="253"]')).toHaveCount(0);
  await expect(page.locator(".dkn-items .dk-it.pc")).toHaveCount(0);
  // Opened (nobody holds it): its page says how many are done.
  await feat.locator(".dkn-tt").click();
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="item"] .dkn-smeta')).toContainText("5 of 6 done");
});
