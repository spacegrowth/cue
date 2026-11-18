// What an item waits on (#250, ui/desk.js deskWaitsOn): its page shows one "Waits on" box with the buttons that clear
// it: another session's file its commit waits on (Ask them to commit, Land without that file); a circle (red tag, the
// loop, Cut this link on each link); the item it's queued after. No "Try again" anywhere.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const patchDesk = (cue, patch) => cue.change((st, p) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), p); }, patch);
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const openItem = async (page, id) => {
  await page.locator(`.dk-items [data-desk-item="${id}"] .dk-t`).click();
  return page.locator(".dk-right .dk-pg");
};

test("a commit waiting on another session's file names it and that session's item; Ask them to commit and Land without that file (#250)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const now = Date.now();
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "commit_wait", wait: "web-lead is editing ui/desk.js", at: now } });
  await patchDesk(cue, { waits: [
    { item: "d5", kind: "commit", on: "d3", by: "web-lead", sid: "web", path: "/r/ui/desk.js", name: "ui/desk.js", how: "editing", since: now - 120000, why: "needs ui/desk.js committed" },
    { item: "d5", kind: "commit", on: "", by: "", sid: "", path: "/r/b.txt", name: "b.txt", how: "changed", since: now - 60000, why: "needs b.txt committed" },
  ] });
  const pane = await openItem(page, "d5");
  const boxes = pane.locator(".dk-wo");
  await expect(boxes).toHaveCount(2);
  await expect(boxes.nth(0)).toContainText("web-lead is editing ui/desk.js");
  await expect(boxes.nth(0)).toContainText("that's #d3");
  await expect(boxes.nth(0)).toContainText("Since ");
  // A file only a shell command wrote is never given a guessed name, and there's nobody to ask.
  await expect(boxes.nth(1)).toContainText("Someone else also changed b.txt");
  await expect(boxes.nth(1).locator('[data-desk-act="ask_commit"]')).toHaveCount(0);
  await expect(pane.locator('[data-desk-act="retry_finish"], [data-desk-act="rerun"]')).toHaveCount(0);
  await cue.override("desk_item_act", () => null);
  await boxes.nth(0).locator('[data-desk-act="ask_commit"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "ask_commit", text: "/r/ui/desk.js" });
  await boxes.nth(1).locator('[data-desk-act="land_without"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "land_without", text: "/r/b.txt" });
  // Once asked, it says so in place of the button.
  await patchItem(cue, "d5", { finish: { step: "commit_wait", at: now, asked: ["web"] } });
  await expect(boxes.nth(0)).toContainText("Asked them to commit");
  await expect(boxes.nth(0).locator('[data-desk-act="ask_commit"]')).toHaveCount(0);
});

test("a circle is shown on each item in it: a red Circle tag, the loop, and Cut this link on every link (#250)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = Date.now() - 30000;
  await patchItem(cue, "d4", { state: "queued", after: "d5" });
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "commit_wait", at } });
  await patchDesk(cue, { circles: [{ key: "d4>d5", items: ["d4", "d5"], words: "#d4 waits on #d5 waits on #d4", at, told: true, links: [
    { item: "d4", on: "d5", why: "queued after it" },
    { item: "d5", on: "d4", why: "needs ui/desk.js committed" },
  ] }] });
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo.circle");
  await expect(box.locator(".dk-wo-tag")).toHaveText("Circle");
  await expect(box).toContainText("Itself, through one other. None can move until one link is cut.");
  await expect(box).toContainText("the lead was told at once");
  await expect(box.locator(".dk-wo-loop")).toHaveText("#d4 waits on #d5 waits on #d4");
  await expect(box.locator('[data-desk-act="cut"]')).toHaveCount(2);
  // No colored left edge on it.
  await expect(box).toHaveCSS("border-left-width", "1px");
  await cue.override("desk_item_act", () => null);
  await box.locator('[data-desk-act="cut"][data-id="d5"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "cut", text: "d4" });
});
