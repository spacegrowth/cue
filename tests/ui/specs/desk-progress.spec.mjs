// Items as Progress (#409, mock J2): the List / Progress switch shows every open item as a row of six step marks
// (from Cue's `steps` stamps) with how long each took, and a Now column; a row opens the item back in the List.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

test("Progress shows each open item's steps, what it's doing now, and opens it on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const now = Date.now();
  await cue.change((st, now) => {
    const items = st.desk.projects.find((x) => x.project === "cue").items;
    items.find((i) => i.id === "d3").steps = [now - 600000, now - 480000, now - 60000, 0, 0, 0];
    items.find((i) => i.id === "d2").steps = [now - 900000, now - 800000, now - 700000, now - 600000, now - 500000, now - 120000];
  }, now);
  await page.locator('.dk-items [data-desk-items-view="progress"]').click();
  const prog = page.locator(".dk-prog");
  await expect(prog.locator(".dk-grh .dk-grhc")).toHaveText(["Picked up", "Building", "Tests", "Commit", "Landing", "To check"]);
  // One row per open item: not the done or dropped ones.
  const rows = prog.locator("[data-desk-prog-row]");
  await expect(rows).toHaveCount(6);
  await expect(prog.locator('[data-desk-prog-row="d7"]')).toHaveCount(0);
  // At its tests: two steps done (8m, 7m), tests lit and counting, three ahead.
  const d3 = prog.locator('[data-desk-prog-row="d3"]');
  await expect(d3.locator(".dk-grt").nth(0)).toHaveText("2m");
  await expect(d3.locator(".dk-grt").nth(1)).toHaveText("7m");
  await expect(d3.locator(".dk-grt.cur")).toHaveCount(1);
  await expect(d3.locator(".dk-grr.on")).toHaveCount(2);
  // Waiting for you to check it: every step reached, its Now in your colour with Looks right.
  const d2 = prog.locator('[data-desk-prog-row="d2"]');
  await expect(d2.locator(".dk-grr.on")).toHaveCount(5);
  await expect(d2.locator(".dk-grw.you")).toHaveCount(1);
  // Asks you something: its Now says so, with one button.
  await expect(prog.locator('[data-desk-prog-row="d1"] .dk-grw')).toHaveClass(/you/);
  await expect(prog.locator('[data-desk-prog-row="d1"] .dk-grb button').first()).toBeVisible();
  await expect(prog).toContainText("need");
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await prog.screenshot({ path: `${dir}/cue-desk-progress-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await prog.screenshot({ path: `${dir}/cue-desk-progress-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });
  // A row opens its item, back in the List.
  await d3.locator(".dk-grtl").click();
  await expect(page.locator(".dk-prog")).toHaveCount(0);
  await expect(page.locator('.dk-items [data-desk-items-view="list"]')).toHaveAttribute("aria-pressed", "true");
});
