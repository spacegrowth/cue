// What Cue is doing with landed work (#783, desk/now_line.js): Cue still sends it (`landing_now`), but nothing shows
// over a Desk's columns: that space stays empty whatever runs, at a laptop's size and the smallest window. An item a
// step carries says that step on its own row instead.
import { test, expect, openDesk } from "./cue.mjs";

const SIZES = [{ width: 1440, height: 900 }, { width: 900, height: 600 }];
/** Desk `project`'s landing state as Cue sends it (null: none). */
const setLine = (cue, project, line) => cue.change((st, [p, l]) => { st.desk.projects.find((x) => x.project === p).landing_now = l; }, [project, line]);
/** What's drawn above the Desk's columns (the same with or without a landing step under way). */
const over = (page) => page.locator("#app .board.dk-c").evaluate((b) => { const o = []; for (let el = b.previousElementSibling; el; el = el.previousElementSibling) o.push(el.className); return o; });

for (const size of SIZES) {
  test(`nothing over the Desk's columns while main is tested, built, installed, or failed at ${size.width}×${size.height} (#783)`, async ({ page, cue }) => {
    await page.setViewportSize(size);
    await cue.open("desk");
    await openDesk(page);
    const top = async () => (await page.locator("#app .board.dk-c").boundingBox()).y;
    const y = await top(), before = await over(page);
    const since = (await page.evaluate(() => now())) - 4 * 60_000;
    for (const l of [
      { state: "run", what: "Testing main at 1e01e83", step: "Rust tests", since, note: "", items: ["d2", "d5"], cmd: false, also: "" },
      { state: "run", what: "Install", step: "", since, note: "", items: ["d5"], cmd: false, also: "" },
      { state: "fail", what: "Failed: desk::tests::header_stays", step: "", since, note: "on 1e01e83", items: ["d2"], cmd: false, also: "" },
    ]) {
      await setLine(cue, "cue", l);
      await expect.poll(() => page.evaluate(() => deskOf("cue").landing_now?.what)).toBe(l.what);
      expect(await over(page)).toEqual(before);
      await expect(page.locator("[data-desk-now-line], .dkl")).toHaveCount(0);
      expect(await top()).toBe(y);
    }
  });
}

test("an item a Desk's own step carries says that step on its row; no line over the columns (#783)", async ({ page, cue }) => {
  await page.setViewportSize(SIZES[0]);
  await cue.open("desk");
  await openDesk(page, "website");
  const began = (await page.evaluate(() => now())) - 60_000;
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "website").items.find((i) => i.id === "w1"), { state: "finishing", finish: { step: "landing", at: Date.now() } }); });
  await setLine(cue, "website", { state: "run", what: "After landing #w1", step: "make deploy", since: began, note: "", items: ["w1"], cmd: true, also: "" });
  await expect(page.locator('.dkn-items [data-desk-item="w1"]')).toContainText("On main · after landing: make deploy");
  await expect(page.locator('.dkn-items [data-desk-item="w1"] .dkl-cmd')).toHaveText("make deploy");
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "website").items.find((i) => i.id === "w1").finish.step = "checking"; });
  await setLine(cue, "website", { state: "run", what: "Checking #w1 before it lands", step: "npm test", since: began, note: "", items: ["w1"], cmd: true, also: "" });
  await expect(page.locator('.dkn-items [data-desk-item="w1"]')).toContainText("Checking before it lands: npm test");
  await expect(page.locator("[data-desk-now-line], .dkl")).toHaveCount(0);
});

test("on cue, an item on main says it waits on tests, then which test step it's in, then that Cue installs it; not just finishing (#783)", async ({ page, cue }) => {
  await page.setViewportSize(SIZES[0]);
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dkn-items [data-desk-item="d5"]');
  const fin = (f) => cue.change((st, ff) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "finishing", finish: ff }); }, f);
  await fin({ step: "queued", wait: "waiting for the full run", commit: "abc9999", at: Date.now() });
  await expect(row).toContainText("On main · waiting on tests");
  const since = (await page.evaluate(() => now())) - 60_000;
  await setLine(cue, "cue", { state: "run", what: "Testing main at 1e01e83", step: "Rust tests", since, note: "", items: ["d5"], cmd: false, also: "" });
  await expect(row).toContainText("On main · being tested · Rust tests");
  await expect(row.locator(".dkl-cmd")).toHaveCount(0);
  await setLine(cue, "cue", null);
  await fin({ step: "installing", commit: "abc9999", at: Date.now() });
  await expect(row).toContainText("On main · Install");
  await expect(row).not.toContainText("finishing");
  await expect(page.locator("[data-desk-now-line], .dkl")).toHaveCount(0);
});
