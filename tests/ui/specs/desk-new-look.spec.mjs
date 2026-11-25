// New Desk works out how a folder builds, checks and lands (#927, the picked mock DS924A): picking a folder starts a
// look, its lines ticked as each part answers, Start Desk waiting for it; then what it found as rows with where each
// came from, Change opening a row's choices in place; Start Desk saves what you accepted, Skip starts with the usual
// settings, a failed look says so, and a newer look throws an older one's answers away.
import { test, expect } from "./cue.mjs";

/** The New Desk sheet open from Desk home, the look's fake steered by `fake` (fakes/desk.js w.lookFake). */
async function openSheet(page, cue, fake = {}) {
  await cue.open("desk");
  await page.evaluate((f) => { CueBridge.world.lookFake = f; }, fake);
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top .hv-menu [data-hv-desk-home]").click();
  await page.locator(".dh-add").click();
  const sheet = page.locator(".nd");
  await expect(sheet).toBeVisible();
  return sheet;
}
const row = (sheet, part) => sheet.locator(`[data-nd-look-row="${part}"]`);
const start = (sheet) => sheet.locator('[type="submit"]');
/** Waits until the look's `part` for `folder` has answered (an answer the window may throw away). */
const answered = (page, folder, part) => page.waitForFunction(([f, p]) => CueBridge.called("desk_look").some((c) => c.args.folder === f && c.args.part === p && ("result" in c || "error" in c)), [folder, part]);

test("a folder picked starts a look: its lines tick as parts answer, Start Desk waits, then the rows say what was found and where from", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { wait: { files: 50, github: 1500, history: 100, sessions: 200 } });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await sheet.locator("#nd-name").fill("shop-web");
  const look = sheet.locator(".nd-looking");
  await expect(look.locator(".nd-lk-hd")).toHaveText("Working out how shop-web builds, checks and lands…");
  // The files answered and ticked; GitHub still asking, with its spinner line.
  await expect(look.locator('.nd-done[data-nd-part="files"]').first()).toContainText("Read package.json · build, test, lint scripts");
  await expect(look.locator('.nd-done[data-nd-part="files"] .nd-mono').first()).toHaveText("package.json");
  await expect(look.locator('.nd-done[data-nd-part="sessions"]')).toHaveText("12 past sessions here: 9 ran tests, 4 pushed");
  await expect(look.locator('.nd-wait[data-nd-part="github"]')).toHaveText("Asking GitHub whether main needs a pull request…");
  await expect(start(sheet)).toBeDisabled();
  await expect(sheet.locator("[data-nd-skip]")).toHaveText("Skip, use the usual settings");
  // All in: the rows.
  const found = sheet.locator(".nd-look-found");
  await expect(found.locator(".nd-lk-hd")).toContainText("How shop-web builds, checks and lands");
  await expect(found.locator(".nd-lk-sum")).toHaveText("4 found · 1 to pick");
  await expect(found.locator(".nd-rl")).toHaveText(["Builds with", "Check first", "Lands on main as", "Then runs", "Send to GitHub"]);
  await expect(found.locator(".nd-chip")).toHaveText(["Found", "Found", "Found", "Couldn't tell", "Follows"]);
  await expect(row(sheet, "after").locator(".nd-chip")).toHaveClass(/nd-chip-unsure/);
  await expect(row(sheet, "build").locator(".nd-val-t")).toHaveText("npm run build");
  await expect(row(sheet, "build").locator(".nd-src")).toHaveText("From package.json, its build script.");
  await expect(row(sheet, "build").locator(".nd-src .nd-mono")).toHaveText("package.json");
  await expect(row(sheet, "check").locator(".nd-steps")).toHaveText("2 steps: Lint · Test");
  await expect(row(sheet, "check").locator(".nd-src .nd-mono")).toHaveText(".github/workflows/ci.yml");
  await expect(row(sheet, "how").locator(".nd-val-t")).toHaveText("Pull request");
  // Couldn't tell: its choices open, none lit, the safe one kept until you pick.
  await expect(row(sheet, "after").locator(".nd-seg button")).toHaveText(["Nothing", "Your own steps"]);
  await expect(row(sheet, "after").locator(".nd-seg button.on")).toHaveCount(0);
  await expect(row(sheet, "after").locator(".nd-src")).toHaveText("Found scripts/deploy.sh, but nothing says it runs after each change. Until you pick, nothing runs.");
  await expect(sheet.locator(".nd-foot-t")).toHaveText("You can change any of these later in Desk settings.");
  await expect(start(sheet)).toBeEnabled();
  // Nothing anywhere has hover text.
  await expect(sheet.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) {
    await page.setViewportSize({ width: 1280, height: 1150 });
    await page.emulateMedia({ colorScheme: c }); await page.waitForTimeout(150);
    await page.screenshot({ clip: await sheet.boundingBox(), path: `${process.env.SHOT_DIR}/nd-look-found-${c}.png` });
  }
});

test("past sessions show as a row's source when no file says it", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { plain: ["/work/notes-plain"] });
  await cue.override("desk_look", (args, _, area) => area(args).then((r) => (args.part === "files" ? { ...r, found: { ...r.found, ci: null } } : r)));
  await sheet.locator("#nd-folder").fill("/work/notes-plain");
  await expect(sheet.locator(".nd-look-found .nd-rl")).toHaveText(["Builds with", "Check first", "Then runs"]);
  await expect(row(sheet, "check").locator(".nd-src")).toHaveText("From 9 past sessions: fix-cart, api-tests, +7.");
  // After a why of its own that doesn't name them, the sessions follow it.
  await cue.override("desk_look_rows", (args, _, area) => area(args).then((l) => ({ ...l, rows: l.rows.map((r) => (r.part === "check" ? { ...r, why: "Your tests, as they've run here." } : r)) })));
  await sheet.locator("#nd-folder").fill("/work/notes-plain/");
  await expect(row(sheet, "check").locator(".nd-src")).toHaveText("Your tests, as they've run here. From 9 past sessions: fix-cart, api-tests, +7.");
});

test("Change on Lands on main as picks another; Start Desk starts it, then saves the settings you accepted", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue);
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await sheet.locator("#nd-name").fill("shop-web");
  await expect(sheet.locator(".nd-look-found")).toBeVisible();
  const how = row(sheet, "how");
  await how.locator("[data-nd-change]").click();
  await expect(how.locator(".nd-seg button")).toHaveText(["One commit", "Merge", "Pull request", "You commit"]);
  await expect(how.locator(".nd-seg button.on")).toHaveText("Pull request");
  await how.locator('[data-nd-choice="merge"]').click();
  await expect(how.locator(".nd-val-t")).toHaveText("Merge");
  await expect(how.locator(".nd-chip")).toHaveCount(0);   // yours now
  await expect(row(sheet, "push").locator(".nd-chip")).toHaveCount(0);   // no longer follows a pull request
  // Then runs: your own steps; the Couldn't tell chip goes.
  const after = row(sheet, "after");
  await after.locator('[data-nd-choice="command"]').click();
  await expect(after.locator(".nd-chip")).toHaveCount(0);
  // A "[Word] " name is only shown; a command that starts with "[ " is kept whole.
  await after.locator("[data-nd-steps]").fill("[Deploy] ./scripts/deploy.sh\n[ -f dist/app ] && make publish");
  await after.locator("[data-nd-change]").click();
  await expect(after.locator(".nd-steps")).toHaveText("2 steps: Deploy · [ -f dist/app ] && make publish");
  await start(sheet).click();
  const started = await cue.called("desk_start", { folder: "/work/shop-web", name: "shop-web" });
  const set = await cue.called("desk_lands_set", { desk: started.result.id });
  expect(set.args.lands).toMatchObject({ where: "copy", how: "merge", push: "after", check: "command", check_steps: ["npm run build", "npm run lint", "npm test"], after: "command", after_steps: ["./scripts/deploy.sh", "[ -f dist/app ] && make publish"] });
  expect(set.error).toBeUndefined();
  await expect(sheet).toHaveCount(0);
});

test("Skip, use the usual settings starts at once, without saving any settings", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { wait: { github: 3000 } });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await sheet.locator("#nd-name").fill("shop-web");
  await expect(sheet.locator(".nd-looking")).toBeVisible();
  await expect(start(sheet)).toBeDisabled();
  await cue.called("desk_look", { folder: "/work/shop-web", part: "github" });   // the look under way, its answer pending
  await sheet.locator("[data-nd-skip]").click();
  await cue.called("desk_start", { folder: "/work/shop-web" });
  await expect(sheet).toHaveCount(0);
  await answered(page, "/work/shop-web", "github");   // the look's late answers change nothing
  expect(await cue.calls("desk_lands_set")).toEqual([]);
  expect((await cue.calls("desk_look_rows")).filter((c) => c.args.folder === "/work/shop-web")).toEqual([]);
});

test("a failed look says so, and Start Desk starts with the usual settings", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { fail: { github: "gh didn't answer in 6 seconds" } });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await expect(sheet.locator(".nd-look-err .nd-lk-hd")).toHaveText("Couldn't work it out: gh didn't answer in 6 seconds");
  await expect(start(sheet)).toBeEnabled();
  await start(sheet).click();
  await cue.called("desk_start", { folder: "/work/shop-web" });
  await expect(sheet).toHaveCount(0);
  expect(await cue.calls("desk_lands_set")).toEqual([]);
});

test("a new folder starts a new look; the old one's late answer is thrown away", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { plain: ["/work/old-notes"] });
  // The old folder's GitHub part answers late.
  await cue.override("desk_look", (args, _, area) => new Promise((r) => setTimeout(r, args.folder === "/work/old-notes" && args.part === "github" ? 1500 : 0)).then(() => area(args)));
  await sheet.locator("#nd-folder").fill("/work/old-notes");
  await cue.called("desk_look", { folder: "/work/old-notes", part: "github" });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await expect(sheet.locator(".nd-look-found .nd-rl")).toHaveCount(5);
  await answered(page, "/work/old-notes", "github");
  await expect(sheet.locator(".nd-look-found .nd-rl")).toHaveCount(5);
  expect((await cue.calls("desk_look_rows")).filter((c) => c.args.folder !== "/work/shop-web").map((c) => c.args.folder)).not.toContain("/work/old-notes");
  // Esc still closes the sheet.
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
});

test("You commit lands in the shared folder: the settings saved are ones Cue takes", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue);
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await expect(sheet.locator(".nd-look-found")).toBeVisible();
  await row(sheet, "how").locator("[data-nd-change]").click();
  await row(sheet, "how").locator('[data-nd-choice="owner"]').click();
  await start(sheet).click();
  const started = await cue.called("desk_start", { folder: "/work/shop-web" });
  const set = await cue.called("desk_lands_set", { desk: started.result.id });
  expect(set.args.lands).toMatchObject({ how: "owner", where: "shared" });
  expect(set.error).toBeUndefined();
  await expect(sheet).toHaveCount(0);
});

test("Enter does nothing while the look goes; Skip when the Desk can't start leaves the sheet ready to start again", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { wait: { github: 1500 } });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await sheet.locator("#nd-name").fill("shop-web");
  await expect(sheet.locator(".nd-looking")).toBeVisible();
  await sheet.locator("#nd-name").press("Enter");
  await expect(sheet.locator(".nd-look-found")).toBeVisible();
  expect(await cue.calls("desk_start")).toEqual([]);
  // Skip with the Desk failing to start: the look is over, Start works again.
  await sheet.locator("#nd-folder").fill("/work/shop-web2");
  await expect(sheet.locator(".nd-looking")).toBeVisible();
  await cue.called("desk_look", { folder: "/work/shop-web2", part: "github" });
  await cue.override("desk_start", () => { throw "no such folder"; });
  await sheet.locator("[data-nd-skip]").click();
  await cue.called("desk_start", { folder: "/work/shop-web2" });
  await expect(sheet.locator(".nd-looking, .nd-look")).toHaveCount(0);
  await expect(sheet.locator(".nd-hint")).toBeVisible();
  await expect(start(sheet)).toBeEnabled();
  await answered(page, "/work/shop-web2", "github");
  await expect(sheet.locator(".nd-look")).toHaveCount(0);
  await page.evaluate(() => CueBridge.override("desk_start", null));
  await start(sheet).click();
  await expect(sheet).toHaveCount(0);
  expect(await cue.calls("desk_lands_set")).toEqual([]);
});

test("closing and opening the sheet again leaves nothing of the last look behind", async ({ page, cue }) => {
  const sheet = await openSheet(page, cue, { wait: { github: 1500 } });
  await sheet.locator("#nd-folder").fill("/work/shop-web");
  await sheet.locator("#nd-name").fill("shop-web");
  await cue.called("desk_look", { folder: "/work/shop-web", part: "github" });
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
  await page.evaluate(() => { CueBridge.world.lookFake = {}; });
  await page.locator(".dh-add").click();
  const folder = await sheet.locator("#nd-folder").inputValue();
  expect(folder).not.toBe("/work/shop-web");
  await expect(sheet.locator(".nd-look-found")).toBeVisible();
  await answered(page, "/work/shop-web", "github");
  await expect(sheet.locator(".nd-look-found .nd-lk-hd")).not.toContainText("shop-web");
  await expect(start(sheet)).toBeEnabled();
  expect((await cue.calls("desk_look_rows")).filter((c) => c.args.folder === "/work/shop-web")).toEqual([]);
});
