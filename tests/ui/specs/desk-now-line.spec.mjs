// The line at the top of each Desk (#783, desk/now_line.js): what Cue is doing with landed work, in that Desk's own
// steps. On cue its full run ("Testing main at 1e01e83 · Rust tests · started 16:57"), its build and install, or what
// failed and whether that run was on an older commit; on a Desk with steps of its own, which of them runs; on one with
// none, nothing. Under it, the items that step carries, each opening its item. At a laptop's size and the smallest
// window, in light and dark: on one line, inside the window, no hover text and no coloured left edge.
import { test, expect, openDesk } from "./cue.mjs";

const SIZES = [{ width: 1440, height: 900 }, { width: 900, height: 600 }];
/** Desk `project`'s top line as Cue sends it (null: none). */
const setLine = (cue, project, line) => cue.change((st, [p, l]) => { st.desk.projects.find((x) => x.project === p).landing_now = l; }, [project, line]);
const lineOf = (page) => page.locator("[data-desk-now-line]");
const at = (page, ms) => page.evaluate((t) => clockAt(t), ms);

test("cue's Desk says its full run, its build and install, and a failure with its commit (#783)", async ({ page, cue }) => {
  await page.setViewportSize(SIZES[0]);
  await cue.open("desk");
  await openDesk(page);
  // None sent: no line, the columns where they were.
  await expect(lineOf(page)).toHaveCount(0);
  const began = (await page.evaluate(() => now())) - 4 * 60_000;
  await setLine(cue, "cue", { state: "run", what: "Testing main at 1e01e83", step: "Rust tests", since: began, note: "", items: ["d2", "d5"], cmd: false, also: "" });
  const line = lineOf(page);
  await expect(line).toHaveAttribute("data-desk-now-line", "run");
  await expect(line.locator(".dkl-what")).toHaveText("Testing main at 1e01e83");
  await expect(line.locator(".dkl-p")).toHaveText(["Rust tests", `started ${await at(page, began)}`]);
  // The items it carries, by number and title; a click opens one.
  await expect(line.locator(".dkl-it")).toHaveCount(2);
  await expect(line.locator(".dkl-it").first()).toContainText("#d2");
  await expect(line.locator(".dkl-it").first()).toContainText(await page.evaluate(() => deskItem("cue", "d2").title));
  await line.locator('.dkl-it[data-desk-item="d5"]').click();
  await expect(page.locator(".dk-right .dkn-ib")).toBeVisible();

  // Then the build and the install.
  await setLine(cue, "cue", { state: "run", what: "Building Cue", step: "", since: began, note: "", items: ["d5"], cmd: false, also: "" });
  await expect(line.locator(".dkl-what")).toHaveText("Building Cue");
  await setLine(cue, "cue", { state: "run", what: "Installing Cue", step: "", since: began, note: "", items: ["d5"], cmd: false, also: "" });
  await expect(line.locator(".dkl-what")).toHaveText("Installing Cue");

  // Failed: its first test, its commit, that main moved past it and it's ignored.
  const note = "on 1e01e83 · an older commit than main (3669aeb), so it's ignored till main's own run";
  await setLine(cue, "cue", { state: "fail", what: "Failed: desk::tests::header_stays and 1 more", step: "", since: began, note, items: ["d2"], cmd: false, also: "" });
  await expect(line).toHaveAttribute("data-desk-now-line", "fail");
  await expect(line.locator(".dkl-what")).toHaveText("Failed: desk::tests::header_stays and 1 more");
  await expect(line.locator(".dkl-note")).toHaveText(note);
  await expect(line.locator(".dkl-p")).toHaveText(`ended ${await at(page, began)}`);
  await expect(page.locator(".dkl [title]")).toHaveCount(0);
});

test("a Desk with steps of its own says which runs, and the item it carries says so too; one with none shows nothing (#783)", async ({ page, cue }) => {
  await page.setViewportSize(SIZES[0]);
  await cue.open("desk");
  await openDesk(page, "website");
  await expect(lineOf(page)).toHaveCount(0);
  await setLine(cue, "website", { state: "idle", what: "Check before landing: npm test, then npm run lint · After landing: make deploy", step: "", since: 0, note: "", items: [], cmd: false, also: "" });
  await expect(lineOf(page)).toHaveAttribute("data-desk-now-line", "idle");
  await expect(lineOf(page).locator(".dkl-what")).toHaveText("Check before landing: npm test, then npm run lint · After landing: make deploy");
  await expect(lineOf(page).locator(".dkl-2")).toHaveCount(0);
  // w1 lands: its own after-landing step runs, named as typed; its row in Items says it instead of just finishing.
  const began = (await page.evaluate(() => now())) - 60_000;
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "website").items.find((i) => i.id === "w1"), { state: "finishing", finish: { step: "landing", at: Date.now() } }); });
  await setLine(cue, "website", { state: "run", what: "After landing #w1", step: "make deploy", since: began, note: "", items: ["w1"], cmd: true, also: "" });
  await expect(lineOf(page).locator(".dkl-p.dkl-cmd")).toHaveText("make deploy");
  await expect(lineOf(page).locator(".dkl-p.dkl-t")).toHaveText(`started ${await at(page, began)}`);
  await expect(page.locator('.dkn-items [data-desk-item="w1"]')).toContainText("On main · after landing: make deploy");
  // A check under way on it reads the same way.
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "website").items.find((i) => i.id === "w1").finish.step = "checking"; });
  await setLine(cue, "website", { state: "run", what: "Checking #w1 before it lands", step: "npm test", since: began, note: "", items: ["w1"], cmd: true, also: "" });
  await expect(page.locator('.dkn-items [data-desk-item="w1"]')).toContainText("Checking before it lands: npm test");
  // A Desk with no check and nothing after landing: Cue sends none, and there's no line.
  await setLine(cue, "website", null);
  await expect(lineOf(page)).toHaveCount(0);
});

for (const size of SIZES) {
  for (const scheme of ["light", "dark"]) {
    test(`the line sits over the columns at ${size.width}×${size.height} in ${scheme}: inside the window, no coloured left edge (#783)`, async ({ page, cue }) => {
      await page.setViewportSize(size);
      await page.emulateMedia({ colorScheme: scheme });
      await cue.open("desk");
      await openDesk(page);
      const since = (await page.evaluate(() => now())) - 4 * 60_000;
      await setLine(cue, "cue", { state: "run", what: "Testing main at 1e01e83", step: "Rust tests", since, note: "", items: ["d2", "d5"], cmd: false, also: "" });
      const line = lineOf(page);
      await expect(line).toBeVisible();
      const box = await line.boundingBox(), board = await page.locator(".board.dk-c").boundingBox();
      // Over the columns, inside the window's width, the first row on one line.
      expect(box.y + box.height).toBeLessThanOrEqual(board.y);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(size.width);
      const row = await line.locator(".dkl-1").boundingBox(), lh = await line.locator(".dkl-what").evaluate((el) => parseFloat(getComputedStyle(el).lineHeight));
      expect(row.height).toBeLessThanOrEqual(lh + 1);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      // Readable on the ground in either theme: its words in the ground's own text colour.
      const [ink, ground] = await page.evaluate(() => [getComputedStyle(document.querySelector(".dkl-what")).color, getComputedStyle(document.body).getPropertyValue("--on-bg").trim()]);
      expect(ink).not.toBe("");
      expect(ground).not.toBe("");
      // No coloured left edge on it or its items, and no hover text.
      for (const el of await line.locator(".dkl-it").all().then((l) => [line, ...l])) {
        expect(await el.evaluate((e) => parseFloat(getComputedStyle(e).borderLeftWidth) || 0)).toBe(0);
      }
      await expect(page.locator(".dkl [title]")).toHaveCount(0);
      await page.screenshot({ path: `test-results/desk-now-line-${size.width}-${scheme}.png` });
    });
  }
}
