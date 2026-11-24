// How work lands, per project (lands.js, lands.rs): the Desk header's one Change (#417) opens the setting: where an item works, items at once, how it lands, the check,
// after landing, push (your own steps for the check and after landing). Choices Cue can't do yet are shown greyed,
// saying "not yet". A folder that isn't a git
// repository says why it works in place.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";

const line = (page) => page.locator(".dk-strip .dk-ab");

test("Cue's own Desk says how its work lands, and Change opens the setting with Cue's setup", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await expect(line(page).locator(".dk-ab-change")).toHaveText("Change");
  await expect(line(page).locator(".dk-ab-words")).toContainText("lands in");
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(dlg.locator(".ld-label")).toHaveText(["Where an item works", "Items at once", "How it lands", "Check before landing", "After landing", "Send finished work to GitHub", "Tests"]);
  const seg = (label) => dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: label }) }).locator(".ld-seg");
  await expect(seg("Where an item works").locator("button.on")).toHaveText("Own copy");
  await expect(dlg.locator(".ld-n")).toHaveText("3");
  await expect(seg("How it lands").locator("button.on")).toHaveText("One commit per item");
  await expect(seg("Check before landing").locator("button.on")).toHaveText("Cue's own check");
  await expect(dlg.locator(".ld-steps li b")).toHaveText(["Build", "Rust tests", "Window loads"]);
  await expect(seg("After landing").locator("button.on")).toHaveText("Install or serve Cue");
  await expect(seg("Send finished work to GitHub").locator("button.on")).toHaveText("Ask me first");
  // Not built yet: shown, can't be picked.
  for (const [label, name] of [["Where an item works", "A branch"]]) {
    const b = seg(label).locator("button", { hasText: name });
    await expect(b).toBeDisabled();
    await expect(b.locator(".ld-not")).toHaveText("not yet");
  }
  // Built: merge, a pull request, and every push choice.
  for (const [label, name] of [["How it lands", "Merge"], ["How it lands", "Pull request"], ["Send finished work to GitHub", "Never"], ["Send finished work to GitHub", "Right away"], ["Check before landing", "Your own steps"], ["After landing", "Your own steps"]]) {
    await expect(seg(label).locator("button", { hasText: name })).toBeEnabled();
  }
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
  await dlg.locator('[data-ld-pick="none"]').click();
  await expect(dlg.locator(".ld-steps")).toHaveCount(0);
  await dlg.locator('[data-ld="less"]').click();
  await expect(dlg.locator(".ld-n")).toHaveText("2");
  await expect(dlg.locator(".ld-row", { hasText: "Items at once" }).locator(".ld-hint")).toHaveText("More than two wait for a free copy.");
  await dlg.locator('[data-ld-pick="owner"]').click();
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Shared folder");
  await expect(dlg.locator(".ld-n")).toHaveCount(0);
  // Saved together by Save (#600), and the screen closes.
  expect(await cue.calls("desk_lands_set")).toHaveLength(0);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_lands_set")).at(-1)?.args.lands).toMatchObject({ check: "none", how: "owner", where: "shared" });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Shared folder");
});

test("Merge keeps each item on its own copy: picked with the shared folder, an own copy comes with it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await dlg.locator('[data-ld-part="where"][data-ld-pick="shared"]').click();
  await dlg.locator('[data-ld-part="how"][data-ld-pick="merge"]').click();
  await expect(dlg.locator('[data-ld-part="how"].on')).toHaveText("Merge");
  await expect(dlg.locator('[data-ld-part="where"].on')).toHaveText("Own copy");
  // The shared folder picked again: back to one commit per item.
  await dlg.locator('[data-ld-part="where"][data-ld-pick="shared"]').click();
  await expect(dlg.locator('[data-ld-part="how"].on')).toHaveText("One commit per item");
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
  await expect(dlg.locator(".ld-why")).toHaveText(why);
  // What the folder rules out isn't offered; what isn't built yet still says so.
  await expect(dlg.locator(".ld-seg button", { hasText: "Own copy" })).toHaveCount(0);
  await expect(dlg.locator(".ld-seg button", { hasText: "One commit per item" })).toHaveCount(0);
  await expect(dlg.locator(".ld-seg button", { hasText: "A branch" })).toBeDisabled();
});

test("a Desk on another project offers no Cue's own check or install, and keeps its own choices apart from Cue's (#460)", async ({ page, cue }) => {
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
  await expect(seg("Check before landing")).toHaveText(["None", "Your own steps"]);
  await expect(seg("After landing")).toHaveText(["Nothing", "Your own steps"]);
  await expect(dlg.getByText("Cue's own check")).toHaveCount(0);
  await expect(dlg.getByText("Install or serve Cue")).toHaveCount(0);
  await expect(dlg.locator("[title]")).toHaveCount(0);
  // Its own steps saved for it alone.
  await seg("After landing").filter({ hasText: "Your own steps" }).click();
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
  // Cue's own Desk keeps its own check and install.
  await page.evaluate(() => { deskSelect("cue"); renderMain(); });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(seg("Check before landing").and(page.locator(".on"))).toHaveText("Cue's own check");
  await expect(seg("After landing").and(page.locator(".on"))).toHaveText("Install or serve Cue");
  await expect(seg("After landing")).toHaveText(["Nothing", "Install or serve Cue", "Your own steps"]);
});

test("a Desk's model for new sessions is picked in its settings (#620)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const pick = dlg.locator("[data-ld-model]");
  await expect(dlg.locator('[data-ld-sec="model"] .ld-model-l')).toHaveText("New sessions start on");
  await expect(pick).toHaveValue("default");
  await pick.selectOption("opus");
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect.poll(async () => (await cue.calls("desk_model_set")).at(-1)?.args).toMatchObject({ model: "opus" });
  await deskAllSessions(page);
  await line(page).locator("[data-lands-change]").click();
  await expect(page.locator(".ld-dialog [data-ld-model]")).toHaveValue("opus");
  await expect(page.locator(".ld-dialog [title]")).toHaveCount(0);
  await page.keyboard.press("Escape");
});

