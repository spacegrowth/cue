// Progress (#409, mock J2): Progress (no button in the Items head since B2, #666; deskProgToggle opens it) opens a popover over the Desk with every item under way (not the
// queued or parked ones) as a row of six step marks (from Cue's `steps` stamps) with how long each took, and a Now
// column; a row opens the item; Esc or a click outside closes it.
import { test, expect, openDesk } from "./cue.mjs";


test("Progress pops up each item under way with its steps, what it's doing now, and opens it on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const now = Date.now();
  await cue.change((st, now) => {
    const items = st.desk.projects.find((x) => x.project === "cue").items;
    items.find((i) => i.id === "d3").steps = [now - 600000, now - 480000, now - 60000, 0, 0, 0];
    items.find((i) => i.id === "d2").steps = [now - 900000, now - 800000, now - 700000, now - 600000, now - 500000, now - 120000];
  }, now);
  await page.evaluate(() => deskProgToggle(true));
  const prog = page.locator(".dk-prog");
  await expect(prog.locator(".dk-grh .dk-grhc")).toHaveText(["Picked up", "Building", "Tests", "Commit", "Landing", "To check"]);
  // One row per item under way: not the queued (d4), new (d6), done or dropped ones.
  const rows = prog.locator("[data-desk-prog-row]");
  await expect(rows).toHaveCount(4);
  for (const id of ["d4", "d6", "d7"]) await expect(prog.locator(`[data-desk-prog-row="${id}"]`)).toHaveCount(0);
  await expect(prog.locator(".dk-grcount")).toHaveText("4 under way · 2 need you · 2 not started");
  // At its tests: two steps done (8m, 7m), tests lit and counting, three ahead.
  const d3 = prog.locator('[data-desk-prog-row="d3"]');
  await expect(d3.locator(".dk-grt").nth(0)).toHaveText("2m");
  await expect(d3.locator(".dk-grt").nth(1)).toHaveText("7m");
  await expect(d3.locator(".dk-grt.cur")).toHaveCount(1);
  await expect(d3.locator(".dk-grr.on")).toHaveCount(2);
  // Waiting for you to check it: every step reached, its Now with Looks right. Your turn, not an error (#443): its
  // last step and Now in the normal text colour, a calm accent mark, no red and no "!".
  const d2 = prog.locator('[data-desk-prog-row="d2"]');
  await expect(d2.locator(".dk-grr.on")).toHaveCount(5);
  await expect(d2.locator(".dk-grw.check")).toHaveCount(1);
  await expect(d2.locator(".dk-grw.you, .dk-grt.cur.you, .dk-grt.cur.fail")).toHaveCount(0);
  const colour = (v) => page.evaluate((v) => { const e = document.createElement("i"); e.style.color = `var(${v})`; document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; }, v);
  const ink = await colour("--ink");
  expect(ink).not.toBe(await colour("--deny"));
  for (const el of [d2.locator(".dk-grw"), d2.locator(".dk-grt.cur")]) await expect(el).toHaveCSS("color", ink);
  const mark = await d2.locator(".dk-grc").last().locator("svg.dk-grm").innerHTML();
  expect(mark).not.toContain("--deny");
  expect(mark).toContain("--accent");
  // Asks you something: its Now says so, with one button.
  await expect(prog.locator('[data-desk-prog-row="d1"] .dk-grw')).toHaveClass(/you/);
  await expect(prog.locator('[data-desk-prog-row="d1"] .dk-grb button').first()).toBeVisible();
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await page.screenshot({ path: `${dir}/cue-desk-progress-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-progress-dark.png` });
  await page.emulateMedia({ colorScheme: "light" });
  // It moves as Cue's state comes in: at its commit, tests' time stops counting.
  await cue.change((st, now) => { st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3").steps[3] = now; }, Date.now());
  await expect(d3.locator(".dk-grr.on")).toHaveCount(3);
  // Esc closes it; so does a click outside.
  await page.keyboard.press("Escape");
  await expect(prog).toHaveCount(0);
  await page.evaluate(() => deskProgToggle(true));
  await page.locator(".dk-items .dk2b-ih b").click();
  await expect(prog).toHaveCount(0);
  // A row opens its item and closes the popover.
  await page.evaluate(() => deskProgToggle(true));
  await d3.locator(".dk-grtl").click();
  await expect(prog).toHaveCount(0);
  // Its page, on the left in the lead's chat's place (B2, #666); no Progress button in the Items head.
  await expect(page.locator(".dk-items [data-desk-prog-toggle]")).toHaveCount(0);
  await expect(page.locator('.dk-panel[data-side-for="d3"]')).toBeVisible();
  await expect(page.locator(".dk-right h1")).toContainText("Search stays centred");
});

test("A reopened item shows a plain line under its steps, its round and your note's first words; a click shows the whole note (#421, #444)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const now = Date.now();
  await cue.change((st, now) => {
    const d3 = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3");
    d3.steps = [now - 900000, now - 60000, 0, 0, 0, 0];
    d3.backs = [{ from: 5, why: "The arrow is missing on Progress\nand more", at_ms: now - 60000, steps: [now - 900000, now - 780000, 0, now - 400000, 0, now - 300000] }];
  }, now);
  await page.evaluate(() => deskProgToggle(true));
  const d3 = page.locator('.dk-prog [data-desk-prog-row="d3"]');
  // No arrow under its steps: one plain line.
  await expect(d3.locator(".dk-grbk svg")).toHaveCount(0);
  const line = d3.locator(".dk-grbk-t");
  await expect(line).toHaveText("Round 2 · you sent it back: The arrow is missing on Progress");
  // A click shows the whole note, in place (it doesn't open the item); another folds it back.
  await line.click();
  await expect(page.locator(".dk-prog")).toHaveCount(1);
  await expect(line).toHaveText("Round 2 · you sent it back: The arrow is missing on Progress\nand more");
  await line.click();
  await expect(line).toHaveText("Round 2 · you sent it back: The arrow is missing on Progress");
  // Picked up took till its first round's Building (2m), not till this round's.
  await expect(d3.locator(".dk-grt").nth(0)).toHaveText("2m");
  // Items that never went back have no such line.
  await expect(page.locator('.dk-prog [data-desk-prog-row="d2"] .dk-grbk')).toHaveCount(0);
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await page.screenshot({ path: `${dir}/cue-desk-progress-back-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${dir}/cue-desk-progress-back-dark.png` });
});

test("Progress lists items landing or waiting to install above those in progress, as the list does (#468)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "finishing", finish: { step: "line", wait: "Landing", at: Date.now() } }); });
  await page.evaluate(() => deskProgToggle(true));
  const ids = await page.locator(".dk-prog [data-desk-prog-row]").evaluateAll((r) => r.map((e) => e.dataset.deskProgRow));
  expect(ids.indexOf("d5")).toBeGreaterThan(-1);
  expect(ids.indexOf("d5")).toBeLessThan(ids.indexOf("d3"));
});
