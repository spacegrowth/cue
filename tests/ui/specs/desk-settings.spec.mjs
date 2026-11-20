// A project's settings on one screen (#381, lands.js): how work lands, Ask me before and old working folders, each
// change saved as you make it (#417). Opened from the Desk header's one Change link (#417); Away is on the Desk's own switch. The lead can propose a change in
// plain words: it shows as its ask, and only your Yes saves it (hub/desk_lands.rs); the screen shows it after.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const lands = (page) => page.locator(".dk-strip .dk-ab");

test("the screen has its parts, no Away and no Save, Ask me before on one line, and × at its top right (#417, #447)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".ld-h")).toHaveText([/^How work lands/, /^Ask me before/]);
  await expect(dlg.locator(".ld-label", { hasText: "Old working folders" })).toBeVisible();
  await expect(dlg.locator('[data-ab-kind="main"]')).toHaveAttribute("aria-checked", "true");
  await expect(dlg.locator('[data-ld-sec="away"]')).toHaveCount(0);
  await expect(dlg.locator('[data-ld="save"]')).toHaveCount(0);
  // The three kinds side by side on one line.
  const tops = await dlg.locator('[data-ld-sec="ask"] .ab-row').evaluateAll((rs) => rs.map((r) => Math.round(r.getBoundingClientRect().top)));
  expect(tops.length).toBe(3);
  expect(new Set(tops).size).toBe(1);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  // One line (#417, #447) under the counts: what it asks before and how work lands, then one Change.
  await expect(lands(page)).toHaveCount(1);
  await expect(lands(page).locator(".dk-ab-words")).toContainText(/^Asks .* · lands in /);
  await expect(page.locator(".dk-strip [data-ab-change]")).toHaveCount(0);
  await expect(page.locator(".dk-strip button", { hasText: "Change" })).toHaveCount(1);
  await lands(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(page.locator(".ab-dialog")).toHaveCount(0);
  // Its close is × at the top right, on the title's line, as on the other screens; no Close at the bottom.
  const x = dlg.locator('[data-ld="close"]');
  await expect(x).toHaveText("×");
  await expect(x).toHaveAttribute("aria-label", "Close");
  const [xb, tb, db] = await Promise.all([x.boundingBox(), dlg.locator(".move-title").boundingBox(), dlg.boundingBox()]);
  expect(Math.abs((xb.y + xb.height / 2) - (tb.y + tb.height / 2))).toBeLessThan(4);
  expect(db.x + db.width - (xb.x + xb.width)).toBeLessThan(28);
  await expect(dlg.locator(".row-btns")).toHaveCount(0);
  await x.click();
  await expect(dlg).toHaveCount(0);
});

test("your own check: steps one per line, saved in order when you leave the box; a tick saves at once with Saved shown", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const check = dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Check before landing" }) });
  await check.locator('[data-ld-pick="command"]').click();
  await check.locator("textarea.ld-cmd").fill("npm test\n\n npm run lint ");
  await check.locator("textarea.ld-cmd").blur();
  await expect(check.locator(".ld-hint")).toContainText("The first that fails stops the rest");
  await expect(check.locator(".ld-hint")).toContainText("all of them 30 minutes together; anything a step leaves running in the background counts toward that time");
  await dlg.locator('[data-ab-kind="release"]').click();
  await expect(dlg.locator(".ld-saved")).toHaveText("Saved");
  await expect(dlg).toBeVisible();
  const call = await cue.called("desk_lands_set");
  expect(call.args.lands.check).toBe("command");
  expect(call.args.lands.check_steps).toEqual(["npm test", "npm run lint"]);
  expect(call.args.lands.after_steps).toBeUndefined();
  const ab = await cue.called("ask_before_set");
  expect(ab.args.kinds).toContain("release");
});

test("a tick that can't be saved says so and goes back; the screen stays", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("ask_before_set", () => { throw "offline"; });
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ab-kind="release"]').click();
  await expect(page.locator(".toast").last()).toHaveText("Couldn't save Ask me before: offline");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator('[data-ab-kind="release"]')).toHaveAttribute("aria-checked", "false");
});

test("the lead's proposed change is its ask; it shows on the screen only after your Yes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const q = "Change how work lands on cue: check → none?";
  await cue.change((st, q) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-lands-ask", agent_id: "", agent_name: "", desk: true, hosted: false,
      tool_input: { questions: [{ question: q, header: "Desk", multiSelect: false, options: [{ label: "Yes", description: "" }, { label: "No", description: "" }] }],
        desk_lands: { project: d.lands.project, folder: "", words: "check → none", why: "the user asked",
          set: { ...d.lands.set, check: "none" } } } });
  }, q);
  const ask = page.locator(".dk-right .dk-ask");
  await expect(ask).toContainText(q);
  await ask.locator("[data-pick]", { hasText: "Yes" }).click();
  await ask.locator('[data-act="submit-answers"]').click();
  expect((await cue.called("respond")).args.id).toBe("n-lands-ask");
  await lands(page).locator("[data-lands-change]").click();
  const check = page.locator(".ld-dialog .ld-row", { has: page.locator(".ld-label", { hasText: "Check before landing" }) });
  await expect(check.locator("button.on")).toHaveText("None");
});

test("the settings line is under the counts at the top left, its words then one Change; narrow, the words shorten and Change stays (#417, #447)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 520, height: 800 });
  await cue.open("desk");
  await openDesk(page);
  const link = lands(page).locator("[data-lands-change]"), sub = page.locator(".dk-strip .dk-sub"), words = lands(page).locator(".dk-ab-words");
  await expect(link).toHaveText("Change");
  await expect(words).toHaveText("Asks before push to main · lands in own copy, checked, one commit per item");
  // Narrow, the words shorten (once laid out at this width).
  await expect.poll(() => words.evaluate((e) => e.scrollWidth > e.clientWidth)).toBe(true);
  const [s, w, b, strip] = await Promise.all([sub.boundingBox(), words.boundingBox(), link.boundingBox(), page.locator(".dk-strip").boundingBox()]);
  // Its own line under the counts, starting where they do.
  expect(w.y).toBeGreaterThan(s.y + s.height - 2);
  expect(Math.abs(w.x - s.x)).toBeLessThan(1);
  // Change on the words' line, inside the strip, never cut.
  expect(Math.abs((b.y + b.height / 2) - (w.y + w.height / 2))).toBeLessThan(4);
  expect(b.x + b.width).toBeLessThanOrEqual(strip.x + strip.width);
  await expect(page.locator(".dk-strip [title]")).toHaveCount(0);
  await link.click();
  await expect(page.locator(".ld-dialog .move-title")).toHaveText("Settings for cue");
});

test("the settings screen is short: it fits a 900-high window without scrolling (#447)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1100, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".ld-steps li b")).toHaveText(["Build", "Rust tests", "Window loads"]);
  const [sh, ch] = await dlg.evaluate((e) => [e.scrollHeight, e.clientHeight]);
  expect(sh).toBeLessThanOrEqual(ch);
  expect(sh).toBeLessThan(780);
});
