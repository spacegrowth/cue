// What an item waits on (#250, ui/desk/desk.js deskWaitsOn): its page shows one "Waits on" box with the buttons that clear
// it: a circle (red tag, the loop, Cut this link on each link); the item it's queued after. No "Try again" anywhere.
import { test, expect, openDesk } from "./cue.mjs";

const patchDesk = (cue, patch) => cue.change((st, p) => { Object.assign(st.desk.projects.find((x) => x.project === "cue"), p); }, patch);
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
// An item's page (#418): its holder's chat in the middle, the item itself in the column at the right, in the Items
// list's place, so another item opens from the list after "‹ Items".
const backToItems = async (page) => {
  if (await page.locator(".dk-panel[data-side-for]").count()) await page.locator(".dk-right .dk-back").click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const openItem = async (page, id) => {
  await backToItems(page);
  await page.locator(`.dk-items [data-desk-item="${id}"] .dk2b-tt`).click();
  await page.locator(`.dk-panel[data-side-for="${id}"]`).waitFor();
  return page.locator(".dk-board");
};

test("a circle is shown on each item in it: a red Circle tag, the loop, and Cut this link on every link (#250)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const at = Date.now() - 30000;
  // Two queued items, each after the other.
  await patchItem(cue, "d4", { state: "queued", after: "d5" });
  await patchItem(cue, "d5", { state: "queued", after: "d4" });
  await patchDesk(cue, { circles: [{ key: "d4>d5", items: ["d4", "d5"], words: "#d4 waits on #d5 waits on #d4", at, told: true, links: [
    { item: "d4", on: "d5", why: "queued after it" },
    { item: "d5", on: "d4", why: "queued after it" },
  ] }] });
  const pane = await openItem(page, "d5");
  const box = pane.locator(".dk-wo.circle");
  await expect(box.locator(".dk-wo-tag")).toHaveText("Circle");
  await expect(box).toContainText("Itself, through one other. None can move until one link is cut.");
  await expect(box).toContainText("the lead was told at once");
  await expect(box.locator(".dk-wo-loop")).toHaveText("#d4 waits on #d5 waits on #d4");
  await expect(box.locator('[data-desk-act="cut"]')).toHaveCount(2);
  await expect(pane.locator('[data-desk-act="retry_finish"], [data-desk-act="rerun"]')).toHaveCount(0);
  // No colored left edge on it.
  await expect(box).toHaveCSS("border-left-width", "1px");
  await cue.override("desk_item_act", () => null);
  await box.locator('[data-desk-act="cut"][data-id="d5"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "cut", text: "d4" });
});
