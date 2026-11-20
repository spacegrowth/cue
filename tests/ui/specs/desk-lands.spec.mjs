// How work lands, per project (lands.js, lands.rs): the Desk's header says it ("… · lands in own copy, checked, one commit
// per item · Settings"), and Settings opens the setting: where an item works, items at once, how it lands, the check,
// after landing, push (your own steps for the check and after landing). Choices Cue can't do yet are shown greyed,
// saying "not yet". A folder that isn't a git
// repository says why it works in place.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const line = (page) => page.locator(".dk-strip .dk-ab");

test("Cue's own Desk says how its work lands, and Settings opens the setting with Cue's setup", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(line(page)).toContainText("lands in own copy, checked, one commit per item · Settings");
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".move-title")).toHaveText("Settings for cue");
  await expect(dlg.locator(".ld-label")).toHaveText(["Where an item works", "Items at once", "How it lands", "Check before landing", "After landing", "Push to main", "Old working folders"]);
  const seg = (label) => dlg.locator(".ld-row", { has: page.locator(".ld-label", { hasText: label }) }).locator(".ld-seg");
  await expect(seg("Where an item works").locator("button.on")).toHaveText("Own copy");
  await expect(dlg.locator(".ld-n")).toHaveText("3");
  await expect(seg("How it lands").locator("button.on")).toHaveText("One commit per item");
  await expect(seg("Check before landing").locator("button.on")).toHaveText("Cue's own check");
  await expect(dlg.locator(".ld-steps li b")).toHaveText(["Build", "Rust tests", "Window loads"]);
  await expect(seg("After landing").locator("button.on")).toHaveText("Install or serve Cue");
  await expect(seg("Push to main").locator("button.on")).toHaveText("Ask each time");
  // Not built yet: shown, can't be picked.
  for (const [label, name] of [["Where an item works", "A branch"], ["How it lands", "Merge"]]) {
    const b = seg(label).locator("button", { hasText: name });
    await expect(b).toBeDisabled();
    await expect(b.locator(".ld-not")).toHaveText("not yet");
  }
  // Built: a pull request, and every push choice.
  for (const [label, name] of [["How it lands", "Pull request"], ["Push to main", "Never"], ["Push to main", "After each landing"], ["Check before landing", "Your own steps"], ["After landing", "Your own steps"]]) {
    await expect(seg(label).locator("button", { hasText: name })).toBeEnabled();
  }
  await expect(dlg.locator("[title]")).toHaveCount(0);
  // Esc closes it, nothing saved.
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
});

test("a change saved shows in the header; you commit means the shared folder", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
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
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await expect(line(page)).toContainText("lands in shared folder, not checked, you commit · Settings");
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
  await expect(line(page)).toContainText("lands in shared folder, not checked, you commit · Settings");
  // Why is on the settings screen, not in the header's one line (#417).
  await expect(page.locator(".dk-strip .dk-ab")).toHaveCount(1);
  await line(page).locator("[data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  await expect(dlg.locator(".ld-why")).toHaveText(why);
  // What the folder rules out isn't offered; what isn't built yet still says so.
  await expect(dlg.locator(".ld-seg button", { hasText: "Own copy" })).toHaveCount(0);
  await expect(dlg.locator(".ld-seg button", { hasText: "One commit per item" })).toHaveCount(0);
  await expect(dlg.locator(".ld-seg button", { hasText: "A branch" })).toBeDisabled();
});
