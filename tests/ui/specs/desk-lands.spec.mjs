// How work lands, per project (lands.js, lands.rs): the Desk header's one Change (#417) opens the setting: where an item works, items at once, how it lands, the check,
// after landing, push (your own steps for the check and after landing). Choices Cue can't do yet are shown greyed,
// saying "not yet". A folder that isn't a git
// repository says why it works in place.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";

const line = (page) => page.locator(".dk-strip .dk-ab");

test("the app's own Desk says how its work lands, and Change opens the setting with its steps as its own (#902)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await expect(line(page).locator(".dk-ab-change")).toHaveText("Change");
  await expect(line(page).locator(".dk-ab-words")).toContainText("lands in");
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  // Option A of #904 (#913): four tabs as Settings has, each with a few rows.
  await expect(dlg.locator("[data-ld-tab]")).toHaveText(["Where it works", "When it's done", "Ask me first", "More"]);
  await expect(dlg.locator(".ld-label")).toHaveText(["Each item works in", "At once", "New sessions start on"]);
  const seg = (label) => dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: label }) }).locator(".ld-seg");
  await expect(seg("Each item works in").locator("button.on")).toHaveText("Own copy");
  await expect(dlg.locator(".ld-n")).toHaveText("3");
  // Not built yet: shown, can't be picked.
  for (const [label, name] of [["Each item works in", "A branch"]]) {
    const b = seg(label).locator("button", { hasText: name });
    await expect(b).toBeDisabled();
    await expect(b.locator(".ld-not")).toHaveText("not yet");
  }
  await dlg.locator('[data-ld-tab="done"]').click();
  await expect(dlg.locator(".ld-label")).toHaveText(["Check first", "Lands on main as", "Then runs", "Send to GitHub"]);
  await expect(seg("Lands on main as").locator("button.on")).toHaveText("One commit");
  // The same choices as any Desk; its window's check is a step of its own (#902), shown as one line until you edit it.
  await expect(seg("Check first").locator("button")).toHaveText(["None", "Your own steps"]);
  await expect(seg("Check first").locator("button.on")).toHaveText("Your own steps");
  await expect(dlg.locator(".ld-steps").first()).toHaveText(/1 step: Window loads\s*Edit steps/);
  await dlg.locator('[data-ld-edit="check"]').click();
  await expect(dlg.locator('[data-ld-steps="check"]')).toHaveValue("[Window loads] test ! -e scripts/check-window.mjs || node scripts/check-window.mjs .");
  await expect(dlg.getByText(/Rust tests|Cue's own/)).toHaveCount(0);
  // Its build, install and restart are named steps after landing, as any project's own (#796).
  await expect(seg("Then runs").locator("button.on")).toHaveText("Your own steps");
  await expect(dlg.locator(".ld-steps")).toHaveText(/3 steps: Build · Install · Restart\s*Edit steps/);
  await dlg.locator('[data-ld-edit="after"]').click();
  await expect(dlg.locator('[data-ld-steps="after"]')).toHaveValue("[Build] app:build\n[Install] app:install\n[Restart] app:restart");
  await expect(seg("Send to GitHub").locator("button.on")).toHaveText("Ask me first");
  // Built: merge, a pull request, and every push choice.
  for (const [label, name] of [["Lands on main as", "Merge"], ["Lands on main as", "Pull request"], ["Send to GitHub", "Never"], ["Send to GitHub", "Right away"], ["Check first", "Your own steps"], ["Then runs", "Your own steps"]]) {
    await expect(seg(label).locator("button", { hasText: name })).toBeEnabled();
  }
  await dlg.locator('[data-ld-tab="more"]').click();
  await expect(dlg.locator(".ld-label")).toHaveText(["Tests", "Items waiting on another start", "Old working folders"]);
  await dlg.locator('[data-ld-tab="ask"]').click();
  // Pushing is said once, on its own row: Ask me before has no push to main row (#692).
  await expect(dlg.locator('[data-ab-kind="main"]')).toHaveCount(0);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  // Esc closes it.
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
});

test("a change saved shows on the screen next time; you commit means the shared folder", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld="less"]').click();
  await expect(dlg.locator(".ld-n")).toHaveText("2");
  await expect(dlg.locator(".ld-row", { hasText: "At once" }).locator(".ld-hint")).toHaveText("More than two wait for a free copy.");
  await dlg.locator('[data-ld-tab="done"]').click();
  await dlg.locator('[data-ld-pick="none"]').click();
  await expect(dlg.locator('[data-ld-steps="check"], [data-ld-edit="check"]')).toHaveCount(0);
  await dlg.locator('[data-ld-pick="owner"]').click();
  await dlg.locator('[data-ld-tab="where"]').click();
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Shared folder");
  // At once limits the shared folder too (#798): the number stays, and says what happens past it.
  await expect(dlg.locator(".ld-n")).toHaveText("2");
  await expect(dlg.locator(".ld-row", { hasText: "At once" }).locator(".ld-hint")).toHaveText("More than two wait in Up next and start by themselves as one finishes.");
  // Saved together by Save (#600), and the screen closes.
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args.lands).toMatchObject({ check: "none", how: "owner", where: "shared" });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await dlg.locator('[data-ld-tab="where"]').click();
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Shared folder");
});

test("Merge keeps each item on its own copy: picked with the shared folder, an own copy comes with it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const tab = (t) => dlg.locator(`[data-ld-tab="${t}"]`).click();
  await tab("where");
  await dlg.locator('[data-ld-part="where"][data-ld-pick="shared"]').click();
  await tab("done");
  await dlg.locator('[data-ld-part="how"][data-ld-pick="merge"]').click();
  await expect(dlg.locator('[data-ld-part="how"].on')).toHaveText("Merge");
  await tab("where");
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Own copy");
  // The shared folder picked again: back to one commit per item.
  await dlg.locator('[data-ld-part="where"][data-ld-pick="shared"]').click();
  await tab("done");
  await expect(dlg.locator('[data-ld-part="how"].on')).toHaveText("One commit");
  await dlg.locator('[data-ld-part="how"][data-ld-pick="merge"]').click();
  await dlg.locator('[data-ld="save"]').click();
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args.lands).toMatchObject({ how: "merge", where: "copy" });
});

test("a folder that isn't a git repository works in place and its settings say why", async ({ page, cue }) => {
  await cue.open("desk");
  const why = "This folder isn't a git repository, so items work in it directly, nothing is checked, and you commit their work yourself.";
  // As lands.rs desk_view gives it for a plain folder: an own copy and one commit per item ruled out by the folder.
  await cue.change((st, why) => {
    const c = (key, label, ok, w = "") => ({ key, label, hint: "", ok, why: w });
    st.desk.projects.find((x) => x.project === "cue").lands = {
      project: "cue", place: "folder", why, summary: "shared folder, not checked, you commit",
      set: { where: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: 3 },
      parts: [
        { key: "where", label: "Where an item works", now: "shared", choices: [c("shared", "Shared folder", true), c("copy", "Own copy", false, "needs a git repository"), c("branch", "A branch", false, "not yet")] },
        { key: "how", label: "How it lands", now: "owner", choices: [c("squash", "One commit per item", false, "needs a git repository"), c("pr", "Pull request", false, "not yet"), c("owner", "You commit", true)] },
      ],
      at_once: { min: 1, max: 9 }, steps: [],
    };
  }, why);
  await openDesk(page);
  await deskAllSessions(page);
  // Why is on the settings screen; the header has its one line, with one Change (#417).
  await expect(line(page).locator(".dk-ab-words")).toContainText("lands in shared folder, not checked, you commit");
  await expect(line(page).locator(".dk-ab-change")).toHaveText("Change");
  await expect(page.locator(".dk-strip .dk-ab")).toHaveCount(1);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld-tab="where"]').click();
  await expect(dlg.locator(".ld-why")).toHaveText(why);
  // What the folder rules out isn't offered; what isn't built yet still says so.
  await expect(dlg.locator(".ld-seg button", { hasText: "Own copy" })).toHaveCount(0);
  await expect(dlg.locator(".ld-seg button", { hasText: "A branch" })).toBeDisabled();
  await dlg.locator('[data-ld-tab="done"]').click();
  await expect(dlg.locator(".ld-seg button", { hasText: "One commit" })).toHaveCount(0);
});

test("a Desk on another project offers the same choices, and keeps its own apart from the app's own Desk (#460, #902)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  // A second Desk in Cue's workspace, working in another repository.
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    st.desk.projects.push({ ...d, id: "desk-web", project: "desk-web", independent: true, name: "Web app", workspace_project: "cue", folder: "/work/web-app", items: [], decisions: [], lands: undefined });
  });
  await page.evaluate(() => { deskSelect("desk-web"); renderMain(); });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".move-title")).toHaveText("Settings for Web app");
  const seg = (label) => dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: label }) }).locator(".ld-seg button");
  await dlg.locator('[data-ld-tab="done"]').click();
  await expect(seg("Check first")).toHaveText(["None", "Your own steps"]);
  await expect(seg("Then runs")).toHaveText(["Nothing", "Your own steps"]);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  // Its own steps saved for it alone.
  await seg("Then runs").filter({ hasText: "Your own steps" }).click();
  // Saved when you leave the box, not while you type in it. A redraw on the way (anything changing) keeps the cursor
  // where it was: without that, the box lost it mid-word and, gone, counted as left.
  const box = dlg.locator('[data-ld-steps="after"]');
  await box.click();
  await page.keyboard.type("npm run");
  await page.evaluate(() => renderMain());
  await expect(box).toBeFocused();
  await page.keyboard.type(" deploy");
  await expect(box).toHaveValue("npm run deploy");
  await page.keyboard.press("Tab");
  await dlg.locator('[data-ld="save"]').click();
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args).toMatchObject({ desk: "desk-web", lands: { after: "command", after_steps: ["npm run deploy"] } });
  // The app's own Desk keeps its own steps: its window's check, and its build, install and restart.
  await page.evaluate(() => { deskSelect("cue"); renderMain(); });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(seg("Check first").and(page.locator(".on"))).toHaveText("Your own steps");
  await dlg.locator('[data-ld-edit="check"]').click();
  await expect(dlg.locator('[data-ld-steps="check"]')).toHaveValue("[Window loads] test ! -e scripts/check-window.mjs || node scripts/check-window.mjs .");
  await expect(seg("Then runs").and(page.locator(".on"))).toHaveText("Your own steps");
  await expect(seg("Then runs")).toHaveText(["Nothing", "Your own steps"]);
});

test("a Desk's model for new sessions is picked in its settings (#620)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const pick = dlg.locator("[data-ld-model]");
  await dlg.locator('[data-ld-tab="where"]').click();
  await expect(dlg.locator('[data-ld-sec="model"] .ld-label')).toHaveText("New sessions start on");
  await expect(pick).toHaveValue("default");
  await pick.selectOption("opus");
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_model_set")).at(-1)?.args).toMatchObject({ model: "opus" });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await page.locator('.ld-dialog [data-ld-tab="where"]').click();
  await expect(page.locator(".ld-dialog [data-ld-model]")).toHaveValue("opus");
  await expect(page.locator(".ld-dialog [title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
});

