// A project's settings on one screen (#381, lands.js): how work lands, Ask me before, Away and old working folders.
// Opened from the Desk header's one Settings link (#417); its Away part opens Away's own card. The lead can propose a change in
// plain words: it shows as its ask, and only your Yes saves it (hub/desk_lands.rs); the screen shows it after.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const lands = (page) => page.locator(".dk-strip .dk-ab");

test("the screen has its four parts, opens from Lands and Ask me before, and Away opens from it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".ld-h")).toHaveText(["How work lands", "Ask me before", "Away"]);
  await expect(dlg.locator(".ld-label", { hasText: "Old working folders" })).toBeVisible();
  await expect(dlg.locator('[data-ab-kind="main"]')).toHaveAttribute("aria-checked", "true");
  await expect(dlg.locator('[data-ld-sec="away"]')).toContainText("Off");
  await expect(dlg.locator("[title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  // One link (#417), just the word Settings on the counts' line (#415); no other Change in the head.
  await expect(lands(page)).toHaveCount(1);
  await expect(lands(page)).toHaveText("Settings");
  await expect(page.locator(".dk-strip [data-ab-change]")).toHaveCount(0);
  await expect(page.locator(".dk-strip button", { hasText: "Change" })).toHaveCount(0);
  await lands(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(page.locator(".ab-dialog")).toHaveCount(0);
  // Away's part opens Away's own card; closing it comes back to the screen.
  await cue.override("desk_away_sweep", () => null);
  await dlg.locator('[data-ld="away"]').click();
  await expect(page.locator(".dk-aw-dialog")).toBeVisible();
  await expect(dlg).toHaveCount(0);
  await page.locator('.dk-aw-dialog [data-away="cancel"]').click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
});

test("your own check: steps one per line, saved in order, Ask me before saved with it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const check = dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Check before landing" }) });
  await check.locator('[data-ld-pick="command"]').click();
  await check.locator("textarea.ld-cmd").fill("npm test\n\n npm run lint ");
  await expect(check.locator(".ld-hint")).toContainText("The first that fails stops the rest");
  await expect(check.locator(".ld-hint")).toContainText("all of them 30 minutes together; anything a step leaves running in the background counts toward that time");
  await dlg.locator('[data-ab-kind="release"]').click();
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  const call = await cue.called("desk_lands_set");
  expect(call.args.lands.check).toBe("command");
  expect(call.args.lands.check_steps).toEqual(["npm test", "npm run lint"]);
  expect(call.args.lands.after_steps).toBeUndefined();
  const ab = await cue.called("ask_before_set");
  expect(ab.args.kinds).toContain("release");
});

test("when Ask me before can't be saved, it says what was saved and what wasn't, and the screen stays", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("ask_before_set", () => { throw "offline"; });
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ab-kind="release"]').click();
  await dlg.locator('[data-ld="save"]').click();
  await expect(page.locator(".toast").last()).toHaveText("How work lands is saved; Ask me before couldn't be saved: offline");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator('[data-ab-kind="release"]')).toHaveAttribute("aria-checked", "true");
  await expect(dlg.locator('[data-ld="save"]')).toBeEnabled();
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

test("Settings sits at the end of the counts' line, takes no line of its own (#415)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 520, height: 800 });
  await cue.open("desk");
  await openDesk(page);
  const link = lands(page).locator("[data-lands-change]"), sub = page.locator(".dk-strip .dk-sub");
  await expect(link).toHaveText("Settings");
  const [s, b, strip] = await Promise.all([sub.boundingBox(), link.boundingBox(), page.locator(".dk-strip").boundingBox()]);
  // Same line as the counts, at the strip's right inside edge.
  expect(Math.abs((b.y + b.height / 2) - (s.y + s.height / 2))).toBeLessThan(4);
  expect(b.x + b.width).toBeLessThanOrEqual(strip.x + strip.width - 19);
  await expect(page.locator(".dk-strip [title]")).toHaveCount(0);
  await link.click();
  await expect(page.locator(".ld-dialog .move-title")).toHaveText("Settings for cue");
});
