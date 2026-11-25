// A project's settings on one screen (#381, lands.js): how work lands, Ask me before and old working folders, saved
// together when you press Save (#600); Cancel drops them, and leaving with changes unsaved asks first. Opened from the Desk header's one Change link (#417); Away is on the Desk's own switch. The lead can propose a change in
// plain words: it shows as its ask, and only your Yes saves it (desk/hub/lead/lands.rs); the screen shows it after.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";

const lands = (page) => page.locator(".dk-strip .dk-ab");

test("the screen has its parts, no Away, Ask me before on one line, and Esc at its top right as Settings has (#417, #447)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  // Option A of #904 (#913): tabs as Settings has them.
  await expect(dlg.locator("[data-ld-tab]")).toHaveText(["Where it works", "When it's done", "Ask me first", "More"]);
  await expect(dlg.locator('[data-ld-tab="where"]')).toHaveAttribute("aria-selected", "true");
  await dlg.locator('[data-ld-tab="more"]').click();
  await expect(dlg.getByRole("button", { name: "Remove old working folders" })).toBeVisible();
  // Pushing to main is its own row, said once (#692): not an Ask me before switch here.
  await dlg.locator('[data-ld-tab="ask"]').click();
  await expect(dlg.locator('[data-ab-kind="main"]')).toHaveCount(0);
  await dlg.locator('[data-ld-tab="done"]').click();
  await expect(dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Send to GitHub" }) }).locator("button.on")).toHaveText("Ask me first");
  await expect(dlg.locator('[data-ld-sec="away"]')).toHaveCount(0);
  // Save and Cancel at its foot; Save comes on only once something changed.
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  await expect(dlg.locator('[data-ld="cancel"]')).toHaveText("Cancel");
  await expect(dlg.locator(".ld-intro")).toHaveText("Kept in Cue, not in the project. Save keeps what you changed on every tab.");
  // Ask me first: a switch per kind Cue checks, as Settings' notifications (pushing to main has its own row).
  await dlg.locator('[data-ld-tab="ask"]').click();
  await expect(dlg.locator('[data-ld-sec="ask"] .ld-label')).toHaveText(["Pushing any branch", "Releasing", "Deleting files"]);
  await expect(dlg.locator('[data-ld-sec="ask"] [role="switch"]')).toHaveCount(3);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  // One line (#417, #447) under the counts: what it asks before and how work lands, then one Change.
  await expect(lands(page)).toHaveCount(1);
  await expect(lands(page).locator(".dk-ab-words")).toContainText(/^Asks .* · lands in /);
  await expect(page.locator(".dk-strip [data-ab-change]")).toHaveCount(0);
  await expect(page.locator(".dk-strip button", { hasText: "Change" })).toHaveCount(1);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(page.locator(".ab-dialog")).toHaveCount(0);
  // Its close is Esc at the top right, on the title's line, as on Settings; no Close at the bottom.
  const x = dlg.locator('[data-ld="close"]');
  await expect(x).toHaveText("Esc");
  await expect(x).toHaveAttribute("aria-label", "Close");
  const [xb, tb, db] = await Promise.all([x.boundingBox(), dlg.locator(".move-title").boundingBox(), dlg.boundingBox()]);
  expect(Math.abs((xb.y + xb.height / 2) - (tb.y + tb.height / 2))).toBeLessThan(4);
  expect(db.x + db.width - (xb.x + xb.width)).toBeLessThan(28);
  await expect(dlg.locator(".row-btns")).toHaveCount(0);
  await x.click();
  await expect(dlg).toHaveCount(0);
  // A click outside closes it too, as on Settings; a click inside doesn't.
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  await dlg.locator(".ld-intro").click();
  await expect(dlg).toHaveCount(1);
  await page.locator(".ld-scrim").click({ position: { x: 5, y: 5 } });
  await expect(dlg).toHaveCount(0);
});

test("the screen is built as Settings is: the same sheet, its title and Esc on one line, its tabs the same (#447, #913)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await page.keyboard.press("Meta+,");
  const set = page.locator(".scrim > .sheet");
  await expect(set.locator(".sheet-head h3")).toHaveText("Settings");
  const look = (sheet) => sheet.evaluate((e) => {
    const s = getComputedStyle(e), h = getComputedStyle(e.querySelector(".sheet-head h3")), k = getComputedStyle(e.querySelector(".sheet-head kbd")), t = getComputedStyle(e.querySelector(".set-tabs button.on"));
    return { radius: s.borderRadius, bg: s.backgroundColor, title: h.fontSize, kbd: [k.fontSize, k.backgroundColor], tab: [t.fontSize, t.backgroundColor, t.color, t.borderRadius] };
  });
  // Read once it's drawn for good: a redraw can swap the sheet between finding it and reading it (a swapped-out node has no style).
  let theirs;
  await expect.poll(async () => (theirs = await look(set)).radius).not.toBe("");
  await page.keyboard.press("Escape");
  await expect(set).toHaveCount(0);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".sheet-head kbd")).toHaveText("Esc");
  await expect.poll(() => look(dlg)).toEqual(theirs);
  await expect(dlg.locator("[title]")).toHaveCount(0);
});

test("your own check: steps one per line, saved in order with a tick by one Save, and the screen closes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld-tab="done"]').click();
  const check = dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Check first" }) });
  await check.locator('[data-ld-pick="command"]').click();
  await check.locator('[data-ld-edit="check"]').click();
  await check.locator("textarea.ld-cmd").fill("npm test\n\n npm run lint ");
  await check.locator("textarea.ld-cmd").blur();
  // Under the open box, what your steps do in full.
  await expect(check.locator(".ld-steps-help")).toContainText("The first that fails stops the rest");
  await expect(check.locator(".ld-steps-help")).toContainText("Up to 15 minutes a step and 30 in all, counting anything a step leaves running in the background");
  await dlg.locator('[data-ld-tab="ask"]').click();
  await dlg.locator('[data-ab-kind="release"]').click();
  // Nothing saved yet: the foot says so.
  await expect(dlg.locator(".ld-foot-t")).toHaveText("Unsaved changes");
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  expect(await cue.calls("ask_before_set")).toHaveLength(0);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  const call = await cue.called("desk_lands_set");
  expect(call.args.lands.check).toBe("command");
  expect(call.args.lands.check_steps).toEqual(["npm test", "npm run lint"]);
  // After landing is named steps on every Desk (#796): Cue's own build, install and restart are its three, kept as they were.
  expect(call.args.lands.after).toBe("command");
  expect(call.args.lands.after_steps).toEqual(["[Build] app:build", "[Install] app:install", "[Restart] app:restart"]);
  const ab = await cue.called("ask_before_set");
  expect(ab.args.kinds).toContain("release");
  // Sessions never push main themselves: saving keeps a push to main asked.
  expect(ab.args.kinds).toContain("main");
});

test("a tick that can't be saved says so on Save; the screen stays with it ticked", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await cue.override("ask_before_set", () => { throw "offline"; });
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld-tab="ask"]').click();
  await dlg.locator('[data-ab-kind="release"]').click();
  await dlg.locator('[data-ld="save"]').click();
  await expect(page.locator(".toast").last()).toHaveText("Couldn't save Ask me before: offline");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator('[data-ab-kind="release"]')).toHaveAttribute("aria-checked", "true");
  await expect(dlg.locator(".ld-foot-t")).toHaveText("Unsaved changes");
});

test("the lead's proposed change is its ask; it shows on the screen only after your Yes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The lead's ask is under its chat (B2, #666: the sessions list would take that chat's place, so it isn't open yet).
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
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  await page.locator('.ld-dialog [data-ld-tab="done"]').click();
  const check = page.locator(".ld-dialog .ld-row", { has: page.locator(".ld-label", { hasText: "Check first" }) });
  await expect(check.locator("button.on")).toHaveText("None");
});

test("the settings line is under the counts at the top left, its words then one Change; narrow, the words shorten and Change stays (#417, #447)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 520, height: 800 });
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  const link = lands(page).locator("[data-lands-change]"), sub = page.locator(".dk-strip .dk-sub"), words = lands(page).locator(".dk-ab-words");
  await expect(link).toHaveText("Change");
  await expect(words).toHaveText("Asks before sending to GitHub · lands in own copy, checked, one commit per item");
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

test("the settings screen is short: every tab fits a 900-high window without scrolling (#447, #913)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1100, height: 900 });
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  for (const tab of ["where", "done", "ask", "more"]) {
    await dlg.locator(`[data-ld-tab="${tab}"]`).click();
    if (tab === "done") {
      // Your steps show as one line each until you edit them; both boxes open still fit (#902).
      await expect(dlg.locator(".ld-steps")).toHaveCount(2);
      await expect(dlg.locator("[data-ld-steps]")).toHaveCount(0);
      for (const k of ["check", "after"]) await dlg.locator(`[data-ld-edit="${k}"]`).click();
      await expect(dlg.locator("[data-ld-steps]")).toHaveCount(2);
    }
    const [sh, ch] = await dlg.evaluate((e) => [e.scrollHeight, e.clientHeight]);
    expect(sh).toBeLessThanOrEqual(ch);
    expect(sh).toBeLessThan(780);
  }
});

test("Cancel drops what you changed; nothing is saved and the screen shows the setting as it was (#600)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const tab = (t) => dlg.locator(`[data-ld-tab="${t}"]`).click();
  const check = dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Check first" }) });
  await tab("done");
  await check.locator('[data-ld-pick="none"]').click();
  await tab("where");
  await dlg.locator('[data-ld="more"]').click();
  await tab("ask");
  await dlg.locator('[data-ab-kind="release"]').click();
  await expect(dlg.locator('[data-ld="save"]')).toBeEnabled();
  await dlg.locator('[data-ld="cancel"]').click();
  await expect(dlg).toHaveCount(0);
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  expect(await cue.calls("ask_before_set")).toHaveLength(0);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  await expect(dlg.locator('[data-ab-kind="release"]')).toHaveAttribute("aria-checked", "false");
  await tab("where");
  await expect(dlg.locator(".ld-n")).toHaveText("3");
  await tab("done");
  await expect(check.locator("button.on")).toHaveText("Your own steps");
  // Changed back by hand counts as nothing changed: Save goes off, and Esc leaves without asking.
  await check.locator('[data-ld-pick="none"]').click();
  await check.locator('[data-ld-pick="command"]').click();
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
});

test("leaving with changes unsaved asks first: Esc, its Esc or a click outside; Keep editing, Discard or Save (#600)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld="less"]').click();
  const ask = dlg.locator(".ld-leaving");
  // Esc asks; Esc again goes back to editing, the change kept.
  await page.keyboard.press("Escape");
  await expect(ask.locator(".ld-foot-t")).toHaveText("Leave without saving your changes?");
  await expect(ask.locator("button")).toHaveText(["Keep editing", "Discard", "Save"]);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(ask).toHaveCount(0);
  await expect(dlg.locator(".ld-n")).toHaveText("2");
  // A click outside asks; Keep editing stays.
  await page.locator(".ld-scrim").click({ position: { x: 5, y: 5 } });
  await ask.locator('[data-ld="stay"]').click();
  await expect(ask).toHaveCount(0);
  await expect(dlg).toBeVisible();
  // Its Esc asks; Discard leaves with nothing saved.
  await dlg.locator('[data-ld="close"]').click();
  await ask.locator('[data-ld="discard"]').click();
  await expect(dlg).toHaveCount(0);
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  // Again, and Save from the ask saves it and closes.
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".ld-n")).toHaveText("3");
  await dlg.locator('[data-ld="less"]').click();
  await page.keyboard.press("Escape");
  await ask.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  expect((await cue.called("desk_lands_set")).args.lands.at_once).toBe(2);
});

test("changing where an item works while items are open says what happens to them before it's saved (#600)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const it = (id, state) => ({ ...d.items[0], id, state });
    d.items = [it("o1", "working"), it("o2", "to_check"), it("o3", "queued"), it("o4", "done")];
  });
  await openDesk(page);
  await deskAllSessions(page);
  await lands(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const where = dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: "Each item works in" }) });
  await expect(where.locator("[data-ld-move]")).toHaveCount(0);
  await where.locator('[data-ld-pick="shared"]').click();
  await expect(where.locator("[data-ld-move]")).toHaveText("3 items are open: 2 already started stay where they are and land from there; 1 not started yet works in the shared folder when it starts.");
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  // Back to Own copy: nothing changes, nothing to say.
  await where.locator('[data-ld-pick="copy"]').click();
  await expect(where.locator("[data-ld-move]")).toHaveCount(0);
  await where.locator('[data-ld-pick="shared"]').click();
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  expect((await cue.called("desk_lands_set")).args.lands.where).toBe("shared");
});
