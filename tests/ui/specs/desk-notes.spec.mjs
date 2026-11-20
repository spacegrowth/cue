import { test, expect, openDesk } from "./cue.mjs";

// What you write on an item's page reaches its holder at every stage (#437), or the page says it didn't and offers Retry.

const set = (cue, id, fields) => cue.change((st, [id, fields]) => Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === id), fields), [id, fields]);
async function openItem(page, cue, id) {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).click();
}

for (const state of ["working", "finishing", "queued"]) {
  test(`a note on an item ${state} goes to its holder as a note`, async ({ cue, page }) => {
    await cue.open("desk");
    await set(cue, "d5", { state });
    await openItem(page, cue, "d5");
    const box = page.locator(".dk-right .dk-box textarea");
    await box.fill("Check the narrow window too");
    await box.press("Enter");
    await cue.called("desk_item_act", { id: "d5", act: "note", text: "Check the narrow window too" });
    expect(await cue.calls("desk_send")).toHaveLength(0);
    await expect(box).toHaveValue("");
  });
}

test("what you type stays in the box when the item changes state under it, and goes where it stands then", async ({ cue, page }) => {
  await cue.open("desk");
  await set(cue, "d5", { state: "working" });
  await openItem(page, cue, "d5");
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("One more thing");
  // Its holder says it's done while you type.
  await set(cue, "d5", { state: "to_check" });
  await expect(box).toHaveValue("One more thing");
  await box.press("Enter");
  await cue.called("desk_item_act", { id: "d5", act: "send_back", text: "One more thing" });
});

test("a note the hub didn't take is red on its Thread with why, and Retry and Edit", async ({ cue, page }) => {
  await cue.open("desk");
  await set(cue, "d5", { state: "working" });
  await cue.override("desk_item_act", () => { throw "the hub is busy"; });
  await openItem(page, cue, "d5");
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("Check the narrow window too");
  await box.press("Enter");
  const line = page.locator(".dk-right .dk-ichat .msg-line.ml-failed");
  await expect(line).toContainText("Couldn't send");
  await expect(line).toContainText("the hub is busy");
  await line.locator('[data-desk-act="msg_edit"]').click();
  await expect(box).toHaveValue("Check the narrow window too");
  await expect(line).toHaveCount(0);
});

test("a note that didn't reach its holder says so and Retry sends it again", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue"), it = d.items.find((i) => i.id === "d5");
    Object.assign(it, { state: "working" });
    d.decisions.unshift({ id: "lost1", project: "cue", item_id: "d5", by: "You", sid: "", text: "Check the narrow window too", why: `to ${it.holder.name}`, at: Date.now() / 1000, undo: "none", call: false, delivery: { session_id: it.holder.sid, text: "About #d5 (x): Check the narrow window too", at_ms: Date.now(), went: "failed: Cue stopped before it went" } });
  });
  await openItem(page, cue, "d5");
  const fail = page.locator(".dk-right .dk-ichat .msg-line.ml-failed");
  await expect(fail).toContainText("Couldn't send");
  await expect(fail).toContainText("Cue stopped before it went");
  await fail.locator("[data-desk-retry]").click();
  await cue.called("desk_item_act", { id: "d5", act: "retry", text: "lost1" });
  await expect(fail).toHaveCount(0);
});
